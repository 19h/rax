//! Legacy VEX and EVEX packed/scalar square-root lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

fn evex_sqrt_round_mode(l_bits: u8) -> FpRoundMode {
    match l_bits {
        0 => FpRoundMode::RoundNearest,
        1 => FpRoundMode::RoundDown,
        2 => FpRoundMode::RoundUp,
        _ => FpRoundMode::RoundTowardZero,
    }
}

impl X86_64Lifter {
    pub(crate) fn lift_vec_sqrt(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() <= prefix.bytes {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.bytes + 1,
            });
        }
        if prefix.map != X86VecMap::Map0F || bytes[prefix.bytes] != 0x51 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) {
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
            let embedded_rounding = prefix.encoding == VecEncodingKind::Evex && prefix.b;
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
            let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
            let src = if modrm.is_memory {
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
                if let Some(cond) = mask_cond {
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
                            width: if elem == VecElementType::F32 {
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
                            width: if elem == VecElementType::F32 {
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
                        dst: vector,
                        scalar,
                        elem,
                        lanes: 1,
                    },
                ));
                vector
            } else {
                let source = self.xmm(
                    modrm.rm
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                            16
                        } else {
                            0
                        },
                );
                if let Some(cond) = mask_cond {
                    // A masked-off scalar source must not participate in the
                    // computation or report SIMD floating-point exceptions.
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VExtractLane {
                            dst: scalar,
                            vec: source,
                            lane: 0,
                            elem,
                            sign: SignExtend::Zero,
                        },
                    ));
                    let scalar = self.append_evex_scalar_select(
                        VecPrefix {
                            zeroing: true,
                            ..operation_prefix
                        },
                        Some(cond),
                        source,
                        scalar,
                        elem,
                        pc,
                        ctx,
                        &mut ops,
                    );
                    let sanitized = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VBroadcast {
                            dst: sanitized,
                            scalar,
                            elem,
                            lanes: 1,
                        },
                    ));
                    sanitized
                } else {
                    source
                }
            };
            let vector_result = ctx.alloc_vreg();
            let scalar_result = ctx.alloc_vreg();
            let (round, suppress_exceptions) = if embedded_rounding {
                (evex_sqrt_round_mode(prefix.l_bits), true)
            } else {
                (FpRoundMode::Dynamic, false)
            };
            let sqrt = OpKind::X86Sqrt {
                dst: vector_result,
                src,
                elem,
                lanes: 1,
                round,
                suppress_exceptions,
            };
            ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, sqrt, hint));
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
                mask_cond,
                dst,
                scalar_result,
                elem,
                pc,
                ctx,
                &mut ops,
            );
            self.append_vex_scalar_result(dst, src1, scalar_result, elem, pc, ctx, &mut ops);
            return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
        }

        let elem = if prefix.pp == X86SsePrefix::OpSize {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        if prefix.vvvv != 0
            || prefix.v_high
            || (prefix.zeroing && prefix.aaa == 0)
            || (prefix.encoding == VecEncodingKind::Evex
                && ((elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let embedded_rounding =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if prefix.encoding == VecEncodingKind::Evex && prefix.l_bits == 3 && !embedded_rounding {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let operation_prefix = if embedded_rounding {
            VecPrefix {
                width: VecWidth::V512,
                ..prefix
            }
        } else {
            prefix
        };
        let hint = self.vec_hint(operation_prefix, opcode);

        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            operation_prefix.width,
        );
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let src = if modrm.is_memory {
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
                    let vector = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: vector,
                            addr,
                            width: prefix.width,
                        },
                    ));
                    vector
                }
            }
        } else {
            let source = self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                operation_prefix.width,
            );
            if mask.is_some() {
                // Masked-off packed floating-point elements must not
                // participate in computation or raise SIMD exceptions.
                let sanitized = ctx.alloc_vreg();
                self.append_evex_vector_mask_result(
                    VecPrefix {
                        zeroing: true,
                        ..operation_prefix
                    },
                    sanitized,
                    source,
                    elem,
                    pc,
                    ctx,
                    &mut ops,
                );
                sanitized
            } else {
                source
            }
        };
        let raw = if mask.is_some() {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let (round, suppress_exceptions) = if embedded_rounding {
            (evex_sqrt_round_mode(prefix.l_bits), true)
        } else {
            (FpRoundMode::Dynamic, false)
        };
        let sqrt = OpKind::X86Sqrt {
            dst: raw,
            src,
            elem,
            lanes: operation_prefix.width.lanes(elem) as u8,
            round,
            suppress_exceptions,
        };
        ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, sqrt, hint));
        if mask.is_some() {
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
