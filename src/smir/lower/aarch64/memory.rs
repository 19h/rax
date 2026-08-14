//! Load, store, and addressing lowering

use crate::smir::lower::aarch64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    ArmDpRegShiftKind, OpKind, SmirOp, X86AdxKind, X86BlsKind, X86CountKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, AtomicOp, Avx10FP16Op, BlockId, Condition, ExtendOp, FenceKind,
    FpPrecision, FpRoundMode, MemWidth, MemoryOrder, OpWidth, ShiftOp, SignExtend, SrcOperand,
    VLaneOp, VReg, VecElementType, VecPermuteKind, VecReduceOp, VecUnaryOp, VecWidth,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

use super::{CodeBuffer, LowerError, LowerResult, Relocation, SmirLowerer};

impl Aarch64Lowerer {
    /// Route memory ops through MMU-translated runtime helpers rather than
    /// inline native loads/stores. Call before `lower_function`.
    pub fn set_mem_helpers(&mut self, enable: bool) {
        self.mem_helpers = enable;
    }

    /// Select the arithmetic width used to form MMU-helper addresses.
    ///
    /// `W64` is the AArch64 default. Cross-lowered AArch32 regions must select
    /// `W32`, which both wraps additions modulo 2^32 and zero-extends the
    /// resulting helper argument according to AAPCS64 register semantics.
    /// Other widths are rejected when a helper address is emitted.
    pub fn set_mem_helper_addr_width(&mut self, width: OpWidth) {
        self.mem_helper_addr_width = width;
    }

    /// Byte offset of a guest base/index register within `Aarch64GuestRegs`
    /// (X(n) → n*8, SP → 248). The helper reads the *frozen* guest value from
    /// the struct, which is why guest-SP-relative addressing is legal under the
    /// helper path (the host SP is the JIT stack, not the guest SP).
    pub(crate) fn arm_struct_slot(vreg: VReg) -> Result<u32, LowerError> {
        match vreg {
            VReg::Arch(ArchReg::Arm(ArmReg::X(n))) if n < 31 => Ok((n as u32) * 8),
            VReg::Arch(ArchReg::Arm(ArmReg::Sp)) => Ok(A64_GUEST_SP_OFFSET),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 mem-helper address register {other:?}"),
            }),
        }
    }

    /// Spill the live guest GPRs a C helper may clobber (x0–x17 caller-saved,
    /// x29) plus NZCV into the state struct, so they survive a `blr`. x18/x28/
    /// x30 are reserved (never live guest state in a region body); x19–x27 are
    /// AAPCS64 callee-saved and survive a compliant helper. Spilling to the
    /// struct (not the host stack) keeps SP exactly 16-aligned for the call.
    pub(crate) fn emit_mem_helper_spill(&mut self) -> Result<(), LowerError> {
        for r in 0u8..=17 {
            self.emit_ldst_unsigned(r, A64_STATE_REG, 3, 0b00, r as u32);
        }
        self.emit_ldst_unsigned(29, A64_STATE_REG, 3, 0b00, 29);
        self.emit_sysreg(9, ArmReg::Nzcv, true)?; // mrs x9, nzcv (x9 already spilled)
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b00, A64_GUEST_NZCV_OFFSET / 8);
        Ok(())
    }

    /// Reverse of [`Self::emit_mem_helper_spill`]: restore NZCV (via x9, before
    /// x9 itself is reloaded) then x0–x17,x29 from the struct.
    pub(crate) fn emit_mem_helper_reload(&mut self) -> Result<(), LowerError> {
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_NZCV_OFFSET / 8);
        self.emit_sysreg(9, ArmReg::Nzcv, false)?; // msr nzcv, x9
        for r in 0u8..=17 {
            self.emit_ldst_unsigned(r, A64_STATE_REG, 3, 0b01, r as u32);
        }
        self.emit_ldst_unsigned(29, A64_STATE_REG, 3, 0b01, 29);
        Ok(())
    }

    /// Compute a guest effective address into x1 (helper arg1) from either a
    /// bounded absolute literal or the spilled state-struct slots — never the
    /// live host regs, which the spill froze and the upcoming `blr` will
    /// clobber. Uses x9 as scratch.
    pub(crate) fn emit_mem_helper_addr(&mut self, addr: &Address) -> Result<(), LowerError> {
        const A: u8 = 1; // x1 = address arg
        const T: u8 = 9; // scratch
        let width = self.mem_helper_addr_width;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 mem-helper address width {width:?}"),
            });
        }
        match addr {
            Address::Direct(base) => {
                let slot = Self::arm_struct_slot(*base)?;
                self.emit_ldst_unsigned(A, A64_STATE_REG, 3, 0b01, slot / 8);
            }
            Address::BaseOffset { base, offset, .. } => {
                let slot = Self::arm_struct_slot(*base)?;
                self.emit_ldst_unsigned(A, A64_STATE_REG, 3, 0b01, slot / 8);
                if *offset != 0 {
                    self.emit_add_signed_imm(A, A, *offset, width)?;
                }
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                if let Some(b) = base {
                    let bslot = Self::arm_struct_slot(*b)?;
                    self.emit_ldst_unsigned(A, A64_STATE_REG, 3, 0b01, bslot / 8);
                } else {
                    self.emit_mov_imm(A, 0, width)?;
                }
                let islot = Self::arm_struct_slot(*index)?;
                self.emit_ldst_unsigned(T, A64_STATE_REG, 3, 0b01, islot / 8);
                let shift = match scale {
                    1 => 0u32,
                    2 => 1,
                    4 => 2,
                    8 => 3,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 mem-helper index scale {other}"),
                        });
                    }
                };
                // add {w,x}1, {w,x}1, {w,x}9, lsl #shift
                self.emit(
                    if width == OpWidth::W64 {
                        0x8b00_0000
                    } else {
                        0x0b00_0000
                    } | ((T as u32) << 16)
                        | (shift << 10)
                        | ((A as u32) << 5)
                        | (A as u32),
                );
                if *disp != 0 {
                    self.emit_add_signed_imm(A, A, *disp as i64, width)?;
                }
            }
            Address::Absolute(address) => {
                if width == OpWidth::W32 && *address > u64::from(u32::MAX) {
                    return Err(LowerError::InvalidOperand {
                        op: "AArch64 W32 mem-helper absolute address".into(),
                        operand: format!("{address:#x}"),
                    });
                }
                self.emit_mov_imm(A, *address as i64, width)?;
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 mem-helper address form {other:?}"),
                });
            }
        }
        Ok(())
    }

    /// Lower a `Load` as an MMU-translated runtime helper call-out:
    /// spill-all → save LR → compute addr → `load_fn(ctx, addr, size, signed)
    /// -> (value in x0, ok in x1)` → restore LR → fault-bail on `!ok` →
    /// deliver value into the dst slot → reload. On fault the faulting op's
    /// guest PC is recorded and the region exits to the interpreter (precise
    /// restart). See the Phase-2b spec.
    pub(crate) fn emit_jit_mem_load_op(
        &mut self,
        guest_pc: u64,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        sign: SignExtend,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr(dst)?;
        let size = Self::mem_width_bytes(width)?;
        let signed = matches!(sign, SignExtend::Sign) as i64;

        self.emit_mem_helper_spill()?;
        self.emit_push_scratch(30); // save trampoline LR around the blr
        self.emit_mem_helper_addr(addr)?; // x1 = effective address
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b01, A64_GUEST_CTX_OFFSET / 8); // x0 = ctx
        self.emit_mov_imm(2, size as i64, OpWidth::W32)?; // w2 = size
        self.emit_mov_imm(3, signed, OpWidth::W32)?; // w3 = signed
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_LOAD_FN_OFFSET / 8);
        self.emit_blr_reg(9); // -> x0 = value, x1 = ok
        self.emit_pop_scratch(30); // restore LR

        let cbz_off = self.code.position();
        self.emit(0xb400_0000 | 1); // cbz x1, <fault>  (back-patched)
        // OK: stash value (x0) into the dst slot, then bulk-reload so the dst
        // register ends up with the value and every other reg is restored.
        // AArch32 architectural registers are W32 values. Signed B1/B2 helpers
        // return a sign-extended u64, so canonicalize through W0 before storing
        // the state slot; a later direct-address use must observe 0x00000000_x,
        // not stale sign bits in the upper half of X0.
        if self.mem_helper_addr_width == OpWidth::W32 {
            self.emit_mov_reg(0, 0, OpWidth::W32)?;
        }
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b00, dst as u32);
        self.emit_mem_helper_reload()?;
        let done_off = self.code.position();
        self.emit(0x1400_0000); // b <done>  (back-patched)
        // fault label:
        self.patch_compare_branch_to_current(cbz_off, 1, false)?;
        self.emit_mem_helper_reload()?;
        self.emit_native_exit(guest_pc)?;
        // done label:
        self.patch_branch_to_current(done_off)?;
        Ok(())
    }

    /// Lower a `Store` as an MMU-translated runtime helper call-out:
    /// `store_fn(ctx, addr, value, size) -> ok in x0`, with the same spill / LR
    /// / fault-bail discipline as [`Self::emit_jit_mem_load_op`].
    pub(crate) fn emit_jit_mem_store_op(
        &mut self,
        guest_pc: u64,
        src: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let size = Self::mem_width_bytes(width)?;

        self.emit_mem_helper_spill()?;
        self.emit_push_scratch(30);
        self.emit_mem_helper_addr(addr)?; // x1 = effective address
        // x2 = value (from the spilled slot, or an immediate)
        match src {
            VReg::Arch(ArchReg::Arm(ArmReg::X(n))) if n < 31 => {
                self.emit_ldst_unsigned(2, A64_STATE_REG, 3, 0b01, n as u32);
            }
            VReg::Imm(v) => {
                self.emit_mov_imm(2, v, OpWidth::W64)?;
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 mem-helper store source {other:?}"),
                });
            }
        }
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b01, A64_GUEST_CTX_OFFSET / 8); // x0 = ctx
        self.emit_mov_imm(3, size as i64, OpWidth::W32)?; // w3 = size
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_STORE_FN_OFFSET / 8);
        self.emit_blr_reg(9); // -> x0 = ok
        self.emit_pop_scratch(30);

        let cbz_off = self.code.position();
        self.emit(0xb400_0000); // cbz x0, <fault>  (back-patched)
        self.emit_mem_helper_reload()?;
        let done_off = self.code.position();
        self.emit(0x1400_0000); // b <done>
        self.patch_compare_branch_to_current(cbz_off, 0, false)?;
        self.emit_mem_helper_reload()?;
        self.emit_native_exit(guest_pc)?;
        self.patch_branch_to_current(done_off)?;
        Ok(())
    }

    pub(crate) fn mem_pair_second_addr(addr: &Address, stride: u32) -> Result<Address, LowerError> {
        match addr {
            Address::Direct(base) => Ok(Address::BaseOffset {
                base: *base,
                offset: i64::from(stride),
                disp_size: crate::smir::ir::types::DispSize::Auto,
            }),
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => Ok(Address::BaseOffset {
                base: *base,
                offset: offset.checked_add(i64::from(stride)).ok_or_else(|| {
                    LowerError::InvalidOperand {
                        op: "AArch64 mem-helper pair address".into(),
                        operand: format!("offset {offset} plus stride {stride}"),
                    }
                })?,
                disp_size: *disp_size,
            }),
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                disp_size,
            } => Ok(Address::BaseIndexScale {
                base: *base,
                index: *index,
                scale: *scale,
                disp: disp.checked_add(stride as i32).ok_or_else(|| {
                    LowerError::InvalidOperand {
                        op: "AArch64 mem-helper pair address".into(),
                        operand: format!("displacement {disp} plus stride {stride}"),
                    }
                })?,
                disp_size: *disp_size,
            }),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 mem-helper pair address {other:?}"),
            }),
        }
    }

    /// Route a `LoadPair` through two scalar helpers while retaining the pair's
    /// all-or-nothing destination contract. The first value is held on the host
    /// stack until the second helper succeeds; either fault restores the frozen
    /// guest state without publishing either destination.
    pub(crate) fn emit_jit_mem_load_pair_op(
        &mut self,
        guest_pc: u64,
        dst1: VReg,
        dst2: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let dst1 = Self::dst_gpr(dst1)?;
        let dst2 = Self::dst_gpr(dst2)?;
        if dst1 == dst2 {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 mem-helper LoadPair".into(),
                operand: format!("aliased destination X{dst1}"),
            });
        }
        let size = Self::mem_width_bytes(width)?;
        let second_addr = Self::mem_pair_second_addr(addr, size)?;

        self.emit_mem_helper_spill()?;
        self.emit_push_scratch(30); // preserve trampoline LR across both calls

        self.emit_mem_helper_addr(addr)?;
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b01, A64_GUEST_CTX_OFFSET / 8);
        self.emit_mov_imm(2, size as i64, OpWidth::W32)?;
        self.emit_mov_imm(3, 0, OpWidth::W32)?;
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_LOAD_FN_OFFSET / 8);
        self.emit_blr_reg(9);
        let first_fault = self.code.position();
        self.emit(0xb400_0000 | 1); // cbz x1, <first_fault>

        self.emit_push_scratch(0); // retain first value; SP remains 16-byte aligned
        self.emit_mem_helper_addr(&second_addr)?;
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b01, A64_GUEST_CTX_OFFSET / 8);
        self.emit_mov_imm(2, size as i64, OpWidth::W32)?;
        self.emit_mov_imm(3, 0, OpWidth::W32)?;
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_LOAD_FN_OFFSET / 8);
        self.emit_blr_reg(9);
        let second_fault = self.code.position();
        self.emit(0xb400_0000 | 1); // cbz x1, <second_fault>

        if self.mem_helper_addr_width == OpWidth::W32 {
            self.emit_mov_reg(0, 0, OpWidth::W32)?;
        }
        self.emit_ldst_unsigned(0, A64_STATE_REG, 3, 0b00, dst2 as u32);
        self.emit_pop_scratch(9); // first value
        if self.mem_helper_addr_width == OpWidth::W32 {
            self.emit_mov_reg(9, 9, OpWidth::W32)?;
        }
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b00, dst1 as u32);
        self.emit_pop_scratch(30);
        self.emit_mem_helper_reload()?;
        let done = self.code.position();
        self.emit(0x1400_0000); // b <done>

        self.patch_compare_branch_to_current(second_fault, 1, false)?;
        self.emit_pop_scratch(9); // discard unpublished first value
        self.emit_pop_scratch(30);
        self.emit_mem_helper_reload()?;
        self.emit_native_exit(guest_pc)?;

        self.patch_compare_branch_to_current(first_fault, 1, false)?;
        self.emit_pop_scratch(30);
        self.emit_mem_helper_reload()?;
        self.emit_native_exit(guest_pc)?;

        self.patch_branch_to_current(done)?;
        Ok(())
    }

    pub(crate) fn emit_jit_mem_store_pair_op(
        &mut self,
        guest_pc: u64,
        src1: VReg,
        src2: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let second_addr = Self::mem_pair_second_addr(addr, Self::mem_width_bytes(width)?)?;
        self.emit_jit_mem_store_op(guest_pc, src1, addr, width)?;
        self.emit_jit_mem_store_op(guest_pc, src2, &second_addr, width)
    }

    pub(crate) fn emit_ldst_unsigned(&mut self, rt: u8, rn: u8, size: u32, opc: u32, imm12: u32) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (0b01 << 24)
                | (opc << 22)
                | (imm12 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn emit_ldst_simm(
        &mut self,
        rt: u8,
        rn: u8,
        size: u32,
        opc: u32,
        imm9: i64,
        mode: u32,
    ) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (opc << 22)
                | (((imm9 as u32) & 0x1ff) << 12)
                | (mode << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn emit_ldst_unscaled(&mut self, rt: u8, rn: u8, size: u32, opc: u32, imm9: i64) {
        self.emit_ldst_simm(rt, rn, size, opc, imm9, 0b00);
    }

    pub(crate) fn emit_ldst_reg_offset(
        &mut self,
        rt: u8,
        rn: u8,
        rm: u8,
        size: u32,
        opc: u32,
        option: u32,
        s: u32,
    ) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (opc << 22)
                | (1 << 21)
                | ((rm as u32) << 16)
                | (option << 13)
                | (s << 12)
                | (0b10 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn emit_ldst_pair(
        &mut self,
        rt: u8,
        rt2: u8,
        rn: u8,
        opc: u32,
        load: bool,
        imm7: i64,
        mode: u32,
    ) {
        self.emit(
            (opc << 30)
                | (0b101 << 27)
                | (mode << 23)
                | ((load as u32) << 22)
                | (((imm7 as u32) & 0x7f) << 15)
                | ((rt2 as u32) << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn emit_load_exclusive(&mut self, rt: u8, rn: u8, size: u32) {
        self.emit_load_exclusive_ordered(rt, rn, size, 0);
    }

    pub(crate) fn emit_load_exclusive_ordered(&mut self, rt: u8, rn: u8, size: u32, acquire: u32) {
        self.emit(
            (size << 30)
                | (0b001000 << 24)
                | (1 << 22)
                | (0b11111 << 16)
                | (acquire << 15)
                | (0b11111 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn emit_store_exclusive(&mut self, rs: u8, rt: u8, rn: u8, size: u32) {
        self.emit_store_exclusive_ordered(rs, rt, rn, size, 0);
    }

    pub(crate) fn emit_store_exclusive_ordered(
        &mut self,
        rs: u8,
        rt: u8,
        rn: u8,
        size: u32,
        release: u32,
    ) {
        self.emit(
            (size << 30)
                | (0b001000 << 24)
                | ((rs as u32) << 16)
                | (release << 15)
                | (0b11111 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }

    pub(crate) fn signed_load_w_parts<'a>(
        load: &'a OpKind,
        extend: &OpKind,
    ) -> Result<Option<(u8, &'a Address, u32, u32)>, LowerError> {
        match (load, extend) {
            (
                OpKind::Load {
                    dst: load_dst,
                    addr,
                    width,
                    sign: SignExtend::Sign,
                },
                OpKind::ZeroExtend {
                    dst,
                    src,
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ) if *src == *load_dst => {
                let size = match width {
                    MemWidth::B1 | MemWidth::B2 => Self::mem_size(*width)?,
                    _ => return Ok(None),
                };
                Ok(Some((Self::dst_gpr_arm_or_x86(*dst)?, addr, size, 0b11)))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn lifted_ldpsw_pair_parts<'a>(
        first: &'a SmirOp,
        second: &'a SmirOp,
    ) -> Result<Option<(u8, u8, &'a Address)>, LowerError> {
        if first.guest_pc != second.guest_pc {
            return Ok(None);
        }

        match (&first.kind, &second.kind) {
            (
                OpKind::Load {
                    dst: dst1,
                    addr: addr1,
                    width,
                    sign: SignExtend::Sign,
                },
                OpKind::Load {
                    dst: dst2,
                    addr: addr2,
                    width: width2,
                    sign: SignExtend::Sign,
                },
            ) if width == width2 => {
                if *width != MemWidth::B8 {
                    return Ok(None);
                }
                if !Self::addr_plus_eq(addr1, addr2, 8) {
                    return Ok(None);
                }
                Ok(Some((
                    Self::dst_gpr_arm_or_x86(*dst1)?,
                    Self::dst_gpr_arm_or_x86(*dst2)?,
                    addr1,
                )))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn lower_mem_access(
        &mut self,
        rt: u8,
        addr: &Address,
        size: u32,
        opc: u32,
    ) -> Result<(), LowerError> {
        if let Address::BaseIndexScale {
            base,
            index,
            scale,
            disp,
            ..
        } = addr
        {
            return self
                .lower_mem_base_index_scale_access(rt, *base, *index, *scale, *disp, size, opc);
        }

        let (base_vreg, base, offset) = match addr {
            Address::Direct(base) => (*base, Self::base_gpr(*base)?, 0),
            Address::BaseOffset { base, offset, .. } => (*base, Self::base_gpr(*base)?, *offset),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native memory address {other:?}"),
                });
            }
        };

        let scale = 1_i64 << size;
        if offset >= 0 && offset % scale == 0 {
            let imm12 = offset / scale;
            if imm12 <= 0xfff {
                self.emit_ldst_unsigned(rt, base, size, opc, imm12 as u32);
                return Ok(());
            }
        }

        if (-256..=255).contains(&offset) {
            self.emit_ldst_unscaled(rt, base, size, opc, offset);
            return Ok(());
        }

        let (scratches, addr) = self.lower_base_offset_to_scratch(&[rt], base_vreg, offset)?;
        self.emit_ldst_unsigned(rt, addr, size, opc, 0);
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_mem_indexed_access(
        &mut self,
        rt: u8,
        base: VReg,
        size: u32,
        opc: u32,
        imm9: i64,
        mode: u32,
    ) -> Result<(), LowerError> {
        if !(-256..=255).contains(&imm9) {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native indexed memory offset".into(),
                operand: format!("{imm9:#x} for size {size}"),
            });
        }

        let rn = Self::base_gpr(base)?;
        self.emit_ldst_simm(rt, rn, size, opc, imm9, mode);
        Ok(())
    }

    pub(crate) fn lower_load(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
        sign: SignExtend,
    ) -> Result<(), LowerError> {
        let rt = Self::dst_gpr_arm_or_x86(dst)?;
        let size = Self::mem_size(width)?;
        let opc = Self::load_opc(width, sign)?;
        self.lower_mem_access(rt, addr, size, opc)
    }

    pub(crate) fn lower_store_imm_addr_to_base(
        &mut self,
        addr: &Address,
    ) -> Result<(Vec<u8>, u8), LowerError> {
        match addr {
            Address::Direct(base) => self.lower_base_offset_to_scratch(&[], *base, 0),
            Address::BaseOffset { base, offset, .. } => {
                self.lower_base_offset_to_scratch(&[], *base, *offset)
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => self.lower_base_index_scale_to_scratch(&[], *base, *index, *scale, *disp),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native immediate store address {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_store_imm(
        &mut self,
        value: i64,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let size = Self::mem_size(width)?;
        if value == 0 {
            self.lower_mem_access(31, addr, size, 0b00)?;
            return Ok(());
        }

        let op_width = match width {
            MemWidth::B1 | MemWidth::B2 | MemWidth::B4 => OpWidth::W32,
            MemWidth::B8 => OpWidth::W64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native immediate store width {other:?}"),
                });
            }
        };
        let (addr_scratches, rn) = self.lower_store_imm_addr_to_base(addr)?;
        let value_scratches = Self::scratch_regs(&addr_scratches, 1)?;
        let rt = value_scratches[0];
        self.emit_scratch_save(&value_scratches);
        self.emit_mov_imm(rt, value, op_width)?;
        self.emit_ldst_unsigned(rt, rn, size, 0b00, 0);
        self.emit_scratch_restore(&value_scratches);
        self.emit_scratch_restore(&addr_scratches);
        Ok(())
    }

    pub(crate) fn lower_store(
        &mut self,
        src: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            return self.lower_store_imm(value, addr, width);
        }

        let rt = Self::gpr_arm_or_x86(src)?;
        let size = Self::mem_size(width)?;
        self.lower_mem_access(rt, addr, size, 0b00)
    }

    pub(crate) fn pred_store_src_to_vreg(src: &SrcOperand) -> Result<VReg, LowerError> {
        match src {
            SrcOperand::Reg(reg) => Ok(*reg),
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => Ok(VReg::Imm(*imm)),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native PredStore source {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_pred_load(
        &mut self,
        dst: VReg,
        cond: VReg,
        addr: &Address,
        width: MemWidth,
        signed: SignExtend,
    ) -> Result<(), LowerError> {
        let cond = Self::gpr_arm_or_x86(cond)?;
        let branch = self.code.position();
        self.emit_test_branch(cond, 0, false, 0)?;
        self.lower_load(dst, addr, width, signed)?;
        self.patch_test_branch_to_current(branch, cond, 0, false)
    }

    pub(crate) fn lower_pred_store(
        &mut self,
        src: &SrcOperand,
        cond: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let cond = Self::gpr_arm_or_x86(cond)?;
        let branch = self.code.position();
        self.emit_test_branch(cond, 0, false, 0)?;
        self.lower_store(Self::pred_store_src_to_vreg(src)?, addr, width)?;
        self.patch_test_branch_to_current(branch, cond, 0, false)
    }

    pub(crate) fn lower_load_exclusive(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let rt = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::exclusive_base_gpr(addr)?;
        let size = Self::mem_size(width)?;
        self.emit_load_exclusive(rt, rn, size);
        Ok(())
    }

    pub(crate) fn lower_store_exclusive(
        &mut self,
        status: VReg,
        src: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        let rs = Self::dst_gpr_arm_or_x86(status)?;
        let rt = Self::gpr_arm_or_x86(src)?;
        let rn = Self::exclusive_base_gpr(addr)?;
        // STXR/STLXR are CONSTRAINED UNPREDICTABLE when the status register Rs is
        // the same register as the stored data Rt or the address base Rn. Emitting
        // such an encoding is rejected by assemblers and can trap (SIGILL) on the
        // host. Bail to the interpreter, which handles the overlap with defined
        // behavior, rather than emitting an unpredictable native instruction. (#10)
        if rs == rt || rs == rn {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 STXR status register overlap (Rs={rs}, Rt={rt}, Rn={rn})"),
            });
        }
        let size = Self::mem_size(width)?;
        self.emit_store_exclusive(rs, rt, rn, size);
        Ok(())
    }

    pub(crate) fn lower_pair_mem_access(
        &mut self,
        rt: u8,
        rt2: u8,
        addr: &Address,
        width: MemWidth,
        load: bool,
    ) -> Result<(), LowerError> {
        if let Address::BaseIndexScale {
            base,
            index,
            scale,
            disp,
            ..
        } = addr
        {
            let (opc, _) = Self::pair_width(width)?;
            let (scratches, addr) =
                self.lower_base_index_scale_to_scratch(&[rt, rt2], *base, *index, *scale, *disp)?;
            self.emit_ldst_pair(rt, rt2, addr, opc, load, 0, 0b10);
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        let (base_vreg, base, offset) = match addr {
            Address::Direct(base) => (*base, Self::base_gpr(*base)?, 0),
            Address::BaseOffset { base, offset, .. } => (*base, Self::base_gpr(*base)?, *offset),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native pair memory address {other:?}"),
                });
            }
        };
        if let Some((opc, imm7)) = Self::pair_scaled_imm(width, offset)? {
            self.emit_ldst_pair(rt, rt2, base, opc, load, imm7, 0b10);
            return Ok(());
        }

        let (opc, _) = Self::pair_width(width)?;
        let (scratches, addr) = self.lower_base_offset_to_scratch(&[rt, rt2], base_vreg, offset)?;
        self.emit_ldst_pair(rt, rt2, addr, opc, load, 0, 0b10);
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_pair_indexed_access(
        &mut self,
        rt: u8,
        rt2: u8,
        base: VReg,
        width: MemWidth,
        load: bool,
        offset: i64,
        mode: u32,
    ) -> Result<(), LowerError> {
        let Some((opc, imm7)) = Self::pair_scaled_imm(width, offset)? else {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native indexed pair memory offset".into(),
                operand: format!("{offset:#x} for width {width:?}"),
            });
        };

        let rn = Self::base_gpr(base)?;
        self.emit_ldst_pair(rt, rt2, rn, opc, load, imm7, mode);
        Ok(())
    }

    pub(crate) fn lower_ldpsw_pair_access(
        &mut self,
        rt: u8,
        rt2: u8,
        base: VReg,
        offset: i64,
        mode: u32,
    ) -> Result<(), LowerError> {
        let Some(imm7) = Self::ldpsw_scaled_imm(offset) else {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native LDPSW pair offset".into(),
                operand: format!("{offset:#x}"),
            });
        };

        self.emit_ldst_pair(rt, rt2, Self::base_gpr(base)?, 0b01, true, imm7, mode);
        Ok(())
    }

    pub(crate) fn lower_load_pair(
        &mut self,
        dst1: VReg,
        dst2: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        self.lower_pair_mem_access(
            Self::dst_gpr_arm_or_x86(dst1)?,
            Self::dst_gpr_arm_or_x86(dst2)?,
            addr,
            width,
            true,
        )
    }

    pub(crate) fn lower_store_pair(
        &mut self,
        src1: VReg,
        src2: VReg,
        addr: &Address,
        width: MemWidth,
    ) -> Result<(), LowerError> {
        self.lower_pair_mem_access(
            Self::gpr_arm_or_x86(src1)?,
            Self::gpr_arm_or_x86(src2)?,
            addr,
            width,
            false,
        )
    }

    pub(crate) fn lower_mem_base_index_scale_access(
        &mut self,
        rt: u8,
        base: Option<VReg>,
        index: VReg,
        scale: u8,
        disp: i32,
        size: u32,
        opc: u32,
    ) -> Result<(), LowerError> {
        if let Some(base) = base {
            if disp == 0 {
                if let Ok(s) = Self::mem_index_scale_bit(scale, size) {
                    return self.lower_mem_reg_offset_access(rt, base, index, size, opc, 0b011, s);
                }
            }
        }

        self.lower_mem_base_index_scale_scratch_access(rt, base, index, scale, disp, size, opc)
    }

    pub(crate) fn lower_mem_base_index_scale_scratch_access(
        &mut self,
        rt: u8,
        base: Option<VReg>,
        index: VReg,
        scale: u8,
        disp: i32,
        size: u32,
        opc: u32,
    ) -> Result<(), LowerError> {
        let mut avoid = Vec::new();
        if rt < 31 {
            avoid.push(rt);
        }

        let (scratches, addr) =
            self.lower_base_index_scale_to_scratch(&avoid, base, index, scale, disp)?;
        self.emit_ldst_unsigned(rt, addr, size, opc, 0);
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_mem_reg_offset_access(
        &mut self,
        rt: u8,
        base: VReg,
        index: VReg,
        size: u32,
        opc: u32,
        option: u32,
        s: u32,
    ) -> Result<(), LowerError> {
        self.emit_ldst_reg_offset(
            rt,
            Self::base_gpr(base)?,
            Self::gpr_arm_or_x86(index)?,
            size,
            opc,
            option,
            s,
        );
        Ok(())
    }

    pub(crate) fn lower_lea_add_disp(&mut self, dst: u8, disp: i64) -> Result<(), LowerError> {
        if disp == 0 {
            return Ok(());
        }
        if Self::signed_addsub_imm_fits(disp) {
            return self.emit_add_signed_imm(dst, dst, disp, OpWidth::W64);
        }

        // LEA is address arithmetic and must NOT touch the guest stack: a stack
        // save/restore would `str`/`ldr` at [SP-16], clobbering the word below SP
        // and faulting if guest SP is unmapped or misaligned. Preserve the scratch
        // via its own slot in the always-mapped guest state struct (x28-relative)
        // instead. The scratch must avoid the reserved host registers — especially
        // x28 (the state pointer used as the spill base). (#35)
        let scratch = Self::scratch_regs(&[dst, 18, 28, 30], 1)?[0];
        self.emit_ldst_unsigned(scratch, A64_STATE_REG, 3, 0b00, scratch as u32); // str scratch, [x28,#slot]
        self.emit_mov_imm(scratch, disp, OpWidth::W64)?;
        self.emit_addsub_reg(dst, dst, scratch, false, false, OpWidth::W64)?;
        self.emit_ldst_unsigned(scratch, A64_STATE_REG, 3, 0b01, scratch as u32); // ldr scratch, [x28,#slot]
        Ok(())
    }

    pub(crate) fn lower_lea(
        &mut self,
        dst: VReg,
        addr: &Address,
        guest_pc: u64,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        match addr {
            Address::Direct(base) => {
                self.emit_add_signed_imm(dst, Self::lea_base_gpr(*base)?, 0, OpWidth::W64)
            }
            Address::BaseOffset { base, offset, .. } => {
                let base = Self::lea_base_gpr(*base)?;
                if Self::signed_addsub_imm_fits(*offset) {
                    self.emit_add_signed_imm(dst, base, *offset, OpWidth::W64)
                } else {
                    self.emit_add_signed_imm(dst, base, 0, OpWidth::W64)?;
                    self.lower_lea_add_disp(dst, *offset)
                }
            }
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                let shift = Self::lea_scale_shift(*scale)?;
                let index = Self::gpr_arm_or_x86(*index)?;
                match base {
                    Some(base) => {
                        let base = Self::lea_base_gpr(*base)?;
                        if base == 31 {
                            self.emit_addsub_extended(
                                dst,
                                31,
                                index,
                                false,
                                false,
                                0b011,
                                shift,
                                OpWidth::W64,
                            )?;
                        } else {
                            self.emit_addsub_shifted(
                                dst,
                                base,
                                index,
                                false,
                                false,
                                0,
                                shift,
                                OpWidth::W64,
                            )?;
                        }
                    }
                    None => {
                        self.emit_addsub_shifted(
                            dst,
                            31,
                            index,
                            false,
                            false,
                            0,
                            shift,
                            OpWidth::W64,
                        )?;
                    }
                }
                self.lower_lea_add_disp(dst, i64::from(*disp))
            }
            Address::Absolute(addr) => self.emit_mov_imm_best(dst, *addr as i64, OpWidth::W64),
            Address::PcRel { offset, base, .. } => {
                // A base-less PC-relative LEA (e.g. ADR) resolves to the CURRENT
                // guest PC + offset, matching the interpreter; the previous
                // `unwrap_or(0)` dropped the PC and computed an offset from 0. (#13)
                let addr = base.unwrap_or(guest_pc).wrapping_add(*offset as u64);
                self.emit_mov_imm_best(dst, addr as i64, OpWidth::W64)
            }
            Address::X86Addr32(_) | Address::GpRel { .. } | Address::SegmentRel { .. } => {
                Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native LEA address {addr:?}"),
                })
            }
        }
    }
}
