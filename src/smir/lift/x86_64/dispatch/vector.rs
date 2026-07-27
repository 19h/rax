//! vector.rs

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
                0x41 | 0x42 | 0x44..=0x47 | 0x4A | 0x4B | 0x90..=0x93 | 0x98 | 0x99
                    if prefix.encoding == VecEncodingKind::Vex =>
                {
                    self.lift_vex_opmask(prefix, opcode, bytes, pc, ctx)
                }
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
                    if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
                {
                    self.lift_vec_packed_fp_arithmetic(prefix, opcode, bytes, pc, ctx)
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
                0x2E | 0x2F => self.lift_vec_fp_flag_compare(prefix, opcode, bytes, pc, ctx),

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
                    self.lift_vec_packed_fp_precision_convert(prefix, bytes, pc, ctx)
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
                0x51 => self.lift_vec_sqrt(prefix, bytes, pc, ctx),

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
                    self.lift_vec_scalar_fp_arithmetic(prefix, opcode, bytes, pc, ctx)
                }

                // VEX.LZ.WIG VLDMXCSR/VSTMXCSR (VEX.0F.AE /2,/3).
                0xAE => {
                    if prefix.encoding != VecEncodingKind::Vex
                        || prefix.pp != X86SsePrefix::None
                        || prefix.width != VecWidth::V128
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
            X86VecMap::Map0F38 => self.lift_vector_map0f38(prefix, opcode, bytes, pc, ctx),
            X86VecMap::Map0F3A => self.lift_vector_map0f3a(prefix, opcode, bytes, pc, ctx),
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
                0x68 | 0x69 | 0x6A | 0x6B
                    if matches!(
                        prefix.pp,
                        X86SsePrefix::None | X86SsePrefix::OpSize | X86SsePrefix::Repne
                    ) =>
                {
                    self.lift_evex_saturating_fp_to_int(prefix, opcode, bytes, pc, ctx)
                }
                0x6C | 0x6D if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) => {
                    self.lift_evex_saturating_fp_to_int(prefix, opcode, bytes, pc, ctx)
                }
                0x6C | 0x6D if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                    self.lift_evex_scalar_saturating_fp_to_int(prefix, opcode, bytes, pc, ctx)
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
}
