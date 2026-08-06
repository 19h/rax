//! SIMD / SSE / MMX vector lowering

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
    pub fn set_preserve_vector_call_helpers(&mut self, on: bool) {
        self.preserve_vector_call_helpers = on;
    }

    pub fn set_narrow_vector_opmask_helpers(&mut self, on: bool) {
        self.narrow_vector_opmask_helpers = on;
    }

    /// Lower an exact destructive register-register MMX operation before the
    /// generic vector paths classify MM registers as a distinct register file.
    /// Returning `false` means the operation has no MM operand and should use
    /// the normal scalar/vector matcher; any mixed or malformed MMX shape is an
    /// error rather than a widening opportunity.
    pub(crate) fn lower_mmx_rr(
        &mut self,
        op: &crate::smir::ir::ops::SmirOp,
    ) -> Result<bool, LowerError> {
        if let OpKind::VMov { dst, src, width } = &op.kind {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if is_mm(dst) || is_mm(src) {
                let opcode = match op.x86_hint {
                    Some(X86OpHint::SseMov {
                        prefix: X86SsePrefix::None,
                        opcode: opcode @ (0x6F | 0x7F),
                    }) => Some(opcode),
                    _ => None,
                };
                let encoding_valid =
                    *width == VecWidth::V64 && is_mm(dst) && is_mm(src) && opcode.is_some();
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX MOVQ".to_string(),
                        operand: "requires exact V64 MM registers and prefix-free MOVQ opcode"
                            .to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let opcode = opcode.unwrap();
                let (reg, rm) = if opcode == 0x6F {
                    (dst_reg, src_reg)
                } else {
                    (src_reg, dst_reg)
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_rr(opcode, reg, rm);
                return Ok(true);
            }
        }

        if let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } = &op.kind
        {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if is_mm(dst) || is_mm(vec) {
                let safe_gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)));
                let encoding_valid = dst == vec
                    && is_mm(dst)
                    && is_mm(vec)
                    && safe_gpr(scalar)
                    && *lane < 4
                    && *elem == VecElementType::I16
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0xC4,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PINSRW".to_string(),
                        operand: "requires an exact destructive I16 MM destination and safe legacy GPR source"
                            .to_string(),
                    });
                }
                let mm_reg = self.get_dst_reg(*dst)?;
                let gpr_reg = self.get_reg(*scalar)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_word_lane_rr_imm(0xC4, mm_reg, gpr_reg, *lane);
                return Ok(true);
            }
        }

        if let OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } = &op.kind
        {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if is_mm(vec) {
                let safe_gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)));
                let encoding_valid = safe_gpr(dst)
                    && *lane < 4
                    && *elem == VecElementType::I16
                    && *sign == SignExtend::Zero
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0xC5,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PEXTRW".to_string(),
                        operand: "requires an exact I16 MM source and safe legacy GPR destination"
                            .to_string(),
                    });
                }
                let mm_reg = self.get_reg(*vec)?;
                let gpr_reg = self.get_dst_reg(*dst)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_word_lane_rr_imm(0xC5, mm_reg, gpr_reg, *lane);
                return Ok(true);
            }
        }

        if let OpKind::X86PackedShuffleImm {
            dst,
            src,
            width,
            elem,
            imm,
            high_words,
        } = &op.kind
        {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if is_mm(dst) || is_mm(src) {
                let encoding_valid = *width == VecWidth::V64
                    && *elem == VecElementType::I16
                    && high_words.is_none()
                    && is_mm(dst)
                    && is_mm(src)
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0x70,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PSHUFW".to_string(),
                        operand: "requires exact I16x4 MM registers and prefix-free opcode"
                            .to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_rr_imm(0x70, dst_reg, src_reg, *imm);
                return Ok(true);
            }
        }

        if let OpKind::X86PackedAlignRight {
            dst,
            high,
            low,
            width,
            amount,
        } = &op.kind
        {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if [dst, high, low].into_iter().any(is_mm) {
                let encoding_valid = *width == VecWidth::V64
                    && dst == high
                    && [dst, high, low].into_iter().all(is_mm)
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0x0F,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PALIGNR".to_string(),
                        operand: "requires exact destructive V64 MM registers and 0F3A opcode"
                            .to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let low_reg = self.get_reg(*low)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_0f3a_rr_imm(0x0F, dst_reg, low_reg, *amount);
                return Ok(true);
            }
        }

        if x86_native_mmx_movd_q_candidate(op) {
            self.lower_native_mmx_movd_q(op)?;
            return Ok(true);
        }

        if let OpKind::X86MovMask {
            dst,
            src,
            elem,
            lanes,
            dst_width,
        } = &op.kind
        {
            let src_is_mm = matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if src_is_mm {
                let safe_gpr_vreg = matches!(
                    dst,
                    VReg::Arch(ArchReg::X86(
                        X86Reg::Rax
                            | X86Reg::Rcx
                            | X86Reg::Rdx
                            | X86Reg::Rbx
                            | X86Reg::Rsi
                            | X86Reg::Rdi
                            | X86Reg::R8
                            | X86Reg::R9
                            | X86Reg::R10
                            | X86Reg::R11
                            | X86Reg::R12
                            | X86Reg::R13
                            | X86Reg::R14
                            | X86Reg::R15
                    ))
                );
                if !safe_gpr_vreg {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PMOVMSKB".to_string(),
                        operand: "requires a safe legacy GPR destination".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let safe_gpr = matches!(
                    dst_reg,
                    PhysReg::Rax
                        | PhysReg::Rcx
                        | PhysReg::Rdx
                        | PhysReg::Rbx
                        | PhysReg::Rsi
                        | PhysReg::Rdi
                        | PhysReg::R8
                        | PhysReg::R9
                        | PhysReg::R10
                        | PhysReg::R11
                        | PhysReg::R12
                        | PhysReg::R13
                        | PhysReg::R14
                        | PhysReg::R15
                );
                let encoding_valid = safe_gpr
                    && matches!(src_reg, PhysReg::Mm(0..=7))
                    && *elem == VecElementType::I8
                    && *lanes == 8
                    && matches!(dst_width, OpWidth::W32 | OpWidth::W64)
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0xD7,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PMOVMSKB".to_string(),
                        operand: "requires an exact I8x8 MM source and safe legacy GPR destination"
                            .to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_mask_rr(
                    None,
                    0xD7,
                    dst_reg,
                    src_reg,
                    *dst_width == OpWidth::W64,
                );
                return Ok(true);
            }
        }

        if let OpKind::VByteShuffle {
            dst,
            src,
            control,
            lanes,
            block_lanes,
        } = &op.kind
        {
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if ![dst, src, control].into_iter().any(is_mm) {
                return Ok(false);
            }
            let encoding_valid = *lanes == 8
                && *block_lanes == 8
                && dst == src
                && [dst, src, control].into_iter().all(is_mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x00,
                    })
                );
            if !encoding_valid {
                return Err(LowerError::InvalidOperand {
                    op: "MMX PSHUFB".to_string(),
                    operand: "requires exact destructive I8x8 MM registers and 0F38 opcode"
                        .to_string(),
                });
            }
            let dst_reg = self.get_dst_reg(*dst)?;
            let control_reg = self.get_reg(*control)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mmx_0f38_rr(0x00, dst_reg, control_reg);
            return Ok(true);
        }

        if let OpKind::VUnary {
            dst,
            src,
            elem,
            lanes,
            op: VecUnaryOp::Abs,
        } = &op.kind
        {
            let expected = match (*elem, *lanes) {
                (VecElementType::I8, 8) => Some(0x1C),
                (VecElementType::I16, 4) => Some(0x1D),
                (VecElementType::I32, 2) => Some(0x1E),
                _ => None,
            };
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if !is_mm(dst) && !is_mm(src) {
                return Ok(false);
            }
            let encoding_valid = expected.is_some()
                && is_mm(dst)
                && is_mm(src)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode,
                    }) if Some(opcode) == expected
                );
            if !encoding_valid {
                return Err(LowerError::InvalidOperand {
                    op: "MMX packed absolute value".to_string(),
                    operand: "requires exact V64 MM registers and 0F38 opcode".to_string(),
                });
            }
            let dst_reg = self.get_dst_reg(*dst)?;
            let src_reg = self.get_reg(*src)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mmx_0f38_rr(expected.unwrap(), dst_reg, src_reg);
            return Ok(true);
        }

        if let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: VLaneOp::Sign,
            signed,
            set_ovf,
        } = &op.kind
        {
            let expected = match (*elem, *lanes, *signed, *set_ovf) {
                (VecElementType::I8, 8, true, false) => Some(0x08),
                (VecElementType::I16, 4, true, false) => Some(0x09),
                (VecElementType::I32, 2, true, false) => Some(0x0A),
                _ => None,
            };
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if ![dst, src1, src2].into_iter().any(is_mm) {
                return Ok(false);
            }
            let encoding_valid = expected.is_some()
                && dst == src1
                && [dst, src1, src2].into_iter().all(is_mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode,
                    }) if Some(opcode) == expected
                );
            if !encoding_valid {
                return Err(LowerError::InvalidOperand {
                    op: "MMX packed sign".to_string(),
                    operand: "requires exact destructive V64 MM registers and 0F38 opcode"
                        .to_string(),
                });
            }
            let dst_reg = self.get_dst_reg(*dst)?;
            let src2_reg = self.get_reg(*src2)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mmx_0f38_rr(expected.unwrap(), dst_reg, src2_reg);
            return Ok(true);
        }

        if let OpKind::VHorizontalBin {
            dst,
            src1,
            src2,
            elem,
            lanes,
            block_lanes,
            subtract,
            saturating,
        } = &op.kind
        {
            let expected = match (*elem, *lanes, *block_lanes, *subtract, *saturating) {
                (VecElementType::I16, 4, 4, false, false) => Some(0x01),
                (VecElementType::I32, 2, 2, false, false) => Some(0x02),
                (VecElementType::I16, 4, 4, false, true) => Some(0x03),
                (VecElementType::I16, 4, 4, true, false) => Some(0x05),
                (VecElementType::I32, 2, 2, true, false) => Some(0x06),
                (VecElementType::I16, 4, 4, true, true) => Some(0x07),
                _ => None,
            };
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if ![dst, src1, src2].into_iter().any(is_mm) {
                return Ok(false);
            }
            let encoding_valid = expected.is_some()
                && dst == src1
                && [dst, src1, src2].into_iter().all(is_mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode,
                    }) if Some(opcode) == expected
                );
            if !encoding_valid {
                return Err(LowerError::InvalidOperand {
                    op: "MMX horizontal integer operation".to_string(),
                    operand: "requires exact destructive V64 MM registers and 0F38 opcode"
                        .to_string(),
                });
            }
            let dst_reg = self.get_dst_reg(*dst)?;
            let src2_reg = self.get_reg(*src2)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mmx_0f38_rr(expected.unwrap(), dst_reg, src2_reg);
            return Ok(true);
        }

        if let OpKind::VDotProduct {
            dst,
            acc,
            src1,
            src2,
            mask,
            src_elem,
            acc_elem,
            width,
            src1_unsigned,
            saturate,
            zeroing,
        } = &op.kind
        {
            let exact_maddubs = *acc == VReg::Imm(0)
                && mask.is_none()
                && *src_elem == VecElementType::I8
                && *acc_elem == VecElementType::I16
                && *width == VecWidth::V64
                && *src1_unsigned
                && *saturate
                && !*zeroing;
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if exact_maddubs && [dst, src1, src2].into_iter().any(is_mm) {
                let encoding_valid = dst == src1
                    && [dst, src1, src2].into_iter().all(is_mm)
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0x04,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PMADDUBSW".to_string(),
                        operand: "requires exact destructive V64 MM registers and 0F38 opcode"
                            .to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src2_reg = self.get_reg(*src2)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_0f38_rr(0x04, dst_reg, src2_reg);
                return Ok(true);
            }
        }

        if let OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem,
            lanes,
            signed1,
            signed2,
            shift_left,
            round,
            sat_bits,
            out_shift,
        } = &op.kind
        {
            let exact_mulhrsw = *src_elem == VecElementType::I16
                && *lanes == 4
                && *signed1
                && *signed2
                && *shift_left == 0
                && *round
                && *sat_bits == 0
                && *out_shift == 15;
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if exact_mulhrsw && [dst, src1, src2].into_iter().any(is_mm) {
                let encoding_valid = dst == src1
                    && [dst, src1, src2].into_iter().all(is_mm)
                    && matches!(
                        op.x86_hint,
                        Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: 0x0B,
                        })
                    );
                if !encoding_valid {
                    return Err(LowerError::InvalidOperand {
                        op: "MMX PMULHRSW".to_string(),
                        operand: "requires exact destructive V64 MM registers and 0F38 opcode"
                            .to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src2_reg = self.get_reg(*src2)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_0f38_rr(0x0B, dst_reg, src2_reg);
                return Ok(true);
            }
        }

        if let OpKind::X86PackedShiftImm {
            dst,
            src,
            width,
            elem,
            shift,
            amount,
            byte_lane,
        } = &op.kind
        {
            let encoding = match (*width, *elem, *shift, *byte_lane) {
                (VecWidth::V64, VecElementType::I16, ShiftOp::Lsr, false) => Some((0x71, 2)),
                (VecWidth::V64, VecElementType::I16, ShiftOp::Asr, false) => Some((0x71, 4)),
                (VecWidth::V64, VecElementType::I16, ShiftOp::Lsl, false) => Some((0x71, 6)),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Lsr, false) => Some((0x72, 2)),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Asr, false) => Some((0x72, 4)),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Lsl, false) => Some((0x72, 6)),
                (VecWidth::V64, VecElementType::I64, ShiftOp::Lsr, false) => Some((0x73, 2)),
                (VecWidth::V64, VecElementType::I64, ShiftOp::Lsl, false) => Some((0x73, 6)),
                _ => None,
            };
            let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
            if !is_mm(dst) && !is_mm(src) {
                return Ok(false);
            }
            let encoding_valid = encoding.is_some()
                && dst == src
                && is_mm(dst)
                && is_mm(src)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode,
                    }) if Some(opcode) == encoding.map(|(opcode, _)| opcode)
                );
            if !encoding_valid {
                return Err(LowerError::InvalidOperand {
                    op: "MMX immediate shift".to_string(),
                    operand: "requires exact destructive V64 register and group opcode".to_string(),
                });
            }
            let (opcode, digit) = encoding.unwrap();
            let dst_reg = self.get_dst_reg(*dst)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mmx_shift_imm(opcode, digit, dst_reg, *amount);
            return Ok(true);
        }

        let (dst, src1, src2, expected_opcode) = match &op.kind {
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xDB)),
            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xDF)),
            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xEB)),
            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xEF)),
            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes) {
                    (VecElementType::I8, 8) => Some(0xFC),
                    (VecElementType::I16, 4) => Some(0xFD),
                    (VecElementType::I32, 2) => Some(0xFE),
                    (VecElementType::I64, 1) => Some(0xD4),
                    _ => None,
                },
            ),
            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes) {
                    (VecElementType::I8, 8) => Some(0xF8),
                    (VecElementType::I16, 4) => Some(0xF9),
                    (VecElementType::I32, 2) => Some(0xFA),
                    (VecElementType::I64, 1) => Some(0xFB),
                    _ => None,
                },
            ),
            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes,
                subtract,
                signed,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes, *subtract, *signed) {
                    (VecElementType::I8, 8, false, true) => Some(0xEC),
                    (VecElementType::I16, 4, false, true) => Some(0xED),
                    (VecElementType::I8, 8, false, false) => Some(0xDC),
                    (VecElementType::I16, 4, false, false) => Some(0xDD),
                    (VecElementType::I8, 8, true, true) => Some(0xE8),
                    (VecElementType::I16, 4, true, true) => Some(0xE9),
                    (VecElementType::I8, 8, true, false) => Some(0xD8),
                    (VecElementType::I16, 4, true, false) => Some(0xD9),
                    _ => None,
                },
            ),
            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes, *cond) {
                    (VecElementType::I8, 8, VecCmpCond::Gt) => Some(0x64),
                    (VecElementType::I16, 4, VecCmpCond::Gt) => Some(0x65),
                    (VecElementType::I32, 2, VecCmpCond::Gt) => Some(0x66),
                    (VecElementType::I8, 8, VecCmpCond::Eq) => Some(0x74),
                    (VecElementType::I16, 4, VecCmpCond::Eq) => Some(0x75),
                    (VecElementType::I32, 2, VecCmpCond::Eq) => Some(0x76),
                    _ => None,
                },
            ),
            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                high,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes, *block_lanes, *high) {
                    (VecElementType::I8, 8, 8, false) => Some(0x60),
                    (VecElementType::I16, 4, 4, false) => Some(0x61),
                    (VecElementType::I32, 2, 2, false) => Some(0x62),
                    (VecElementType::I8, 8, 8, true) => Some(0x68),
                    (VecElementType::I16, 4, 4, true) => Some(0x69),
                    (VecElementType::I32, 2, 2, true) => Some(0x6A),
                    _ => None,
                },
            ),
            OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            } => (
                dst,
                src2,
                src1,
                match (*src_elem, *src_lanes, *block_lanes, *to_unsigned) {
                    (VecElementType::I16, 4, 4, false) => Some(0x63),
                    (VecElementType::I16, 4, 4, true) => Some(0x67),
                    (VecElementType::I32, 2, 2, false) => Some(0x6B),
                    _ => None,
                },
            ),
            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: lane_op,
                signed,
                set_ovf,
            } => (
                dst,
                src1,
                src2,
                match (*elem, *lanes, *lane_op, *signed, *set_ovf) {
                    (VecElementType::I8, 8, VLaneOp::Min, false, false) => Some(0xDA),
                    (VecElementType::I8, 8, VLaneOp::Max, false, false) => Some(0xDE),
                    (VecElementType::I16, 4, VLaneOp::Min, true, false) => Some(0xEA),
                    (VecElementType::I16, 4, VLaneOp::Max, true, false) => Some(0xEE),
                    (VecElementType::I8, 8, VLaneOp::AvgRnd, false, false) => Some(0xE0),
                    (VecElementType::I16, 4, VLaneOp::AvgRnd, false, false) => Some(0xE3),
                    _ => None,
                },
            ),
            OpKind::VDotProduct {
                dst,
                acc,
                src1,
                src2,
                mask,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing,
            } => (
                dst,
                src1,
                src2,
                (*acc == VReg::Imm(0)
                    && mask.is_none()
                    && *src_elem == VecElementType::I16
                    && *acc_elem == VecElementType::I32
                    && *width == VecWidth::V64
                    && !*src1_unsigned
                    && !*saturate
                    && !*zeroing)
                    .then_some(0xF5),
            ),
            OpKind::VSadBytes {
                dst,
                src1,
                src2,
                width,
            } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xF6)),
            OpKind::X86PackedShift {
                dst,
                src,
                count,
                width,
                elem,
                shift,
            } => (
                dst,
                src,
                count,
                match (*width, *elem, *shift) {
                    (VecWidth::V64, VecElementType::I16, ShiftOp::Lsr) => Some(0xD1),
                    (VecWidth::V64, VecElementType::I32, ShiftOp::Lsr) => Some(0xD2),
                    (VecWidth::V64, VecElementType::I64, ShiftOp::Lsr) => Some(0xD3),
                    (VecWidth::V64, VecElementType::I16, ShiftOp::Asr) => Some(0xE1),
                    (VecWidth::V64, VecElementType::I32, ShiftOp::Asr) => Some(0xE2),
                    (VecWidth::V64, VecElementType::I16, ShiftOp::Lsl) => Some(0xF1),
                    (VecWidth::V64, VecElementType::I32, ShiftOp::Lsl) => Some(0xF2),
                    (VecWidth::V64, VecElementType::I64, ShiftOp::Lsl) => Some(0xF3),
                    _ => None,
                },
            ),
            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => (
                dst,
                src1,
                src2,
                (*elem == VecElementType::I16 && *lanes == 4).then_some(0xD5),
            ),
            OpKind::VMulShiftSat {
                dst,
                src1,
                src2,
                src_elem,
                lanes,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            } => (
                dst,
                src1,
                src2,
                match (
                    *src_elem,
                    *lanes,
                    *signed1,
                    *signed2,
                    *shift_left,
                    *round,
                    *sat_bits,
                    *out_shift,
                ) {
                    (VecElementType::I16, 4, false, false, 0, false, 0, 16) => Some(0xE4),
                    (VecElementType::I16, 4, true, true, 0, false, 0, 16) => Some(0xE5),
                    _ => None,
                },
            ),
            _ => return Ok(false),
        };
        let is_mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if ![dst, src1, src2].into_iter().any(is_mm) {
            return Ok(false);
        }
        let encoding_valid = expected_opcode.is_some()
            && dst == src1
            && [dst, src1, src2].into_iter().all(is_mm)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if Some(opcode) == expected_opcode
            );
        if !encoding_valid {
            return Err(LowerError::InvalidOperand {
                op: "MMX register operation".to_string(),
                operand: "requires exact destructive V64 registers and classic opcode".to_string(),
            });
        }

        let dst_reg = self.get_dst_reg(*dst)?;
        let src2_reg = self.get_reg(*src2)?;
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mmx_rr(expected_opcode.unwrap(), dst_reg, src2_reg);
        Ok(true)
    }

    pub(crate) fn coerce_vec_encoding(
        &self,
        mut encoding: VecEncoding,
        regs: &[PhysReg],
    ) -> VecEncoding {
        if self.vec_requires_evex(encoding.width, regs) {
            encoding.kind = VecEncodingKind::Evex;
        }
        encoding
    }

    /// Select the mandatory prefix for a legacy vector move without conflating
    /// an explicit `SseMov { prefix: None }` with the absence of an encoding
    /// hint. The former is the canonical MOVAPS/MOVUPS no-prefix form; only an
    /// absent/non-SSE hint may fall back to the default MOVDQU-style prefix.
    pub(crate) fn legacy_vec_move_prefix(&self, hint: Option<X86OpHint>) -> Option<u8> {
        match hint {
            Some(X86OpHint::SseMov { .. } | X86OpHint::SseOp { .. }) => self.sse_prefix(hint),
            _ => self.vec_move_prefix(hint),
        }
    }

    pub(crate) fn default_vec_mov_encoding(
        &self,
        width: VecWidth,
        opcode: u8,
        hint: Option<X86OpHint>,
    ) -> VecEncoding {
        VecEncoding {
            kind: VecEncodingKind::Vex,
            map: X86VecMap::Map0F,
            pp: self.vec_move_pp(hint),
            opcode,
            width,
            w: false,
        }
    }

    pub(crate) fn emit_vec_rrr(
        &mut self,
        encoding: VecEncoding,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
    ) {
        let encoding = self.coerce_vec_encoding(encoding, &[dst, src1, src2]);
        let mut emitter = X86Emitter::new(&mut self.code);
        match encoding.kind {
            VecEncodingKind::Vex => {
                emitter.emit_vex_rrr(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    encoding.w,
                    encoding.opcode,
                    dst,
                    src1,
                    src2,
                );
            }
            VecEncodingKind::Evex => {
                emitter.emit_evex_rrr(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    encoding.w,
                    encoding.opcode,
                    dst,
                    src1,
                    src2,
                );
            }
        }
    }

    pub(crate) fn emit_vec_rrr_imm(
        &mut self,
        encoding: VecEncoding,
        dst: PhysReg,
        src1: PhysReg,
        src2: PhysReg,
        imm: u8,
    ) {
        let encoding = self.coerce_vec_encoding(encoding, &[dst, src1, src2]);
        let mut emitter = X86Emitter::new(&mut self.code);
        match encoding.kind {
            VecEncodingKind::Vex => emitter.emit_vex_rrr(
                encoding.map,
                encoding.pp,
                encoding.width,
                encoding.w,
                encoding.opcode,
                dst,
                src1,
                src2,
            ),
            VecEncodingKind::Evex => emitter.emit_evex_rrr(
                encoding.map,
                encoding.pp,
                encoding.width,
                encoding.w,
                encoding.opcode,
                dst,
                src1,
                src2,
            ),
        }
        emitter.code.emit_u8(imm);
    }

    pub(crate) fn emit_vec_rr(
        &mut self,
        encoding: VecEncoding,
        reg: PhysReg,
        rm: PhysReg,
        vvvv: u8,
    ) {
        let encoding = self.coerce_vec_encoding(encoding, &[reg, rm]);
        let r = reg.vec_ext();
        let r2 = reg.vec_ext2();
        let b = rm.vec_ext();
        let b2 = rm.vec_ext2();
        let w = encoding.w;
        let mut emitter = X86Emitter::new(&mut self.code);
        match encoding.kind {
            VecEncodingKind::Vex => {
                emitter.emit_vex_prefix(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    w,
                    r,
                    0,
                    b,
                    vvvv,
                );
            }
            VecEncodingKind::Evex => {
                emitter.emit_evex_prefix(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    w,
                    r,
                    0,
                    b,
                    r2,
                    0,
                    b2,
                    vvvv,
                );
            }
        }
        emitter.code.emit_u8(encoding.opcode);
        emitter.emit_modrm_rr(reg, rm);
    }

    pub(crate) fn emit_vec_shift_imm(
        &mut self,
        encoding: VecEncoding,
        dst: PhysReg,
        src: PhysReg,
        imm: u8,
    ) {
        let encoding = self.coerce_vec_encoding(encoding, &[dst, src]);
        let b = src.vec_ext();
        let b2 = src.vec_ext2();
        let vvvv = dst.encoding() & 0x1F;
        let mut emitter = X86Emitter::new(&mut self.code);
        match encoding.kind {
            VecEncodingKind::Vex => {
                emitter.emit_vex_prefix(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    encoding.w,
                    0,
                    0,
                    b,
                    vvvv,
                );
            }
            VecEncodingKind::Evex => {
                emitter.emit_evex_prefix(
                    encoding.map,
                    encoding.pp,
                    encoding.width,
                    encoding.w,
                    0,
                    0,
                    b,
                    0,
                    0,
                    b2,
                    vvvv,
                );
            }
        }
        emitter.code.emit_u8(encoding.opcode);
        emitter.emit_modrm_digit(0b11, 6, src);
        emitter.code.emit_u8(imm);
    }
}
