//! Shared VEX/EVEX binary floating-point lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

pub(crate) fn x86_fp_binary_operation(opcode: u8) -> Option<X86FpBinaryOp> {
    match opcode {
        0x58 => Some(X86FpBinaryOp::Add),
        0x59 => Some(X86FpBinaryOp::Mul),
        0x5C => Some(X86FpBinaryOp::Sub),
        0x5D => Some(X86FpBinaryOp::Min),
        0x5E => Some(X86FpBinaryOp::Div),
        0x5F => Some(X86FpBinaryOp::Max),
        _ => None,
    }
}

fn evex_fp_binary_round_mode(l_bits: u8) -> FpRoundMode {
    match l_bits {
        0 => FpRoundMode::RoundNearest,
        1 => FpRoundMode::RoundDown,
        2 => FpRoundMode::RoundUp,
        _ => FpRoundMode::RoundTowardZero,
    }
}

impl X86_64Lifter {
    pub(crate) fn lift_vec_scalar_fp_arithmetic(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
            || !matches!(opcode, 0x58 | 0x59 | 0x5C..=0x5F)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let elem = if prefix.pp == X86SsePrefix::Rep {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        if prefix.encoding == VecEncodingKind::Evex
            && ((prefix.zeroing && prefix.aaa == 0)
                || (elem == VecElementType::F32 && prefix.w)
                || (elem == VecElementType::F64 && !prefix.w)
                || (prefix.b && modrm.is_memory))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let operation_prefix = VecPrefix {
            width: VecWidth::V128,
            ..prefix
        };
        let hint = self.vec_hint(operation_prefix, opcode);
        let dst = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let src1 = self.xmm(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
        );
        let mask = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
        let mem_width = if elem == VecElementType::F32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                elem,
                ctx,
            );
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
            if let Some(cond) = mask {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: scalar,
                        src: SrcOperand::Imm(0),
                        width: if elem == VecElementType::F32 {
                            OpWidth::W32
                        } else {
                            OpWidth::W64
                        },
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond,
                        addr,
                        width: mem_width,
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
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem,
                    lanes: 1,
                },
            ));
            vector
        } else {
            self.xmm(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
            )
        };

        let arithmetic = matches!(opcode, 0x58 | 0x59 | 0x5C | 0x5E);
        let embedded_control = prefix.encoding == VecEncodingKind::Evex && prefix.b;
        let round = if arithmetic && embedded_control {
            evex_fp_binary_round_mode(prefix.l_bits)
        } else {
            FpRoundMode::Dynamic
        };
        let operation =
            x86_fp_binary_operation(opcode).ok_or_else(|| LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            })?;
        let vector_result = ctx.alloc_vreg();
        let scalar_result = ctx.alloc_vreg();
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpBinary {
                dst: vector_result,
                src1,
                src2,
                mask,
                elem,
                lanes: 1,
                op: operation,
                round,
                suppress_exceptions: embedded_control,
            },
            hint,
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: scalar_result,
                vec: vector_result,
                lane: 0,
                elem,
                sign: SignExtend::Zero,
            },
        ));
        let scalar_result = self.append_evex_scalar_select(
            prefix,
            mask,
            dst,
            scalar_result,
            elem,
            pc,
            ctx,
            &mut ops,
        );
        self.append_vex_scalar_result(dst, src1, scalar_result, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    /// Lift packed VEX/EVEX binary32/binary64 arithmetic with exact MXCSR,
    /// write-mask, broadcast, SAE, and embedded-rounding behavior.
    pub(crate) fn lift_vec_packed_fp_arithmetic(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let operation =
            x86_fp_binary_operation(opcode).ok_or_else(|| LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            })?;
        if prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.pp == X86SsePrefix::None {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        if prefix.encoding == VecEncodingKind::Evex
            && ((elem == VecElementType::F32 && prefix.w)
                || (elem == VecElementType::F64 && !prefix.w))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let embedded_control =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        let arithmetic = matches!(
            operation,
            X86FpBinaryOp::Add | X86FpBinaryOp::Sub | X86FpBinaryOp::Mul | X86FpBinaryOp::Div
        );
        if prefix.encoding == VecEncodingKind::Evex && prefix.l_bits == 3 && !embedded_control {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        // Register EVEX.b repurposes L'L as RC for arithmetic. The operation
        // remains 512 bits wide for all four RC encodings. MIN/MAX use the
        // same implied 512-bit length and SAE; RC is immaterial to their
        // non-rounding semantics, so all four L'L bit patterns remain valid.
        let operation_prefix = if embedded_control {
            VecPrefix {
                width: VecWidth::V512,
                ..prefix
            }
        } else {
            prefix
        };
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let hint = self.vec_hint(operation_prefix, opcode);
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if broadcast {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem.bytes(),
                    ctx,
                )
            } else {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            match (mask, broadcast) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
                }
                (None, false) => {
                    let loaded = ctx.alloc_vreg();
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: loaded,
                            addr,
                            width: prefix.width,
                        },
                        X86OpHint::VecAlign(X86VecAlign::Unaligned),
                    ));
                    loaded
                }
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                operation_prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            operation_prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            operation_prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let round = if arithmetic && embedded_control {
            evex_fp_binary_round_mode(prefix.l_bits)
        } else {
            FpRoundMode::Dynamic
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpBinary {
                dst: raw,
                src1,
                src2,
                mask,
                elem,
                lanes: operation_prefix.width.lanes(elem) as u8,
                op: operation,
                round,
                suppress_exceptions: embedded_control,
            },
            hint,
        ));
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(
                operation_prefix,
                dst,
                raw,
                elem,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
