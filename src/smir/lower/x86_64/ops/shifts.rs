//! Shift lowering

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
    pub(crate) fn lower_op_shifts(
        &mut self,
        op: &crate::smir::ir::ops::SmirOp,
    ) -> Result<(), LowerError> {
        let is_non_accumulating_madd = matches!(
            &op.kind,
            OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                src1_unsigned: true,
                saturate: true,
                zeroing: false,
                ..
            } | OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I16,
                acc_elem: VecElementType::I32,
                src1_unsigned: false,
                saturate: false,
                zeroing: false,
                ..
            }
        );
        let is_classic_mpsadbw = matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMpsadbw { .. },
                Some(X86OpHint::SseOp { .. } | X86OpHint::VexOp { .. })
            )
        );
        if !is_non_accumulating_madd && !is_classic_mpsadbw {
            if let Some(result) = avx10::Avx10Lowerer::new().try_lower(&op.kind, &mut self.code) {
                return result;
            }
        }

        let alu_hint = match op.x86_hint {
            Some(X86OpHint::AluEncoding(enc)) => Some(enc),
            _ => None,
        };

        match &op.kind {
            // ================================================================
            // Shifts
            // ================================================================
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if matches!(op.x86_hint, Some(X86OpHint::ShiftGroup6)) {
                    return self.lower_x86_shift_group6(op);
                }
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shl".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Shl,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Shl, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shr".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Shr,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Shr, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Sar".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Sar,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Sar, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_rotate_candidate(op) {
                    if !x86_state_backed_gpr_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rol".to_string(),
                            operand: format!(
                                "invalid x86 GPR rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self
                        .lower_state_backed_gpr_rotate(*dst, *src, amount, *width, *flags, false);
                }
                self.lower_shift_reg_op(ShiftRegOp::Rol, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_rotate_candidate(op) {
                    if !x86_state_backed_gpr_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Ror".to_string(),
                            operand: format!(
                                "invalid x86 GPR rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self
                        .lower_state_backed_gpr_rotate(*dst, *src, amount, *width, *flags, true);
                }
                self.lower_shift_reg_op(ShiftRegOp::Ror, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_carry_rotate_candidate(op) {
                    if !x86_state_backed_gpr_carry_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rcl".to_string(),
                            operand: format!(
                                "invalid x86 GPR carry rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_carry_rotate(
                        *dst, *src, amount, *width, *flags, false,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Rcl, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_carry_rotate_candidate(op) {
                    if !x86_state_backed_gpr_carry_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rcr".to_string(),
                            operand: format!(
                                "invalid x86 GPR carry rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_carry_rotate(
                        *dst, *src, amount, *width, *flags, true,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Rcr, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Shld {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shld".to_string(),
                            operand: format!(
                                "invalid x86 GPR double shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *dst, *src, amount, *width, *flags, true,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shld_rr_imm(dst_reg, src_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            return Err(LowerError::InvalidOperand {
                                op: "Shld".to_string(),
                                operand: "requires CL".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shld_rr_cl(dst_reg, src_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shld with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Shrd {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shrd".to_string(),
                            operand: format!(
                                "invalid x86 GPR double shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *dst, *src, amount, *width, *flags, false,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shrd_rr_imm(dst_reg, src_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            return Err(LowerError::InvalidOperand {
                                op: "Shrd".to_string(),
                                operand: "requires CL".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shrd_rr_cl(dst_reg, src_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shrd with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::X86NddDoubleShift {
                dst,
                base,
                fill,
                amount,
                width,
                left,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86NddDoubleShift".to_string(),
                            operand: format!(
                                "invalid x86 GPR NDD double shift {dst:?} {base:?} {fill:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *base, *fill, amount, *width, *flags, *left,
                    );
                }
                self.lower_x86_ndd_double_shift(*dst, *base, *fill, amount, *width, *left, *flags)?
            }

            _ => return self.lower_op_comparisons(op),
        }

        Ok(())
    }
}
