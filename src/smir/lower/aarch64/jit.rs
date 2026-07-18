//! JIT prologue/epilogue, scratch, patch, and finalization

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

    #[cfg(target_arch = "aarch64")]
    pub(crate) fn detect_crc_available() -> bool {
        std::arch::is_aarch64_feature_detected!("crc")
    }


    #[cfg(not(target_arch = "aarch64"))]
    pub(crate) fn detect_crc_available() -> bool {
        true
    }


    /// Lower a validated AArch32 register-indirect branch as an interworking
    /// dispatcher exit, never as a native branch to the guest-controlled value.
    pub fn set_guest_indirect_exits(&mut self, enable: bool) {
        self.guest_indirect_exits = enable;
    }


    /// Emit a direct AArch32 BLX dispatcher exit with an explicit destination
    /// execution state. The SMIR target is an architectural PC, not a tagged
    /// function pointer, so it is stored without further masking.
    pub(crate) fn emit_guest_direct_interworking_exit(
        &mut self,
        resume_pc: u64,
        thumb: bool,
    ) -> Result<(), LowerError> {
        if resume_pc > u64::from(u32::MAX)
            || (thumb && resume_pc & 1 != 0)
            || (!thumb && resume_pc & 3 != 0)
        {
            return Err(LowerError::InvalidOperand {
                op: "AArch32 direct interworking exit".into(),
                operand: format!("PC={resume_pc:#x}, Thumb={thumb}"),
            });
        }
        const SCRATCH: u8 = 9;
        self.emit_push_scratch(SCRATCH);
        self.emit_mov_imm(SCRATCH, resume_pc as i64, OpWidth::W64);
        self.emit_ldst_unsigned(SCRATCH, A64_STATE_REG, 3, 0b00, A64_GUEST_PC_OFFSET / 8);
        let flags =
            A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID | if thumb { A64_EXIT_AARCH32_T } else { 0 };
        self.emit_mov_imm(SCRATCH, flags, OpWidth::W64);
        self.emit_ldst_unsigned(
            SCRATCH,
            A64_STATE_REG,
            3,
            0b00,
            A64_GUEST_EXIT_FLAGS_OFFSET / 8,
        );
        self.emit_pop_scratch(SCRATCH);
        self.emit(0xd65f_03c0);
        Ok(())
    }


    /// Emit an AArch32 BX-style interworking exit. The target is consumed as a
    /// W register, so both PC and state selection follow 32-bit AArch32
    /// semantics even when the host X register has non-zero upper bits.
    pub(crate) fn emit_guest_indirect_exit(&mut self, target: VReg) -> Result<(), LowerError> {
        let target = match target {
            VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if index < 15 => index,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch32 interworking exit target {other:?}"),
                });
            }
        };
        self.emit_guest_indirect_exit_reg(target, false)
    }


    /// Physical-register form used by the BLX-LR snapshot path. When
    /// `restore_target` is set, the caller has already saved `target` on the
    /// host stack and this routine restores it immediately before returning.
    pub(crate) fn emit_guest_indirect_exit_reg(
        &mut self,
        target: u8,
        restore_target: bool,
    ) -> Result<(), LowerError> {
        let scratch = Self::scratch_regs(&[target], 1)?[0];
        self.emit_push_scratch(scratch);

        let (n, immr, imms) = Self::logical_bitmask_imm(0xffff_fffe, OpWidth::W32)?;
        self.emit_logic_imm(scratch, target, 0b00, n, immr, imms, OpWidth::W32)?;
        self.emit_ldst_unsigned(scratch, A64_STATE_REG, 3, 0b00, A64_GUEST_PC_OFFSET / 8);

        // flags = EXIT_VALID | T_VALID | ((target & 1) << 1). None of these
        // instructions updates NZCV, so guest condition flags remain live.
        self.emit_bitfield(scratch, target, 0b10, 0, 0, OpWidth::W32)?;
        self.emit_bitfield(scratch, scratch, 0b10, 31, 30, OpWidth::W32)?;
        let fixed = A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID;
        self.emit_addsub_imm(scratch, scratch, fixed, false, false, OpWidth::W32)?;
        self.emit_ldst_unsigned(
            scratch,
            A64_STATE_REG,
            3,
            0b00,
            A64_GUEST_EXIT_FLAGS_OFFSET / 8,
        );
        self.emit_pop_scratch(scratch);
        if restore_target {
            self.emit_pop_scratch(target);
        }
        self.emit(0xd65f_03c0); // ret to the identity trampoline
        Ok(())
    }


    /// Lower a `VLoad` (SIMD/vector load) as a runtime helper call-out. The
    /// helper reads `size` bytes from guest memory and writes them (zero-padded
    /// to 16) into the destination V register's slot in the state struct; the
    /// lowered code then reloads that V register with `ldr q`. Same spill / LR /
    /// fault-bail discipline as the scalar mem helpers. The helper takes the
    /// STATE pointer (x28) as arg0 so it can reach both the vcpu and the V slots.
    pub(crate) fn emit_jit_vload_op(
        &mut self,
        guest_pc: u64,
        dst: VReg,
        addr: &Address,
        width: VecWidth,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::fp_reg(dst)?;
        let size = Self::vec_width_bytes(width)?;

        self.emit_mem_helper_spill()?;
        self.emit_simd_spill_all(); // V regs survive the call; helper overwrites dst slot
        self.emit_push_scratch(30);
        self.emit_mem_helper_addr(addr)?; // x1 = addr
        self.emit_mov_reg(0, A64_STATE_REG, OpWidth::W64)?; // x0 = state ptr
        self.emit_mov_imm(2, dst_idx as i64, OpWidth::W32)?; // w2 = dst V index
        self.emit_mov_imm(3, size as i64, OpWidth::W32)?; // w3 = size
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_VEC_LOAD_FN_OFFSET / 8);
        self.emit_blr_reg(9); // -> x0 = ok; helper wrote struct.v[dst]
        self.emit_pop_scratch(30);

        let cbz_off = self.code.position();
        self.emit(0xb400_0000); // cbz x0, <fault>
        self.emit_mem_helper_reload()?;
        self.emit_simd_reload_all(); // dst = loaded vector; all others restored
        let done_off = self.code.position();
        self.emit(0x1400_0000); // b <done>
        self.patch_compare_branch_to_current(cbz_off, 0, false)?;
        self.emit_mem_helper_reload()?;
        self.emit_simd_reload_all();
        self.emit_native_exit(guest_pc)?;
        self.patch_branch_to_current(done_off)?;
        Ok(())
    }


    /// Lower a `VStore` (SIMD/vector store) as a runtime helper call-out: publish
    /// the source V register into its state-struct slot (`str q`), then call the
    /// helper to store `size` bytes to guest memory.
    pub(crate) fn emit_jit_vstore_op(
        &mut self,
        guest_pc: u64,
        src: VReg,
        addr: &Address,
        width: VecWidth,
    ) -> Result<(), LowerError> {
        let src_idx = Self::fp_reg(src)?;
        let size = Self::vec_width_bytes(width)?;

        self.emit_mem_helper_spill()?;
        self.emit_simd_spill_all(); // publishes V_src to its slot + preserves all V
        self.emit_push_scratch(30);
        self.emit_mem_helper_addr(addr)?; // x1 = addr
        self.emit_mov_reg(0, A64_STATE_REG, OpWidth::W64)?; // x0 = state ptr
        self.emit_mov_imm(2, src_idx as i64, OpWidth::W32)?; // w2 = src V index
        self.emit_mov_imm(3, size as i64, OpWidth::W32)?; // w3 = size
        self.emit_ldst_unsigned(9, A64_STATE_REG, 3, 0b01, A64_GUEST_VEC_STORE_FN_OFFSET / 8);
        self.emit_blr_reg(9); // -> x0 = ok
        self.emit_pop_scratch(30);

        let cbz_off = self.code.position();
        self.emit(0xb400_0000); // cbz x0, <fault>
        self.emit_mem_helper_reload()?;
        self.emit_simd_reload_all();
        let done_off = self.code.position();
        self.emit(0x1400_0000); // b <done>
        self.patch_compare_branch_to_current(cbz_off, 0, false)?;
        self.emit_mem_helper_reload()?;
        self.emit_simd_reload_all();
        self.emit_native_exit(guest_pc)?;
        self.patch_branch_to_current(done_off)?;
        Ok(())
    }


    pub(crate) fn emit_push_scratch(&mut self, rt: u8) {
        self.emit_ldst_simm(rt, 31, 3, 0b00, -16, 0b11);
    }


    pub(crate) fn emit_pop_scratch(&mut self, rt: u8) {
        self.emit_ldst_simm(rt, 31, 3, 0b01, 16, 0b01);
    }


    pub(crate) fn lower_base_offset_to_scratch(
        &mut self,
        avoid: &[u8],
        base: VReg,
        offset: i64,
    ) -> Result<(Vec<u8>, u8), LowerError> {
        let base = Self::base_gpr(base)?;
        let mut avoid = avoid.to_vec();
        if base != 31 {
            avoid.push(base);
        }

        let scratches = Self::scratch_regs(&avoid, 1)?;
        let addr = scratches[0];
        self.emit_scratch_save(&scratches);
        if base == 31 {
            let saved_sp_delta = (scratches.len() as i64) * 16;
            self.emit_add_signed_imm(addr, 31, saved_sp_delta, OpWidth::W64)?;
        } else {
            self.emit_add_signed_imm(addr, base, 0, OpWidth::W64)?;
        }
        self.lower_lea_add_disp(addr, offset)?;
        Ok((scratches, addr))
    }


    pub(crate) fn lower_base_index_scale_to_scratch(
        &mut self,
        avoid: &[u8],
        base: Option<VReg>,
        index: VReg,
        scale: u8,
        disp: i32,
    ) -> Result<(Vec<u8>, u8), LowerError> {
        let shift = Self::lea_scale_shift(scale)?;
        let base_reg = base.map(Self::base_gpr).transpose()?;
        let index_reg = Self::gpr_arm_or_x86(index)?;
        let disp = i64::from(disp);
        let needs_disp_reg = disp != 0 && !Self::signed_addsub_imm_fits(disp);

        let mut avoid = avoid.to_vec();
        if let Some(base_reg) = base_reg {
            if base_reg < 31 {
                avoid.push(base_reg);
            }
        }
        if index_reg < 31 {
            avoid.push(index_reg);
        }

        let scratches = Self::scratch_regs(&avoid, 1 + usize::from(needs_disp_reg))?;
        let addr = scratches[0];
        let disp_reg = scratches.get(1).copied();
        self.emit_scratch_save(&scratches);

        match base_reg {
            Some(31) => {
                let saved_sp_delta = (scratches.len() as i64) * 16;
                self.emit_add_signed_imm(addr, 31, saved_sp_delta, OpWidth::W64)?;
                self.emit_addsub_shifted(
                    addr,
                    addr,
                    index_reg,
                    false,
                    false,
                    0,
                    shift,
                    OpWidth::W64,
                )?;
            }
            Some(base_reg) => {
                self.emit_addsub_shifted(
                    addr,
                    base_reg,
                    index_reg,
                    false,
                    false,
                    0,
                    shift,
                    OpWidth::W64,
                )?;
            }
            None => {
                self.emit_addsub_shifted(
                    addr,
                    31,
                    index_reg,
                    false,
                    false,
                    0,
                    shift,
                    OpWidth::W64,
                )?;
            }
        }

        if disp != 0 {
            if let Some(disp_reg) = disp_reg {
                self.emit_mov_imm(disp_reg, disp, OpWidth::W64)?;
                self.emit_addsub_reg(addr, addr, disp_reg, false, false, OpWidth::W64)?;
            } else {
                self.emit_add_signed_imm(addr, addr, disp, OpWidth::W64)?;
            }
        }

        Ok((scratches, addr))
    }


    pub(crate) fn x86_partial_write_scratch(
        dst: VReg,
        width: OpWidth,
        sources: &[VReg],
        source_operands: &[&SrcOperand],
    ) -> Result<Option<(u8, u8)>, LowerError> {
        if !matches!(dst, VReg::Arch(ArchReg::X86(_)))
            || !matches!(width, OpWidth::W8 | OpWidth::W16)
        {
            return Ok(None);
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let mut avoid = vec![dst];
        for source in sources {
            if !matches!(source, VReg::Imm(_)) {
                avoid.push(Self::gpr_arm_or_x86(*source)?);
            }
        }
        for source in source_operands {
            let source = match source {
                SrcOperand::Reg(reg)
                | SrcOperand::Shifted { reg, .. }
                | SrcOperand::Extended { reg, .. } => Some(*reg),
                SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            };
            if let Some(source) = source {
                avoid.push(Self::gpr_arm_or_x86(source)?);
            }
        }

        Ok(Some((dst, Self::scratch_regs(&avoid, 1)?[0])))
    }


    pub(crate) fn scratch_regs(avoid: &[u8], count: usize) -> Result<Vec<u8>, LowerError> {
        const CANDIDATES: [u8; 31] = [
            16, 17, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30,
        ];

        let mut regs = Vec::with_capacity(count);
        for reg in CANDIDATES {
            if avoid.contains(&reg) || regs.contains(&reg) {
                continue;
            }
            regs.push(reg);
            if regs.len() == count {
                return Ok(regs);
            }
        }

        Err(LowerError::UnsupportedOp {
            op: format!("AArch64 native lowering needs {count} scratch registers"),
        })
    }


    pub(crate) fn emit_scratch_save(&mut self, regs: &[u8]) {
        for &reg in regs {
            self.emit_push_scratch(reg);
        }
    }


    pub(crate) fn emit_scratch_restore(&mut self, regs: &[u8]) {
        for &reg in regs.iter().rev() {
            self.emit_pop_scratch(reg);
        }
    }


    pub(crate) fn finish_cmove_width(&mut self, dst: u8, width: OpWidth) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                let imms = if width == OpWidth::W8 { 7 } else { 15 };
                self.emit_bitfield(dst, dst, 0b10, 0, imms, OpWidth::W32)
            }
            OpWidth::W32 => self.emit_mov_reg(dst, dst, OpWidth::W32),
            OpWidth::W64 => Ok(()),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native CMove width {other:?}"),
            }),
        }
    }


    pub(crate) fn finish_select_width(&mut self, dst: VReg, width: OpWidth) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                let imms = if width == OpWidth::W8 { 7 } else { 15 };
                let dst = Self::dst_gpr_arm_or_x86(dst)?;
                self.emit_bitfield(dst, dst, 0b10, 0, imms, OpWidth::W32)
            }
            OpWidth::W32 | OpWidth::W64 => Ok(()),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Select width {other:?}"),
            }),
        }
    }


    /// Lower the one BLX shape that cannot be expressed as an ordinary
    /// architectural-register terminator use: `BLX LR` snapshots old LR into
    /// a virtual W32 value before writing the return address. The snapshot is
    /// assigned a spilled host scratch register for the duration of the exit.
    pub(crate) fn try_lower_guest_blx_lr_exit(&mut self, block: &SmirBlock) -> Result<bool, LowerError> {
        let Terminator::Call {
            target: CallTarget::IndirectInterworking(VReg::Virtual(snapshot)),
            args,
            ..
        } = &block.terminator
        else {
            return Ok(false);
        };
        if !self.guest_interworking_call_exits || !args.is_empty() {
            return Err(LowerError::UnsupportedOp {
                op: "AArch32 BLX-LR dispatcher exit is not enabled or has arguments".into(),
            });
        }
        let [prefix @ .., snapshot_op, link_op] = block.ops.as_slice() else {
            return Err(LowerError::UnsupportedOp {
                op: "AArch32 BLX-LR exit is missing snapshot/link operations".into(),
            });
        };
        let valid_snapshot = matches!(
            snapshot_op.kind,
            OpKind::Mov {
                dst: VReg::Virtual(id),
                src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::X(14)))),
                width: OpWidth::W32,
            } if id == *snapshot
        );
        let valid_link = matches!(
            link_op.kind,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                src: SrcOperand::Imm(_),
                width: OpWidth::W32,
            }
        );
        if !valid_snapshot || !valid_link {
            return Err(LowerError::UnsupportedOp {
                op: "malformed AArch32 BLX-LR snapshot/link sequence".into(),
            });
        }

        self.lower_ops(prefix)?;
        let target = Self::scratch_regs(&[14, A64_STATE_REG], 1)?[0];
        self.emit_push_scratch(target);
        self.emit_mov_reg(target, 14, OpWidth::W32)?;
        self.lower_op(link_op)?;
        self.emit_guest_indirect_exit_reg(target, true)?;
        Ok(true)
    }
}
