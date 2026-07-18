//! Top-level opcode map dispatch (legacy, 0F, 0F38, 0F3A, VEX/EVEX)

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {

    /// Lift 0F 38-prefixed (three-byte) opcodes
    pub(crate) fn lift_0f38_opcode(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + 1,
                need: prefix.cursor + 2,
            });
        }

        let opcode3 = bytes[0];
        let after_opcode = &bytes[1..];
        let prefix3 = X86Prefix {
            cursor: prefix.cursor + 1,
            ..prefix.clone()
        };

        match opcode3 {
            0x00 => self.lift_sse_pshufb(after_opcode, &prefix3, pc, ctx),
            0x01..=0x03 | 0x05..=0x07 => {
                self.lift_sse_horizontal_integer(opcode3, after_opcode, &prefix3, pc, ctx)
            }
            0x04 => self.lift_sse_pmaddubsw(after_opcode, &prefix3, pc, ctx),
            0x08..=0x0A => self.lift_sse_psign(opcode3, after_opcode, &prefix3, pc, ctx),
            0x0B => self.lift_sse_pmulhrsw(after_opcode, &prefix3, pc, ctx),
            0x10 | 0x14 | 0x15 => {
                self.lift_sse_variable_blend(opcode3, after_opcode, &prefix3, pc, ctx)
            }
            0x17 => self.lift_sse_ptest(after_opcode, &prefix3, pc, ctx),
            0x1C..=0x1E => self.lift_sse_pabs(opcode3, after_opcode, &prefix3, pc, ctx),
            0x20..=0x25 | 0x30..=0x35 => {
                self.lift_sse_packed_extend(opcode3, after_opcode, &prefix3, pc, ctx)
            }
            0x28 => self.lift_sse_pmuldq(after_opcode, &prefix3, true, pc, ctx),
            0x2A => self.lift_sse_movntdqa(after_opcode, &prefix3, pc, ctx),
            0x2B => self.lift_sse_integer_pack(opcode3, after_opcode, &prefix3, pc, ctx),
            0x29 | 0x37 => self.lift_sse_integer_compare(opcode3, after_opcode, &prefix3, pc, ctx),
            0x38..=0x3F => self.lift_sse_packed_minmax(opcode3, after_opcode, &prefix3, pc, ctx),
            0x40 => self.lift_sse_pmulld(after_opcode, &prefix3, pc, ctx),
            0x41 => self.lift_sse_phminposuw(after_opcode, &prefix3, pc, ctx),
            0xCF => self.lift_sse_gfni(opcode3, after_opcode, &prefix3, false, pc, ctx),
            0xDB..=0xDF => self.lift_sse_aes_round(opcode3, after_opcode, &prefix3, pc, ctx),
            0x8A | 0x8B => self.lift_movrs_0f38(opcode3, after_opcode, &prefix3, pc, ctx),
            0xF6 => self.lift_adx_0f38(after_opcode, &prefix3, pc, ctx),
            0xF0 | 0xF1 if prefix3.rep_prefix == Some(0xF2) => {
                self.lift_crc32_0f38(opcode3, after_opcode, &prefix3, pc, ctx)
            }
            0xF0 | 0xF1 => self.lift_movbe_0f38(opcode3, after_opcode, &prefix3, pc, ctx),
            0xFC => self.lift_rao_int_0f38(after_opcode, &prefix3, pc, ctx),
            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x0F 0x38 0x{:02X}", opcode3),
                    })
                } else {
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix3.cursor,
                    ))
                }
            }
        }
    }


    pub(crate) fn lift_0f3a_opcode(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + 1,
                need: prefix.cursor + 2,
            });
        }
        let opcode = bytes[0];
        let after_opcode = &bytes[1..];
        let prefix3 = X86Prefix {
            cursor: prefix.cursor + 1,
            ..prefix.clone()
        };
        match opcode {
            0x08..=0x0B => self.lift_sse_round(opcode, after_opcode, &prefix3, pc, ctx),
            0x0C..=0x0E => self.lift_sse_immediate_blend(opcode, after_opcode, &prefix3, pc, ctx),
            0x0F => self.lift_sse_palignr(after_opcode, &prefix3, pc, ctx),
            0x14..=0x17 => self.lift_sse_extract_0f3a(opcode, after_opcode, &prefix3, pc, ctx),
            0x20..=0x22 => self.lift_sse_insert_0f3a(opcode, after_opcode, &prefix3, pc, ctx),
            0x40 | 0x41 => self.lift_sse_dot_product(opcode, after_opcode, &prefix3, pc, ctx),
            0x42 => self.lift_sse_mpsadbw(after_opcode, &prefix3, pc, ctx),
            0x44 => self.lift_sse_pclmulqdq(after_opcode, &prefix3, pc, ctx),
            0xCE | 0xCF => self.lift_sse_gfni(opcode, after_opcode, &prefix3, true, pc, ctx),
            0xDF => self.lift_sse_aes_keygen(after_opcode, &prefix3, pc, ctx),
            _ if self.strict => Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("0x0F 0x3A 0x{opcode:02X}"),
            }),
            _ => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                prefix3.cursor,
            )),
        }
    }


    pub(crate) fn lift_vec_opcode(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < prefix.bytes + 1 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.bytes + 1,
            });
        }

        let opcode = bytes[prefix.bytes];
        let after_opcode = &bytes[prefix.bytes + 1..];
        let cursor = prefix.bytes + 1;
        let prefix_modrm = X86Prefix {
            rex: prefix.rex,
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            cursor,
            ..X86Prefix::default()
        };

        let mut ops = Vec::new();
        let hint = self.vec_hint(prefix, opcode);

        match prefix.map {
            X86VecMap::Map0F => match opcode {
                // Packed integer/FP32/FP64 conversions. The six 0F 5B/E6
                // families have VEX encodings; the unsigned and I64 result
                // families are EVEX-only.
                0x5B | 0xE6 => self.lift_vec_packed_int_fp_convert(prefix, bytes, pc, ctx),
                0x78 | 0x79
                    if prefix.encoding == VecEncodingKind::Evex
                        && matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
                {
                    self.lift_vec_packed_int_fp_convert(prefix, bytes, pc, ctx)
                }
                0x7A if prefix.encoding == VecEncodingKind::Evex
                    && matches!(
                        prefix.pp,
                        X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
                    ) =>
                {
                    self.lift_vec_packed_int_fp_convert(prefix, bytes, pc, ctx)
                }
                0x7B if prefix.encoding == VecEncodingKind::Evex
                    && prefix.pp == X86SsePrefix::OpSize =>
                {
                    self.lift_vec_packed_int_fp_convert(prefix, bytes, pc, ctx)
                }
                0x58 | 0x59 | 0x5C..=0x5F
                    if prefix.encoding == VecEncodingKind::Evex
                        && matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
                {
                    self.lift_evex_packed_fp_arithmetic(prefix, opcode, bytes, pc, ctx)
                }
                0xF6 => self.lift_vec_psadbw(prefix, bytes, pc, ctx),
                0xC4 | 0xC5 => self.lift_vec_pinsrw_pextrw(prefix, opcode, bytes, pc, ctx),
                0x63 | 0x67 | 0x6B if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_integer_pack(prefix, opcode, bytes, pc, ctx)
                }
                0x63 | 0x67 | 0x6B if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_integer_pack(prefix, opcode, bytes, pc, ctx)
                }
                0x60 | 0x61 | 0x62 | 0x68 | 0x69 | 0x6A | 0x6C | 0x6D
                    if prefix.encoding == VecEncodingKind::Vex =>
                {
                    self.lift_vex_integer_unpack(prefix, opcode, bytes, pc, ctx)
                }
                0x60 | 0x61 | 0x62 | 0x68 | 0x69 | 0x6A | 0x6C | 0x6D
                    if prefix.encoding == VecEncodingKind::Evex =>
                {
                    self.lift_evex_integer_unpack(prefix, opcode, bytes, pc, ctx)
                }

                0x64 | 0x65 | 0x66 | 0x74 | 0x75 | 0x76
                    if prefix.encoding == VecEncodingKind::Vex =>
                {
                    self.lift_vex_integer_compare(prefix, opcode, bytes, pc, ctx)
                }
                0x64 | 0x65 | 0x66 | 0x74 | 0x75 | 0x76
                    if prefix.encoding == VecEncodingKind::Evex =>
                {
                    self.lift_evex_integer_compare(prefix, opcode, bytes, pc, ctx)
                }

                // VMOVQ scalar vector load/store forms. VEX encodings are WIG;
                // the EVEX encodings require W=1. Both are fixed at 128-bit
                // vector length and reserve masking, broadcast, and vvvv/V'.
                0x7E if prefix.pp == X86SsePrefix::Rep => {
                    if prefix.l_bits != 0
                        || prefix.vvvv != 0
                        || prefix.v_high
                        || prefix.aaa != 0
                        || prefix.zeroing
                        || prefix.b
                        || (prefix.encoding == VecEncodingKind::Evex && !prefix.w)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let dst = self.xmm(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                    );
                    let scalar = if modrm.is_memory {
                        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            8,
                            ctx,
                        );
                        ops.extend(pre_ops);
                        let scalar = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Load {
                                dst: scalar,
                                addr,
                                width: MemWidth::B8,
                                sign: SignExtend::Zero,
                            },
                        ));
                        scalar
                    } else {
                        let src = self.xmm(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                        );
                        let scalar = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VExtractLane {
                                dst: scalar,
                                vec: src,
                                lane: 0,
                                elem: VecElementType::I64,
                                sign: SignExtend::Zero,
                            },
                        ));
                        scalar
                    };
                    self.append_scalar_zeroed_xmm_result(
                        dst,
                        scalar,
                        VecElementType::I64,
                        true,
                        pc,
                        ctx,
                        &mut ops,
                    );
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                0xD6 if prefix.pp == X86SsePrefix::OpSize => {
                    if prefix.l_bits != 0
                        || prefix.vvvv != 0
                        || prefix.v_high
                        || prefix.aaa != 0
                        || prefix.zeroing
                        || prefix.b
                        || (prefix.encoding == VecEncodingKind::Evex && !prefix.w)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let src = self.xmm(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                    );
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VExtractLane {
                            dst: scalar,
                            vec: src,
                            lane: 0,
                            elem: VecElementType::I64,
                            sign: SignExtend::Zero,
                        },
                    ));
                    if modrm.is_memory {
                        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            8,
                            ctx,
                        );
                        ops.extend(pre_ops);
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Store {
                                src: scalar,
                                addr,
                                width: MemWidth::B8,
                            },
                        ));
                    } else {
                        let dst = self.xmm(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                        );
                        self.append_scalar_zeroed_xmm_result(
                            dst,
                            scalar,
                            VecElementType::I64,
                            true,
                            pc,
                            ctx,
                            &mut ops,
                        );
                    }
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VMOVD/VMOVQ between XMM and GPR/memory operands. These
                // encodings are scalar-width, unmasked, and require L=0 and
                // the reserved vvvv/V' fields to encode all ones.
                0x6E | 0x7E => {
                    if prefix.pp != X86SsePrefix::OpSize
                        || prefix.l_bits != 0
                        || prefix.vvvv != 0
                        || prefix.v_high
                        || prefix.aaa != 0
                        || prefix.zeroing
                        || prefix.b
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    if prefix.encoding == VecEncodingKind::Evex
                        && !modrm.is_memory
                        && prefix.rm_high
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let (elem, op_width, mem_width) = if prefix.w {
                        (VecElementType::I64, OpWidth::W64, MemWidth::B8)
                    } else {
                        (VecElementType::I32, OpWidth::W32, MemWidth::B4)
                    };
                    let xmm = self.xmm(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                    );

                    if !modrm.is_memory {
                        let (dst, src, zero_upper) = if opcode == 0x6E {
                            (xmm, self.gpr(modrm.rm), true)
                        } else {
                            (self.gpr(modrm.rm), xmm, false)
                        };
                        ops.push(SmirOp::with_hint(
                            OpId(0),
                            pc,
                            OpKind::X86MovdQ {
                                dst,
                                src,
                                width: op_width,
                                zero_upper,
                            },
                            hint,
                        ));
                    } else if opcode == 0x6E {
                        let scalar = {
                            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                                prefix,
                                modrm.addr.as_ref().unwrap(),
                                next_pc,
                                mem_width.bytes(),
                                ctx,
                            );
                            ops.extend(pre_ops);
                            let scalar = ctx.alloc_vreg();
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
                            scalar
                        };
                        self.append_scalar_zeroed_xmm_result(
                            xmm, scalar, elem, true, pc, ctx, &mut ops,
                        );
                    } else {
                        let scalar = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VExtractLane {
                                dst: scalar,
                                vec: xmm,
                                lane: 0,
                                elem,
                                sign: SignExtend::Zero,
                            },
                        ));
                        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            mem_width.bytes(),
                            ctx,
                        );
                        ops.extend(pre_ops);
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Store {
                                src: scalar,
                                addr,
                                width: mem_width,
                            },
                        ));
                    }

                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VMOVMSKPS/VMOVMSKPD. VEX.vvvv is reserved as 1111b and
                // ModR/M must select a vector register source.
                0x50 => {
                    if prefix.encoding != VecEncodingKind::Vex
                        || prefix.vvvv != 0
                        || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    if modrm.is_memory {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let elem = if prefix.pp == X86SsePrefix::OpSize {
                        VecElementType::F64
                    } else {
                        VecElementType::F32
                    };
                    let src = self.vec_reg(modrm.rm, prefix.width);
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86MovMask {
                            dst: self.gpr(modrm.reg),
                            src,
                            elem,
                            lanes: prefix.width.lanes(elem) as u8,
                            dst_width: OpWidth::W32,
                        },
                        X86OpHint::VexOp {
                            map: X86VecMap::Map0F,
                            pp: prefix.pp,
                            opcode: 0x50,
                            width: prefix.width,
                            w: prefix.w,
                        },
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VLDDQU xmm/ymm, m128/m256.
                0xF0 => {
                    if prefix.encoding != VecEncodingKind::Vex
                        || prefix.pp != X86SsePrefix::Repne
                        || prefix.vvvv != 0
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    if !modrm.is_memory {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let (addr, pre_ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                    ops.extend(pre_ops);
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: self.vec_reg(modrm.reg, prefix.width),
                            addr,
                            width: prefix.width,
                        },
                        hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Scalar GPR/memory-to-FP32/FP64 conversions. Opcode 2A is
                // signed; opcode 7B is the EVEX-only unsigned family.
                0x2A | 0x7B => {
                    let signed = opcode == 0x2A;
                    if !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
                        || (!signed && prefix.encoding != VecEncodingKind::Evex)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let elem = if prefix.pp == X86SsePrefix::Rep {
                        VecElementType::F32
                    } else {
                        VecElementType::F64
                    };
                    let int_width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
                    let mem_width = if int_width == OpWidth::W64 {
                        MemWidth::B8
                    } else {
                        MemWidth::B4
                    };
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    if prefix.encoding == VecEncodingKind::Evex
                        && ((prefix.b && modrm.is_memory) || (!modrm.is_memory && prefix.rm_high))
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let src = if modrm.is_memory {
                        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            mem_width.bytes(),
                            ctx,
                        );
                        ops.extend(pre_ops);
                        let value = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Load {
                                dst: value,
                                addr,
                                width: mem_width,
                                sign: if signed {
                                    SignExtend::Sign
                                } else {
                                    SignExtend::Zero
                                },
                            },
                        ));
                        value
                    } else {
                        self.gpr(modrm.rm)
                    };
                    let dst = self.xmm(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                    );
                    let merge = self.xmm(
                        prefix.vvvv
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                                16
                            } else {
                                0
                            },
                    );
                    // CVT(U)SI2SD with W=0 is exact for every 32-bit source;
                    // Intel specifies that attempted EVEX embedded rounding is
                    // ignored for this encoding.
                    let embedded_rounding = prefix.encoding == VecEncodingKind::Evex
                        && prefix.b
                        && !(elem == VecElementType::F64 && !prefix.w);
                    let round = if embedded_rounding {
                        match prefix.l_bits {
                            0 => FpRoundMode::RoundNearest,
                            1 => FpRoundMode::RoundDown,
                            2 => FpRoundMode::RoundUp,
                            _ => FpRoundMode::RoundTowardZero,
                        }
                    } else {
                        FpRoundMode::Dynamic
                    };
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86IntToFp {
                            dst,
                            merge,
                            src,
                            elem,
                            int_width,
                            signed,
                            round,
                            suppress_exceptions: embedded_rounding,
                            zero_upper: true,
                        },
                        hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Scalar FP32/FP64-to-GPR conversions. Opcodes 2C/2D are the
                // signed VEX/EVEX forms; 78/79 are EVEX-only unsigned forms.
                0x2C | 0x2D | 0x78 | 0x79 => {
                    let signed = matches!(opcode, 0x2C | 0x2D);
                    let truncate = matches!(opcode, 0x2C | 0x78);
                    if prefix.vvvv != 0
                        || prefix.v_high
                        || prefix.reg_high
                        || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
                        || (!signed && prefix.encoding != VecEncodingKind::Evex)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let elem = if prefix.pp == X86SsePrefix::Rep {
                        VecElementType::F32
                    } else {
                        VecElementType::F64
                    };
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    if prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
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
                        scalar
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
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86FpToInt {
                            dst: self.gpr(modrm.reg),
                            src,
                            elem,
                            int_width: if prefix.w { OpWidth::W64 } else { OpWidth::W32 },
                            signed,
                            truncate,
                            round: if truncate {
                                FpRoundMode::RoundTowardZero
                            } else if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                                match prefix.l_bits {
                                    0 => FpRoundMode::RoundNearest,
                                    1 => FpRoundMode::RoundDown,
                                    2 => FpRoundMode::RoundUp,
                                    _ => FpRoundMode::RoundTowardZero,
                                }
                            } else {
                                FpRoundMode::Dynamic
                            },
                            suppress_exceptions: prefix.encoding == VecEncodingKind::Evex
                                && prefix.b,
                        },
                        hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VCOMISS/VUCOMISS/VCOMISD/VUCOMISD.
                0x2E | 0x2F => {
                    if prefix.vvvv != 0
                        || prefix.v_high
                        || matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let elem = if prefix.pp == X86SsePrefix::OpSize {
                        VecElementType::F64
                    } else {
                        VecElementType::F32
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
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let src1 = self.xmm(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                    );
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
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86FpCompare {
                            src1,
                            src2,
                            elem,
                            signaling: opcode == 0x2F,
                        },
                        hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VEX VMOVUPS/VMOVUPD and scalar VMOVSS/VMOVSD. Scalar register
                // forms merge XMM lanes from vvvv; scalar memory forms reserve
                // vvvv and zero everything above the loaded lane.
                0x10 | 0x11 => {
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let scalar_elem = match prefix.pp {
                        X86SsePrefix::Rep => Some(VecElementType::F32),
                        X86SsePrefix::Repne => Some(VecElementType::F64),
                        _ => None,
                    };

                    if let Some(elem) = scalar_elem {
                        if prefix.encoding == VecEncodingKind::Evex
                            && ((prefix.zeroing && prefix.aaa == 0)
                                || prefix.b
                                || (elem == VecElementType::F32 && prefix.w)
                                || (elem == VecElementType::F64 && !prefix.w))
                        {
                            return Err(LiftError::InvalidEncoding {
                                addr: pc,
                                bytes: bytes.to_vec(),
                            });
                        }
                        let mem_width = if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        };
                        let reg_index = modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            };
                        let rm_index = modrm.rm
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                16
                            } else {
                                0
                            };
                        let v_index = prefix.vvvv
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                                16
                            } else {
                                0
                            };
                        let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
                        if modrm.is_memory {
                            if prefix.vvvv != 0 || prefix.v_high {
                                return Err(LiftError::InvalidEncoding {
                                    addr: pc,
                                    bytes: bytes.to_vec(),
                                });
                            }
                            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                                prefix,
                                modrm.addr.as_ref().unwrap(),
                                next_pc,
                                elem,
                                ctx,
                            );
                            ops.extend(pre_ops);
                            if opcode == 0x10 {
                                let scalar = ctx.alloc_vreg();
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
                                let scalar = self.append_evex_scalar_select(
                                    prefix,
                                    mask_cond,
                                    self.xmm(reg_index),
                                    scalar,
                                    elem,
                                    pc,
                                    ctx,
                                    &mut ops,
                                );
                                ops.push(SmirOp::with_hint(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::VBroadcast {
                                        dst: self.xmm(reg_index),
                                        scalar,
                                        elem,
                                        lanes: 1,
                                    },
                                    hint,
                                ));
                            } else {
                                if prefix.zeroing {
                                    return Err(LiftError::InvalidEncoding {
                                        addr: pc,
                                        bytes: bytes.to_vec(),
                                    });
                                }
                                let scalar = ctx.alloc_vreg();
                                ops.push(SmirOp::new(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::VExtractLane {
                                        dst: scalar,
                                        vec: self.xmm(reg_index),
                                        lane: 0,
                                        elem,
                                        sign: SignExtend::Zero,
                                    },
                                ));
                                if let Some(cond) = mask_cond {
                                    ops.push(SmirOp::with_hint(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::PredStore {
                                            src: SrcOperand::Reg(scalar),
                                            cond,
                                            addr,
                                            width: mem_width,
                                        },
                                        hint,
                                    ));
                                } else {
                                    ops.push(SmirOp::with_hint(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::Store {
                                            src: scalar,
                                            addr,
                                            width: mem_width,
                                        },
                                        hint,
                                    ));
                                }
                            }
                        } else {
                            let low_src = if opcode == 0x10 {
                                self.xmm(rm_index)
                            } else {
                                self.xmm(reg_index)
                            };
                            let dst = if opcode == 0x10 {
                                self.xmm(reg_index)
                            } else {
                                self.xmm(rm_index)
                            };
                            let scalar = ctx.alloc_vreg();
                            ops.push(SmirOp::new(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::VExtractLane {
                                    dst: scalar,
                                    vec: low_src,
                                    lane: 0,
                                    elem,
                                    sign: SignExtend::Zero,
                                },
                            ));
                            let scalar = self.append_evex_scalar_select(
                                prefix, mask_cond, dst, scalar, elem, pc, ctx, &mut ops,
                            );
                            self.append_vex_scalar_result(
                                dst,
                                self.xmm(v_index),
                                scalar,
                                elem,
                                pc,
                                ctx,
                                &mut ops,
                            );
                        }
                        return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
                    }

                    let elem = if prefix.pp == X86SsePrefix::OpSize {
                        VecElementType::F64
                    } else {
                        VecElementType::F32
                    };
                    if prefix.vvvv != 0
                        || (prefix.encoding == VecEncodingKind::Evex
                            && (prefix.v_high
                                || prefix.l_bits == 3
                                || prefix.b
                                || (prefix.zeroing && prefix.aaa == 0)
                                || prefix.w != (elem == VecElementType::F64)))
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let reg = self.vec_reg(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let rm = self.vec_reg(
                        modrm.rm
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
                    if opcode == 0x10 {
                        if modrm.is_memory {
                            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                                self.vec_full_addr_to_smir(
                                    prefix,
                                    modrm.addr.as_ref().unwrap(),
                                    next_pc,
                                    ctx,
                                )
                            } else {
                                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
                            };
                            ops.extend(pre_ops);
                            if let Some(mask) = mask {
                                let raw = self.append_evex_masked_vector_source(
                                    addr,
                                    elem,
                                    prefix.width,
                                    false,
                                    mask,
                                    pc,
                                    ctx,
                                    &mut ops,
                                );
                                self.append_evex_vector_mask_result(
                                    prefix, reg, raw, elem, pc, ctx, &mut ops,
                                );
                            } else {
                                ops.push(SmirOp::with_hint(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::VLoad {
                                        dst: reg,
                                        addr,
                                        width: prefix.width,
                                    },
                                    hint,
                                ));
                            }
                        } else if mask.is_some() {
                            self.append_evex_vector_mask_result(
                                prefix, reg, rm, elem, pc, ctx, &mut ops,
                            );
                        } else {
                            ops.push(SmirOp::with_hint(
                                OpId(0),
                                pc,
                                OpKind::VAnd {
                                    dst: reg,
                                    src1: rm,
                                    src2: rm,
                                    width: prefix.width,
                                },
                                hint,
                            ));
                        }
                    } else if modrm.is_memory {
                        if prefix.zeroing {
                            return Err(LiftError::InvalidEncoding {
                                addr: pc,
                                bytes: bytes.to_vec(),
                            });
                        }
                        let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                            self.vec_full_addr_to_smir(
                                prefix,
                                modrm.addr.as_ref().unwrap(),
                                next_pc,
                                ctx,
                            )
                        } else {
                            self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
                        };
                        ops.extend(pre_ops);
                        if let Some(mask) = mask {
                            self.append_evex_masked_vector_store(
                                addr,
                                reg,
                                elem,
                                prefix.width,
                                mask,
                                pc,
                                ctx,
                                &mut ops,
                            );
                        } else {
                            ops.push(SmirOp::with_hint(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::VStore {
                                    src: reg,
                                    addr,
                                    width: prefix.width,
                                },
                                hint,
                            ));
                        }
                    } else if mask.is_some() {
                        self.append_evex_vector_mask_result(
                            prefix, rm, reg, elem, pc, ctx, &mut ops,
                        );
                    } else {
                        ops.push(SmirOp::with_hint(
                            OpId(0),
                            pc,
                            OpKind::VAnd {
                                dst: rm,
                                src1: reg,
                                src2: reg,
                                width: prefix.width,
                            },
                            hint,
                        ));
                    }
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Packed VEX/EVEX VCVTPS2PD/VCVTPD2PS.
                0x5A if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) => {
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
                    let embedded_rounding =
                        prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
                    if (prefix.encoding == VecEncodingKind::Evex
                        && !embedded_rounding
                        && prefix.l_bits == 3)
                        || (embedded_rounding && from != VecElementType::F64)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let instruction_width = if embedded_rounding {
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
                                let cond = ctx.alloc_vreg();
                                ops.push(SmirOp::new(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::And {
                                        dst: cond,
                                        src1: mask_reg,
                                        src2: SrcOperand::Imm((1i64 << lanes) - 1),
                                        width: OpWidth::W64,
                                        flags: FlagUpdate::None,
                                    },
                                ));
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
                    let round = if embedded_rounding {
                        match prefix.l_bits {
                            0 => FpRoundMode::RoundNearest,
                            1 => FpRoundMode::RoundDown,
                            2 => FpRoundMode::RoundUp,
                            _ => FpRoundMode::RoundTowardZero,
                        }
                    } else {
                        FpRoundMode::Dynamic
                    };
                    let conversion_hint = if embedded_rounding {
                        X86OpHint::EvexOp {
                            map: prefix.map,
                            pp: prefix.pp,
                            opcode,
                            width: instruction_width,
                            w: prefix.w,
                        }
                    } else {
                        hint
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
                            suppress_exceptions: embedded_rounding,
                            report_fp16_denormal: false,
                        },
                        conversion_hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Scalar VCVTSS2SD/VCVTSD2SS.
                0x5A if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                    let (from, to) = if prefix.pp == X86SsePrefix::Rep {
                        (VecElementType::F32, VecElementType::F64)
                    } else {
                        (VecElementType::F64, VecElementType::F32)
                    };
                    self.lift_vec_scalar_fp_convert(prefix, bytes, pc, ctx, from, to)
                }

                // Packed and scalar VSQRTPS/VSQRTPD/VSQRTSS/VSQRTSD.
                0x51 => {
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: "EVEX square-root broadcast / embedded-rounding".to_string(),
                        });
                    }
                    if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) {
                        let elem = if prefix.pp == X86SsePrefix::Rep {
                            VecElementType::F32
                        } else {
                            VecElementType::F64
                        };
                        if prefix.encoding == VecEncodingKind::Evex
                            && ((prefix.zeroing && prefix.aaa == 0)
                                || (elem == VecElementType::F32 && prefix.w)
                                || (elem == VecElementType::F64 && !prefix.w))
                        {
                            return Err(LiftError::InvalidEncoding {
                                addr: pc,
                                bytes: bytes.to_vec(),
                            });
                        }
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
                            self.xmm(
                                modrm.rm
                                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high
                                    {
                                        16
                                    } else {
                                        0
                                    },
                            )
                        };
                        let vector_result = ctx.alloc_vreg();
                        let scalar_result = ctx.alloc_vreg();
                        ops.push(SmirOp::with_hint(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VUnary {
                                dst: vector_result,
                                src,
                                elem,
                                lanes: 1,
                                op: VecUnaryOp::FSqrt,
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
                            mask_cond,
                            dst,
                            scalar_result,
                            elem,
                            pc,
                            ctx,
                            &mut ops,
                        );
                        self.append_vex_scalar_result(
                            dst,
                            src1,
                            scalar_result,
                            elem,
                            pc,
                            ctx,
                            &mut ops,
                        );
                        return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
                    }

                    if prefix.vvvv != 0
                        || prefix.v_high
                        || (prefix.encoding == VecEncodingKind::Evex && prefix.l_bits == 3)
                        || (prefix.zeroing && prefix.aaa == 0)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let elem = if prefix.pp == X86SsePrefix::OpSize {
                        VecElementType::F64
                    } else {
                        VecElementType::F32
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
                    let dst = self.vec_reg(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
                    let src = if modrm.is_memory {
                        let (addr, pre_ops) = self.vec_full_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            ctx,
                        );
                        ops.extend(pre_ops);
                        if let Some(mask) = mask {
                            self.append_evex_masked_vector_source(
                                addr,
                                elem,
                                prefix.width,
                                false,
                                mask,
                                pc,
                                ctx,
                                &mut ops,
                            )
                        } else {
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
                    } else {
                        let source = self.vec_reg(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                            prefix.width,
                        );
                        if mask.is_some() {
                            // Masked-off packed floating-point elements must not
                            // participate in computation or raise SIMD exceptions.
                            // Replace them with +0 before the vector square root;
                            // the architectural merge/zero selection is applied
                            // independently to the result below.
                            let sanitized = ctx.alloc_vreg();
                            self.append_evex_vector_mask_result(
                                VecPrefix {
                                    zeroing: true,
                                    ..prefix
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
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VUnary {
                            dst: raw,
                            src,
                            elem,
                            lanes: prefix.width.lanes(elem) as u8,
                            op: VecUnaryOp::FSqrt,
                        },
                        hint,
                    ));
                    if mask.is_some() {
                        self.append_evex_vector_mask_result(
                            prefix, dst, raw, elem, pc, ctx, &mut ops,
                        );
                    }
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Packed floating-point/integer bitwise logic and integer add/subtract.
                0x54 | 0x55 | 0x56 | 0x57 | 0xD4 | 0xD8 | 0xD9 | 0xDB | 0xDC | 0xDD | 0xDF
                | 0xE8 | 0xE9 | 0xEB | 0xEC | 0xED | 0xEF | 0xF8 | 0xF9 | 0xFA | 0xFB | 0xFC
                | 0xFD | 0xFE => {
                    let integer_logic = matches!(opcode, 0xDB | 0xDF | 0xEB | 0xEF);
                    let packed_add = matches!(opcode, 0xD4 | 0xFC | 0xFD | 0xFE);
                    let packed_sub = matches!(opcode, 0xF8 | 0xF9 | 0xFA | 0xFB);
                    let packed_sat = matches!(
                        opcode,
                        0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED
                    );
                    let elem = if packed_add || packed_sub || packed_sat {
                        if prefix.pp != X86SsePrefix::OpSize {
                            return Err(LiftError::InvalidEncoding {
                                addr: pc,
                                bytes: bytes.to_vec(),
                            });
                        }
                        match opcode {
                            0xD8 | 0xDC | 0xE8 | 0xEC | 0xF8 | 0xFC => VecElementType::I8,
                            0xD9 | 0xDD | 0xE9 | 0xED | 0xF9 | 0xFD => VecElementType::I16,
                            0xFA | 0xFE => VecElementType::I32,
                            0xD4 | 0xFB => VecElementType::I64,
                            _ => unreachable!(),
                        }
                    } else if integer_logic {
                        if prefix.pp != X86SsePrefix::OpSize {
                            return Err(LiftError::InvalidEncoding {
                                addr: pc,
                                bytes: bytes.to_vec(),
                            });
                        }
                        if prefix.encoding == VecEncodingKind::Evex && prefix.w {
                            VecElementType::I64
                        } else {
                            VecElementType::I32
                        }
                    } else {
                        match prefix.pp {
                            X86SsePrefix::None => VecElementType::F32,
                            X86SsePrefix::OpSize => VecElementType::F64,
                            _ => {
                                return Err(LiftError::InvalidEncoding {
                                    addr: pc,
                                    bytes: bytes.to_vec(),
                                });
                            }
                        }
                    };
                    if prefix.encoding == VecEncodingKind::Evex
                        && (prefix.l_bits == 3
                            || prefix.zeroing && prefix.aaa == 0
                            || (opcode == 0xFE && prefix.w)
                            || (opcode == 0xD4 && !prefix.w)
                            || (opcode == 0xFA && prefix.w)
                            || (opcode == 0xFB && !prefix.w)
                            || (!integer_logic
                                && !packed_add
                                && !packed_sub
                                && !packed_sat
                                && ((elem == VecElementType::F32 && prefix.w)
                                    || (elem == VecElementType::F64 && !prefix.w))))
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let broadcast_allowed = matches!(
                        opcode,
                        0x54 | 0x55 | 0x56 | 0xD4 | 0xDB | 0xDF | 0xEB | 0xEF | 0xFA | 0xFB | 0xFE
                    );
                    if prefix.encoding == VecEncodingKind::Evex
                        && prefix.b
                        && (!modrm.is_memory || !broadcast_allowed)
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let dst = self.vec_reg(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let src1 = self.vec_reg(
                        prefix.vvvv
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let lanes = prefix.width.lanes(elem) as u8;
                    let elem_mem_width = match elem.bytes() {
                        1 => MemWidth::B1,
                        2 => MemWidth::B2,
                        4 => MemWidth::B4,
                        8 => MemWidth::B8,
                        _ => unreachable!(),
                    };
                    let elem_op_width = match elem.bytes() {
                        1 => OpWidth::W8,
                        2 => OpWidth::W16,
                        4 => OpWidth::W32,
                        8 => OpWidth::W64,
                        _ => unreachable!(),
                    };
                    let mask = if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
                        Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
                    } else {
                        None
                    };
                    let src2 = if modrm.is_memory {
                        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
                        let tuple_bytes = if broadcast {
                            elem.bytes()
                        } else {
                            prefix.width.bytes()
                        };
                        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                            prefix,
                            modrm.addr.as_ref().unwrap(),
                            next_pc,
                            tuple_bytes,
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
                                let cond = ctx.alloc_vreg();
                                ops.push(SmirOp::new(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::And {
                                        dst: cond,
                                        src1: mask_reg,
                                        src2: SrcOperand::Imm((1i64 << lanes) - 1),
                                        width: OpWidth::W64,
                                        flags: FlagUpdate::None,
                                    },
                                ));
                                ops.push(SmirOp::new(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::PredLoad {
                                        dst: scalar,
                                        cond,
                                        addr,
                                        width: elem_mem_width,
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
                                        width: elem_mem_width,
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
                                    elem,
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
                                    elem,
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
                                            i64::from(lane) * i64::from(elem.bytes()),
                                        ),
                                        width: elem_mem_width,
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
                                        elem,
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
                                    width: prefix.width,
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
                            prefix.width,
                        )
                    };
                    let old_dst = mask.map(|_| {
                        let old = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VMov {
                                dst: old,
                                src: dst,
                                width: prefix.width,
                            },
                        ));
                        old
                    });
                    let raw_dst = if mask.is_some() {
                        ctx.alloc_vreg()
                    } else {
                        dst
                    };
                    let kind = match opcode {
                        0x54 | 0xDB => OpKind::VAnd {
                            dst: raw_dst,
                            src1,
                            src2,
                            width: prefix.width,
                        },
                        0x55 | 0xDF => OpKind::VAndNot {
                            dst: raw_dst,
                            src1,
                            src2,
                            width: prefix.width,
                        },
                        0x56 | 0xEB => OpKind::VOr {
                            dst: raw_dst,
                            src1,
                            src2,
                            width: prefix.width,
                        },
                        0x57 | 0xEF => OpKind::VXor {
                            dst: raw_dst,
                            src1,
                            src2,
                            width: prefix.width,
                        },
                        0xD4 | 0xFC | 0xFD | 0xFE => OpKind::VAdd {
                            dst: raw_dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0xF8 | 0xF9 | 0xFA | 0xFB => OpKind::VSub {
                            dst: raw_dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED => {
                            OpKind::VAddSubSat {
                                dst: raw_dst,
                                src1,
                                src2,
                                elem,
                                lanes,
                                subtract: matches!(opcode, 0xD8 | 0xD9 | 0xE8 | 0xE9),
                                signed: matches!(opcode, 0xE8 | 0xE9 | 0xEC | 0xED),
                            }
                        }
                        _ => unreachable!(),
                    };
                    ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
                    if let Some(mask_reg) = mask {
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
                        for lane in 0..lanes {
                            let shifted = ctx.alloc_vreg();
                            let cond = ctx.alloc_vreg();
                            let active = ctx.alloc_vreg();
                            let inactive = if prefix.zeroing {
                                zero
                            } else {
                                let old = ctx.alloc_vreg();
                                ops.push(SmirOp::new(
                                    OpId(ops.len() as u16),
                                    pc,
                                    OpKind::VExtractLane {
                                        dst: old,
                                        vec: old_dst.unwrap(),
                                        lane,
                                        elem,
                                        sign: SignExtend::Zero,
                                    },
                                ));
                                old
                            };
                            let selected = ctx.alloc_vreg();
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
                                OpKind::VExtractLane {
                                    dst: active,
                                    vec: raw_dst,
                                    lane,
                                    elem,
                                    sign: SignExtend::Zero,
                                },
                            ));
                            ops.push(SmirOp::new(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::Select {
                                    dst: selected,
                                    cond,
                                    src_true: active,
                                    src_false: inactive,
                                    width: elem_op_width,
                                },
                            ));
                            ops.push(SmirOp::new(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::VInsertLane {
                                    dst,
                                    vec: if lane == 0 { raw_dst } else { dst },
                                    scalar: selected,
                                    lane,
                                    elem,
                                },
                            ));
                        }
                    }
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Full-register VMOVAPS/VMOVAPD and VMOVDQA/VMOVDQU. Aligned
                // forms emit an explicit precondition; EVEX VMOVDQU8/16/32/64
                // support element-granular masking. Compressed displacements
                // scale by the full vector width.
                0x28 | 0x29 | 0x6F | 0x7F => {
                    let valid_prefix = match opcode {
                        0x28 | 0x29 => {
                            matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
                        }
                        0x6F | 0x7F => {
                            matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Rep)
                                || prefix.encoding == VecEncodingKind::Evex
                                    && prefix.pp == X86SsePrefix::Repne
                        }
                        _ => unreachable!(),
                    };
                    let wrong_evex_w = prefix.encoding == VecEncodingKind::Evex
                        && matches!(opcode, 0x28 | 0x29)
                        && (prefix.w != (prefix.pp == X86SsePrefix::OpSize));
                    let evex_unaligned_int = prefix.encoding == VecEncodingKind::Evex
                        && matches!(opcode, 0x6F | 0x7F)
                        && matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne);
                    let evex_aligned = prefix.encoding == VecEncodingKind::Evex
                        && (matches!(opcode, 0x28 | 0x29)
                            || matches!(opcode, 0x6F | 0x7F) && prefix.pp == X86SsePrefix::OpSize);
                    let evex_maskable = evex_unaligned_int || evex_aligned;
                    if !valid_prefix
                        || prefix.l_bits == 3
                        || prefix.vvvv != 0
                        || prefix.v_high
                        || (!evex_maskable && prefix.aaa != 0)
                        || (prefix.zeroing && prefix.aaa == 0)
                        || prefix.b
                        || wrong_evex_w
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let dst_reg = self.vec_reg(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let rm_reg = self.vec_reg(
                        modrm.rm
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let aligned =
                        matches!(opcode, 0x28 | 0x29) || prefix.pp == X86SsePrefix::OpSize;
                    let mask_elem = evex_maskable.then(|| match (opcode, prefix.pp, prefix.w) {
                        (0x28 | 0x29, X86SsePrefix::None, false) => VecElementType::F32,
                        (0x28 | 0x29, X86SsePrefix::OpSize, true) => VecElementType::F64,
                        (0x6F | 0x7F, X86SsePrefix::Repne, false) => VecElementType::I8,
                        (0x6F | 0x7F, X86SsePrefix::Repne, true) => VecElementType::I16,
                        (0x6F | 0x7F, X86SsePrefix::Rep, false)
                        | (0x6F | 0x7F, X86SsePrefix::OpSize, false) => VecElementType::I32,
                        (0x6F | 0x7F, X86SsePrefix::Rep, true)
                        | (0x6F | 0x7F, X86SsePrefix::OpSize, true) => VecElementType::I64,
                        _ => unreachable!(),
                    });
                    let mask = (evex_maskable && prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));

                    match opcode {
                        0x28 | 0x6F => {
                            if modrm.is_memory {
                                let x86_addr = modrm.addr.as_ref().unwrap();
                                let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                                    self.vec_full_addr_to_smir(prefix, x86_addr, next_pc, ctx)
                                } else {
                                    self.x86_addr_to_smir(x86_addr, next_pc, ctx)
                                };
                                ops.extend(pre_ops);
                                if aligned {
                                    ops.push(SmirOp::new(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::X86CheckAlignment {
                                            addr: addr.clone(),
                                            alignment: prefix.width.bytes() as u8,
                                        },
                                    ));
                                }
                                if let Some(mask) = mask {
                                    let elem = mask_elem.unwrap();
                                    let raw = self.append_evex_masked_vector_source(
                                        addr,
                                        elem,
                                        prefix.width,
                                        false,
                                        mask,
                                        pc,
                                        ctx,
                                        &mut ops,
                                    );
                                    self.append_evex_vector_mask_result(
                                        prefix, dst_reg, raw, elem, pc, ctx, &mut ops,
                                    );
                                } else {
                                    ops.push(SmirOp::with_hint(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::VLoad {
                                            dst: dst_reg,
                                            addr,
                                            width: prefix.width,
                                        },
                                        hint,
                                    ));
                                }
                            } else if let Some(elem) = mask_elem.filter(|_| mask.is_some()) {
                                self.append_evex_vector_mask_result(
                                    prefix, dst_reg, rm_reg, elem, pc, ctx, &mut ops,
                                );
                            } else {
                                ops.push(SmirOp::with_hint(
                                    OpId(0),
                                    pc,
                                    OpKind::VMov {
                                        dst: dst_reg,
                                        src: rm_reg,
                                        width: prefix.width,
                                    },
                                    hint,
                                ));
                            }
                        }
                        0x29 | 0x7F => {
                            if modrm.is_memory {
                                if prefix.zeroing {
                                    return Err(LiftError::InvalidEncoding {
                                        addr: pc,
                                        bytes: bytes.to_vec(),
                                    });
                                }
                                let x86_addr = modrm.addr.as_ref().unwrap();
                                let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                                    self.vec_full_addr_to_smir(prefix, x86_addr, next_pc, ctx)
                                } else {
                                    self.x86_addr_to_smir(x86_addr, next_pc, ctx)
                                };
                                ops.extend(pre_ops);
                                if aligned {
                                    ops.push(SmirOp::new(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::X86CheckAlignment {
                                            addr: addr.clone(),
                                            alignment: prefix.width.bytes() as u8,
                                        },
                                    ));
                                }
                                if let Some(mask) = mask {
                                    self.append_evex_masked_vector_store(
                                        addr,
                                        dst_reg,
                                        mask_elem.unwrap(),
                                        prefix.width,
                                        mask,
                                        pc,
                                        ctx,
                                        &mut ops,
                                    );
                                } else {
                                    ops.push(SmirOp::with_hint(
                                        OpId(ops.len() as u16),
                                        pc,
                                        OpKind::VStore {
                                            src: dst_reg,
                                            addr,
                                            width: prefix.width,
                                        },
                                        hint,
                                    ));
                                }
                            } else if let Some(elem) = mask_elem.filter(|_| mask.is_some()) {
                                self.append_evex_vector_mask_result(
                                    prefix, rm_reg, dst_reg, elem, pc, ctx, &mut ops,
                                );
                            } else {
                                ops.push(SmirOp::with_hint(
                                    OpId(0),
                                    pc,
                                    OpKind::VMov {
                                        dst: rm_reg,
                                        src: dst_reg,
                                        width: prefix.width,
                                    },
                                    hint,
                                ));
                            }
                        }
                        _ => {}
                    }

                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // Packed/scalar vector ADD/MUL/SUB/DIV/MIN/MAX.
                0x58 | 0x59 | 0x5C | 0x5D | 0x5E | 0x5F => {
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;

                    if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) {
                        if prefix.encoding == VecEncodingKind::Evex
                            && ((prefix.zeroing && prefix.aaa == 0)
                                || prefix.b
                                || (prefix.pp == X86SsePrefix::Rep && prefix.w)
                                || (prefix.pp == X86SsePrefix::Repne && !prefix.w))
                        {
                            return Err(LiftError::Unsupported {
                                addr: pc,
                                mnemonic: format!("scalar vector opcode 0x{opcode:02X}"),
                            });
                        }
                        let elem = if prefix.pp == X86SsePrefix::Rep {
                            VecElementType::F32
                        } else {
                            VecElementType::F64
                        };
                        let mem_width = if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        };
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
                                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high
                                    {
                                        16
                                    } else {
                                        0
                                    },
                            )
                        };
                        let vector_result = ctx.alloc_vreg();
                        let scalar_result = ctx.alloc_vreg();
                        let kind = match opcode {
                            0x58 => OpKind::VAdd {
                                dst: vector_result,
                                src1,
                                src2,
                                elem,
                                lanes: 1,
                            },
                            0x59 => OpKind::VMul {
                                dst: vector_result,
                                src1,
                                src2,
                                elem,
                                lanes: 1,
                            },
                            0x5C => OpKind::VSub {
                                dst: vector_result,
                                src1,
                                src2,
                                elem,
                                lanes: 1,
                            },
                            0x5E => OpKind::VDiv {
                                dst: vector_result,
                                src1,
                                src2,
                                elem,
                                lanes: 1,
                            },
                            0x5D | 0x5F => OpKind::VX86MinMax {
                                dst: vector_result,
                                src1,
                                src2,
                                elem,
                                lanes: 1,
                                min: opcode == 0x5D,
                            },
                            _ => unreachable!(),
                        };
                        ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
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
                        self.append_vex_scalar_result(
                            dst,
                            src1,
                            scalar_result,
                            elem,
                            pc,
                            ctx,
                            &mut ops,
                        );
                        return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
                    }

                    let dst = self.vec_reg(
                        modrm.reg
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );
                    let src1 = self.vec_reg(
                        prefix.vvvv
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    );

                    let elem = match prefix.pp {
                        X86SsePrefix::None => VecElementType::F32,
                        X86SsePrefix::OpSize => VecElementType::F64,
                        X86SsePrefix::Rep | X86SsePrefix::Repne => unreachable!(),
                    };
                    let lanes = prefix.width.lanes(elem) as u8;
                    if prefix.encoding == VecEncodingKind::Evex
                        && ((elem == VecElementType::F32 && prefix.w)
                            || (elem == VecElementType::F64 && !prefix.w))
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }

                    let src2 = if modrm.is_memory {
                        let x86_addr = modrm.addr.as_ref().unwrap();
                        let (addr, pre_ops) =
                            self.vec_full_addr_to_smir(prefix, x86_addr, next_pc, ctx);
                        ops.extend(pre_ops);
                        let tmp = ctx.alloc_vreg();
                        ops.push(SmirOp::with_hint(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VLoad {
                                dst: tmp,
                                addr,
                                width: prefix.width,
                            },
                            X86OpHint::VecAlign(X86VecAlign::Unaligned),
                        ));
                        tmp
                    } else {
                        self.vec_reg(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                            prefix.width,
                        )
                    };

                    let op_kind = match opcode {
                        0x58 => OpKind::VAdd {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0x5C => OpKind::VSub {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0x59 => OpKind::VMul {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0x5E => OpKind::VDiv {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        0x5D | 0x5F => OpKind::VX86MinMax {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                            min: opcode == 0x5D,
                        },
                        _ => {
                            return Err(LiftError::Unsupported {
                                addr: pc,
                                mnemonic: format!("VEX opcode 0x{:02X}", opcode),
                            });
                        }
                    };

                    ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, op_kind, hint));

                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                // VEX.128 VLDMXCSR/VSTMXCSR (VEX.0F.AE /2,/3).
                0xAE => {
                    if prefix.encoding != VecEncodingKind::Vex
                        || prefix.pp != X86SsePrefix::None
                        || prefix.width != VecWidth::V128
                        || prefix.w
                        || prefix.vvvv != 0
                    {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
                    let group = (modrm.byte >> 3) & 7;
                    if !modrm.is_memory || !matches!(group, 2 | 3) {
                        return Err(LiftError::InvalidEncoding {
                            addr: pc,
                            bytes: bytes.to_vec(),
                        });
                    }
                    let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
                    let (addr, mut ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        if group == 2 {
                            OpKind::X86LoadMxcsr { addr }
                        } else {
                            OpKind::X86StoreMxcsr { addr }
                        },
                        hint,
                    ));
                    Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
                }

                0x70 => self.lift_vec_packed_shuffle_imm(prefix, bytes, pc, ctx),
                0x14 | 0x15 => self.lift_vec_fp_unpack(prefix, opcode, bytes, pc, ctx),
                0x12 | 0x13 | 0x16 | 0x17
                    if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
                {
                    self.lift_vec_half_move(prefix, opcode, bytes, pc, ctx)
                }
                0x2B | 0xE7 => self.lift_vec_movnt(prefix, opcode, bytes, pc, ctx),
                0x77 => self.lift_vec_vzero(prefix, bytes, pc, ctx),
                0xD7 => self.lift_vec_pmovmskb(prefix, bytes, pc, ctx),
                0xF7 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_maskmovdqu(prefix, bytes, pc, ctx)
                }
                0xD5 => self.lift_vec_pmul_low(prefix, opcode, bytes, pc, ctx),
                0xF5 => self.lift_vec_pmaddwd(prefix, bytes, pc, ctx),
                0xF4 => self.lift_vec_pmuldq(prefix, bytes, false, pc, ctx),
                0xE4 | 0xE5 => self.lift_vec_pmul_high_word(prefix, opcode, bytes, pc, ctx),
                0xE0 | 0xE3 => self.lift_vec_packed_average(prefix, opcode, bytes, pc, ctx),
                0xDA | 0xDE | 0xEA | 0xEE => {
                    self.lift_vec_packed_minmax(prefix, opcode, bytes, pc, ctx)
                }
                0xD1..=0xD3 | 0xE1 | 0xE2 | 0xF1..=0xF3 => {
                    self.lift_vec_packed_shift_count(prefix, opcode, bytes, pc, ctx)
                }
                0x71..=0x73 => self.lift_vec_packed_shift_imm(prefix, opcode, bytes, pc, ctx),
                0xC2 => self.lift_vec_fp_compare(prefix, bytes, pc, ctx),
                0xC6 => self.lift_vec_two_source_shuffle_imm(prefix, bytes, pc, ctx),
                0x12 | 0x16 if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_vec_duplicate_move(prefix, opcode, bytes, pc, ctx)
                }
                0x12 if prefix.pp == X86SsePrefix::Repne => {
                    self.lift_vec_duplicate_move(prefix, opcode, bytes, pc, ctx)
                }
                0x52 | 0x53 => self.lift_vec_fp_estimate(prefix, opcode, bytes, pc, ctx),
                0x7C | 0x7D | 0xD0 => {
                    self.lift_vec_addsub_horizontal(prefix, opcode, bytes, pc, ctx)
                }

                _ => Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("VEX opcode 0x{:02X}", opcode),
                }),
            },
            X86VecMap::Map0F38 => match opcode {
                0xC8 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_exp2(prefix, opcode, bytes, pc, ctx)
                }
                0x4C..=0x4F if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_approx14(prefix, opcode, bytes, pc, ctx)
                }
                0x52 | 0x53
                    if prefix.encoding == VecEncodingKind::Evex
                        && prefix.pp == X86SsePrefix::Repne =>
                {
                    self.lift_evex_four_dot_product(prefix, opcode, bytes, pc, ctx)
                }
                0xCA..=0xCD if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_approx28(prefix, opcode, bytes, pc, ctx)
                }
                0x2C | 0x2D if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_scale_f(prefix, opcode, bytes, pc, ctx)
                }
                0x42 | 0x43 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_get_exponent(prefix, opcode, bytes, pc, ctx)
                }
                0x13 if prefix.pp == X86SsePrefix::OpSize => self.lift_vec_packed_fp16_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F16,
                    VecElementType::F32,
                ),
                0x64..=0x66 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_mask_blend(prefix, opcode, bytes, pc, ctx)
                }
                0x2A | 0x3A if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_mask_broadcast(prefix, opcode, bytes, pc, ctx)
                }
                0x18..=0x1B | 0x58..=0x5B | 0x78..=0x7C if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_vec_load_broadcast(prefix, opcode, bytes, pc, ctx)
                }
                0x10..=0x12 | 0x45..=0x47 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_vec_packed_shift_variable(prefix, opcode, bytes, pc, ctx)
                }
                0x83 => self.lift_evex_multishift_qb(prefix, bytes, pc, ctx),
                0x70..=0x73 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_packed_funnel_shift(prefix, opcode, bytes, pc, ctx)
                }
                0x14 | 0x15 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_packed_rotate_variable(prefix, opcode, bytes, pc, ctx)
                }
                0x44 => self.lift_evex_vplzcnt(prefix, bytes, pc, ctx),
                0x50..=0x53 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_vec_vnni_dot(prefix, opcode, bytes, pc, ctx)
                }
                0x50 | 0x51 => self.lift_vex_vnni_dot_ext(prefix, opcode, bytes, pc, ctx),
                0x52 if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_bf16_dot(prefix, bytes, pc, ctx)
                }
                0x72 if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                    self.lift_bf16_convert(prefix, bytes, pc, ctx)
                }
                0x62 | 0x63 | 0x88..=0x8B => {
                    self.lift_evex_compress_expand(prefix, opcode, bytes, pc, ctx)
                }
                0x68 if prefix.pp == X86SsePrefix::Repne => {
                    self.lift_evex_pair_intersect(prefix, bytes, pc, ctx)
                }
                0x8F => self.lift_evex_vpshufbitqmb(prefix, bytes, pc, ctx),
                0xD2 | 0xD3 => self.lift_vex_vnni_dot_ext(prefix, opcode, bytes, pc, ctx),
                0x54 | 0x55 => self.lift_evex_vpopcnt(prefix, opcode, bytes, pc, ctx),
                0xC6 | 0xC7 => self.lift_evex_sparse_prefetch(prefix, opcode, bytes, pc),
                0xC4 => self.lift_evex_vpconflict(prefix, bytes, pc, ctx),
                0x0C | 0x0D | 0x16 | 0x36 | 0x8D => {
                    self.lift_vec_permute_variable(prefix, opcode, bytes, pc, ctx)
                }
                0x75..=0x77 | 0x7D..=0x7F => {
                    self.lift_evex_permute_two_table(prefix, opcode, bytes, pc, ctx)
                }
                0xB0 | 0xB1 => self.lift_vex_ne_convert(prefix, opcode, bytes, pc, ctx),
                0xCB..=0xCD => self.lift_vex_sha512(prefix, opcode, bytes, pc, ctx),
                0xCF => self.lift_vec_gfni(prefix, opcode, bytes, pc, ctx),
                0xDA if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) => {
                    self.lift_vex_sm3_message(prefix, bytes, pc, ctx)
                }
                0xDA if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                    self.lift_vex_sm4(prefix, bytes, pc, ctx)
                }
                0xDB..=0xDF => self.lift_vec_aes_round(prefix, opcode, bytes, pc, ctx),
                0x00 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_pshufb(prefix, bytes, pc, ctx)
                }
                0x00 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_pshufb(prefix, bytes, pc, ctx)
                }
                0x01..=0x03 | 0x05..=0x07 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_horizontal_integer(prefix, opcode, bytes, pc, ctx)
                }
                0x04 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_pmaddubsw(prefix, bytes, pc, ctx)
                }
                0x04 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_pmaddubsw(prefix, bytes, pc, ctx)
                }
                0x08..=0x0A if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_psign(prefix, opcode, bytes, pc, ctx)
                }
                0x0B if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_pmulhrsw(prefix, bytes, pc, ctx)
                }
                0x0B if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_pmulhrsw(prefix, bytes, pc, ctx)
                }
                0x0E | 0x0F => self.lift_vex_testp(prefix, opcode, bytes, pc, ctx),
                0x17 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_ptest(prefix, bytes, pc, ctx)
                }
                0x1C..=0x1E if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_pabs(prefix, opcode, bytes, pc, ctx)
                }
                0x1C..=0x1F if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_pabs(prefix, opcode, bytes, pc, ctx)
                }
                0x26 | 0x27 if matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Rep) => {
                    self.lift_evex_integer_test_mask(prefix, opcode, bytes, pc, ctx)
                }
                0x28 | 0x29 | 0x38 | 0x39 if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_mask_vector_convert(prefix, opcode, bytes, pc, ctx)
                }
                0x10..=0x15 | 0x20..=0x25 | 0x30..=0x35 if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_integer_narrow(prefix, opcode, bytes, pc, ctx)
                }
                0x20..=0x25 | 0x30..=0x35 => {
                    self.lift_vec_packed_extend(prefix, opcode, bytes, pc, ctx)
                }
                0x28 => self.lift_vec_pmuldq(prefix, bytes, true, pc, ctx),
                0x2C..=0x2F | 0x8C | 0x8E if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_masked_memory(prefix, opcode, bytes, pc, ctx)
                }
                0x90..=0x93 => self.lift_vec_gather(prefix, opcode, bytes, pc, ctx),
                0xA0..=0xA3 => self.lift_evex_scatter(prefix, opcode, bytes, pc, ctx),
                0xB4 | 0xB5 => self.lift_vec_vpmadd52(prefix, opcode, bytes, pc, ctx),
                0x9A | 0x9B | 0xAA | 0xAB
                    if prefix.encoding == VecEncodingKind::Evex
                        && prefix.pp == X86SsePrefix::Repne =>
                {
                    self.lift_evex_four_fma(prefix, opcode, bytes, pc, ctx)
                }
                0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF => {
                    self.lift_vec_fma3(prefix, opcode, bytes, pc, ctx)
                }
                0x2A => self.lift_vec_movntdqa(prefix, bytes, pc, ctx),
                0x2B if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_integer_pack(prefix, opcode, bytes, pc, ctx)
                }
                0x2B if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_integer_pack(prefix, opcode, bytes, pc, ctx)
                }
                0x29 | 0x37 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_integer_compare(prefix, opcode, bytes, pc, ctx)
                }
                0x29 | 0x37 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_integer_compare(prefix, opcode, bytes, pc, ctx)
                }
                0x38..=0x3F => self.lift_vec_packed_minmax(prefix, opcode, bytes, pc, ctx),
                0x41 if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_phminposuw(prefix, bytes, pc, ctx)
                }
                0xE0..=0xEF => self.lift_cmpccxadd(prefix, opcode, bytes, pc, ctx),
                0xF2 if prefix.encoding == VecEncodingKind::Vex
                    && prefix.pp == X86SsePrefix::None =>
                {
                    self.lift_vex_andn_0f38(prefix, bytes, pc, ctx)
                }
                0xF3 if prefix.encoding == VecEncodingKind::Vex
                    && prefix.pp == X86SsePrefix::None =>
                {
                    self.lift_vex_bls_0f38(prefix, bytes, pc, ctx)
                }
                0xF5 | 0xF7
                    if prefix.encoding == VecEncodingKind::Vex
                        && prefix.pp == X86SsePrefix::None =>
                {
                    self.lift_vex_bzhi_bextr_0f38(prefix, opcode, bytes, pc, ctx)
                }
                0xF5 if prefix.encoding == VecEncodingKind::Vex
                    && matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) =>
                {
                    self.lift_vex_pdep_pext_0f38(prefix, bytes, pc, ctx)
                }
                0xF6 if prefix.encoding == VecEncodingKind::Vex
                    && prefix.pp == X86SsePrefix::Repne =>
                {
                    self.lift_vex_mulx_0f38(prefix, bytes, pc, ctx)
                }
                0xF7 if prefix.encoding == VecEncodingKind::Vex
                    && matches!(
                        prefix.pp,
                        X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
                    ) =>
                {
                    self.lift_vex_bmi2_shift_0f38(prefix, bytes, pc, ctx)
                }
                0xF5 | 0xF6 | 0xF7
                    if prefix.encoding == VecEncodingKind::Evex
                        && !matches!(prefix.pp, X86SsePrefix::None) =>
                {
                    self.lift_apx_bmi2_0f38(opcode, bytes, pc, ctx)
                }
                0xF2 | 0xF3 | 0xF5 | 0xF7
                    if prefix.encoding == VecEncodingKind::Evex
                        && prefix.pp == X86SsePrefix::None =>
                {
                    self.lift_apx_nf_bmi_0f38(opcode, bytes, pc, ctx)
                }
                0x40 => self.lift_vec_pmul_low(prefix, opcode, bytes, pc, ctx),
                _ => Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("VEX 0F38 opcode 0x{:02X}", opcode),
                }),
            },
            X86VecMap::Map0F3A => match opcode {
                0x26 | 0x27
                    if prefix.encoding == VecEncodingKind::Evex
                        && matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
                {
                    self.lift_evex_get_mantissa(prefix, opcode, bytes, pc, ctx)
                }
                0x1D if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_vec_packed_f32_to_f16_store(prefix, bytes, pc, ctx)
                }
                0x18 | 0x19 | 0x1A | 0x1B | 0x38 | 0x39 | 0x3A | 0x3B
                    if prefix.encoding == VecEncodingKind::Evex =>
                {
                    self.lift_evex_chunk_extract_insert(prefix, opcode, bytes, pc, ctx)
                }
                0x23 | 0x43 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_shuffle_128_chunks(prefix, opcode, bytes, pc, ctx)
                }
                0x66 | 0x67 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_fp_class(prefix, opcode, bytes, pc, ctx)
                }
                0xCE | 0xCF => self.lift_vec_gfni(prefix, opcode, bytes, pc, ctx),
                0x1E | 0x1F | 0x3E | 0x3F if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_integer_compare(prefix, opcode, bytes, pc, ctx)
                }
                0x03 => self.lift_evex_vector_align(prefix, bytes, pc, ctx),
                0x70..=0x73 => self.lift_evex_packed_funnel_shift(prefix, opcode, bytes, pc, ctx),
                0x25 => self.lift_evex_ternary_logic(prefix, bytes, pc, ctx),
                0x00 | 0x01 | 0x04 | 0x05 => {
                    self.lift_vec_permute_immediate(prefix, opcode, bytes, pc, ctx)
                }
                0x06 | 0x46 => self.lift_vex_permute2x128(prefix, bytes, pc, ctx),
                0xDE => self.lift_vex_sm3_rounds2(prefix, bytes, pc, ctx),
                0x08..=0x0B if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_round_scale(prefix, opcode, bytes, pc, ctx)
                }
                0x08..=0x0B => self.lift_vex_round(prefix, opcode, bytes, pc, ctx),
                0x56 | 0x57 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_reduce(prefix, opcode, bytes, pc, ctx)
                }
                0x50 | 0x51 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_range(prefix, opcode, bytes, pc, ctx)
                }
                0x54 | 0x55 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_evex_fixup_imm(prefix, opcode, bytes, pc, ctx)
                }
                0x0C..=0x0E if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_immediate_blend(prefix, opcode, bytes, pc, ctx)
                }
                0x0F => self.lift_vec_palignr(prefix, bytes, pc, ctx),
                0x14..=0x17 => self.lift_vec_extract_0f3a(prefix, opcode, bytes, pc, ctx),
                0x20..=0x22 => self.lift_vec_insert_0f3a(prefix, opcode, bytes, pc, ctx),
                0x40 | 0x41 => self.lift_vex_dot_product(prefix, opcode, bytes, pc, ctx),
                0x42 => self.lift_vec_mpsadbw(prefix, bytes, pc, ctx),
                0x44 => self.lift_vec_pclmulqdq(prefix, bytes, pc, ctx),
                0xDF => self.lift_vec_aes_keygen(prefix, bytes, pc, ctx),
                0x4A..=0x4C if prefix.encoding == VecEncodingKind::Vex => {
                    self.lift_vex_variable_blend(prefix, opcode, bytes, pc, ctx)
                }
                0xF0 if prefix.encoding == VecEncodingKind::Vex
                    && prefix.pp == X86SsePrefix::Repne =>
                {
                    self.lift_vex_bmi2_rorx_0f3a(prefix, bytes, pc, ctx)
                }
                0xF0 if prefix.encoding == VecEncodingKind::Evex => {
                    self.lift_apx_bmi2_rorx(bytes, pc, ctx)
                }
                0xC2 => self.lift_vec_fp_compare(prefix, bytes, pc, ctx),
                _ => Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "VEX 0F3A".to_string(),
                }),
            },
            X86VecMap::Map5 => match opcode {
                0x5A if prefix.pp == X86SsePrefix::None => self.lift_vec_packed_fp16_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F16,
                    VecElementType::F64,
                ),
                0x5A if prefix.pp == X86SsePrefix::OpSize => self.lift_vec_packed_fp16_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F64,
                    VecElementType::F16,
                ),
                0x5A if prefix.pp == X86SsePrefix::Repne => self.lift_vec_scalar_fp_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F64,
                    VecElementType::F16,
                ),
                0x5A if prefix.pp == X86SsePrefix::Rep => self.lift_vec_scalar_fp_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F16,
                    VecElementType::F64,
                ),
                0x1D if prefix.pp == X86SsePrefix::None => self.lift_vec_scalar_fp_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F32,
                    VecElementType::F16,
                ),
                0x1D if prefix.pp == X86SsePrefix::OpSize => self.lift_vec_packed_fp16_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F32,
                    VecElementType::F16,
                ),
                0x10 | 0x11 => self.lift_evex_fp16_scalar_move(prefix, opcode, bytes, pc, ctx),
                0x2A | 0x7B if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_int_to_fp16(prefix, opcode, bytes, pc, ctx)
                }
                0x2C | 0x2D | 0x78 | 0x79 if prefix.pp == X86SsePrefix::Rep => {
                    self.lift_evex_fp16_to_int(prefix, opcode, bytes, pc, ctx)
                }
                0x2E | 0x2F => self.lift_evex_fp16_flag_compare(prefix, opcode, bytes, pc, ctx),
                0x51 => self.lift_evex_fp16_sqrt(prefix, bytes, pc, ctx),
                0x5B if prefix.pp == X86SsePrefix::None => {
                    self.lift_evex_packed_int_to_fp16(prefix, bytes, pc, ctx)
                }
                0x7A if prefix.pp == X86SsePrefix::Repne => {
                    self.lift_evex_packed_int_to_fp16(prefix, bytes, pc, ctx)
                }
                0x7D if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                    self.lift_evex_packed_int_to_fp16(prefix, bytes, pc, ctx)
                }
                0x5B | 0x78 | 0x79 | 0x7A | 0x7B | 0x7C | 0x7D => {
                    self.lift_evex_packed_fp16_to_int(prefix, bytes, pc, ctx)
                }
                0x58 | 0x59 | 0x5C | 0x5D | 0x5E | 0x5F => {
                    self.lift_evex_fp16_arithmetic(prefix, opcode, bytes, pc, ctx)
                }
                0x6E | 0x7E => self.lift_evex_word_move(prefix, opcode, bytes, pc, ctx),
                _ => self.unsupported_evex_map_opcode(prefix.map, opcode, pc),
            },
            X86VecMap::Map6 => match opcode {
                0x4C..=0x4F if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_fp16_approx(prefix, opcode, bytes, pc, ctx)
                }
                0x56 | 0x57 | 0xD6 | 0xD7
                    if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) =>
                {
                    self.lift_evex_fp16_complex(prefix, opcode, bytes, pc, ctx)
                }
                0x2C | 0x2D if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_scale_f(prefix, opcode, bytes, pc, ctx)
                }
                0x42 | 0x43 if prefix.pp == X86SsePrefix::OpSize => {
                    self.lift_evex_get_exponent(prefix, opcode, bytes, pc, ctx)
                }
                0x13 if prefix.pp == X86SsePrefix::OpSize => self.lift_vec_packed_fp16_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F16,
                    VecElementType::F32,
                ),
                0x13 if prefix.pp == X86SsePrefix::None => self.lift_vec_scalar_fp_convert(
                    prefix,
                    bytes,
                    pc,
                    ctx,
                    VecElementType::F16,
                    VecElementType::F32,
                ),
                0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF => {
                    self.lift_vec_fma3(prefix, opcode, bytes, pc, ctx)
                }
                _ => self.unsupported_evex_map_opcode(prefix.map, opcode, pc),
            },
        }
    }


    pub(crate) fn lift_vex_evex(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix = match bytes.first().copied() {
            Some(0x62) => decode_evex_prefix(bytes, pc)?,
            _ => decode_vex_prefix(bytes, pc)?,
        };

        self.lift_vec_opcode(prefix, bytes, pc, ctx)
    }


    pub(crate) fn lift_prefixed_vec(
        &self,
        pc: u64,
        bytes: &[u8],
        legacy: &X86Prefix,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // VEX/EVEX may follow address-size or segment prefixes, but must not
        // be preceded by REX, LOCK, or a separately encoded SIMD prefix.
        if legacy.rex.is_some()
            || legacy.rex2.is_some()
            || legacy.lock
            || legacy.operand_size_override
            || legacy.rep_prefix.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mut prefix = match bytes.get(legacy.cursor) {
            Some(0x62) => decode_evex_prefix(&bytes[legacy.cursor..], pc)?,
            _ => decode_vex_prefix(&bytes[legacy.cursor..], pc)?,
        };
        prefix.bytes += legacy.cursor;
        prefix.address_size_override = legacy.address_size_override;
        prefix.segment_override = legacy.segment_override;
        self.lift_vec_opcode(prefix, bytes, pc, ctx)
    }


    /// Lift the main instruction
    pub(crate) fn lift_insn_inner(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        if bytes[0] == 0x62 {
            let is_apx_map4 = bytes.get(1).map_or(false, |p0| (p0 & 0x07) == 4);
            if is_apx_map4 || bytes.len() < 2 {
                return self.lift_apx_evex_map4(pc, bytes, ctx);
            }
            return self.lift_vex_evex(pc, bytes, ctx);
        }

        if matches!(bytes[0], 0xC4 | 0xC5) {
            return self.lift_vex_evex(pc, bytes, ctx);
        }

        // Decode prefixes
        let prefix = decode_prefixes(bytes)?;
        let opcode_bytes = &bytes[prefix.cursor..];

        if opcode_bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.cursor + 1,
            });
        }

        let prefixed_vec = matches!(opcode_bytes[0], 0x62 | 0xC4 | 0xC5);
        if prefixed_vec {
            return self.lift_prefixed_vec(pc, bytes, &prefix, ctx);
        }

        if prefix.rex2_m() {
            return self.lift_0f_opcode(opcode_bytes, &prefix, pc, ctx, 1);
        }

        let opcode = opcode_bytes[0];
        let after_opcode = &opcode_bytes[1..];

        match opcode {
            // XCHG rax, r64 / NOP / PAUSE (with REP prefix)
            0x90..=0x97 => {
                if opcode == 0x90 && prefix.rep_prefix == Some(0xF3) {
                    // PAUSE - treat as NOP for lifting
                    Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1))
                } else if opcode == 0x90 && prefix.rex_b() == 0 {
                    // 90 (including 66/REX.W 90) is the architectural NOP
                    // alias, not a 32-bit self-write that clears EAX[63:32].
                    Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1))
                } else {
                    self.lift_xchg_rax(
                        opcode,
                        &X86Prefix {
                            cursor: prefix.cursor + 1,
                            ..prefix
                        },
                        pc,
                    )
                }
            }

            // CMC/CLC/STC
            0xF5 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::CmcCF)],
                prefix.cursor + 1,
            )),
            0xF8 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetCF { value: false })],
                prefix.cursor + 1,
            )),
            0xF9 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetCF { value: true })],
                prefix.cursor + 1,
            )),
            0xFC => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetDF { value: false })],
                prefix.cursor + 1,
            )),
            0xFD => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetDF { value: true })],
                prefix.cursor + 1,
            )),
            0xCC => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::Breakpoint)],
                prefix.cursor + 1,
            )),

            // HLT
            0xF4 => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::Halt,
                },
                branch_targets: vec![],
            }),

            // Instructions architecturally invalid in 64-bit mode. Model the
            // guaranteed #UD explicitly rather than reporting missing support.
            0x27 | 0x2F | 0x37 | 0x3F // DAA/DAS/AAA/AAS
            | 0x60 | 0x61             // PUSHA/POPA
            | 0x82                    // legacy Group-1 alias
            | 0x9A | 0xEA             // far CALL/JMP immediate
            | 0xCE                    // INTO
            | 0xD4 => Ok(LiftResult { // AAM (D5 is APX REX2 in this decoder)
                ops: vec![],
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            }),

            // Two-byte opcode prefix
            0x0F => self.lift_0f_opcode(after_opcode, &prefix, pc, ctx, 2),

            // Control flow
            0xEB => self.lift_jmp_rel8(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE9 => self.lift_jmp_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE8 => self.lift_call_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC2 => self.lift_ret_imm16(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC3 => self.lift_ret(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC8 => self.lift_enter(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC9 => self.lift_leave(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x99 => self.lift_cwd_cdq_cqo(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x98 => self.lift_cbw_cwde_cdqe(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x9C | 0x9D => self.lift_stack_flags(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x9E | 0x9F => self.lift_ah_flags(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            // WAIT/FWAIT has no state effect in the base emulator profile.
            0x9B if !prefix.lock => Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1)),
            0x9B => Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..(prefix.cursor + 1).min(bytes.len())].to_vec(),
            }),
            0x70..=0x7F => self.lift_jcc_rel8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE0..=0xE3 => self.lift_loop_rel8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xA1 if prefix.rex2.is_some() => self.lift_jmp_abs(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xA0..=0xA3 => self.lift_mov_moffs(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // Data movement
            0xB0..=0xB7 => self.lift_mov_r8_imm8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xB8..=0xBF => self.lift_mov_r_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x88..=0x8B => self.lift_mov_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x86 | 0x87 => self.lift_xchg_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC6 | 0xC7 => self.lift_mov_rm_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x8D => self.lift_lea(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x63 => self.lift_movsxd(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x50..=0x57 => self.lift_push_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x58..=0x5F => self.lift_pop_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x8F => self.lift_pop_rm(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x6A | 0x68 => self.lift_push_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xF6 | 0xF7 => self.lift_group3(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x69 | 0x6B => self.lift_imul_rmi(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Arithmetic
            0x00..=0x05 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADD
            0x08..=0x0D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // OR
            0x10..=0x15 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADC
            0x18..=0x1D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SBB
            0x20..=0x25 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // AND
            0x28..=0x2D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SUB
            0x30..=0x35 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x38..=0x3D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // CMP

            // Group 1 immediate (80/81/83)
            0x80 | 0x81 | 0x83 => self.lift_group1_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Logic
            0x84 | 0x85 => self.lift_test_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xA8 | 0xA9 => self.lift_test_acc_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // String ops
            0xA4..=0xA7 | 0xAA..=0xAF => self.lift_string(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xD7 => self.lift_xlat(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xD8..=0xDF => self.lift_x87_escape(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (C0/C1) - immediate
            0xC0 | 0xC1 => self.lift_shift_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (D0/D1) - count = 1
            0xD0 | 0xD1 => self.lift_shift_one(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (D2/D3) - count in CL
            0xD2 | 0xD3 => self.lift_shift_cl(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Group 5 (FF)
            0xFE => self.lift_group4(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xFF => self.lift_group5(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // I/O port instructions
            0xE4 | 0xE5 | 0xEC | 0xED => self.lift_in(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xE6 | 0xE7 | 0xEE | 0xEF => self.lift_out(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // Unsupported - return error with mnemonic
            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x{:02X}", opcode),
                    })
                } else {
                    // In non-strict mode, emit a Nop and continue
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix.cursor + 1,
                    ))
                }
            }
        }
    }


    /// Lift every architecturally defined 3DNow! suffix-selected
    /// `0F 0F /r imm8` form. PAVGUSB and PSWAPD reuse generic packed-integer
    /// operations; the remaining operations use the atomic 3DNow! IR family.
    pub(crate) fn lift_3dnow(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let suffix_offset = modrm.bytes_consumed;
        let Some(&suffix) = bytes.get(suffix_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + suffix_offset,
                need: prefix.cursor + suffix_offset + 1,
            });
        };
        let three_d_now_kind = match suffix {
            0x0C => Some(X86ThreeDNowKind::Pi2Fw),
            0x0D => Some(X86ThreeDNowKind::Pi2Fd),
            0x1C => Some(X86ThreeDNowKind::Pf2Iw),
            0x1D => Some(X86ThreeDNowKind::Pf2Id),
            0x8A => Some(X86ThreeDNowKind::PfNAcc),
            0x8E => Some(X86ThreeDNowKind::PfPNAcc),
            0x90 => Some(X86ThreeDNowKind::PfCmpGe),
            0x94 => Some(X86ThreeDNowKind::PfMin),
            0x96 => Some(X86ThreeDNowKind::PfRcp),
            0x97 => Some(X86ThreeDNowKind::PfRsqrt),
            0x9A => Some(X86ThreeDNowKind::PfSub),
            0x9E => Some(X86ThreeDNowKind::PfAdd),
            0xA0 => Some(X86ThreeDNowKind::PfCmpGt),
            0xA4 => Some(X86ThreeDNowKind::PfMax),
            0xA6 => Some(X86ThreeDNowKind::PfRcpIt1),
            0xA7 => Some(X86ThreeDNowKind::PfRsqIt1),
            0xAA => Some(X86ThreeDNowKind::PfSubR),
            0xAE => Some(X86ThreeDNowKind::PfAcc),
            0xB0 => Some(X86ThreeDNowKind::PfCmpEq),
            0xB4 => Some(X86ThreeDNowKind::PfMul),
            0xB6 => Some(X86ThreeDNowKind::PfRcpIt2),
            0xB7 => Some(X86ThreeDNowKind::PmulHrw),
            0xBB | 0xBF => None,
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("3DNow! suffix 0x{suffix:02X}"),
                });
            }
        };
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V64,
                },
            ));
            loaded
        } else {
            // 3DNow! has only the eight legacy MM registers. REX.R/REX.B do
            // not extend either register field.
            self.mm(modrm.rm & 7)
        };
        let dst = self.mm(modrm.reg & 7);
        let kind = match suffix {
            0xBB => OpKind::X86PackedShuffleImm {
                dst,
                src,
                width: VecWidth::V64,
                elem: VecElementType::I32,
                imm: 0x01,
                high_words: None,
            },
            0xBF => {
                Self::packed_unsigned_average_kind(dst, dst, src, VecWidth::V64, VecElementType::I8)
            }
            _ => OpKind::X86ThreeDNow {
                dst,
                src1: dst,
                src2: src,
                kind: three_d_now_kind.expect("defined non-generic 3DNow! suffix"),
            },
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }


    /// Lift 0F-prefixed (two-byte) opcodes
    pub(crate) fn lift_0f_opcode(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        map_len: usize,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + map_len.saturating_sub(1),
                need: prefix.cursor + map_len,
            });
        }

        let opcode2 = bytes[0];
        let after_opcode = &bytes[1..];
        let prefix2 = X86Prefix {
            cursor: prefix.cursor + map_len,
            ..prefix.clone()
        };

        match opcode2 {
            0x01 => self.lift_xcr_0f01(after_opcode, &prefix2, pc),

            // Cache-maintenance instructions modeled as no-ops by the base
            // emulator profile.
            0x08 | 0x09 => Ok(LiftResult::fallthrough(vec![], prefix2.cursor)),

            // 3DNow! uses the final imm8 after ModR/M and any displacement as
            // an opcode suffix.
            0x0F => self.lift_3dnow(after_opcode, &prefix2, pc, ctx),

            // EMMS marks every x87/MMX register empty while preserving the
            // aliased payloads. FEMMS performs the same defined tag transition
            // but leaves those payloads architecturally undefined; retaining
            // them is one permitted deterministic outcome.
            0x0E | 0x77 => {
                if prefix2.lock || prefix2.rex2.is_some() {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: vec![opcode2],
                    });
                }
                Ok(LiftResult::fallthrough(
                    vec![SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::X86X87Control {
                            kind: X86X87ControlKind::EmptyMmx,
                            addr: None,
                        },
                    )],
                    prefix2.cursor,
                ))
            }

            // NOP/cache/prefetch hint encodings still consume a complete
            // ModR/M addressing form even though they have no state effect.
            0x0D | 0x18 | 0x1A | 0x1B | 0x1E | 0x1F => {
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                if opcode2 == 0x1E
                    && prefix2.rep_prefix == Some(0xF3)
                    && !modrm.is_memory
                    && ((modrm.byte >> 3) & 7) == 1
                {
                    return Ok(LiftResult {
                        ops: vec![],
                        bytes_consumed: prefix2.cursor + modrm.bytes_consumed,
                        control_flow: ControlFlow::Trap {
                            kind: TrapKind::InvalidOpcode,
                        },
                        branch_targets: vec![],
                    });
                }
                Ok(LiftResult::fallthrough(
                    vec![],
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // CLDEMOTE m8 (0F 1C /0). It is an architectural hint with no
            // memory-address exceptions; register forms and nonzero ModR/M.reg
            // encodings are observed fallthrough hints on supported silicon.
            0x1C => {
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                if prefix2.lock {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: after_opcode[..modrm.bytes_consumed.min(after_opcode.len())]
                            .to_vec(),
                    });
                }
                if modrm.is_memory && ((modrm.byte >> 3) & 7) == 0 {
                    let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;
                    let (addr, mut ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86CacheControl {
                            addr,
                            kind: X86CacheControlKind::Cldemote,
                        },
                    ));
                    Ok(LiftResult::fallthrough(
                        ops,
                        prefix2.cursor + modrm.bytes_consumed,
                    ))
                } else {
                    Ok(LiftResult::fallthrough(
                        vec![],
                        prefix2.cursor + modrm.bytes_consumed,
                    ))
                }
            }

            // Jcc rel32 (0F 80 - 0F 8F)
            0x80..=0x8F => {
                if after_opcode.len() < 4 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: prefix2.cursor + after_opcode.len(),
                        need: prefix2.cursor + 4,
                    });
                }

                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);
                let rel = i32::from_le_bytes([
                    after_opcode[0],
                    after_opcode[1],
                    after_opcode[2],
                    after_opcode[3],
                ]) as i64;

                let insn_len = prefix2.cursor + 4;
                let next_pc = pc + insn_len as u64;
                let target = (next_pc as i64 + rel) as u64;

                Ok(LiftResult::cond_branch(
                    vec![],
                    insn_len,
                    cond,
                    target,
                    next_pc,
                ))
            }

            // SETcc (0F 90 - 0F 9F)
            0x90..=0x9F => {
                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);

                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::SetCC {
                            dst: tmp,
                            cond,
                            width: OpWidth::W8,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: tmp,
                            addr,
                            width: MemWidth::B1,
                        },
                    ));
                } else {
                    let high_dst = self.high_byte_base(modrm.rm, &prefix2);
                    let dst = if high_dst.is_some() {
                        ctx.alloc_vreg()
                    } else {
                        self.gpr(modrm.rm)
                    };
                    ops.push(SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::SetCC {
                            dst,
                            cond,
                            width: OpWidth::W8,
                        },
                    ));
                    if let Some(base) = high_dst {
                        self.merge_high_byte(base, dst, pc, ctx, &mut ops);
                    }
                }

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // CMOVcc (0F 40 - 0F 4F)
            0x40..=0x4F => {
                let cc = opcode2 & 0x0F;
                let cond = self.x86_cond(cc);
                let op_size = prefix.op_size();
                let width = self.size_to_width(op_size);

                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: self.size_to_memwidth(op_size),
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::CMove {
                        dst: self.gpr(modrm.reg),
                        src,
                        cond,
                        width,
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // Packed immediate dword/high-word/low-word shuffles.
            0x70 => self.lift_sse_packed_shuffle_imm(after_opcode, &prefix2, pc, ctx),
            0x5B | 0xE6 => {
                self.lift_sse_packed_int_fp_convert(opcode2, after_opcode, &prefix2, pc, ctx)
            }
            0x52 | 0x53 => self.lift_sse_fp_estimate(opcode2, after_opcode, &prefix2, pc, ctx),
            0x14 | 0x15 => self.lift_sse_fp_unpack(opcode2, after_opcode, &prefix2, pc, ctx),
            0x12 | 0x13 | 0x16 | 0x17 if prefix2.rep_prefix.is_none() => {
                self.lift_sse_half_move(opcode2, after_opcode, &prefix2, pc, ctx)
            }
            0x2B | 0xE7 => self.lift_sse_movnt(opcode2, after_opcode, &prefix2, pc, ctx),
            0xC2 => self.lift_sse_fp_compare(after_opcode, &prefix2, pc, ctx),
            0xC6 => self.lift_sse_two_source_shuffle_imm(after_opcode, &prefix2, pc, ctx),
            0x12 | 0x16 if prefix2.rep_prefix == Some(0xF3) => {
                self.lift_sse_duplicate_move(opcode2, after_opcode, &prefix2, pc, ctx)
            }
            0x12 if prefix2.rep_prefix == Some(0xF2) => {
                self.lift_sse_duplicate_move(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Prefix-free MMX MOVQ and packed legacy SSE/SSE2 moves.
            0x10 | 0x11 | 0x28 | 0x29 | 0x6F | 0x7F => {
                self.lift_sse_mov(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Scalar MOVQ vector load/store forms.
            0x7E if prefix2.rep_prefix == Some(0xF3) => {
                self.lift_sse_movq_vec(opcode2, after_opcode, &prefix2, pc, ctx)
            }
            0xD6 if prefix2.operand_size_override => {
                self.lift_sse_movq_vec(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // MOVD/MOVQ between XMM or MMX and GPR/memory operands.
            0x6E | 0x7E => self.lift_sse_movd_q(opcode2, after_opcode, &prefix2, pc, ctx),

            // MOVMSKPS/MOVMSKPD sign-bit extraction.
            0x50 => self.lift_sse_movmask(after_opcode, &prefix2, pc, ctx),

            // PMOVMSKB byte sign-bit extraction. Prefix-free forms target MMX.
            0xD7 => self.lift_sse_pmovmskb(after_opcode, &prefix2, pc, ctx),

            // Packed low-word multiply. Prefix-free forms target MMX.
            0xD5 => self.lift_sse_pmullw(after_opcode, &prefix2, pc, ctx),

            // Packed signed/unsigned high-word multiply. Prefix-free forms target MMX.
            0xE4 | 0xE5 => self.lift_sse_pmul_high_word(opcode2, after_opcode, &prefix2, pc, ctx),

            // Packed rounded unsigned averages. Prefix-free forms target MMX.
            0xE0 | 0xE3 => self.lift_sse_packed_average(opcode2, after_opcode, &prefix2, pc, ctx),

            // Original packed unsigned-byte and signed-word min/max forms.
            // Prefix-free encodings target MMX state.
            0xDA | 0xDE | 0xEA | 0xEE => {
                self.lift_sse_packed_minmax(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Packed unsigned dword-to-qword multiply. Prefix-free forms target MMX.
            0xF4 => self.lift_sse_pmuldq(after_opcode, &prefix2, false, pc, ctx),

            // Pairwise signed-word multiply-add. Prefix-free forms target MMX.
            0xF5 => self.lift_sse_pmaddwd(after_opcode, &prefix2, pc, ctx),

            // SSE3 alternating and horizontal packed floating-point arithmetic.
            0x7C | 0x7D | 0xD0 => {
                self.lift_sse_addsub_horizontal(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Byte-selective store through the implicit DS:(E)DI/RDI address.
            // Prefix-free forms target MMX state.
            0xF7 => self.lift_sse_maskmovdqu(after_opcode, &prefix2, pc, ctx),

            // Packed shifts by the low 64 bits of an XMM/m128 count source.
            // Prefix-free forms target MMX state.
            0xD1..=0xD3 | 0xE1 | 0xE2 | 0xF1..=0xF3 => {
                self.lift_sse_packed_shift_count(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Packed element and 128-bit-lane shifts by an immediate count.
            // Prefix-free forms target MMX state.
            0x71..=0x73 => self.lift_sse_packed_shift_imm(opcode2, after_opcode, &prefix2, pc, ctx),

            // MOVNTI non-temporal scalar GPR store.
            0xC3 => self.lift_movnti(after_opcode, &prefix2, pc, ctx),

            // LDDQU unaligned 128-bit integer load.
            0xF0 => self.lift_sse_lddqu(after_opcode, &prefix2, pc, ctx),

            // MMX/XMM packed-integer logical operations.
            0xDB | 0xDF | 0xEB | 0xEF => {
                self.lift_sse_integer_logic(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Packed integer equality and signed greater-than comparisons.
            0x64 | 0x65 | 0x66 | 0x74 | 0x75 | 0x76 => {
                self.lift_sse_integer_compare(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Packed integer low/high interleaves.
            0x60 | 0x61 | 0x62 | 0x68 | 0x69 | 0x6A | 0x6C | 0x6D => {
                self.lift_sse_integer_unpack(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // XMM signed/unsigned saturating packs. Prefix-free forms target MMX.
            0x63 | 0x67 | 0x6B => {
                self.lift_sse_integer_pack(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Packed sums of absolute byte differences. Prefix-free forms
            // target MMX state.
            0xF6 => self.lift_sse_psadbw(after_opcode, &prefix2, pc, ctx),

            // Scalar ordered/unordered FP compare setting integer flags.
            0x2E | 0x2F => self.lift_sse_comi(opcode2, after_opcode, &prefix2, pc, ctx),

            // Scalar FP-to-signed-integer conversions.
            0x2C | 0x2D => self.lift_sse_fp_to_int(opcode2, after_opcode, &prefix2, pc, ctx),

            // Scalar signed-integer-to-FP conversions.
            0x2A => self.lift_sse_int_to_fp(after_opcode, &prefix2, pc, ctx),

            // Packed legacy SSE/SSE2 floating-point arithmetic.
            0x58 | 0x59 | 0x5C | 0x5D | 0x5E | 0x5F => {
                self.lift_sse_packed_arith(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // Scalar single/double precision conversion.
            0x5A => self.lift_sse_scalar_fp_convert(after_opcode, &prefix2, pc, ctx),

            // Packed and scalar SQRTPS/SQRTPD/SQRTSS/SQRTSD.
            0x51 => self.lift_sse_sqrt(after_opcode, &prefix2, pc, ctx),

            // Packed legacy SSE/SSE2 bitwise logic.
            0x54 | 0x55 | 0x56 | 0x57 => {
                self.lift_sse_logic(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // XMM wrapping and saturating packed integer add/subtract.
            0xD4 | 0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED | 0xF8 | 0xF9 | 0xFA
            | 0xFB | 0xFC | 0xFD | 0xFE => {
                self.lift_sse_packed_add_sub(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // SSE4.1 opcodes (0F 38)
            0x38 if prefix2.rex2_m() => Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            }),
            0x38 => self.lift_0f38_opcode(after_opcode, &prefix2, pc, ctx),
            0x3A if prefix2.rex2_m() => Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            }),
            0x3A => self.lift_0f3a_opcode(after_opcode, &prefix2, pc, ctx),

            // SHLD/SHRD (0F A4/A5/AC/AD)
            0xA4 | 0xA5 | 0xAC | 0xAD => {
                self.lift_shld_shrd(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // BT/BTS/BTR/BTC register-index and immediate Group-8 forms.
            0xA3 | 0xAB | 0xB3 | 0xBB => {
                self.lift_bit_test_reg(opcode2, after_opcode, &prefix2, pc, ctx)
            }
            0xBA => self.lift_bit_test_imm(after_opcode, &prefix2, pc, ctx),

            // LFENCE/MFENCE/SFENCE (0F AE /5,/6,/7 register encodings).
            0xAE => self.lift_fence_0f(after_opcode, &prefix2, pc, ctx),

            // CMPXCHG r/m, r (0F B0/0F B1)
            0xB0 | 0xB1 => self.lift_cmpxchg(opcode2, after_opcode, &prefix2, pc, ctx),

            // XADD r/m, r (0F C0/0F C1)
            0xC0 | 0xC1 => self.lift_xadd(opcode2, after_opcode, &prefix2, pc, ctx),

            // SSE2 word insert/extract forms. Prefix-free encodings target MMX.
            0xC4 | 0xC5 => self.lift_sse_pinsrw_pextrw(opcode2, after_opcode, &prefix2, pc, ctx),

            // Group 9 CMPXCHG, compacted XSAVE, random, and RDPID forms.
            0xC7 => self.lift_xsave_group9_0fc7(after_opcode, &prefix2, pc, ctx),

            // BSWAP r32/r64 (0F C8+rd)
            0xC8..=0xCF => self.lift_bswap_opcode(opcode2, &prefix2, pc),

            // MOVZX r, r/m8 (0F B6)
            0xB6 => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;
                let src_is_rex_byte_reg = !modrm.is_memory && prefix2.has_rex();
                let src_is_legacy_high_byte =
                    !modrm.is_memory && !prefix2.has_rex() && (4..=7).contains(&(modrm.rm & 7));

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B1,
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else if src_is_legacy_high_byte {
                    self.gpr((modrm.rm & 7) - 4)
                } else {
                    self.gpr(modrm.rm)
                };

                let mut op = SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::ZeroExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W8,
                        to_width: self.size_to_width(op_size),
                    },
                );
                if src_is_rex_byte_reg {
                    op.x86_hint = Some(X86OpHint::RexByteReg);
                } else if src_is_legacy_high_byte {
                    op.x86_hint = Some(X86OpHint::LegacyHighByteReg);
                }
                ops.push(op);

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVZX r, r/m16 (0F B7)
            0xB7 => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B2,
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::ZeroExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W16,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // POPCNT/TZCNT/LZCNT (mandatory F3 prefix).
            0xB8 | 0xBC | 0xBD if prefix2.rep_prefix == Some(0xF3) => {
                self.lift_count_0f(opcode2, after_opcode, &prefix2, pc, ctx)
            }

            // BSF/BSR (0F BC/0F BD without F3)
            0xBC | 0xBD => self.lift_bsf_bsr(opcode2, after_opcode, &prefix2, pc, ctx),

            // MOVSX r, r/m8 (0F BE)
            0xBE => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;
                let src_is_rex_byte_reg = !modrm.is_memory && prefix2.has_rex();
                let src_is_legacy_high_byte =
                    !modrm.is_memory && !prefix2.has_rex() && (4..=7).contains(&(modrm.rm & 7));

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B1,
                            sign: SignExtend::Sign,
                        },
                    ));
                    tmp
                } else if src_is_legacy_high_byte {
                    self.gpr((modrm.rm & 7) - 4)
                } else {
                    self.gpr(modrm.rm)
                };

                let mut op = SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::SignExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W8,
                        to_width: self.size_to_width(op_size),
                    },
                );
                if src_is_rex_byte_reg {
                    op.x86_hint = Some(X86OpHint::RexByteReg);
                } else if src_is_legacy_high_byte {
                    op.x86_hint = Some(X86OpHint::LegacyHighByteReg);
                }
                ops.push(op);

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // MOVSX r, r/m16 (0F BF)
            0xBF => {
                let op_size = prefix.op_size();
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: MemWidth::B2,
                            sign: SignExtend::Sign,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::SignExtend {
                        dst: self.gpr(modrm.reg),
                        src,
                        from_width: OpWidth::W16,
                        to_width: self.size_to_width(op_size),
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // IMUL r, r/m (0F AF)
            0xAF => {
                let op_size = prefix.op_size();
                let width = self.size_to_width(op_size);
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                let mut ops = Vec::new();
                let next_pc = pc + prefix2.cursor as u64 + modrm.bytes_consumed as u64;

                let src = if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let tmp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: tmp,
                            addr,
                            width: self.size_to_memwidth(op_size),
                            sign: SignExtend::Zero,
                        },
                    ));
                    tmp
                } else {
                    self.gpr(modrm.rm)
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::MulS {
                        dst_lo: self.gpr(modrm.reg),
                        dst_hi: None,
                        src1: self.gpr(modrm.reg),
                        src2: SrcOperand::Reg(src),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));

                Ok(LiftResult::fallthrough(
                    ops,
                    prefix2.cursor + modrm.bytes_consumed,
                ))
            }

            // SYSCALL (0F 05)
            0x05 => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix2.cursor,
                control_flow: ControlFlow::Syscall,
                branch_targets: vec![],
            }),

            // RDTSC (0F 31): EDX:EAX := time-stamp counter.
            0x31 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86ReadTsc {
                        dst_lo: self.gpr(0),
                        dst_hi: self.gpr(2),
                    },
                )],
                prefix2.cursor,
            )),

            // UD2 (0F 0B): architecturally guaranteed invalid opcode trap.
            0x0B => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix2.cursor,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            }),

            // RSM is invalid outside SMM; SMIR exposes no SMM execution state.
            0xAA => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix2.cursor,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            }),

            // UD1 (0F B9 /r) always traps but consumes its ModR/M address form.
            0xB9 => {
                let modrm = decode_modrm(after_opcode, &prefix2, pc)?;
                Ok(LiftResult {
                    ops: vec![],
                    bytes_consumed: prefix2.cursor + modrm.bytes_consumed,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode,
                    },
                    branch_targets: vec![],
                })
            }

            // SYSRET (0F 07)
            0x07 => {
                // Treat as return for lifting purposes
                Ok(LiftResult::ret(vec![], prefix2.cursor))
            }

            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x0F 0x{:02X}", opcode2),
                    })
                } else {
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix2.cursor,
                    ))
                }
            }
        }
    }
}
