//! Bitwise-operation lowering

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
    pub(crate) fn lower_op_bitwise(
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
            // Bitwise Operations
            // ================================================================
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if let Some(shape) = x86_state_backed_stack_group1_lowerable(op) {
                    return self.lower_state_backed_stack_gpr_group1(&shape);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "And",
                                &[dst_reg, src1_reg, src2_reg],
                            )?;
                        }
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        let operand = if dst_reg != src1_reg && dst_reg == src2_reg {
                            src1_reg
                        } else {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            src2_reg
                        };
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_alu_rr_dir(0x20, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("And", &[dst_reg, src1_reg])?;
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x24, *val, *width);
                        } else {
                            emitter.emit_and_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "And with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::AndNot {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_and_not_candidate(op) {
                    if !x86_state_backed_gpr_and_not_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed AndNot".to_string(),
                            operand: format!("invalid x86 GPR ANDN {width:?} {flags:?}"),
                        });
                    }
                    let SrcOperand::Reg(src2) = src2 else {
                        unreachable!();
                    };
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_and_not(
                        *dst,
                        *src1,
                        *src2,
                        *width,
                        defined_rflags_mask,
                    );
                }
                self.lower_and_not(*dst, *src1, src2, *width, *flags)?;
            }

            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if let Some(shape) = x86_state_backed_stack_group1_lowerable(op) {
                    return self.lower_state_backed_stack_gpr_group1(&shape);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Or",
                                &[dst_reg, src1_reg, src2_reg],
                            )?;
                        }
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        let operand = if dst_reg != src1_reg && dst_reg == src2_reg {
                            src1_reg
                        } else {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            src2_reg
                        };
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_alu_rr_dir(0x08, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Or", &[dst_reg, src1_reg])?;
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x0C, *val, *width);
                        } else {
                            emitter.emit_or_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Or with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if let Some(shape) = x86_state_backed_stack_group1_lowerable(op) {
                    return self.lower_state_backed_stack_gpr_group1(&shape);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Xor",
                                &[dst_reg, src1_reg, src2_reg],
                            )?;
                        }
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        let operand = if dst_reg != src1_reg && dst_reg == src2_reg {
                            src1_reg
                        } else {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            src2_reg
                        };
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_alu_rr_dir(0x30, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Xor", &[dst_reg, src1_reg])?;
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x34, *val, *width);
                        } else {
                            emitter.emit_xor_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Xor with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Not { dst, src, width } => {
                if x86_state_backed_gpr_not_candidate(op) {
                    if !x86_state_backed_gpr_not_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Not".to_string(),
                            operand: format!("invalid x86 GPR complement {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_not(*dst, *src, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_not(dst_reg, *width);
            }

            OpKind::Bswap { dst, src, width } => {
                if x86_state_backed_gpr_bswap_candidate(op) {
                    if !x86_state_backed_gpr_bswap_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bswap".to_string(),
                            operand: format!("invalid x86 GPR byte swap {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bswap(*dst, *src, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                match width {
                    OpWidth::W16 => {
                        self.code.emit_u8(0x9C); // pushfq
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_rol_ri(dst_reg, 8, *width);
                        self.code.emit_u8(0x9D); // popfq
                    }
                    OpWidth::W32 | OpWidth::W64 => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_bswap(dst_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bswap".to_string(),
                            operand: format!("unsupported width {width:?}"),
                        });
                    }
                }
            }

            OpKind::Bt { src, index, width } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bt".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Test,
                        None,
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Test, None, *src, index, *width)?;
            }

            OpKind::Bts {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bts".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Set,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Set, Some(*dst), *src, index, *width)?;
            }

            OpKind::Btr {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Btr".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Reset,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Reset, Some(*dst), *src, index, *width)?;
            }

            OpKind::Btc {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Btc".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Complement,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Complement, Some(*dst), *src, index, *width)?;
            }

            OpKind::Crc32C {
                dst,
                crc,
                data,
                data_width,
            } => {
                if x86_state_backed_gpr_crc32_candidate(op) {
                    if !x86_state_backed_gpr_crc32_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Crc32C".to_string(),
                            operand: format!("invalid x86 GPR CRC32C {data_width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_crc32c(*dst, *crc, *data, *data_width);
                }
                self.lower_crc32c(*dst, *crc, *data, *data_width)?;
            }

            OpKind::Bsf {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bit_scan_candidate(op) {
                    if !x86_state_backed_gpr_bit_scan_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bsf".to_string(),
                            operand: format!("invalid x86 GPR bit scan {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_scan(*dst, *src, *width, *flags, false);
                }
                self.lower_bit_scan(*dst, *src, *width, *flags, false)?;
            }

            OpKind::Bsr {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bit_scan_candidate(op) {
                    if !x86_state_backed_gpr_bit_scan_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bsr".to_string(),
                            operand: format!("invalid x86 GPR bit scan {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_scan(*dst, *src, *width, *flags, true);
                }
                self.lower_bit_scan(*dst, *src, *width, *flags, true)?;
            }

            OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bextr_bzhi_candidate(op) {
                    if !x86_state_backed_gpr_bextr_bzhi_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bextr".to_string(),
                            operand: format!("invalid x86 GPR BEXTR {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x841),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bextr_bzhi(
                        *dst,
                        *src,
                        *control,
                        *width,
                        defined_rflags_mask,
                        false,
                    );
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Bextr width {width:?}"),
                    });
                }
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(set)
                        if *set == FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF) =>
                    {
                        Some(0x841)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bextr".to_string(),
                            operand: format!("unsupported flag update {flags:?}"),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if let VReg::Imm(value) = control {
                    self.emit_x86_bextr_imm_regs(
                        dst_reg,
                        src_reg,
                        *value,
                        *width,
                        defined_rflags_mask,
                    )?;
                } else {
                    let control_reg = self.get_reg(*control)?;
                    Self::ensure_flag_stack_operands_safe(
                        "Bextr",
                        &[dst_reg, src_reg, control_reg],
                    )?;
                    self.code.emit_u8(0x9C); // pushfq
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_vex_bmi_rr(0xF7, dst_reg, src_reg, control_reg, *width);
                    self.finish_bmi_flags(dst_reg, defined_rflags_mask);
                }
            }

            OpKind::Bzhi {
                dst,
                src,
                index,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bextr_bzhi_candidate(op) {
                    if !x86_state_backed_gpr_bextr_bzhi_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bzhi".to_string(),
                            operand: format!("invalid x86 GPR BZHI {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bextr_bzhi(
                        *dst,
                        *src,
                        *index,
                        *width,
                        defined_rflags_mask,
                        true,
                    );
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Bzhi width {width:?}"),
                    });
                }
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(set)
                        if *set
                            == FlagSet::CF
                                .union(FlagSet::ZF)
                                .union(FlagSet::SF)
                                .union(FlagSet::OF) =>
                    {
                        Some(0x8C1)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bzhi".to_string(),
                            operand: format!("unsupported flag update {flags:?}"),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let index_reg = self.get_reg(*index)?;
                Self::ensure_flag_stack_operands_safe("Bzhi", &[dst_reg, src_reg, index_reg])?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr(0xF5, dst_reg, src_reg, index_reg, *width);
                self.finish_bmi_flags(dst_reg, defined_rflags_mask);
            }

            OpKind::X86Bls {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_bls_candidate(op) {
                    if !x86_state_backed_gpr_bls_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Bls".to_string(),
                            operand: format!("invalid x86 GPR BLS {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bls(
                        *dst,
                        *src,
                        *width,
                        *kind,
                        defined_rflags_mask,
                    );
                }
                self.lower_x86_bls(*dst, *src, *width, *kind, *flags)?;
            }

            OpKind::X86Tbm {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                if op.x86_hint.is_some() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Tbm".to_string(),
                        operand: "encoding hints are unsupported".to_string(),
                    });
                }
                if x86_state_backed_gpr_tbm_candidate(op) {
                    if !x86_state_backed_gpr_tbm_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Tbm".to_string(),
                            operand: format!("invalid x86 GPR TBM {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_tbm(
                        *dst,
                        *src,
                        *width,
                        *kind,
                        defined_rflags_mask,
                    );
                }
                self.lower_x86_tbm(*dst, *src, *width, *kind, *flags)?;
            }

            OpKind::X86XopPackedBit { .. } => {
                self.emit_x86_xop_packed_bit(op)?;
            }

            OpKind::X86Adx {
                dst,
                src1,
                src2,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_adx_candidate(op) {
                    if !x86_state_backed_gpr_adx_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Adx".to_string(),
                            operand: format!("invalid x86 GPR ADX {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    let output_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(match kind {
                            X86AdxKind::Adcx => 1,
                            X86AdxKind::Adox => 1 << 11,
                        }),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_adx(
                        *dst,
                        *src1,
                        *src2,
                        *width,
                        *kind,
                        output_rflags_mask,
                    );
                }
                self.lower_x86_adx(*dst, *src1, *src2, *width, *kind, *flags)?;
            }

            OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            } => {
                if x86_state_backed_gpr_pdep_pext_candidate(op) {
                    if !x86_state_backed_gpr_pdep_pext_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Pdep".to_string(),
                            operand: format!("invalid x86 GPR PDEP {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_pdep_pext(*dst, *src, *mask, *width, false);
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Pdep width {width:?}"),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let mask_reg = self.get_reg(*mask)?;
                Self::ensure_flag_stack_operands_safe("Pdep", &[dst_reg, src_reg, mask_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr_pp(
                    0xF5,
                    X86SsePrefix::Repne,
                    dst_reg,
                    mask_reg,
                    src_reg,
                    *width,
                );
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Pext {
                dst,
                src,
                mask,
                width,
            } => {
                if x86_state_backed_gpr_pdep_pext_candidate(op) {
                    if !x86_state_backed_gpr_pdep_pext_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Pext".to_string(),
                            operand: format!("invalid x86 GPR PEXT {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_pdep_pext(*dst, *src, *mask, *width, true);
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Pext width {width:?}"),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let mask_reg = self.get_reg(*mask)?;
                Self::ensure_flag_stack_operands_safe("Pext", &[dst_reg, src_reg, mask_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr_pp(
                    0xF5,
                    X86SsePrefix::Rep,
                    dst_reg,
                    mask_reg,
                    src_reg,
                    *width,
                );
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Clz { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Clz", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lzcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Ctz { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Ctz", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_tzcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Popcnt { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Popcnt", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_popcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::X86Count {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_count_candidate(op) {
                    if !x86_state_backed_gpr_count_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Count".to_string(),
                            operand: format!("invalid x86 GPR count {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_count(*dst, *src, *width, *kind, *flags);
                }
                self.lower_x86_count(*dst, *src, *width, *kind, *flags)?;
            }

            _ => return self.lower_op_shifts(op),
        }

        Ok(())
    }
}
