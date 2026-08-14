//! State-backed GPR lowering (register file materialized in guest memory)

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
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
    pub(crate) fn x86_status_rflags_mask(flags: FlagSet) -> i64 {
        let mut mask = 0i64;
        if flags.contains(FlagSet::CF) {
            mask |= 1 << 0;
        }
        if flags.contains(FlagSet::PF) {
            mask |= 1 << 2;
        }
        if flags.contains(FlagSet::AF) {
            mask |= 1 << 4;
        }
        if flags.contains(FlagSet::ZF) {
            mask |= 1 << 6;
        }
        if flags.contains(FlagSet::SF) {
            mask |= 1 << 7;
        }
        if flags.contains(FlagSet::OF) {
            mask |= 1 << 11;
        }
        mask
    }

    pub(crate) fn x86_state_backed_gpr(v: VReg) -> bool {
        Self::x86_gpr_index(v).is_some_and(|index| index >= 16 || matches!(index, 4 | 5))
    }

    pub(crate) fn mov_touches_state_backed_gpr(kind: &OpKind) -> bool {
        matches!(
            kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            } if Self::x86_gpr_index(*dst).is_some()
                && Self::x86_gpr_index(*src).is_some()
                && (Self::x86_state_backed_gpr(*dst) || Self::x86_state_backed_gpr(*src))
        ) || matches!(
            kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(_) | SrcOperand::Imm64(_),
                width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            } if Self::x86_state_backed_gpr(*dst)
        )
    }

    pub(crate) fn alu_touches_state_backed_stack_gpr(kind: &OpKind) -> bool {
        let valid = |dst: VReg, src1: VReg, src2: &SrcOperand, width: OpWidth| {
            matches!(
                width,
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
            ) && Self::x86_gpr_index(dst).is_some()
                && Self::x86_gpr_index(src1).is_some()
                && match src2 {
                    SrcOperand::Reg(src2) => Self::x86_gpr_index(*src2).is_some(),
                    SrcOperand::Imm(value) => {
                        width != OpWidth::W64 || i32::try_from(*value).is_ok()
                    }
                    _ => false,
                }
                && [dst, src1]
                    .into_iter()
                    .chain(match src2 {
                        SrcOperand::Reg(src2) => Some(*src2),
                        _ => None,
                    })
                    .any(|reg| Self::x86_gpr_index(reg).is_some_and(|index| matches!(index, 4 | 5)))
        };

        match kind {
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            }
            | OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                valid(*dst, *src1, src2, *width)
                    && matches!(flags, FlagUpdate::None | FlagUpdate::All)
            }
            _ => false,
        }
    }

    pub(crate) fn emit_load_state_ptr_rax(&mut self) {
        // mov rax, [rbp+state_ptr]
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x8B);
        self.code.emit_u8(0x45);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8);
    }

    pub(crate) fn emit_spill_legacy_gprs_to_state_from_rax(&mut self, saved_rax_stack_off: u8) {
        for enc in [1u8, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
            self.emit_struct_mov(PhysReg::Rax, enc, (enc as i32) * 8, true);
        }
        // mov rcx, [rsp+saved_rax_stack_off]
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x8B);
        self.code.emit_u8(0x4C);
        self.code.emit_u8(0x24);
        self.code.emit_u8(saved_rax_stack_off);
        self.emit_struct_mov(PhysReg::Rax, 1, 0, true);
    }

    pub(crate) fn emit_store_gpr_slot_from_reg(
        &mut self,
        idx: u8,
        src: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let off = (idx as i32) * 8;
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rax, off, src, width);
            }
            OpWidth::W32 | OpWidth::W64 => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rax, off, src, OpWidth::W64);
            }
            OpWidth::W128 => {
                return Err(LowerError::UnsupportedOp {
                    op: "EGPR MOV with 128-bit width".to_string(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_mov(
        &mut self,
        dst: VReg,
        src: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if width == OpWidth::W128 {
            return Err(LowerError::UnsupportedOp {
                op: "EGPR MOV with 128-bit width".to_string(),
            });
        }

        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::UnsupportedOp {
            op: "state-backed MOV destination is not an x86 GPR".to_string(),
        })?;

        let src_idx = match src {
            SrcOperand::Reg(r) => {
                Some(
                    Self::x86_gpr_index(*r).ok_or_else(|| LowerError::UnsupportedOp {
                        op: "state-backed MOV source is not an x86 GPR".to_string(),
                    })?,
                )
            }
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: "state-backed MOV with non-scalar source".to_string(),
                });
            }
        };

        self.code.emit_u8(0x50); // push rax: preserve guest RAX while it is spilled.
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match (src, src_idx) {
                (SrcOperand::Reg(_), Some(idx)) => {
                    let load_width = if width == OpWidth::W32 {
                        OpWidth::W32
                    } else {
                        width
                    };
                    emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, (idx as i32) * 8, load_width);
                }
                (SrcOperand::Imm(val), None) => {
                    emitter.emit_mov_ri(PhysReg::Rdx, *val, width);
                }
                (SrcOperand::Imm64(val), None) => {
                    if width == OpWidth::W64 {
                        emitter.emit_mov_ri_imm64(PhysReg::Rdx, *val);
                    } else {
                        emitter.emit_mov_ri(PhysReg::Rdx, *val as i64, width);
                    }
                }
                _ => unreachable!(),
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;

        // The native function prologue saved the trampoline's guest RBP at
        // [RBP]. Keep that saved copy coherent with the state-backed slot so
        // the epilogue POP returns the updated guest value to the trampoline,
        // which performs its ordinary architectural write-back. RSP remains
        // entirely state-backed and never aliases the live host stack pointer.
        if dst_idx == 5 {
            if matches!(width, OpWidth::W8 | OpWidth::W16) {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, width);
            } else {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_cmove(
        &mut self,
        dst: VReg,
        src: VReg,
        cond: Condition,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed CMOVcc".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed CMOVcc".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            let dst_seed_width = if width == OpWidth::W32 {
                OpWidth::W32
            } else {
                OpWidth::W64
            };
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                i32::from(dst_idx) * 8,
                dst_seed_width,
            );
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src_idx) * 8, width);
            emitter.emit_cmovcc(
                X86Cond::from_condition(cond),
                PhysReg::Rdx,
                PhysReg::Rdi,
                width,
            );
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_setcc(
        &mut self,
        dst: VReg,
        cond: Condition,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed SETcc".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_setcc(X86Cond::from_condition(cond), PhysReg::Rdx);
            if width == OpWidth::W64 {
                emitter.emit_movzx(PhysReg::Rdx, PhysReg::Rdx, OpWidth::W8, OpWidth::W64);
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_not(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Not".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Not".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
            emitter.emit_not(PhysReg::Rdx, width);
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_neg(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Neg".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Neg".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        if !flags.updates_any() {
            self.code.emit_u8(0x9C); // pushfq: APX NF preserves every status flag
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_neg(PhysReg::Rdx, width);
        }
        if !flags.updates_any() {
            self.code.emit_u8(0x9D); // popfq
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_inc_dec(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        flags: FlagUpdate,
        decrement: bool,
    ) -> Result<(), LowerError> {
        let op_name = if decrement { "Dec" } else { "Inc" };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {op_name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {op_name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        if !flags.updates_any() {
            self.code.emit_u8(0x9C); // pushfq: APX NF preserves every status flag
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if decrement {
                emitter.emit_dec(PhysReg::Rdx, width);
            } else {
                emitter.emit_inc(PhysReg::Rdx, width);
            }
        }
        if !flags.updates_any() {
            self.code.emit_u8(0x9D); // popfq
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    /// Merge a native ROL/ROR status image into the incoming image while
    /// restoring the staged result. The active stack layout is native RFLAGS,
    /// result, incoming RFLAGS at offsets 0, 8, and 16 respectively.
    pub(crate) fn emit_finish_state_backed_gpr_rotate_flags(&mut self, native_mask: i64) {
        if native_mask != 0 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 0, OpWidth::W64);
            emitter.emit_and_ri(PhysReg::Rdi, native_mask, OpWidth::W64);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                16,
                DispSize::Auto,
                !native_mask,
                OpWidth::W64,
            );
            emitter.emit_alu_mem_disp(
                0x08,
                PhysReg::Rdi,
                PhysReg::Rsp,
                16,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard native RFLAGS
            emitter.emit_pop(PhysReg::Rdx); // restore rotate result
        }
        self.code.emit_u8(0x9D); // popfq: incoming or merged status image
    }

    pub(crate) fn lower_state_backed_gpr_rotate(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        right: bool,
    ) -> Result<(), LowerError> {
        let name = if right { "Ror" } else { "Rol" };
        let kind = if right {
            ShiftRegOp::Ror
        } else {
            ShiftRegOp::Rol
        };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let (immediate, count_idx) = match amount {
            SrcOperand::Imm(value) => (Some(*value as u8), None),
            SrcOperand::Reg(reg) => {
                let index =
                    Self::x86_gpr_index(*reg).ok_or_else(|| LowerError::InvalidOperand {
                        op: format!("state-backed {name}"),
                        operand: "count is not an architectural x86 GPR".to_string(),
                    })?;
                (None, Some(index))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "count is neither an immediate nor an architectural x86 GPR"
                        .to_string(),
                });
            }
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(index) = count_idx {
                emitter.emit_mov_rm(
                    PhysReg::Rcx,
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    OpWidth::W64,
                );
            }
        }

        self.code.emit_u8(0x9C); // pushfq: complete incoming image
        if let Some(value) = immediate {
            self.emit_shift_reg_imm(kind, PhysReg::Rdx, value, width);
        } else {
            self.emit_shift_reg_cl(kind, PhysReg::Rdx, width);
        }

        if !flags.updates_any() {
            self.code.emit_u8(0x9D); // popfq: APX NF preserves every flag
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.code.emit_u8(0x9C); // pushfq: native rotate image

            const CF: i64 = 1;
            const CF_OF: i64 = 1 | (1 << 11);
            let count_mask = if width == OpWidth::W64 { 0x3f } else { 0x1f };
            if let Some(value) = immediate {
                let masked = value & count_mask;
                let native_mask = if masked == 0 {
                    0
                } else if masked == 1 {
                    CF_OF
                } else {
                    CF
                };
                self.emit_finish_state_backed_gpr_rotate_flags(native_mask);
            } else {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rcx, OpWidth::W64);
                    emitter.emit_and_ri(PhysReg::Rdi, i64::from(count_mask), OpWidth::W64);
                    emitter.emit_test_rr(PhysReg::Rdi, PhysReg::Rdi, OpWidth::W64);
                }
                let count_zero = self.emit_jcc_placeholder(X86Cond::E);
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_cmp_ri(PhysReg::Rdi, 1, OpWidth::W64);
                }
                let count_one = self.emit_jcc_placeholder(X86Cond::E);

                self.emit_finish_state_backed_gpr_rotate_flags(CF);
                self.code.emit_u8(0xE9);
                let multi_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_one)?;
                self.emit_finish_state_backed_gpr_rotate_flags(CF_OF);
                self.code.emit_u8(0xE9);
                let one_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_zero)?;
                self.emit_finish_state_backed_gpr_rotate_flags(0);
                self.patch_rel32_to_current(multi_done)?;
                self.patch_rel32_to_current(one_done)?;
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_carry_rotate(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        right: bool,
    ) -> Result<(), LowerError> {
        let name = if right { "Rcr" } else { "Rcl" };
        let kind = if right {
            ShiftRegOp::Rcr
        } else {
            ShiftRegOp::Rcl
        };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let (immediate, count_idx) = match amount {
            SrcOperand::Imm(value) => (Some(*value as u8), None),
            SrcOperand::Reg(reg) => {
                let index =
                    Self::x86_gpr_index(*reg).ok_or_else(|| LowerError::InvalidOperand {
                        op: format!("state-backed {name}"),
                        operand: "count is not an architectural x86 GPR".to_string(),
                    })?;
                (None, Some(index))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "count is neither an immediate nor an architectural x86 GPR"
                        .to_string(),
                });
            }
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(index) = count_idx {
                emitter.emit_mov_rm(
                    PhysReg::Rcx,
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    OpWidth::W64,
                );
            }
        }

        // Snapshot creation is flag-neutral, so the native operation consumes
        // the guest's incoming CF directly. The saved image is also the source
        // for NF restoration and selective CF/OF merging.
        self.code.emit_u8(0x9C); // pushfq: complete incoming image
        if let Some(value) = immediate {
            self.emit_shift_reg_imm(kind, PhysReg::Rdx, value, width);
        } else {
            self.emit_shift_reg_cl(kind, PhysReg::Rdx, width);
        }

        if !flags.updates_any() {
            self.code.emit_u8(0x9D); // popfq: suppressed outputs preserve every flag
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.code.emit_u8(0x9C); // pushfq: native carry-rotate image

            const CF: i64 = 1;
            const CF_OF: i64 = 1 | (1 << 11);
            let count_mask = if width == OpWidth::W64 { 0x3f } else { 0x1f };
            if let Some(value) = immediate {
                let masked = value & count_mask;
                let native_mask = if masked == 0 {
                    0
                } else if masked == 1 {
                    CF_OF
                } else {
                    CF
                };
                // A subword full-period count (9 for W8, 17 for W16) leaves
                // native CF unchanged, so merging that identical bit is exact.
                self.emit_finish_state_backed_gpr_rotate_flags(native_mask);
            } else {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rcx, OpWidth::W64);
                    emitter.emit_and_ri(PhysReg::Rdi, i64::from(count_mask), OpWidth::W64);
                    emitter.emit_test_rr(PhysReg::Rdi, PhysReg::Rdi, OpWidth::W64);
                }
                let count_zero = self.emit_jcc_placeholder(X86Cond::E);
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_cmp_ri(PhysReg::Rdi, 1, OpWidth::W64);
                }
                let count_one = self.emit_jcc_placeholder(X86Cond::E);

                self.emit_finish_state_backed_gpr_rotate_flags(CF);
                self.code.emit_u8(0xE9);
                let multi_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_one)?;
                self.emit_finish_state_backed_gpr_rotate_flags(CF_OF);
                self.code.emit_u8(0xE9);
                let one_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_zero)?;
                self.emit_finish_state_backed_gpr_rotate_flags(0);
                self.patch_rel32_to_current(multi_done)?;
                self.patch_rel32_to_current(one_done)?;
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn emit_state_backed_gpr_double_shift_flag_case(&mut self, count: u8) {
        const CF_RESULT: i64 = 1 | (1 << 2) | (1 << 6) | (1 << 7);
        const OF: i64 = 1 << 11;

        debug_assert!(count != 0);
        if count == 1 {
            self.emit_finish_state_backed_gpr_shift_flags(CF_RESULT | OF, 0, None);
        } else {
            // Rax's deterministic policy clears architecturally undefined OF
            // for multi-bit double shifts while retaining incoming AF.
            self.emit_finish_state_backed_gpr_shift_flags(CF_RESULT, OF, None);
        }
    }

    pub(crate) fn lower_state_backed_gpr_double_shift(
        &mut self,
        dst: VReg,
        base: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        left: bool,
    ) -> Result<(), LowerError> {
        let name = if left { "Shld" } else { "Shrd" };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let base_idx = Self::x86_gpr_index(base).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "base source is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "fill source is not an architectural x86 GPR".to_string(),
        })?;
        let (immediate, count_idx) = match amount {
            SrcOperand::Imm(value) => (Some(*value as u8), None),
            SrcOperand::Reg(reg) => {
                let index =
                    Self::x86_gpr_index(*reg).ok_or_else(|| LowerError::InvalidOperand {
                        op: format!("state-backed {name}"),
                        operand: "count is not an architectural x86 GPR".to_string(),
                    })?;
                (None, Some(index))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "count is neither an immediate nor an architectural x86 GPR"
                        .to_string(),
                });
            }
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(base_idx) * 8, width);
            emitter.emit_mov_rm(PhysReg::Rsi, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(index) = count_idx {
                emitter.emit_mov_rm(
                    PhysReg::Rcx,
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    OpWidth::W64,
                );
            }
        }

        let count_mask = if width == OpWidth::W64 { 0x3f } else { 0x1f };
        self.code.emit_u8(0x9C); // pushfq: complete incoming image

        let emit_native = |emitter: &mut X86Emitter<'_>, immediate: Option<u8>| {
            if left {
                if let Some(value) = immediate {
                    emitter.emit_shld_rr_imm(PhysReg::Rdx, PhysReg::Rsi, value, width);
                } else {
                    emitter.emit_shld_rr_cl(PhysReg::Rdx, PhysReg::Rsi, width);
                }
            } else if let Some(value) = immediate {
                emitter.emit_shrd_rr_imm(PhysReg::Rdx, PhysReg::Rsi, value, width);
            } else {
                emitter.emit_shrd_rr_cl(PhysReg::Rdx, PhysReg::Rsi, width);
            }
        };

        if let Some(value) = immediate {
            let masked = value & count_mask;
            let defined = masked != 0 && !(width == OpWidth::W16 && masked > 16);
            if !defined {
                self.code.emit_u8(0x9D); // popfq: zero/undefined W16 count is a no-op
            } else if !flags.updates_any() {
                let mut emitter = X86Emitter::new(&mut self.code);
                emit_native(&mut emitter, Some(value));
                self.code.emit_u8(0x9D); // popfq: APX NF preserves every flag
            } else {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(PhysReg::Rdx); // original destination for stack contract
                    emit_native(&mut emitter, Some(value));
                    emitter.emit_push(PhysReg::Rdx);
                }
                self.code.emit_u8(0x9C); // pushfq: native double-shift image
                self.emit_state_backed_gpr_double_shift_flag_case(masked);
            }
        } else {
            let mut no_op = Vec::with_capacity(2);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rcx, OpWidth::W64);
                emitter.emit_and_ri(PhysReg::Rdi, i64::from(count_mask), OpWidth::W64);
                emitter.emit_test_rr(PhysReg::Rdi, PhysReg::Rdi, OpWidth::W64);
            }
            no_op.push(self.emit_jcc_placeholder(X86Cond::E));
            if width == OpWidth::W16 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmp_ri(PhysReg::Rdi, 16, OpWidth::W64);
                no_op.push(self.emit_jcc_placeholder(X86Cond::A));
            }

            if !flags.updates_any() {
                let mut emitter = X86Emitter::new(&mut self.code);
                emit_native(&mut emitter, None);
                self.code.emit_u8(0x9D); // popfq: APX NF preserves every flag
            } else {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(PhysReg::Rdx); // original destination for stack contract
                    emit_native(&mut emitter, None);
                    emitter.emit_push(PhysReg::Rdx);
                }
                self.code.emit_u8(0x9C); // pushfq: native double-shift image
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_cmp_ri(PhysReg::Rdi, 1, OpWidth::W64);
                }
                let count_one = self.emit_jcc_placeholder(X86Cond::E);
                self.emit_state_backed_gpr_double_shift_flag_case(2);
                self.code.emit_u8(0xE9);
                let flags_done = self.code.position();
                self.code.emit_u32(0);
                self.patch_rel32_to_current(count_one)?;
                self.emit_state_backed_gpr_double_shift_flag_case(1);
                self.patch_rel32_to_current(flags_done)?;
            }

            self.code.emit_u8(0xE9);
            let operation_done = self.code.position();
            self.code.emit_u32(0);
            for jump in no_op {
                self.patch_rel32_to_current(jump)?;
            }
            self.code.emit_u8(0x9D); // popfq: zero/undefined W16 count is a no-op
            self.patch_rel32_to_current(operation_done)?;
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    /// Merge a native SHL/SHR/SAR status image into the incoming image while
    /// restoring the staged result. The active stack layout is native RFLAGS,
    /// result, original operand, incoming RFLAGS at offsets 0, 8, 16, and 24.
    /// `reconstructed_cf_bit` selects a bit from the original operand when the
    /// interpreter defines CF where the host architecture leaves it undefined.
    pub(crate) fn emit_finish_state_backed_gpr_shift_flags(
        &mut self,
        native_mask: i64,
        clear_mask: i64,
        reconstructed_cf_bit: Option<u8>,
    ) {
        if native_mask != 0 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 0, OpWidth::W64);
            emitter.emit_and_ri(PhysReg::Rdi, native_mask, OpWidth::W64);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                24,
                DispSize::Auto,
                !native_mask,
                OpWidth::W64,
            );
            emitter.emit_alu_mem_disp(
                0x08,
                PhysReg::Rdi,
                PhysReg::Rsp,
                24,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
        }
        if clear_mask != 0 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                24,
                DispSize::Auto,
                !clear_mask,
                OpWidth::W64,
            );
        }
        if let Some(bit) = reconstructed_cf_bit {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mi_disp(4, PhysReg::Rsp, 24, DispSize::Auto, !1, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 16, OpWidth::W64);
            if bit != 0 {
                emitter.emit_shr_ri(PhysReg::Rdi, bit, OpWidth::W64);
            }
            emitter.emit_and_ri(PhysReg::Rdi, 1, OpWidth::W64);
            emitter.emit_alu_mem_disp(
                0x08,
                PhysReg::Rdi,
                PhysReg::Rsp,
                24,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard native RFLAGS
            emitter.emit_pop(PhysReg::Rdx); // restore shift result
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard original operand
        }
        self.code.emit_u8(0x9D); // popfq: incoming or merged status image
    }

    pub(crate) fn emit_state_backed_gpr_shift_flag_case(
        &mut self,
        kind: ShiftRegOp,
        width: OpWidth,
        count: u8,
    ) {
        const CF: i64 = 1;
        const RESULT_FLAGS: i64 = (1 << 2) | (1 << 6) | (1 << 7);
        const OF: i64 = 1 << 11;

        if count == 0 {
            self.emit_finish_state_backed_gpr_shift_flags(0, 0, None);
        } else if count == 1 {
            self.emit_finish_state_backed_gpr_shift_flags(CF | RESULT_FLAGS | OF, 0, None);
        } else if u32::from(count) < width.bits() {
            self.emit_finish_state_backed_gpr_shift_flags(CF | RESULT_FLAGS, OF, None);
        } else {
            match kind {
                ShiftRegOp::Shl if u32::from(count) == width.bits() => {
                    self.emit_finish_state_backed_gpr_shift_flags(RESULT_FLAGS, OF, Some(0));
                }
                ShiftRegOp::Shr if u32::from(count) == width.bits() => {
                    self.emit_finish_state_backed_gpr_shift_flags(
                        RESULT_FLAGS,
                        OF,
                        Some((width.bits() - 1) as u8),
                    );
                }
                ShiftRegOp::Shl | ShiftRegOp::Shr => {
                    self.emit_finish_state_backed_gpr_shift_flags(RESULT_FLAGS, CF | OF, None);
                }
                ShiftRegOp::Sar => {
                    self.emit_finish_state_backed_gpr_shift_flags(
                        RESULT_FLAGS,
                        OF,
                        Some((width.bits() - 1) as u8),
                    );
                }
                _ => unreachable!("state-backed GPR shift kind was validated"),
            }
        }
    }

    pub(crate) fn lower_state_backed_gpr_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        kind: ShiftRegOp,
    ) -> Result<(), LowerError> {
        let name = match kind {
            ShiftRegOp::Shl => "Shl",
            ShiftRegOp::Shr => "Shr",
            ShiftRegOp::Sar => "Sar",
            _ => unreachable!("state-backed GPR shift kind was validated"),
        };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let (immediate, count_idx) = match amount {
            SrcOperand::Imm(value) => (Some(*value as u8), None),
            SrcOperand::Reg(reg) => {
                let index =
                    Self::x86_gpr_index(*reg).ok_or_else(|| LowerError::InvalidOperand {
                        op: format!("state-backed {name}"),
                        operand: "count is not an architectural x86 GPR".to_string(),
                    })?;
                (None, Some(index))
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "count is neither an immediate nor an architectural x86 GPR"
                        .to_string(),
                });
            }
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(index) = count_idx {
                emitter.emit_mov_rm(
                    PhysReg::Rcx,
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    OpWidth::W64,
                );
            }
        }

        self.code.emit_u8(0x9C); // pushfq: complete incoming image
        if flags.updates_any() {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rdx); // retain the original operand for CF reconstruction
        }
        if let Some(value) = immediate {
            self.emit_shift_reg_imm(kind, PhysReg::Rdx, value, width);
        } else {
            self.emit_shift_reg_cl(kind, PhysReg::Rdx, width);
        }

        if !flags.updates_any() {
            self.code.emit_u8(0x9D); // popfq: APX NF preserves every flag
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.code.emit_u8(0x9C); // pushfq: native shift image

            let count_mask = if width == OpWidth::W64 { 0x3f } else { 0x1f };
            if let Some(value) = immediate {
                self.emit_state_backed_gpr_shift_flag_case(kind, width, value & count_mask);
            } else {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rcx, OpWidth::W64);
                    emitter.emit_and_ri(PhysReg::Rdi, i64::from(count_mask), OpWidth::W64);
                    emitter.emit_test_rr(PhysReg::Rdi, PhysReg::Rdi, OpWidth::W64);
                }
                let count_zero = self.emit_jcc_placeholder(X86Cond::E);
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_cmp_ri(PhysReg::Rdi, 1, OpWidth::W64);
                }
                let count_one = self.emit_jcc_placeholder(X86Cond::E);
                let subword = matches!(width, OpWidth::W8 | OpWidth::W16);
                let (count_boundary, count_oversized) = if subword {
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_cmp_ri(PhysReg::Rdi, i64::from(width.bits()), OpWidth::W64);
                    }
                    (
                        Some(self.emit_jcc_placeholder(X86Cond::E)),
                        Some(self.emit_jcc_placeholder(X86Cond::A)),
                    )
                } else {
                    (None, None)
                };

                self.emit_state_backed_gpr_shift_flag_case(kind, width, 2);
                self.code.emit_u8(0xE9);
                let multi_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_one)?;
                self.emit_state_backed_gpr_shift_flag_case(kind, width, 1);
                self.code.emit_u8(0xE9);
                let one_done = self.code.position();
                self.code.emit_u32(0);

                self.patch_rel32_to_current(count_zero)?;
                self.emit_state_backed_gpr_shift_flag_case(kind, width, 0);
                if let (Some(count_boundary), Some(count_oversized)) =
                    (count_boundary, count_oversized)
                {
                    self.code.emit_u8(0xE9);
                    let zero_done = self.code.position();
                    self.code.emit_u32(0);

                    self.patch_rel32_to_current(count_boundary)?;
                    self.emit_state_backed_gpr_shift_flag_case(kind, width, width.bits() as u8);
                    self.code.emit_u8(0xE9);
                    let boundary_done = self.code.position();
                    self.code.emit_u32(0);

                    self.patch_rel32_to_current(count_oversized)?;
                    self.emit_state_backed_gpr_shift_flag_case(kind, width, width.bits() as u8 + 1);
                    self.patch_rel32_to_current(zero_done)?;
                    self.patch_rel32_to_current(boundary_done)?;
                }
                self.patch_rel32_to_current(multi_done)?;
                self.patch_rel32_to_current(one_done)?;
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_count(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86CountKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed X86Count".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed X86Count".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let requested = flags.as_set();

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        let emit_count = |emitter: &mut X86Emitter<'_>| match kind {
            X86CountKind::Popcnt => emitter.emit_popcnt(PhysReg::Rdx, PhysReg::Rdx, width),
            X86CountKind::Tzcnt => emitter.emit_tzcnt(PhysReg::Rdx, PhysReg::Rdx, width),
            X86CountKind::Lzcnt => emitter.emit_lzcnt(PhysReg::Rdx, PhysReg::Rdx, width),
        };

        if requested.is_empty() {
            self.code.emit_u8(0x9C); // pushfq: APX NF preserves every status flag
            let mut emitter = X86Emitter::new(&mut self.code);
            emit_count(&mut emitter);
            self.code.emit_u8(0x9D); // popfq
        } else if kind == X86CountKind::Popcnt && requested == FlagSet::ALL_X86 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emit_count(&mut emitter);
        } else {
            let rflags_mask = Self::x86_status_rflags_mask(requested);
            self.code.emit_u8(0x9C); // pushfq (old)
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emit_count(&mut emitter);
                emitter.emit_push(PhysReg::Rdx);
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
                emitter.emit_pop(PhysReg::Rdx); // requested new status bits
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
                    PhysReg::Rdx,
                    PhysReg::Rsp,
                    8,
                    DispSize::Auto,
                    OpWidth::W64,
                    X86AluEncoding::RmReg,
                );
                emitter.emit_pop(PhysReg::Rdx); // restore count result
            }
            self.code.emit_u8(0x9D); // popfq (merged)
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_bit_scan(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        flags: FlagUpdate,
        reverse: bool,
    ) -> Result<(), LowerError> {
        let op_name = if reverse { "Bsr" } else { "Bsf" };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {op_name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {op_name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        self.code.emit_u8(0x9C); // pushfq: preserve old flags for None/ZF merge
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if reverse {
                emitter.emit_bsr(PhysReg::Rdx, PhysReg::Rdx, width);
            } else {
                emitter.emit_bsf(PhysReg::Rdx, PhysReg::Rdx, width);
            }
        }

        // x86 leaves the destination undefined for a zero source. Match the
        // VCPU interpreter's retained-destination policy explicitly without
        // changing the ZF produced by the native scan.
        let nonzero = self.emit_jcc_placeholder(X86Cond::Ne);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                i32::from(dst_idx) * 8,
                OpWidth::W64,
            );
        }
        self.patch_rel32_to_current(nonzero)?;

        if flags.updates_any() {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.code.emit_u8(0x9C); // pushfq (new)
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mi_disp(4, PhysReg::Rsp, 0, DispSize::Auto, 1 << 6, OpWidth::W64);
                emitter.emit_pop(PhysReg::Rdx); // masked new ZF
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
                    PhysReg::Rdx,
                    PhysReg::Rsp,
                    8,
                    DispSize::Auto,
                    OpWidth::W64,
                    X86AluEncoding::RmReg,
                );
                emitter.emit_pop(PhysReg::Rdx); // restore scan result
            }
        }
        self.code.emit_u8(0x9D); // popfq (old or ZF-merged)

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_bit_test(
        &mut self,
        kind: BitTestRegOp,
        dst: Option<VReg>,
        src: VReg,
        index: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {}", kind.name()),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let dst_idx = dst
            .map(|dst| {
                Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
                    op: format!("state-backed {}", kind.name()),
                    operand: "destination is not an architectural x86 GPR".to_string(),
                })
            })
            .transpose()?;
        let index_idx = match index {
            SrcOperand::Reg(reg) => {
                Some(
                    Self::x86_gpr_index(*reg).ok_or_else(|| LowerError::InvalidOperand {
                        op: format!("state-backed {}", kind.name()),
                        operand: "index is not an architectural x86 GPR".to_string(),
                    })?,
                )
            }
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {}", kind.name()),
                    operand: format!("unsupported bit index {index:?}"),
                });
            }
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(index_idx) = index_idx {
                emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(index_idx) * 8, width);
            }
        }

        self.code.emit_u8(0x9C); // pushfq: preserve every undefined status flag
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match (index, index_idx) {
                (SrcOperand::Reg(_), Some(_)) => {
                    emitter.emit_bit_test_rr(kind, PhysReg::Rdx, PhysReg::Rdi, width)
                }
                (SrcOperand::Imm(index), None) => {
                    emitter.emit_bit_test_ri(kind, PhysReg::Rdx, *index as u8, width)
                }
                (SrcOperand::Imm64(index), None) => {
                    emitter.emit_bit_test_ri(kind, PhysReg::Rdx, *index as u8, width)
                }
                _ => unreachable!(),
            }
        }
        self.finish_bmi_flags(PhysReg::Rdx, Some(1 << 0));

        if let Some(dst_idx) = dst_idx {
            self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
            if dst_idx == 5 {
                let commit_width = if width == OpWidth::W16 {
                    OpWidth::W16
                } else {
                    OpWidth::W64
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_crc32c(
        &mut self,
        dst: VReg,
        crc: VReg,
        data: VReg,
        data_width: OpWidth,
    ) -> Result<(), LowerError> {
        if dst != crc {
            return Err(LowerError::InvalidOperand {
                op: "state-backed Crc32C".to_string(),
                operand: "x86 CRC32 requires dst == crc".to_string(),
            });
        }
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Crc32C".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let data_idx = Self::x86_gpr_index(data).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Crc32C".to_string(),
            operand: "data source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(
            data_width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed Crc32C".to_string(),
                operand: format!("unsupported data width {data_width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                i32::from(dst_idx) * 8,
                OpWidth::W64,
            );
            emitter.emit_mov_rm(
                PhysReg::Rdi,
                PhysReg::Rax,
                i32::from(data_idx) * 8,
                data_width,
            );
            emitter.emit_crc32_rr(PhysReg::Rdx, PhysReg::Rdi, data_width);
        }

        // Every CRC32 encoding produces a 32-bit Castagnoli remainder and
        // zero-extends it to the full architectural destination.
        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, OpWidth::W32)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_and_not(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        defined_rflags_mask: Option<i64>,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed AndNot".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src1_idx = Self::x86_gpr_index(src1).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed AndNot".to_string(),
            operand: "first source is not an architectural x86 GPR".to_string(),
        })?;
        let src2_idx = Self::x86_gpr_index(src2).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed AndNot".to_string(),
            operand: "second source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed AndNot".to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src1_idx) * 8, width);
            emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(src2_idx) * 8, width);
        }
        self.code.emit_u8(0x9C); // pushfq: preserve undefined or all status flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::R8, width);
            emitter.emit_not(PhysReg::Rdx, width);
            emitter.emit_and_rr(PhysReg::Rdx, PhysReg::Rdi, width);
        }
        self.finish_bmi_flags(PhysReg::Rdx, defined_rflags_mask);

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_bextr_bzhi(
        &mut self,
        dst: VReg,
        src: VReg,
        control: VReg,
        width: OpWidth,
        defined_rflags_mask: Option<i64>,
        bzhi: bool,
    ) -> Result<(), LowerError> {
        let name = if bzhi { "Bzhi" } else { "Bextr" };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let control_idx = Self::x86_gpr_index(control);
        let control_imm = match control {
            VReg::Imm(value) if !bzhi => Some(value),
            VReg::Imm(_) => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "BZHI index must be an architectural x86 GPR".to_string(),
                });
            }
            _ if control_idx.is_some() => None,
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: format!("state-backed {name}"),
                    operand: "control is neither an architectural x86 GPR nor an immediate"
                        .to_string(),
                });
            }
        };
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("state-backed {name}"),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src_idx) * 8, width);
            if let Some(control_idx) = control_idx {
                emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(control_idx) * 8, width);
            } else {
                emitter.emit_mov_ri(
                    PhysReg::R8,
                    control_imm.expect("validated immediate control"),
                    OpWidth::W64,
                );
            }
        }
        self.code.emit_u8(0x9C); // pushfq: preserve undefined or all status flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_vex_bmi_rr(
                if bzhi { 0xF5 } else { 0xF7 },
                PhysReg::Rdx,
                PhysReg::Rdi,
                PhysReg::R8,
                width,
            );
        }
        self.finish_bmi_flags(PhysReg::Rdx, defined_rflags_mask);

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_bls(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86BlsKind,
        defined_rflags_mask: Option<i64>,
    ) -> Result<(), LowerError> {
        let name = match kind {
            X86BlsKind::Blsr => "Blsr",
            X86BlsKind::Blsmsk => "Blsmsk",
            X86BlsKind::Blsi => "Blsi",
        };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("state-backed {name}"),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        self.code.emit_u8(0x9C); // pushfq: preserve undefined or all status flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_vex_bls_rr(kind, PhysReg::Rdx, PhysReg::Rdi, width);
        }
        self.finish_bmi_flags(PhysReg::Rdx, defined_rflags_mask);

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_adx(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        kind: X86AdxKind,
        output_rflags_mask: Option<i64>,
    ) -> Result<(), LowerError> {
        let name = match kind {
            X86AdxKind::Adcx => "Adcx",
            X86AdxKind::Adox => "Adox",
        };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src1_idx = Self::x86_gpr_index(src1).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "first source is not an architectural x86 GPR".to_string(),
        })?;
        let src2_idx = Self::x86_gpr_index(src2).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "second source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("state-backed {name}"),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        // All staging instructions preserve RFLAGS, so the scratch ADX consumes
        // the guest's incoming CF/OF after both sources have been snapshotted.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src1_idx) * 8, width);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src2_idx) * 8, width);
        }
        self.code.emit_u8(0x9C); // pushfq: preserve non-output or every flag when suppressed
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_adx_rr(kind, PhysReg::Rdx, PhysReg::Rdi, width);
        }
        self.finish_bmi_flags(PhysReg::Rdx, output_rflags_mask);

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_pdep_pext(
        &mut self,
        dst: VReg,
        src: VReg,
        mask: VReg,
        width: OpWidth,
        extract: bool,
    ) -> Result<(), LowerError> {
        let name = if extract { "Pext" } else { "Pdep" };
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        let mask_idx = Self::x86_gpr_index(mask).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed {name}"),
            operand: "mask is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("state-backed {name}"),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src_idx) * 8, width);
            emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(mask_idx) * 8, width);
            emitter.emit_vex_bmi_rr_pp(
                0xF5,
                if extract {
                    X86SsePrefix::Rep
                } else {
                    X86SsePrefix::Repne
                },
                PhysReg::Rdx,
                PhysReg::R8,
                PhysReg::Rdi,
                width,
            );
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_gpr_bswap(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Bswap".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Bswap".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        match width {
            OpWidth::W16 => {
                self.code.emit_u8(0x9C); // pushfq: ROL defines status flags
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_rol_ri(PhysReg::Rdx, 8, OpWidth::W16);
                self.code.emit_u8(0x9D); // popfq
            }
            OpWidth::W32 | OpWidth::W64 => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_bswap(PhysReg::Rdx, width);
            }
            _ => {
                return Err(LowerError::InvalidOperand {
                    op: "state-backed Bswap".to_string(),
                    operand: format!("unsupported width {width:?}"),
                });
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_state_backed_stack_gpr_alu(
        &mut self,
        subtract: bool,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::UnsupportedOp {
                op: "state-backed stack ADD/SUB with non-scalar width".to_string(),
            });
        }
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::UnsupportedOp {
            op: "state-backed stack ADD/SUB destination is not an x86 GPR".to_string(),
        })?;
        let src1_idx = Self::x86_gpr_index(src1).ok_or_else(|| LowerError::UnsupportedOp {
            op: "state-backed stack ADD/SUB source is not an x86 GPR".to_string(),
        })?;
        let src2_idx = match src2 {
            SrcOperand::Reg(src) => {
                Some(
                    Self::x86_gpr_index(*src).ok_or_else(|| LowerError::UnsupportedOp {
                        op: "state-backed stack ADD/SUB source is not an x86 GPR".to_string(),
                    })?,
                )
            }
            SrcOperand::Imm(_) => None,
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: "state-backed stack ADD/SUB with non-scalar source".to_string(),
                });
            }
        };
        if let SrcOperand::Imm(value) = src2 {
            if width == OpWidth::W64 && i32::try_from(*value).is_err() {
                return Err(LowerError::InvalidOperand {
                    op: "state-backed stack ADD/SUB".to_string(),
                    operand: format!("64-bit immediate {value} is not sign-extended imm32"),
                });
            }
        }

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        let preserve_flags = !flags.updates_any();
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq; guest RAX is now at [rsp+8]
        }
        self.emit_spill_legacy_gprs_to_state_from_rax(if preserve_flags { 8 } else { 0 });

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src1_idx) * 8, width);
            match (src2, src2_idx) {
                (SrcOperand::Reg(_), Some(index)) => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(index) * 8, width);
                    if subtract {
                        emitter.emit_sub_rr(PhysReg::Rdx, PhysReg::Rdi, width);
                    } else {
                        emitter.emit_add_rr(PhysReg::Rdx, PhysReg::Rdi, width);
                    }
                }
                (SrcOperand::Imm(value), None) => {
                    if subtract {
                        emitter.emit_sub_ri(PhysReg::Rdx, *value, width);
                    } else {
                        emitter.emit_add_ri(PhysReg::Rdx, *value, width);
                    }
                }
                _ => unreachable!(),
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                width
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        if preserve_flags {
            self.code.emit_u8(0x9D); // popfq
        }
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    /// Reload all 14 allocatable guest GPRs from the GuestRegs struct via `base`
    /// (RCX, the state pointer); RSP/RBP are not JIT-managed. RCX is reloaded
    /// LAST since it doubles as the base.
    pub(crate) fn emit_reload_all(&mut self, base: PhysReg) {
        for enc in [0u8, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
            self.emit_struct_mov(base, enc, (enc as i32) * 8, false);
        }
        self.emit_struct_mov(base, 1, 8, false); // RCX last
    }

    /// Synchronize the guest-RBP word saved by the native prologue from the
    /// state file. This is required after a semantic interpreter callout: the
    /// callee may modify RBP, while hardware RBP must remain the trusted native
    /// frame pointer until the epilogue POP.
    pub(crate) fn emit_sync_saved_rbp_from_state(&mut self, base: PhysReg) {
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mov_rm(PhysReg::Rax, base, 5 * 8, OpWidth::W64);
        emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rax, OpWidth::W64);
    }

    pub(crate) fn x86_vector_state_index(reg: VReg, width: VecWidth) -> Option<u8> {
        match (reg, width) {
            (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
            | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
            | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => {
                Some(index)
            }
            _ => None,
        }
    }
}
