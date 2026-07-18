//! Comparison lowering

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
    pub(crate) fn lower_op_comparisons(&mut self, op: &crate::smir::ir::ops::SmirOp) -> Result<(), LowerError> {
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
            // Comparisons
            // ================================================================
            OpKind::Cmp { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        emitter.emit_alu_rr_dir(0x38, src1_reg, src2_reg, *width, encoding);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && src1_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x3C, *val, *width);
                        } else {
                            emitter.emit_cmp_ri(src1_reg, *val, *width);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Cmp with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Test { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_rr(src1_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_ri(src1_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Test with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::SetCC { dst, cond, width } => {
                if x86_state_backed_gpr_setcc_candidate(op) {
                    if !x86_state_backed_gpr_setcc_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed SETcc".to_string(),
                            operand: format!("invalid x86 GPR conditional set {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_setcc(*dst, *cond, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let x86_cond = X86Cond::from_condition(*cond);

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_setcc(x86_cond, dst_reg);

                // Zero-extend to full width if needed
                if *width != OpWidth::W8 {
                    emitter.emit_movzx(dst_reg, dst_reg, OpWidth::W8, *width);
                }
            }

            OpKind::ReadFlags { dst } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                Self::ensure_flag_stack_operands_safe("ReadFlags", &[dst_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(dst_reg);
            }

            OpKind::WriteFlags { src } => {
                let src_reg = self.get_reg(*src)?;
                Self::ensure_flag_stack_operands_safe("WriteFlags", &[src_reg])?;

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(src_reg);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
            } => {
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpCompare".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let pp = match elem {
                    VecElementType::F32 => X86SsePrefix::None,
                    VecElementType::F64 => X86SsePrefix::OpSize,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpCompare {elem:?}"),
                        });
                    }
                };
                let opcode = if *signaling { 0x2F } else { 0x2E };
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    self.emit_vec_rr(
                        VecEncoding {
                            width: VecWidth::V128,
                            opcode,
                            ..enc_hint
                        },
                        src1_reg,
                        src2_reg,
                        0,
                    );
                } else if src1_reg.vec_ext2() != 0 || src2_reg.vec_ext2() != 0 {
                    self.emit_vec_rr(
                        VecEncoding {
                            kind: VecEncodingKind::Evex,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode,
                            width: VecWidth::V128,
                            w: *elem == VecElementType::F64,
                        },
                        src1_reg,
                        src2_reg,
                        0,
                    );
                } else {
                    let prefix = if pp == X86SsePrefix::OpSize {
                        Some(0x66)
                    } else {
                        None
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, src1_reg, src2_reg);
                }
            }

            OpKind::X86GetExponent {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetExponent".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (map, w) = match elem {
                    VecElementType::F16 => (X86VecMap::Map6, false),
                    VecElementType::F32 => (X86VecMap::Map0F38, false),
                    VecElementType::F64 => (X86VecMap::Map0F38, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetExponent".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = if *scalar { 0x43 } else { 0x42 };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: hint_map,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_map == map
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86GetExponent".to_string(),
                        operand: "non-canonical VGETEXP shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    map,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86GetMantissa {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetMantissa".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w) = match elem {
                    VecElementType::F16 => (X86SsePrefix::None, false),
                    VecElementType::F32 => (X86SsePrefix::OpSize, false),
                    VecElementType::F64 => (X86SsePrefix::OpSize, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetMantissa".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = if *scalar { 0x27 } else { 0x26 };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86GetMantissa".to_string(),
                        operand: "non-canonical VGETMANT shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86RoundScale {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86RoundScale".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w, opcode) = match (elem, scalar) {
                    (VecElementType::F16, false) => (X86SsePrefix::None, false, 0x08),
                    (VecElementType::F16, true) => (X86SsePrefix::None, false, 0x0A),
                    (VecElementType::F32, false) => (X86SsePrefix::OpSize, false, 0x08),
                    (VecElementType::F32, true) => (X86SsePrefix::OpSize, false, 0x0A),
                    (VecElementType::F64, false) => (X86SsePrefix::OpSize, true, 0x09),
                    (VecElementType::F64, true) => (X86SsePrefix::OpSize, true, 0x0B),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86RoundScale".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86RoundScale".to_string(),
                        operand: "non-canonical VRNDSCALE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86Reduce {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Reduce".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w, opcode) = match (elem, scalar) {
                    (VecElementType::F16, false) => (X86SsePrefix::None, false, 0x56),
                    (VecElementType::F16, true) => (X86SsePrefix::None, false, 0x57),
                    (VecElementType::F32, false) => (X86SsePrefix::OpSize, false, 0x56),
                    (VecElementType::F32, true) => (X86SsePrefix::OpSize, false, 0x57),
                    (VecElementType::F64, false) => (X86SsePrefix::OpSize, true, 0x56),
                    (VecElementType::F64, true) => (X86SsePrefix::OpSize, true, 0x57),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Reduce".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Reduce".to_string(),
                        operand: "non-canonical VREDUCE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86Range {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Range".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Range".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && *imm <= 0x0F
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x51 } else { 0x50 };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Range".to_string(),
                        operand: "non-canonical VRANGE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr_imm_sae(
                    X86VecMap::Map0F3A,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *imm,
                );
            }

            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FixupImm".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FixupImm".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x55 } else { 0x54 };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FixupImm".to_string(),
                        operand: "non-canonical VFIXUPIMM shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr_imm_sae(
                    X86VecMap::Map0F3A,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *imm,
                );
            }

            OpKind::X86Exp2 {
                dst,
                src,
                mask,
                elem,
                width,
                lanes,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Exp2".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Exp2".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let valid_shape = dst_reg.is_zmm()
                    && src_reg.is_zmm()
                    && *width == VecWidth::V512
                    && *lanes == width.lanes(*elem) as u8
                    && (!*mask_zeroing || aaa != 0);
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0xC8,
                        width: VecWidth::V512,
                        w: hint_w,
                    }) if hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Exp2".to_string(),
                        operand: "non-canonical VEXP2 shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    0xC8,
                    dst_reg,
                    None,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt14 { .. });
                let op_name = if rsqrt { "X86Rsqrt14" } else { "X86Recip14" };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0x4C,
                    (false, true) => 0x4D,
                    (true, false) => 0x4E,
                    (true, true) => 0x4F,
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        register_matches_width(dst_reg, *width)
                            && register_matches_width(src_reg, *width)
                            && merge_reg.is_none()
                            && *lanes == width.lanes(*elem) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRT14" } else { "VRCP14" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    false,
                    None,
                );
            }

            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86RsqrtFp16 { .. });
                let op_name = if rsqrt {
                    "X86RsqrtFp16"
                } else {
                    "X86RecipFp16"
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0x4C,
                    (false, true) => 0x4D,
                    (true, false) => 0x4E,
                    (true, true) => 0x4F,
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        register_matches_width(dst_reg, *width)
                            && register_matches_width(src_reg, *width)
                            && merge_reg.is_none()
                            && *lanes == width.lanes(VecElementType::F16) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map6,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: false,
                    }) if hint_opcode == opcode && hint_width == *width
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRTFP16" } else { "VRCPFP16" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map6,
                    X86SsePrefix::OpSize,
                    *width,
                    false,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    false,
                    None,
                );
            }

            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            }
            | OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt28 { .. });
                let op_name = if rsqrt { "X86Rsqrt28" } else { "X86Recip28" };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0xCA,
                    (false, true) => 0xCB,
                    (true, false) => 0xCC,
                    (true, true) => 0xCD,
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        dst_reg.is_zmm()
                            && src_reg.is_zmm()
                            && merge_reg.is_none()
                            && *width == VecWidth::V512
                            && *lanes == width.lanes(*elem) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRT28" } else { "VRCP28" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86ScaleF {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86ScaleF".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (map, w) = match elem {
                    VecElementType::F16 => (X86VecMap::Map6, false),
                    VecElementType::F32 => (X86VecMap::Map0F38, false),
                    VecElementType::F64 => (X86VecMap::Map0F38, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86ScaleF".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_round = matches!(
                    round,
                    FpRoundMode::Dynamic
                        | FpRoundMode::RoundNearest
                        | FpRoundMode::RoundDown
                        | FpRoundMode::RoundUp
                        | FpRoundMode::RoundTowardZero
                ) && (*suppress_exceptions == (*round != FpRoundMode::Dynamic));
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && (!*mask_zeroing || aaa != 0)
                    && valid_round
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x2D } else { 0x2C };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: hint_map,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_map == map
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86ScaleF".to_string(),
                        operand: "non-canonical VSCALEF shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr(
                    map,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *round,
                    *suppress_exceptions,
                );
            }

            OpKind::X86FP16Complex {
                dst,
                src1,
                src2,
                mask,
                width,
                pairs,
                scalar,
                mask_zeroing,
                accumulate,
                conjugate,
                round,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FP16Complex".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let embedded_rounding = *round != FpRoundMode::Dynamic;
                let valid_round = matches!(
                    round,
                    FpRoundMode::Dynamic
                        | FpRoundMode::RoundNearest
                        | FpRoundMode::RoundDown
                        | FpRoundMode::RoundUp
                        | FpRoundMode::RoundTowardZero
                );
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && dst_reg != src1_reg
                    && dst_reg != src2_reg
                    && (!*mask_zeroing || aaa != 0)
                    && valid_round
                    && if *scalar {
                        *width == VecWidth::V128 && *pairs == 1
                    } else {
                        *pairs == (width.bytes() / 4) as u8
                            && (!embedded_rounding || *width == VecWidth::V512)
                    };
                let pp = if *conjugate {
                    X86SsePrefix::Repne
                } else {
                    X86SsePrefix::Rep
                };
                let opcode = match (*accumulate, *scalar) {
                    (true, false) => 0x56,
                    (true, true) => 0x57,
                    (false, false) => 0xD6,
                    (false, true) => 0xD7,
                };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map6,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: false,
                    }) if hint_pp == pp && hint_opcode == opcode && hint_width == *width
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FP16Complex".to_string(),
                        operand: "non-canonical AVX512-FP16 complex shape or encoding metadata"
                            .to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr(
                    X86VecMap::Map6,
                    pp,
                    *width,
                    false,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *round,
                    embedded_rounding,
                );
            }

            OpKind::X86FpToInt {
                dst,
                src,
                elem,
                int_width,
                signed,
                truncate,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if dst_reg.is_vec() || !src_reg.is_vec() || src_reg.vec_ext2() != 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpToInt".to_string(),
                        operand: "requires a GPR destination and XMM0-XMM15 source".to_string(),
                    });
                }
                if !matches!(int_width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86FpToInt width {int_width:?}"),
                    });
                }
                if !*signed
                    || *suppress_exceptions
                    || (*truncate && *round != FpRoundMode::RoundTowardZero)
                    || (!*truncate && *round != FpRoundMode::Dynamic)
                {
                    return Err(LowerError::UnsupportedOp {
                        op: format!(
                            "X86FpToInt signed={signed}, rounding {round:?}, truncate={truncate}, sae={suppress_exceptions}"
                        ),
                    });
                }
                let prefix = match elem {
                    VecElementType::F32 => 0xF3,
                    VecElementType::F64 => 0xF2,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpToInt element {elem:?}"),
                        });
                    }
                };
                let opcode = if *truncate { 0x2C } else { 0x2D };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_fp_to_int_rr(prefix, opcode, dst_reg, src_reg, *int_width);
            }

            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem,
                int_width,
                signed,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let merge_reg = self.get_reg(*merge)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec()
                    || merge_reg != dst_reg
                    || src_reg.is_vec()
                    || dst_reg.vec_ext2() != 0
                    || *zero_upper
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86IntToFp".to_string(),
                        operand: "native legacy lowering requires dst=merge XMM0-XMM15".to_string(),
                    });
                }
                if !matches!(int_width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86IntToFp width {int_width:?}"),
                    });
                }
                if !*signed || *round != FpRoundMode::Dynamic || *suppress_exceptions {
                    return Err(LowerError::UnsupportedOp {
                        op: format!(
                            "X86IntToFp signed={signed}, rounding {round:?}, sae={suppress_exceptions}"
                        ),
                    });
                }
                let prefix = match elem {
                    VecElementType::F32 => 0xF3,
                    VecElementType::F64 => 0xF2,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86IntToFp element {elem:?}"),
                        });
                    }
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_fp_to_int_rr(prefix, 0x2A, dst_reg, src_reg, *int_width);
            }

            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let merge_reg = self.get_reg(*merge)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec()
                    || !src_reg.is_vec()
                    || merge_reg != dst_reg
                    || dst_reg.vec_ext2() != 0
                    || src_reg.vec_ext2() != 0
                    || mask.is_some()
                    || *mask_zeroing
                    || *round != FpRoundMode::Dynamic
                    || *suppress_exceptions
                    || *zero_upper
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpConvert".to_string(),
                        operand: "native legacy lowering requires dst=merge XMM0-XMM15".to_string(),
                    });
                }
                let prefix = match (*from, *to) {
                    (VecElementType::F32, VecElementType::F64) => Some(0xF3),
                    (VecElementType::F64, VecElementType::F32) => Some(0xF2),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpConvert {from:?}->{to:?}"),
                        });
                    }
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_rr(prefix, 0x5A, dst_reg, src_reg);
            }

            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                report_fp16_denormal,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpConvert".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                if *report_fp16_denormal {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedFpConvert FP16 denormal reporting".to_string(),
                    });
                }
                let pp = match (*from, *to) {
                    (VecElementType::F32, VecElementType::F64) => X86SsePrefix::None,
                    (VecElementType::F64, VecElementType::F32) => X86SsePrefix::OpSize,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86PackedFpConvert {from:?}->{to:?}"),
                        });
                    }
                };
                let instruction_width = match (*from, *lanes) {
                    (VecElementType::F64, 2) => VecWidth::V128,
                    (VecElementType::F64, 4) => VecWidth::V256,
                    (VecElementType::F64, 8) => VecWidth::V512,
                    (VecElementType::F32, 2 | 4 | 8) => *dst_width,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                if *mask_zeroing && aaa == 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpConvert".to_string(),
                        operand: "zeroing requires a nonzero opmask".to_string(),
                    });
                }
                if let Some(X86OpHint::EvexOp { map, .. }) = op.x86_hint {
                    if !*zero_upper
                        || !matches!(*lanes, 2 | 4 | 8)
                        || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                        || (*round != FpRoundMode::Dynamic
                            && !(*from == VecElementType::F64
                                && *lanes == 8
                                && instruction_width == VecWidth::V512))
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid EVEX packed conversion shape".to_string(),
                        });
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_evex_masked_rr(
                        map,
                        pp,
                        instruction_width,
                        *from == VecElementType::F64,
                        0x5A,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        *round != FpRoundMode::Dynamic,
                        *round,
                    );
                } else if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    if !*zero_upper
                        || mask.is_some()
                        || *mask_zeroing
                        || *round != FpRoundMode::Dynamic
                        || *suppress_exceptions
                        || !matches!(*lanes, 2 | 4)
                        || !matches!(instruction_width, VecWidth::V128 | VecWidth::V256)
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid VEX packed conversion shape".to_string(),
                        });
                    }
                    self.emit_vec_rr(
                        VecEncoding {
                            pp,
                            opcode: 0x5A,
                            width: instruction_width,
                            ..enc_hint
                        },
                        dst_reg,
                        src_reg,
                        0,
                    );
                } else {
                    if *zero_upper
                        || mask.is_some()
                        || *mask_zeroing
                        || *round != FpRoundMode::Dynamic
                        || *suppress_exceptions
                        || *lanes != 2
                        || *dst_width != VecWidth::V128
                        || dst_reg.vec_ext2() != 0
                        || src_reg.vec_ext2() != 0
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid legacy packed conversion shape".to_string(),
                        });
                    }
                    let prefix = if pp == X86SsePrefix::OpSize {
                        Some(0x66)
                    } else {
                        None
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, 0x5A, dst_reg, src_reg);
                }
            }

            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*int_elem, *fp_elem, *signed) {
                    (VecElementType::I32, VecElementType::F32, true) => {
                        (X86SsePrefix::None, 0x5B, false)
                    }
                    (VecElementType::I64, VecElementType::F32, true) => {
                        (X86SsePrefix::None, 0x5B, true)
                    }
                    (VecElementType::I32, VecElementType::F64, true) => {
                        (X86SsePrefix::Rep, 0xE6, false)
                    }
                    (VecElementType::I64, VecElementType::F64, true) => {
                        (X86SsePrefix::Rep, 0xE6, true)
                    }
                    (VecElementType::I32, VecElementType::F32, false) => {
                        (X86SsePrefix::Repne, 0x7A, false)
                    }
                    (VecElementType::I64, VecElementType::F32, false) => {
                        (X86SsePrefix::Repne, 0x7A, true)
                    }
                    (VecElementType::I32, VecElementType::F64, false) => {
                        (X86SsePrefix::Rep, 0x7A, false)
                    }
                    (VecElementType::I64, VecElementType::F64, false) => {
                        (X86SsePrefix::Rep, 0x7A, true)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "elements must be I32/I64 to F32/F64".to_string(),
                        });
                    }
                };
                let operation_bytes = u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes());
                let operation_width = match operation_bytes {
                    16 => VecWidth::V128,
                    32 => VecWidth::V256,
                    64 => VecWidth::V512,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let exact_width = |bytes: u32| match bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let register_width = |bytes: u32| match bytes {
                    0..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let expected_src_width = exact_width(u32::from(*lanes) * int_elem.bytes());
                let expected_dst_width = register_width(u32::from(*lanes) * fp_elem.bytes());
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                if *src_width != expected_src_width
                    || *dst_width != expected_dst_width
                    || (*mask_zeroing && aaa == 0)
                    || *round == FpRoundMode::RoundNearestTiesAway
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp".to_string(),
                        operand: "invalid packed integer-to-FP shape".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::EvexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let exact_no_er =
                            *int_elem == VecElementType::I32 && *fp_elem == VecElementType::F64;
                        if map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || hinted_w != w
                            || !*zero_upper
                            || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                            || (*suppress_exceptions
                                && (operation_width != VecWidth::V512 || exact_no_er))
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid EVEX packed conversion metadata".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_evex_masked_rr(
                            map,
                            pp,
                            operation_width,
                            w,
                            opcode,
                            dst_reg,
                            src_reg,
                            aaa,
                            *mask_zeroing,
                            *suppress_exceptions,
                            *round,
                        );
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let vex_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        if !vex_family
                            || map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || !matches!(operation_width, VecWidth::V128 | VecWidth::V256)
                            || !*zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != FpRoundMode::Dynamic
                            || *suppress_exceptions
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid VEX packed conversion metadata".to_string(),
                            });
                        }
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: operation_width,
                                w: hinted_w,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    None => {
                        let legacy_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        if !legacy_family
                            || *zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != FpRoundMode::Dynamic
                            || *suppress_exceptions
                            || operation_width != VecWidth::V128
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid legacy packed conversion shape".to_string(),
                            });
                        }
                        let prefix = match pp {
                            X86SsePrefix::None => None,
                            X86SsePrefix::Rep => Some(0xF3),
                            _ => unreachable!(),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src_reg);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "X86PackedIntToFp without canonical encoding metadata".to_string(),
                        });
                    }
                }
            }

            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpToInt".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*fp_elem, *int_elem, *signed, *truncate) {
                    (VecElementType::F32, VecElementType::I32, true, false) => {
                        (X86SsePrefix::OpSize, 0x5B, false)
                    }
                    (VecElementType::F32, VecElementType::I32, true, true) => {
                        (X86SsePrefix::Rep, 0x5B, false)
                    }
                    (VecElementType::F64, VecElementType::I32, true, false) => {
                        (X86SsePrefix::Repne, 0xE6, true)
                    }
                    (VecElementType::F64, VecElementType::I32, true, true) => {
                        (X86SsePrefix::OpSize, 0xE6, true)
                    }
                    (VecElementType::F32, VecElementType::I64, true, false) => {
                        (X86SsePrefix::OpSize, 0x7B, false)
                    }
                    (VecElementType::F64, VecElementType::I64, true, false) => {
                        (X86SsePrefix::OpSize, 0x7B, true)
                    }
                    (VecElementType::F32, VecElementType::I64, true, true) => {
                        (X86SsePrefix::OpSize, 0x7A, false)
                    }
                    (VecElementType::F64, VecElementType::I64, true, true) => {
                        (X86SsePrefix::OpSize, 0x7A, true)
                    }
                    (VecElementType::F32, VecElementType::I32, false, false) => {
                        (X86SsePrefix::None, 0x79, false)
                    }
                    (VecElementType::F64, VecElementType::I32, false, false) => {
                        (X86SsePrefix::None, 0x79, true)
                    }
                    (VecElementType::F32, VecElementType::I32, false, true) => {
                        (X86SsePrefix::None, 0x78, false)
                    }
                    (VecElementType::F64, VecElementType::I32, false, true) => {
                        (X86SsePrefix::None, 0x78, true)
                    }
                    (VecElementType::F32, VecElementType::I64, false, false) => {
                        (X86SsePrefix::OpSize, 0x79, false)
                    }
                    (VecElementType::F64, VecElementType::I64, false, false) => {
                        (X86SsePrefix::OpSize, 0x79, true)
                    }
                    (VecElementType::F32, VecElementType::I64, false, true) => {
                        (X86SsePrefix::OpSize, 0x78, false)
                    }
                    (VecElementType::F64, VecElementType::I64, false, true) => {
                        (X86SsePrefix::OpSize, 0x78, true)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "elements must be F32/F64 to I32/I64".to_string(),
                        });
                    }
                };
                let operation_bytes = u32::from(*lanes) * fp_elem.bytes().max(int_elem.bytes());
                let operation_width = match operation_bytes {
                    16 => VecWidth::V128,
                    32 => VecWidth::V256,
                    64 => VecWidth::V512,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let exact_width = |bytes: u32| match bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let register_width = |bytes: u32| match bytes {
                    0..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let expected_src_width = exact_width(u32::from(*lanes) * fp_elem.bytes());
                let expected_dst_width = register_width(u32::from(*lanes) * int_elem.bytes());
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let rounding_valid = if *truncate {
                    *round == FpRoundMode::RoundTowardZero
                } else {
                    *round != FpRoundMode::RoundNearestTiesAway
                        && *suppress_exceptions == (*round != FpRoundMode::Dynamic)
                };
                if *src_width != expected_src_width
                    || *dst_width != expected_dst_width
                    || (*mask_zeroing && aaa == 0)
                    || !rounding_valid
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpToInt".to_string(),
                        operand: "invalid packed FP-to-integer shape".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::EvexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        if map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || hinted_w != w
                            || !*zero_upper
                            || (*suppress_exceptions && operation_width != VecWidth::V512)
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid EVEX packed conversion metadata".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if *truncate && *suppress_exceptions {
                            emitter.emit_evex_masked_rr(
                                map,
                                pp,
                                VecWidth::V128,
                                w,
                                opcode,
                                dst_reg,
                                src_reg,
                                aaa,
                                *mask_zeroing,
                                true,
                                FpRoundMode::Dynamic,
                            );
                        } else {
                            emitter.emit_evex_masked_rr(
                                map,
                                pp,
                                operation_width,
                                w,
                                opcode,
                                dst_reg,
                                src_reg,
                                aaa,
                                *mask_zeroing,
                                *suppress_exceptions,
                                *round,
                            );
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let vex_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        let expected_round = if *truncate {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        };
                        if !vex_family
                            || map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || !matches!(operation_width, VecWidth::V128 | VecWidth::V256)
                            || !*zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != expected_round
                            || *suppress_exceptions
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid VEX packed conversion metadata".to_string(),
                            });
                        }
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: operation_width,
                                w: hinted_w,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    None => {
                        let legacy_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        let expected_round = if *truncate {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        };
                        if !legacy_family
                            || *zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != expected_round
                            || *suppress_exceptions
                            || operation_width != VecWidth::V128
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid legacy packed conversion shape".to_string(),
                            });
                        }
                        let prefix = match pp {
                            X86SsePrefix::None => None,
                            X86SsePrefix::OpSize => Some(0x66),
                            X86SsePrefix::Rep => Some(0xF3),
                            X86SsePrefix::Repne => Some(0xF2),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src_reg);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "X86PackedFpToInt without canonical encoding metadata".to_string(),
                        });
                    }
                }
            }

            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem,
                signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp16".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*int_elem, *signed) {
                    (VecElementType::I16, true) => (X86SsePrefix::Rep, 0x7D, false),
                    (VecElementType::I16, false) => (X86SsePrefix::Repne, 0x7D, false),
                    (VecElementType::I32, true) => (X86SsePrefix::None, 0x5B, false),
                    (VecElementType::I32, false) => (X86SsePrefix::Repne, 0x7A, false),
                    (VecElementType::I64, true) => (X86SsePrefix::None, 0x5B, true),
                    (VecElementType::I64, false) => (X86SsePrefix::Repne, 0x7A, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp16".to_string(),
                            operand: "integer element must be I16, I32, or I64".to_string(),
                        });
                    }
                };
                let expected_lanes = src_width.lanes(*int_elem) as u8;
                let dst_bytes = u32::from(expected_lanes) * VecElementType::F16.bytes();
                let expected_dst_width = match dst_bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp16".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let Some(X86OpHint::EvexOp {
                    map,
                    pp: hinted_pp,
                    opcode: hinted_opcode,
                    width: hinted_width,
                    w: hinted_w,
                }) = op.x86_hint
                else {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedIntToFp16 without canonical EVEX metadata".to_string(),
                    });
                };
                if map != X86VecMap::Map5
                    || hinted_pp != pp
                    || hinted_opcode != opcode
                    || hinted_width != *src_width
                    || hinted_w != w
                    || *lanes != expected_lanes
                    || *dst_width != expected_dst_width
                    || !*zero_upper
                    || (*mask_zeroing && aaa == 0)
                    || *round == FpRoundMode::RoundNearestTiesAway
                    || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                    || (*suppress_exceptions && *src_width != VecWidth::V512)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp16".to_string(),
                        operand: "invalid packed integer-to-FP16 EVEX shape".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_masked_rr(
                    X86VecMap::Map5,
                    pp,
                    *src_width,
                    w,
                    opcode,
                    dst_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *round,
                );
            }

            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
                int_elem,
                signed,
                truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFp16ToInt".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode) = match (*int_elem, *signed, *truncate) {
                    (VecElementType::I16, true, false) => (X86SsePrefix::OpSize, 0x7D),
                    (VecElementType::I16, true, true) => (X86SsePrefix::OpSize, 0x7C),
                    (VecElementType::I16, false, false) => (X86SsePrefix::None, 0x7D),
                    (VecElementType::I16, false, true) => (X86SsePrefix::None, 0x7C),
                    (VecElementType::I32, true, false) => (X86SsePrefix::OpSize, 0x5B),
                    (VecElementType::I32, true, true) => (X86SsePrefix::Rep, 0x5B),
                    (VecElementType::I32, false, false) => (X86SsePrefix::None, 0x79),
                    (VecElementType::I32, false, true) => (X86SsePrefix::None, 0x78),
                    (VecElementType::I64, true, false) => (X86SsePrefix::OpSize, 0x7B),
                    (VecElementType::I64, true, true) => (X86SsePrefix::OpSize, 0x7A),
                    (VecElementType::I64, false, false) => (X86SsePrefix::OpSize, 0x79),
                    (VecElementType::I64, false, true) => (X86SsePrefix::OpSize, 0x78),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFp16ToInt".to_string(),
                            operand: "integer element must be I16, I32, or I64".to_string(),
                        });
                    }
                };
                let expected_lanes = dst_width.lanes(*int_elem) as u8;
                let src_bytes = u32::from(expected_lanes) * 2;
                let expected_src_width = match src_bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFp16ToInt".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let Some(X86OpHint::EvexOp {
                    map,
                    pp: hinted_pp,
                    opcode: hinted_opcode,
                    width: hinted_width,
                    w: hinted_w,
                }) = op.x86_hint
                else {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedFp16ToInt without canonical EVEX metadata".to_string(),
                    });
                };
                let rounding_valid = if *truncate {
                    *round == FpRoundMode::RoundTowardZero
                } else {
                    *suppress_exceptions == (*round != FpRoundMode::Dynamic)
                        && *round != FpRoundMode::RoundNearestTiesAway
                };
                if map != X86VecMap::Map5
                    || hinted_pp != pp
                    || hinted_opcode != opcode
                    || hinted_width != *dst_width
                    || hinted_w
                    || *lanes != expected_lanes
                    || *src_width != expected_src_width
                    || !*zero_upper
                    || (*mask_zeroing && aaa == 0)
                    || !rounding_valid
                    || (*suppress_exceptions && *dst_width != VecWidth::V512)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFp16ToInt".to_string(),
                        operand: "invalid packed FP16-to-integer EVEX shape".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                if *truncate && *suppress_exceptions {
                    // SAE-only forms use EVEX.b=1 with L'L ignored. Emit the
                    // canonical LLVM encoding (L'L=00b), while ZMM operands
                    // still select the architecturally fixed 512-bit form.
                    emitter.emit_evex_masked_rr(
                        X86VecMap::Map5,
                        pp,
                        VecWidth::V128,
                        false,
                        opcode,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        true,
                        FpRoundMode::Dynamic,
                    );
                } else {
                    emitter.emit_evex_masked_rr(
                        X86VecMap::Map5,
                        pp,
                        *dst_width,
                        false,
                        opcode,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        *suppress_exceptions,
                        *round,
                    );
                }
            }

            OpKind::X86PackedFpConvertStore { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: "X86PackedFpConvertStore".to_string(),
                });
            }

            OpKind::MaterializeFlags => {}

            OpKind::SetCF { value } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                if *value {
                    emitter.emit_stc();
                } else {
                    emitter.emit_clc();
                }
            }

            OpKind::SetDF { value } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                if *value {
                    emitter.emit_std();
                } else {
                    emitter.emit_cld();
                }
            }

            OpKind::CmcCF => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmc();
            }

            _ => return self.lower_op_memory(op),
        }

        Ok(())
    }
}
