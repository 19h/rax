//! Packed binary32/binary64 precision-conversion lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vec_packed_fp_precision_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        debug_assert!(matches!(
            prefix.pp,
            X86SsePrefix::None | X86SsePrefix::OpSize
        ));
        let opcode = 0x5A;
        let cursor = prefix.bytes + 1;
        let after_opcode = &bytes[cursor..];
        let prefix_modrm = X86Prefix {
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            ..prefix.modrm_prefix(cursor)
        };

        if prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex && prefix.v_high)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (from, to) = if prefix.pp == X86SsePrefix::None {
            (VecElementType::F32, VecElementType::F64)
        } else {
            (VecElementType::F64, VecElementType::F32)
        };
        if prefix.encoding == VecEncodingKind::Evex
            && ((from == VecElementType::F32 && prefix.w)
                || (from == VecElementType::F64 && !prefix.w))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
        let embedded_control =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if prefix.encoding == VecEncodingKind::Evex && !embedded_control && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let instruction_width = if embedded_control {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = match instruction_width {
            VecWidth::V128 => 2,
            VecWidth::V256 => 4,
            VecWidth::V512 => 8,
            VecWidth::V64 => unreachable!(),
        };
        let src_width = match (from, instruction_width) {
            (VecElementType::F32, VecWidth::V128) => VecWidth::V64,
            (VecElementType::F32, VecWidth::V256) => VecWidth::V128,
            (VecElementType::F32, VecWidth::V512) => VecWidth::V256,
            (VecElementType::F64, width) => width,
            _ => unreachable!(),
        };
        let dst_width = if to == VecElementType::F32 {
            if lanes == 8 {
                VecWidth::V256
            } else {
                VecWidth::V128
            }
        } else {
            instruction_width
        };
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
            Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
        } else {
            None
        };
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    from.bytes()
                } else {
                    src_width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            if broadcast {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: scalar,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                if let Some(mask_reg) = mask {
                    let cond = self.append_nonzero_mask_predicate(
                        mask_reg,
                        (1u64 << lanes) - 1,
                        pc,
                        ctx,
                        &mut ops,
                    );
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
                            addr,
                            width: if from == VecElementType::F32 {
                                MemWidth::B4
                            } else {
                                MemWidth::B8
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: if from == VecElementType::F32 {
                                MemWidth::B4
                            } else {
                                MemWidth::B8
                            },
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar,
                        elem: from,
                        lanes,
                    },
                ));
            } else if let Some(mask_reg) = mask {
                let zero = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar: zero,
                        elem: from,
                        lanes,
                    },
                ));
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                for lane in 0..lanes {
                    let shifted = ctx.alloc_vreg();
                    let cond = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: mask_reg,
                            amount: SrcOperand::Imm(i64::from(lane)),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: cond,
                            src1: shifted,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
                            addr: Address::base_off(
                                base,
                                i64::from(lane) * i64::from(from.bytes()),
                            ),
                            width: if from == VecElementType::F32 {
                                MemWidth::B4
                            } else {
                                MemWidth::B8
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: value,
                            vec: value,
                            scalar,
                            lane,
                            elem: from,
                        },
                    ));
                }
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: src_width,
                    },
                ));
            }
            value
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                src_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            dst_width,
        );
        let round = if embedded_control && from == VecElementType::F64 {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let conversion_hint = if embedded_control {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: instruction_width,
                w: prefix.w,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
                report_fp16_denormal: false,
            },
            conversion_hint,
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
