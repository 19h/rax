//! Integer ALU, shift, bit, and flag lowering

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86TbmKind, X86VecAlign, X86VecMap,
    X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

impl X86_64Lowerer {
    pub(crate) fn emit_shift_reg_imm(
        &mut self,
        kind: ShiftRegOp,
        dst_reg: PhysReg,
        imm: u8,
        width: OpWidth,
    ) {
        let mut emitter = X86Emitter::new(&mut self.code);
        match kind {
            ShiftRegOp::Rol => emitter.emit_rol_ri(dst_reg, imm, width),
            ShiftRegOp::Ror => emitter.emit_ror_ri(dst_reg, imm, width),
            ShiftRegOp::Rcl => emitter.emit_rcl_ri(dst_reg, imm, width),
            ShiftRegOp::Rcr => emitter.emit_rcr_ri(dst_reg, imm, width),
            ShiftRegOp::Shl => emitter.emit_shl_ri(dst_reg, imm, width),
            ShiftRegOp::Shr => emitter.emit_shr_ri(dst_reg, imm, width),
            ShiftRegOp::Sar => emitter.emit_sar_ri(dst_reg, imm, width),
        }
    }

    pub(crate) fn emit_shift_reg_cl(&mut self, kind: ShiftRegOp, dst_reg: PhysReg, width: OpWidth) {
        let mut emitter = X86Emitter::new(&mut self.code);
        match kind {
            ShiftRegOp::Rol => emitter.emit_rol_cl(dst_reg, width),
            ShiftRegOp::Ror => emitter.emit_ror_cl(dst_reg, width),
            ShiftRegOp::Rcl => emitter.emit_rcl_cl(dst_reg, width),
            ShiftRegOp::Rcr => emitter.emit_rcr_cl(dst_reg, width),
            ShiftRegOp::Shl => emitter.emit_shl_cl(dst_reg, width),
            ShiftRegOp::Shr => emitter.emit_shr_cl(dst_reg, width),
            ShiftRegOp::Sar => emitter.emit_sar_cl(dst_reg, width),
        }
    }

    pub(crate) fn lower_shift_reg_op(
        &mut self,
        kind: ShiftRegOp,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let dst_reg = self.get_dst_reg(dst)?;
        let src_reg = self.get_reg(src)?;
        let preserve_flags = !flags.updates_any();
        Self::ensure_flag_stack_operands_safe(kind.name(), &[dst_reg, src_reg])?;

        match amount {
            SrcOperand::Imm(val) => {
                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, width);
                }
                if preserve_flags {
                    self.code.emit_u8(0x9C); // pushfq
                }
                self.emit_shift_reg_imm(kind, dst_reg, *val as u8, width);
                if preserve_flags {
                    self.code.emit_u8(0x9D); // popfq
                }
            }
            SrcOperand::Reg(r) => {
                let amt_reg = self.get_reg(*r)?;
                Self::ensure_flag_stack_operands_safe(kind.name(), &[dst_reg, src_reg, amt_reg])?;

                if dst_reg == PhysReg::Rcx && amt_reg != PhysReg::Rcx {
                    {
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src_reg {
                            emitter.emit_mov_rr(dst_reg, src_reg, width);
                        }
                        emitter.emit_push(dst_reg);
                        emitter.emit_mov_rr(PhysReg::Rcx, amt_reg, OpWidth::W8);
                        emitter.emit_shift_m_disp(
                            kind.digit(),
                            PhysReg::Rsp,
                            0,
                            DispSize::Auto,
                            width,
                            ShiftCount::Cl,
                        );
                        emitter.emit_pop(dst_reg);
                        if width == OpWidth::W32 {
                            emitter.emit_mov_rr(dst_reg, dst_reg, OpWidth::W32);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    return Ok(());
                }

                if dst_reg == PhysReg::Rcx && amt_reg == PhysReg::Rcx && src_reg != dst_reg {
                    if preserve_flags {
                        self.code.emit_u8(0x9C); // pushfq
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // Keep the old RCX live as CL while shifting a stack-resident
                    // destination seeded from src. Starting with old RCX retains
                    // the destination's upper bits for W8/W16 partial writes.
                    emitter.emit_push(dst_reg);
                    emitter.emit_mov_mr(PhysReg::Rsp, 0, src_reg, width);
                    emitter.emit_shift_m_disp(
                        kind.digit(),
                        PhysReg::Rsp,
                        0,
                        DispSize::Auto,
                        width,
                        ShiftCount::Cl,
                    );
                    emitter.emit_pop(dst_reg);
                    if width == OpWidth::W32 {
                        emitter.emit_mov_rr(dst_reg, dst_reg, OpWidth::W32);
                    }
                    if preserve_flags {
                        self.code.emit_u8(0x9D); // popfq
                    }
                    return Ok(());
                }

                if amt_reg != PhysReg::Rcx {
                    if preserve_flags {
                        self.code.emit_u8(0x9C); // pushfq
                    }
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        // CL is the only architectural variable-count input.
                        // Save both guest RCX and the count before copying the
                        // source, because the destination may alias the count
                        // and the source may alias RCX.
                        emitter.emit_push(PhysReg::Rcx);
                        emitter.emit_push(amt_reg);
                        if dst_reg != src_reg {
                            emitter.emit_mov_rr(dst_reg, src_reg, width);
                        }
                        emitter.emit_pop(PhysReg::Rcx);
                    }
                    self.emit_shift_reg_cl(kind, dst_reg, width);
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        // POP preserves the native result flags.
                        emitter.emit_pop(PhysReg::Rcx);
                    }
                    if preserve_flags {
                        self.code.emit_u8(0x9D); // popfq
                    }
                    return Ok(());
                }

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, width);
                }

                if preserve_flags {
                    self.code.emit_u8(0x9C); // pushfq
                }
                self.emit_shift_reg_cl(kind, dst_reg, width);
                if preserve_flags {
                    self.code.emit_u8(0x9D); // popfq
                }
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("{} with shifted operand", kind.name()),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn lower_and_not(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: "AndNot".to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        let SrcOperand::Reg(src2) = src2 else {
            return Err(LowerError::InvalidOperand {
                op: "AndNot".to_string(),
                operand: "x86 native lowering requires a register second source".to_string(),
            });
        };
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        if !matches!(flags, FlagUpdate::None | FlagUpdate::All)
            && flags != FlagUpdate::Specific(defined)
        {
            return Err(LowerError::InvalidOperand {
                op: "AndNot".to_string(),
                operand: format!("unsupported flag update {flags:?}"),
            });
        }

        let dst_reg = self.get_dst_reg(dst)?;
        let src1_reg = self.get_reg(src1)?;
        let src2_reg = self.get_reg(*src2)?;
        Self::ensure_flag_stack_operands_safe("AndNot", &[dst_reg, src1_reg, src2_reg])?;

        if flags != FlagUpdate::All {
            self.code.emit_u8(0x9C); // pushfq: old flags
        }
        {
            // Saving src1 on the host stack makes every destination/source
            // alias shape exact without reserving a guest GPR as scratch.
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(src1_reg);
            emitter.emit_mov_rr(dst_reg, src2_reg, width);
            emitter.emit_not(dst_reg, width);
            emitter.emit_alu_mem_disp(
                0x20,
                dst_reg,
                PhysReg::Rsp,
                0,
                DispSize::Auto,
                width,
                X86AluEncoding::RegRm,
            );
            // Discard the saved source without changing the AND result flags.
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
        }

        match flags {
            FlagUpdate::None => self.code.emit_u8(0x9D), // popfq
            FlagUpdate::Specific(_) => {
                self.finish_bmi_flags(dst_reg, Some(Self::x86_status_rflags_mask(defined)));
            }
            FlagUpdate::All => {}
        }
        Ok(())
    }

    pub(crate) fn lower_x86_bls(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86BlsKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let op = match kind {
            X86BlsKind::Blsr => "Blsr",
            X86BlsKind::Blsmsk => "Blsmsk",
            X86BlsKind::Blsi => "Blsi",
        };
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        let defined_rflags_mask = match flags {
            FlagUpdate::None => None,
            FlagUpdate::Specific(set) if set == defined => {
                Some(Self::x86_status_rflags_mask(defined))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: op.to_string(),
                    operand: format!("unsupported flag update {flags:?}"),
                });
            }
        };

        let dst_reg = self.get_dst_reg(dst)?;
        let src_reg = self.get_reg(src)?;
        Self::ensure_flag_stack_operands_safe(op, &[dst_reg, src_reg])?;
        self.code.emit_u8(0x9C); // pushfq: old flags
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_vex_bls_rr(kind, dst_reg, src_reg, width);
        self.finish_bmi_flags(dst_reg, defined_rflags_mask);
        Ok(())
    }

    pub(crate) fn emit_x86_tbm_regs(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        width: OpWidth,
        kind: X86TbmKind,
        defined_rflags_mask: Option<i64>,
    ) {
        let decrement = matches!(
            kind,
            X86TbmKind::Blsfill | X86TbmKind::Blsic | X86TbmKind::Tzmsk
        );
        let invert_source = matches!(
            kind,
            X86TbmKind::Blcic | X86TbmKind::Blsic | X86TbmKind::T1mskc | X86TbmKind::Tzmsk
        );
        let logical_opcode = match kind {
            X86TbmKind::Blcfill | X86TbmKind::Blcic | X86TbmKind::Tzmsk => 0x20,
            X86TbmKind::Blcmsk => 0x30,
            X86TbmKind::Blci
            | X86TbmKind::Blcs
            | X86TbmKind::Blsfill
            | X86TbmKind::Blsic
            | X86TbmKind::T1mskc => 0x08,
        };

        self.code.emit_u8(0x9C); // old RFLAGS
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_push(src); // original source
        emitter.emit_mov_rr(dst, src, width);
        emitter.emit_alu_ri(if decrement { 5 } else { 0 }, dst, 1, width);
        emitter.code.emit_u8(0x9C); // pseudo ADD/SUB RFLAGS

        if kind == X86TbmKind::Blci {
            emitter.emit_not(dst, width);
        } else if invert_source {
            emitter.emit_group3_m_disp(2, PhysReg::Rsp, 8, DispSize::Auto, width);
        }
        emitter.emit_alu_mem_disp(
            logical_opcode,
            dst,
            PhysReg::Rsp,
            8,
            DispSize::Auto,
            width,
            X86AluEncoding::RegRm,
        );

        // Save the final logical flags, then splice in only pseudo ADD/SUB.CF.
        // BT leaves several status flags undefined, so it is used only while
        // editing the saved image; POPFQ restores the exact OF/SF/ZF image
        // before the ordinary deterministic undefined-flag merge.
        emitter.code.emit_u8(0x9C); // final logical RFLAGS
        emitter.emit_alu_mi_disp(4, PhysReg::Rsp, 0, DispSize::Auto, !1, OpWidth::W64);
        emitter.emit_bit_test_mi_disp(BitTestRegOp::Test, PhysReg::Rsp, 8, 0, OpWidth::W64);
        emitter.emit_alu_mi_disp(2, PhysReg::Rsp, 0, DispSize::Auto, 0, OpWidth::W64);
        emitter.code.emit_u8(0x9D); // merged logical flags
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        self.finish_bmi_flags(dst, defined_rflags_mask);
    }

    pub(crate) fn emit_x86_bextr_imm_regs(
        &mut self,
        dst: PhysReg,
        src: PhysReg,
        control: i64,
        width: OpWidth,
        defined_rflags_mask: Option<i64>,
    ) -> Result<(), LowerError> {
        // Host BMI1 BEXTR requires a register control operand. Reuse the dead
        // incoming destination when it does not alias the source. For an alias,
        // save one explicit guest-mapped scratch so materializing the immediate
        // cannot corrupt any architectural GPR.
        let saved_scratch = if dst == src {
            Some(if dst == PhysReg::Rax {
                PhysReg::Rcx
            } else {
                PhysReg::Rax
            })
        } else {
            None
        };
        let control_reg = saved_scratch.unwrap_or(dst);
        Self::ensure_flag_stack_operands_safe("Bextr", &[dst, src, control_reg])?;
        if let Some(scratch) = saved_scratch {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(scratch);
        }
        self.code.emit_u8(0x9C); // pushfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_ri(control_reg, control, OpWidth::W64);
            emitter.emit_vex_bmi_rr(0xF7, dst, src, control_reg, width);
        }
        self.finish_bmi_flags(dst, defined_rflags_mask);
        if let Some(scratch) = saved_scratch {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_pop(scratch);
        }
        Ok(())
    }

    pub(crate) fn lower_x86_tbm(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86TbmKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("X86Tbm::{kind:?}"),
                operand: format!("unsupported width {width:?}"),
            });
        }
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        let defined_rflags_mask = match flags {
            FlagUpdate::None => None,
            FlagUpdate::Specific(set) if set == defined => {
                Some(Self::x86_status_rflags_mask(defined))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("X86Tbm::{kind:?}"),
                    operand: format!("unsupported flag update {flags:?}"),
                });
            }
        };

        let dst_reg = self.get_dst_reg(dst)?;
        let src_reg = self.get_reg(src)?;
        Self::ensure_flag_stack_operands_safe("X86Tbm", &[dst_reg, src_reg])?;
        self.emit_x86_tbm_regs(dst_reg, src_reg, width, kind, defined_rflags_mask);
        Ok(())
    }

    pub(crate) fn lower_x86_adx(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        kind: X86AdxKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let (op, output) = match kind {
            X86AdxKind::Adcx => ("Adcx", FlagSet::CF),
            X86AdxKind::Adox => ("Adox", FlagSet::OF),
        };
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        if flags != FlagUpdate::None && flags != FlagUpdate::Specific(output) {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported flag update {flags:?}"),
            });
        }

        let dst_reg = self.get_dst_reg(dst)?;
        let src1_reg = self.get_reg(src1)?;
        let src2_reg = self.get_reg(src2)?;
        Self::ensure_flag_stack_operands_safe(op, &[dst_reg, src1_reg, src2_reg])?;

        let mut emitter = X86Emitter::new(&mut self.code);
        if dst_reg == src2_reg && dst_reg != src1_reg {
            emitter.emit_push(src2_reg);
            emitter.emit_mov_rr(dst_reg, src1_reg, width);
            emitter.emit_adx_rsp_mem(kind, dst_reg, width);
            // LEA discards the saved operand without modifying the ADX output
            // flag or any other guest status bit.
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
        } else {
            if dst_reg != src1_reg {
                emitter.emit_mov_rr(dst_reg, src1_reg, width);
            }
            emitter.emit_adx_rr(kind, dst_reg, src2_reg, width);
        }
        Ok(())
    }

    pub(crate) fn lower_x86_count(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86CountKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let op = match kind {
            X86CountKind::Popcnt => "X86Count::Popcnt",
            X86CountKind::Tzcnt => "X86Count::Tzcnt",
            X86CountKind::Lzcnt => "X86Count::Lzcnt",
        };
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }

        let requested = flags.as_set();
        let defined = match kind {
            X86CountKind::Popcnt => FlagSet::ALL_X86,
            X86CountKind::Tzcnt | X86CountKind::Lzcnt => FlagSet::CF.union(FlagSet::ZF),
        };
        if !requested.difference(defined).is_empty() {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported flag update {flags:?}"),
            });
        }

        let dst_reg = self.get_dst_reg(dst)?;
        let src_reg = self.get_reg(src)?;
        Self::ensure_count_native_stack_safe(op, dst_reg, src_reg)?;
        let emit_count = |emitter: &mut X86Emitter<'_>| match kind {
            X86CountKind::Popcnt => emitter.emit_popcnt(dst_reg, src_reg, width),
            X86CountKind::Tzcnt => emitter.emit_tzcnt(dst_reg, src_reg, width),
            X86CountKind::Lzcnt => emitter.emit_lzcnt(dst_reg, src_reg, width),
        };

        if requested.is_empty() {
            self.code.emit_u8(0x9C); // pushfq: APX NF/preserved status
            let mut emitter = X86Emitter::new(&mut self.code);
            emit_count(&mut emitter);
            self.code.emit_u8(0x9D); // popfq
            return Ok(());
        }

        if kind == X86CountKind::Popcnt && requested == FlagSet::ALL_X86 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emit_count(&mut emitter);
            return Ok(());
        }

        // Execute the host instruction, then merge only its requested,
        // architecturally defined status bits into the old RFLAGS. The saved
        // result occupies [rsp], old flags [rsp+8], while the new flags are
        // transiently at the top of the stack.
        let rflags_mask = Self::x86_status_rflags_mask(requested);
        self.code.emit_u8(0x9C); // pushfq (old)
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emit_count(&mut emitter);
            emitter.emit_push(dst_reg);
        }
        self.code.emit_u8(0x9C); // pushfq (new)
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                0,
                DispSize::Auto,
                rflags_mask,
                OpWidth::W64,
            );
            emitter.emit_pop(dst_reg); // requested new status bits
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                !rflags_mask,
                OpWidth::W64,
            );
            emitter.emit_alu_mem_disp(
                0x08,
                dst_reg,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
            emitter.emit_pop(dst_reg); // restore count result
        }
        self.code.emit_u8(0x9D); // popfq (merged)
        Ok(())
    }

    pub(crate) fn lower_bit_scan(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        flags: FlagUpdate,
        reverse: bool,
    ) -> Result<(), LowerError> {
        let op = if reverse { "Bsr" } else { "Bsf" };
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        let dst_reg = self.get_dst_reg(dst)?;
        let src_reg = self.get_reg(src)?;
        Self::ensure_flag_stack_operands_safe(op, &[dst_reg, src_reg])?;

        let emit_scan = |emitter: &mut X86Emitter<'_>| {
            if reverse {
                emitter.emit_bsr(dst_reg, src_reg, width);
            } else {
                emitter.emit_bsf(dst_reg, src_reg, width);
            }
        };

        match flags {
            FlagUpdate::All => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emit_scan(&mut emitter);
            }
            FlagUpdate::None => {
                self.code.emit_u8(0x9C); // pushfq: preserve every guest flag
                let mut emitter = X86Emitter::new(&mut self.code);
                emit_scan(&mut emitter);
                self.code.emit_u8(0x9D); // popfq
            }
            FlagUpdate::Specific(set) if set == FlagSet::ZF => {
                // BSF/BSR define only ZF. Execute the host instruction, then
                // merge its ZF into the pre-instruction RFLAGS while keeping
                // the result register and every undefined status flag intact:
                //   [rsp+8] = old flags, [rsp] = saved result.
                self.code.emit_u8(0x9C); // pushfq (old)
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emit_scan(&mut emitter);
                    emitter.emit_push(dst_reg);
                }
                self.code.emit_u8(0x9C); // pushfq (new)
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_alu_mi_disp(
                        4,
                        PhysReg::Rsp,
                        0,
                        DispSize::Auto,
                        1 << 6,
                        OpWidth::W64,
                    );
                    emitter.emit_pop(dst_reg); // masked new ZF
                    emitter.emit_alu_mi_disp(
                        4,
                        PhysReg::Rsp,
                        8,
                        DispSize::Auto,
                        !(1i64 << 6),
                        OpWidth::W64,
                    );
                    emitter.emit_alu_mem_disp(
                        0x08,
                        dst_reg,
                        PhysReg::Rsp,
                        8,
                        DispSize::Auto,
                        OpWidth::W64,
                        X86AluEncoding::RmReg,
                    );
                    emitter.emit_pop(dst_reg); // restore scan result
                }
                self.code.emit_u8(0x9D); // popfq (merged)
            }
            FlagUpdate::Specific(set) => {
                return Err(LowerError::InvalidOperand {
                    op: op.to_string(),
                    operand: format!("unsupported flag update {set:?}"),
                });
            }
        }
        Ok(())
    }

    /// Lower register-only BT/BTS/BTR/BTC while retaining the emulator's
    /// deterministic policy for architecturally undefined status flags. Native
    /// x86 supplies CF directly; [`Self::finish_bmi_flags`] merges only that bit
    /// into the saved incoming RFLAGS image.
    pub(crate) fn lower_bit_test(
        &mut self,
        kind: BitTestRegOp,
        dst: Option<VReg>,
        src: VReg,
        index: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: kind.name().to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        if dst.is_some_and(|dst| dst != src) {
            return Err(LowerError::InvalidOperand {
                op: kind.name().to_string(),
                operand: "register update requires dst == src".to_string(),
            });
        }

        let operand = if let Some(dst) = dst {
            self.get_dst_reg(dst)?
        } else {
            self.get_reg(src)?
        };

        let index_reg = match index {
            SrcOperand::Reg(reg) => Some(self.get_reg(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: kind.name().to_string(),
                    operand: format!("unsupported bit index {index:?}"),
                });
            }
        };
        let mut operands = vec![operand];
        operands.extend(index_reg);
        Self::ensure_flag_stack_operands_safe(kind.name(), &operands)?;

        self.code.emit_u8(0x9C); // pushfq: preserve undefined status flags.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match (index, index_reg) {
                (SrcOperand::Reg(_), Some(index)) => {
                    emitter.emit_bit_test_rr(kind, operand, index, width)
                }
                (SrcOperand::Imm(index), None) => {
                    emitter.emit_bit_test_ri(kind, operand, *index as u8, width)
                }
                (SrcOperand::Imm64(index), None) => {
                    emitter.emit_bit_test_ri(kind, operand, *index as u8, width)
                }
                _ => unreachable!(),
            }
        }
        self.finish_bmi_flags(operand, Some(1 << 0));
        Ok(())
    }

    /// Lower the register-source SSE4.2 CRC32 family. The architectural
    /// instruction is destructive (`dst == crc`) and does not modify RFLAGS.
    /// W8/W16/W32 sources write a 32-bit destination; W64 selects the r64
    /// encoding, whose CRC result is still zero-extended from 32 bits.
    pub(crate) fn lower_crc32c(
        &mut self,
        dst: VReg,
        crc: VReg,
        data: VReg,
        data_width: OpWidth,
    ) -> Result<(), LowerError> {
        if dst != crc {
            return Err(LowerError::InvalidOperand {
                op: "Crc32C".to_string(),
                operand: "x86 CRC32 requires dst == crc".to_string(),
            });
        }
        if !matches!(
            data_width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::InvalidOperand {
                op: "Crc32C".to_string(),
                operand: format!("unsupported data width {data_width:?}"),
            });
        }
        if Self::x86_gpr_index(dst).is_none() || Self::x86_gpr_index(data).is_none() {
            return Err(LowerError::InvalidOperand {
                op: "Crc32C".to_string(),
                operand: "operands must be architectural x86 GPRs".to_string(),
            });
        }

        let dst = self.get_dst_reg(dst)?;
        let data = self.get_reg(data)?;
        Self::ensure_flag_stack_operands_safe("Crc32C", &[dst, data])?;
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_crc32_rr(dst, data, data_width);
        Ok(())
    }

    /// Complete a BMI operation after its pre-instruction PUSHFQ. `None`
    /// restores every incoming flag (APX NF). A mask merges the native defined
    /// flags into the saved value while retaining the interpreter's explicit
    /// preservation of architecturally undefined flags.
    pub(crate) fn finish_bmi_flags(&mut self, dst: PhysReg, defined_rflags_mask: Option<i64>) {
        let Some(mask) = defined_rflags_mask else {
            self.code.emit_u8(0x9D); // popfq
            return;
        };

        // Stack before this sequence: [old RFLAGS]. Save the result, capture
        // native BMI flags, use dst as a temporary merge register, restore the
        // result, then install the merged architectural flags.
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_push(dst);
        emitter.code.emit_u8(0x9C); // pushfq
        emitter.emit_pop(dst);
        emitter.emit_and_ri(dst, mask, OpWidth::W64);
        emitter.emit_alu_mi_disp(4, PhysReg::Rsp, 8, DispSize::Auto, !mask, OpWidth::W64);
        emitter.emit_alu_mem_disp(
            0x08,
            dst,
            PhysReg::Rsp,
            8,
            DispSize::Auto,
            OpWidth::W64,
            X86AluEncoding::RegRm,
        );
        emitter.emit_mov_mr(PhysReg::Rsp, 8, dst, OpWidth::W64);
        emitter.emit_pop(dst);
        emitter.code.emit_u8(0x9D); // popfq
    }

    pub(crate) fn emit_noncommutative_alu_alias(
        &mut self,
        op: &'static str,
        opcode: u8,
        dst_reg: PhysReg,
        src1_reg: PhysReg,
        src2_reg: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::ensure_flag_stack_operands_safe(op, &[dst_reg, src1_reg, src2_reg])?;
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_push(src2_reg);
        emitter.emit_mov_rr(dst_reg, src1_reg, width);
        emitter.emit_alu_mem_disp(
            opcode,
            dst_reg,
            PhysReg::Rsp,
            0,
            DispSize::Auto,
            width,
            X86AluEncoding::RegRm,
        );
        // LEA discards the saved source without modifying the ALU result flags.
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_x86_ndd_double_shift(
        &mut self,
        dst: VReg,
        base: VReg,
        fill: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        left: bool,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: "X86NddDoubleShift".to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }
        if !matches!(flags, FlagUpdate::None | FlagUpdate::All) {
            return Err(LowerError::InvalidOperand {
                op: "X86NddDoubleShift".to_string(),
                operand: format!("unsupported flag contract {flags:?}"),
            });
        }
        let dst_reg = self.get_dst_reg(dst)?;
        let base_reg = self.get_reg(base)?;
        let fill_reg = self.get_reg(fill)?;
        let (amount_reg, amount_imm) = match amount {
            SrcOperand::Imm(value) => (None, Some(*value as u8)),
            SrcOperand::Reg(reg) => {
                let reg = self.get_reg(*reg)?;
                if reg != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "X86NddDoubleShift".to_string(),
                        operand: "register count must be architectural CL".to_string(),
                    });
                }
                (Some(reg), None)
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: "X86NddDoubleShift with shifted count".to_string(),
                });
            }
        };
        let mut regs = vec![dst_reg, base_reg, fill_reg];
        if let Some(reg) = amount_reg {
            regs.push(reg);
        }
        Self::ensure_flag_stack_operands_safe("X86NddDoubleShift", &regs)?;

        let preserve_flags = !flags.updates_any();
        let needs_stack_destination = dst_reg != base_reg
            && (dst_reg == fill_reg || amount_reg.is_some_and(|reg| dst_reg == reg));
        if needs_stack_destination {
            if preserve_flags {
                self.code.emit_u8(0x9C); // pushfq
            }
            let mut emitter = X86Emitter::new(&mut self.code);
            // Seed a stack-resident destination from `base` while leaving an
            // aliased fill register or CL untouched until the shift consumes it.
            // Starting from the old destination preserves upper bits for W16.
            emitter.emit_push(dst_reg);
            emitter.emit_mov_mr(PhysReg::Rsp, 0, base_reg, width);
            if left {
                emitter.emit_shld_mr_disp(
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    fill_reg,
                    amount_imm,
                    width,
                );
            } else {
                emitter.emit_shrd_mr_disp(
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    fill_reg,
                    amount_imm,
                    width,
                );
            }
            emitter.emit_pop(dst_reg);
            if width == OpWidth::W32 {
                emitter.emit_mov_rr(dst_reg, dst_reg, OpWidth::W32);
            }
            if preserve_flags {
                self.code.emit_u8(0x9D); // popfq
            }
            return Ok(());
        }

        if dst_reg != base_reg {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(dst_reg, base_reg, width);
        }
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq
        }
        let mut emitter = X86Emitter::new(&mut self.code);
        match (left, amount_imm) {
            (true, Some(imm)) => emitter.emit_shld_rr_imm(dst_reg, fill_reg, imm, width),
            (true, None) => emitter.emit_shld_rr_cl(dst_reg, fill_reg, width),
            (false, Some(imm)) => emitter.emit_shrd_rr_imm(dst_reg, fill_reg, imm, width),
            (false, None) => emitter.emit_shrd_rr_cl(dst_reg, fill_reg, width),
        }
        if preserve_flags {
            self.code.emit_u8(0x9D); // popfq
        }
        Ok(())
    }

    /// `movabs <reg64 enc>, imm64`.
    pub(crate) fn emit_movabs(&mut self, reg_enc: u8, imm: u64) {
        let mut rex = 0x48u8;
        if reg_enc >= 8 {
            rex |= 0x01; // REX.B
        }
        self.code.emit_u8(rex);
        self.code.emit_u8(0xB8 + (reg_enc & 7));
        self.code.emit_u32(imm as u32);
        self.code.emit_u32((imm >> 32) as u32);
    }
}
