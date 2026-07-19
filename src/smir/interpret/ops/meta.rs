//! Meta/debug op execution

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86Sha32Op, X86ThreeDNowKind,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_meta(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // META / DEBUG
            // ==================================================================
            OpKind::Nop => {}

            OpKind::Undefined { opcode } => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: *opcode,
                });
            }

            OpKind::Breakpoint => {
                ctx.request_exit(ExitReason::Breakpoint { addr: ctx.pc });
            }

            OpKind::VShuffleBitQM {
                dst,
                src,
                indices,
                mask: write_mask,
                width,
            } => {
                let src_val = Self::read_vec(ctx, *src);
                let idx_val = Self::read_vec(ctx, *indices);
                let mut result = 0u64;
                let bytes = width.bytes();

                for qword_idx in 0..(bytes / 8) {
                    let lane_base = (qword_idx * 8) as u8;
                    let qword = Self::get_lane(&src_val, qword_idx as u8, 64);
                    for byte_idx in 0..8 {
                        let idx = Self::get_lane(&idx_val, lane_base + byte_idx as u8, 8) & 0x3f;
                        let bit = (qword >> idx) & 1;
                        result |= bit << (qword_idx * 8 + byte_idx);
                    }
                }

                let mask = if bytes >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bytes) - 1
                };
                let write_mask = write_mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                ctx.write_vreg(*dst, result & mask & write_mask);
            }

            OpKind::VCompress {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut output = 0u8;
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let value = Self::get_lane(&source, lane, bits);
                        Self::set_lane(&mut result, output, bits, value);
                        output += 1;
                    }
                }
                if !zeroing {
                    for lane in output..lanes {
                        let value = Self::get_lane(&old, lane, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VExpand {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut input = 0u8;
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let value = Self::get_lane(&source, input, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                        input += 1;
                    } else if !zeroing {
                        let value = Self::get_lane(&old, lane, bits);
                        Self::set_lane(&mut result, lane, bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86NarrowInt {
                dst,
                src,
                mask,
                src_elem,
                dst_elem,
                width,
                mode,
                zeroing,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let control = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let lanes = width.lanes(*src_elem) as u8;
                let src_bits = src_elem.bytes() * 8;
                let dst_bits = dst_elem.bytes() * 8;
                let dst_mask = if dst_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << dst_bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    if control & (1u64 << lane) != 0 {
                        let raw = Self::get_lane(&source, lane, src_bits);
                        let shift = 128 - src_bits;
                        let signed = (i128::from(raw) << shift) >> shift;
                        let value = match mode {
                            X86NarrowMode::Truncate => raw & dst_mask,
                            X86NarrowMode::SignedSaturate => {
                                let low = -(1i128 << (dst_bits - 1));
                                let high = (1i128 << (dst_bits - 1)) - 1;
                                signed.clamp(low, high) as u64 & dst_mask
                            }
                            X86NarrowMode::UnsignedSaturate => raw.min(dst_mask),
                        };
                        Self::set_lane(&mut result, lane, dst_bits, value);
                    } else if !zeroing {
                        let value = Self::get_lane(&old, lane, dst_bits);
                        Self::set_lane(&mut result, lane, dst_bits, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

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
            } => {
                debug_assert!(matches!(src_elem, VecElementType::I8 | VecElementType::I16));
                debug_assert!(matches!(
                    acc_elem,
                    VecElementType::I16 | VecElementType::I32
                ));
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let src_bits = src_elem.bytes() * 8;
                let acc_bits = acc_elem.bytes() * 8;
                debug_assert!(acc_bits >= src_bits && acc_bits % src_bits == 0);

                // Snapshot every input before writing `dst`: VNNI normally aliases
                // dst/acc, while PMADDUBSW and PMADDWD can alias either multiplicand.
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let terms = acc_bits / src_bits;
                let lanes = width.lanes(*acc_elem) as u8;
                let src_mask = (1u64 << src_bits) - 1;
                let acc_mask = (1u64 << acc_bits) - 1;
                let signed = |value: u64, bits: u32| -> i128 {
                    let shift = 128 - bits;
                    ((i128::from(value) << shift) >> shift) as i128
                };
                let acc_low = -(1i128 << (acc_bits - 1));
                let acc_high = (1i128 << (acc_bits - 1)) - 1;
                let mut result = [0u64; 16];

                for lane in 0..lanes {
                    let mut sum = signed(Self::get_lane(&accumulator, lane, acc_bits), acc_bits);
                    let first_term = u32::from(lane) * terms;
                    for term in 0..terms {
                        let source_lane = (first_term + term) as u8;
                        let a_raw = Self::get_lane(&first, source_lane, src_bits) & src_mask;
                        let b_raw = Self::get_lane(&second, source_lane, src_bits) & src_mask;
                        let a = if *src1_unsigned {
                            i128::from(a_raw)
                        } else {
                            signed(a_raw, src_bits)
                        };
                        let b = signed(b_raw, src_bits);
                        sum += a * b;
                    }
                    let narrowed = if *saturate {
                        sum.clamp(acc_low, acc_high)
                    } else {
                        sum
                    };
                    Self::set_lane(&mut result, lane, acc_bits, narrowed as u64 & acc_mask);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *acc_elem,
                );
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                width,
                imm,
                zeroing,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot both inputs before writing because both the legacy
                // and non-destructive register forms may alias the destination.
                // AVX10.2 merge masking also reads the pre-instruction dst.
                let blocks = match width {
                    VecWidth::V128 => 1u8,
                    VecWidth::V256 => 2,
                    VecWidth::V512 => 4,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let old_dst = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for block in 0..blocks {
                    // The low imm3 controls even-numbered 128-bit lanes and
                    // the high imm3 controls odd-numbered lanes. AVX10.2
                    // repeats the pair for lanes 2 and 3 at VL=512.
                    let control = if block & 1 == 0 { *imm } else { *imm >> 3 };
                    let first_select = ((control >> 2) & 1) * 4;
                    let second_select = (control & 3) * 4;
                    let block_base = block * 16;
                    for output in 0..8u8 {
                        let mut sum = 0u16;
                        for tap in 0..4u8 {
                            let first_byte =
                                Self::get_lane(&first, block_base + first_select + output + tap, 8)
                                    as u8;
                            let second_byte =
                                Self::get_lane(&second, block_base + second_select + tap, 8) as u8;
                            sum += u16::from(first_byte.abs_diff(second_byte));
                        }
                        Self::set_lane(&mut result, block * 8 + output, 16, u64::from(sum));
                    }
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old_dst,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I16,
                );
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VSadBytes {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot both inputs before writing: every register form may
                // alias the destination architecturally.
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for block in 0..(width.bytes() / 8) as u8 {
                    let mut sum = 0u16;
                    for byte in 0..8u8 {
                        let lane = block * 8 + byte;
                        let a = Self::get_lane(&first, lane, 8) as u8;
                        let b = Self::get_lane(&second, lane, 8) as u8;
                        sum += u16::from(a.abs_diff(b));
                    }
                    Self::set_lane(&mut result, block, 64, u64::from(sum));
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::X86Phminposuw { dst, src } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                // Snapshot before writing because the two-operand source may
                // alias the destination in both legacy and VEX encodings.
                let source = Self::read_vec(ctx, *src);
                let mut minimum = Self::get_lane(&source, 0, 16) as u16;
                let mut index = 0u8;
                for lane in 1..8u8 {
                    let candidate = Self::get_lane(&source, lane, 16) as u16;
                    if candidate < minimum {
                        minimum = candidate;
                        index = lane;
                    }
                }
                let mut result = [0u64; 16];
                result[0] = u64::from(minimum) | (u64::from(index) << 16);
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::X86MovMask {
                dst,
                src,
                elem,
                lanes,
                dst_width,
            } => {
                let source = Self::read_vec(ctx, *src);
                let lane_bits = elem.bytes() * 8;
                let mut mask = 0u64;
                for lane in 0..*lanes {
                    let sign = Self::get_lane(&source, lane, lane_bits) >> (lane_bits - 1);
                    mask |= (sign & 1) << lane;
                }
                Self::write_x86_partial(ctx, *dst, mask, *dst_width);
            }

            OpKind::X86MovdQ {
                dst,
                src,
                width,
                zero_upper,
            } => {
                if matches!(
                    dst,
                    VReg::Arch(ArchReg::X86(X86Reg::Mm(_) | X86Reg::Xmm(_)))
                ) {
                    let scalar = ctx.read_vreg(*src) & width.mask();
                    let old = Self::read_vec(ctx, *dst);
                    let mut result = if *zero_upper { [0; 16] } else { old };
                    result[0] = scalar;
                    result[1] = 0;
                    Self::write_vec(ctx, *dst, result);
                } else {
                    let scalar = Self::read_vec(ctx, *src)[0] & width.mask();
                    Self::write_x86_partial(ctx, *dst, scalar, *width);
                }
            }

            OpKind::X86Aes {
                dst,
                src1,
                src2,
                width,
                op,
                imm,
            } => {
                use crate::isa::x86_64::execute::crypto::aes;

                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let mut result = [0u64; 16];
                for lane in 0..(width.bytes() / 16) as usize {
                    let word = lane * 2;
                    let (lo, hi) = match op {
                        X86AesOp::Enc => aes::aesenc(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::EncLast => aes::aesenclast(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::Dec => aes::aesdec(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::DecLast => aes::aesdeclast(
                            first[word],
                            first[word + 1],
                            second.as_ref().unwrap()[word],
                            second.as_ref().unwrap()[word + 1],
                        ),
                        X86AesOp::InvMixColumns => aes::aesimc(first[word], first[word + 1]),
                        X86AesOp::KeygenAssist => {
                            aes::aeskeygenassist(first[word], first[word + 1], *imm)
                        }
                    };
                    result[word] = lo;
                    result[word + 1] = hi;
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha32 {
                dst,
                src1,
                src2,
                wk,
                op: sha_op,
                imm,
            } => {
                use crate::isa::x86_64::execute::crypto::sha;

                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let value = match sha_op {
                    X86Sha32Op::Sha1Nexte => {
                        sha::sha1nexte(first[0], first[1], second[0], second[1])
                    }
                    X86Sha32Op::Sha1Msg1 => sha::sha1msg1(first[0], first[1], second[0], second[1]),
                    X86Sha32Op::Sha1Msg2 => sha::sha1msg2(first[0], first[1], second[0], second[1]),
                    X86Sha32Op::Sha1Rounds4 => {
                        sha::sha1rnds4(first[0], first[1], second[0], second[1], *imm)
                    }
                    X86Sha32Op::Sha256Msg1 => {
                        sha::sha256msg1(first[0], first[1], second[0], second[1])
                    }
                    X86Sha32Op::Sha256Msg2 => {
                        sha::sha256msg2(first[0], first[1], second[0], second[1])
                    }
                    X86Sha32Op::Sha256Rounds2 => {
                        let Some(wk) = wk else {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: op.guest_pc,
                                opcode: 0,
                            });
                            return Ok(());
                        };
                        let work = Self::read_vec(ctx, *wk);
                        sha::sha256rnds2(first[0], first[1], second[0], second[1], work[0])
                    }
                };
                let mut result = [0u64; 16];
                result[0] = value.0;
                result[1] = value.1;
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Msg1 { dst, src } => {
                let old = Self::read_vec(ctx, *dst);
                let source = Self::read_vec(ctx, *src);
                let sigma0 = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
                let mut result = [0u64; 16];
                result[0] = old[0].wrapping_add(sigma0(old[1]));
                result[1] = old[1].wrapping_add(sigma0(old[2]));
                result[2] = old[2].wrapping_add(sigma0(old[3]));
                result[3] = old[3].wrapping_add(sigma0(source[0]));
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Msg2 { dst, src } => {
                let old = Self::read_vec(ctx, *dst);
                let source = Self::read_vec(ctx, *src);
                let sigma1 = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
                let w16 = old[0].wrapping_add(sigma1(source[2]));
                let w17 = old[1].wrapping_add(sigma1(source[3]));
                let w18 = old[2].wrapping_add(sigma1(w16));
                let w19 = old[3].wrapping_add(sigma1(w17));
                let mut result = [0u64; 16];
                result[..4].copy_from_slice(&[w16, w17, w18, w19]);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sha512Rounds2 { dst, state, wk } => {
                let cdgh = Self::read_vec(ctx, *dst);
                let abef = Self::read_vec(ctx, *state);
                let constants = Self::read_vec(ctx, *wk);
                let mut a = abef[3];
                let mut b = abef[2];
                let mut c = cdgh[3];
                let mut d = cdgh[2];
                let mut e = abef[1];
                let mut f = abef[0];
                let mut g = cdgh[1];
                let mut h = cdgh[0];
                for &round_constant in &constants[..2] {
                    let choose = (e & f) ^ (g & !e);
                    let majority = (a & b) ^ (a & c) ^ (b & c);
                    let big1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
                    let big0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
                    let t1 = choose
                        .wrapping_add(big1)
                        .wrapping_add(round_constant)
                        .wrapping_add(h);
                    let next_a = t1.wrapping_add(majority).wrapping_add(big0);
                    let next_e = t1.wrapping_add(d);
                    (h, g, f, e, d, c, b, a) = (g, f, e, next_e, c, b, a, next_a);
                }
                let mut result = [0u64; 16];
                result[..4].copy_from_slice(&[f, e, b, a]);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Msg1 { dst, src1, src2 } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let p1 = |x: u32| x ^ x.rotate_left(15) ^ x.rotate_left(23);
                let mut result = [0u64; 16];
                for index in 0..4u8 {
                    let mut tmp = lane(&old, index) ^ lane(&second, index);
                    if index < 3 {
                        tmp ^= lane(&first, index).rotate_left(15);
                    }
                    Self::set_lane(&mut result, index, 32, u64::from(p1(tmp)));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Msg2 { dst, src1, src2 } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let mut words = [0u32; 4];
                for index in 0..4u8 {
                    words[index as usize] = lane(&first, index).rotate_left(7)
                        ^ lane(&second, index)
                        ^ lane(&old, index);
                }
                words[3] ^=
                    words[0].rotate_left(6) ^ words[0].rotate_left(15) ^ words[0].rotate_left(30);
                let mut result = [0u64; 16];
                for (index, value) in words.into_iter().enumerate() {
                    Self::set_lane(&mut result, index as u8, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm3Rounds2 {
                dst,
                state,
                words,
                imm,
            } => {
                let cdgh = Self::read_vec(ctx, *dst);
                let abef = Self::read_vec(ctx, *state);
                let message = Self::read_vec(ctx, *words);
                let lane = |value: &VecValue, index| Self::get_lane(value, index, 32) as u32;
                let mut a = lane(&abef, 3);
                let mut b = lane(&abef, 2);
                let mut c = lane(&cdgh, 3).rotate_left(9);
                let mut d = lane(&cdgh, 2).rotate_left(9);
                let mut e = lane(&abef, 1);
                let mut f = lane(&abef, 0);
                let mut g = lane(&cdgh, 1).rotate_left(19);
                let mut h = lane(&cdgh, 0).rotate_left(19);
                let w = [
                    lane(&message, 0),
                    lane(&message, 1),
                    lane(&message, 2),
                    lane(&message, 3),
                ];
                let round = imm & 0x3E;
                let mut constant = if round < 16 {
                    0x79CC_4519u32
                } else {
                    0x7A87_9D8A
                }
                .rotate_left(u32::from(round));
                for index in 0..2usize {
                    let a12 = a.rotate_left(12);
                    let s1 = a12.wrapping_add(e).wrapping_add(constant).rotate_left(7);
                    let s2 = s1 ^ a12;
                    let ff = if round < 16 {
                        a ^ b ^ c
                    } else {
                        (a & b) | (a & c) | (b & c)
                    };
                    let gg = if round < 16 {
                        e ^ f ^ g
                    } else {
                        (e & f) | (!e & g)
                    };
                    let t1 = ff
                        .wrapping_add(d)
                        .wrapping_add(s2)
                        .wrapping_add(w[index] ^ w[index + 2]);
                    let t2 = gg.wrapping_add(h).wrapping_add(s1).wrapping_add(w[index]);
                    let next_e = t2 ^ t2.rotate_left(9) ^ t2.rotate_left(17);
                    (d, c, b, a) = (c, b.rotate_left(9), a, t1);
                    (h, g, f, e) = (g, f.rotate_left(19), e, next_e);
                    constant = constant.rotate_left(1);
                }
                let mut result = [0u64; 16];
                for (index, value) in [f, e, b, a].into_iter().enumerate() {
                    Self::set_lane(&mut result, index as u8, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Sm4 {
                dst,
                src1,
                src2,
                width,
                key_schedule,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let substitute = |value: u32| {
                    let bytes = value.to_le_bytes();
                    u32::from_le_bytes([
                        X86_SM4_SBOX[bytes[0] as usize],
                        X86_SM4_SBOX[bytes[1] as usize],
                        X86_SM4_SBOX[bytes[2] as usize],
                        X86_SM4_SBOX[bytes[3] as usize],
                    ])
                };
                let transform = |value: u32| {
                    let value = substitute(value);
                    if *key_schedule {
                        value ^ value.rotate_left(13) ^ value.rotate_left(23)
                    } else {
                        value
                            ^ value.rotate_left(2)
                            ^ value.rotate_left(10)
                            ^ value.rotate_left(18)
                            ^ value.rotate_left(24)
                    }
                };
                let groups = width.bytes() / 16;
                let mut result = [0u64; 16];
                for group in 0..groups as u8 {
                    let base = group * 4;
                    let p = [
                        Self::get_lane(&first, base, 32) as u32,
                        Self::get_lane(&first, base + 1, 32) as u32,
                        Self::get_lane(&first, base + 2, 32) as u32,
                        Self::get_lane(&first, base + 3, 32) as u32,
                    ];
                    let keys = [
                        Self::get_lane(&second, base, 32) as u32,
                        Self::get_lane(&second, base + 1, 32) as u32,
                        Self::get_lane(&second, base + 2, 32) as u32,
                        Self::get_lane(&second, base + 3, 32) as u32,
                    ];
                    let c0 = p[0] ^ transform(p[1] ^ p[2] ^ p[3] ^ keys[0]);
                    let c1 = p[1] ^ transform(p[2] ^ p[3] ^ c0 ^ keys[1]);
                    let c2 = p[2] ^ transform(p[3] ^ c0 ^ c1 ^ keys[2]);
                    let c3 = p[3] ^ transform(c0 ^ c1 ^ c2 ^ keys[3]);
                    for (lane, value) in [c0, c1, c2, c3].into_iter().enumerate() {
                        Self::set_lane(&mut result, base + lane as u8, 32, u64::from(value));
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Convert16ToFp32 {
                dst,
                src,
                width,
                fp16,
                odd,
                broadcast,
            } => {
                let packed = if *broadcast {
                    None
                } else {
                    Some(Self::read_vec(ctx, *src))
                };
                let scalar = if *broadcast {
                    ctx.read_vreg(*src) as u16
                } else {
                    0
                };
                let mut result = [0u64; 16];
                let lanes = width.lanes(VecElementType::F32) as u8;
                for lane in 0..lanes {
                    let input = if *broadcast {
                        scalar
                    } else {
                        Self::get_lane(packed.as_ref().unwrap(), lane * 2 + u8::from(*odd), 16)
                            as u16
                    };
                    let converted = if *fp16 {
                        Self::x86_fp16_to_fp32_bits(input)
                    } else {
                        u32::from(input) << 16
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(converted));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShiftImm {
                dst,
                src,
                width,
                elem,
                shift,
                amount,
                byte_lane,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                if *byte_lane {
                    let amount = usize::from(*amount);
                    for block in 0..(width.bytes() / 16) as usize {
                        for lane in 0..16usize {
                            let source_lane = match shift {
                                ShiftOp::Lsl => lane.checked_sub(amount),
                                ShiftOp::Lsr => {
                                    lane.checked_add(amount).filter(|index| *index < 16)
                                }
                                _ => unreachable!(),
                            };
                            if let Some(source_lane) = source_lane {
                                let value =
                                    Self::get_lane(&input, (block * 16 + source_lane) as u8, 8);
                                Self::set_lane(&mut result, (block * 16 + lane) as u8, 8, value);
                            }
                        }
                    }
                } else {
                    let bits = elem.bytes() * 8;
                    let lanes = width.lanes(*elem) as u8;
                    let amount = u32::from(*amount);
                    let mask = if bits == 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    for lane in 0..lanes {
                        let value = Self::get_lane(&input, lane, bits);
                        let shifted = if amount >= bits {
                            if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                                mask
                            } else {
                                0
                            }
                        } else {
                            match shift {
                                ShiftOp::Lsl => (value << amount) & mask,
                                ShiftOp::Lsr => value >> amount,
                                ShiftOp::Asr => {
                                    let signed = if bits == 64 {
                                        value as i64
                                    } else {
                                        ((value << (64 - bits)) as i64) >> (64 - bits)
                                    };
                                    ((signed >> amount) as u64) & mask
                                }
                                _ => unreachable!(),
                            }
                        };
                        Self::set_lane(&mut result, lane, bits, shifted);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedAlignRight {
                dst,
                high,
                low,
                width,
                amount,
            } => {
                let high = Self::read_vec(ctx, *high);
                let low = Self::read_vec(ctx, *low);
                let mut result = [0u64; 16];
                let width_bytes = width.bytes() as usize;
                let block_bytes = usize::min(width_bytes, 16);
                for block in 0..width_bytes / block_bytes {
                    let base = block * block_bytes;
                    for lane in 0..block_bytes {
                        let selected = usize::from(*amount) + lane;
                        let value = if selected < block_bytes {
                            Self::get_lane(&low, (base + selected) as u8, 8)
                        } else if selected < block_bytes * 2 {
                            Self::get_lane(&high, (base + selected - block_bytes) as u8, 8)
                        } else {
                            0
                        };
                        Self::set_lane(&mut result, (base + lane) as u8, 8, value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShuffleImm {
                dst,
                src,
                width,
                elem,
                imm,
                high_words,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                let lanes = width.lanes(*elem) as u8;
                let block_lanes = if *elem == VecElementType::I32 { 4 } else { 8 };
                let bits = elem.bytes() * 8;
                for lane in 0..lanes {
                    let within = lane % block_lanes;
                    let block = lane - within;
                    let shuffled = match high_words {
                        None => true,
                        Some(true) => within >= 4,
                        Some(false) => within < 4,
                    };
                    let selector = if shuffled {
                        let output = within % 4;
                        block
                            + if *high_words == Some(true) { 4 } else { 0 }
                            + ((*imm >> (output * 2)) & 3)
                    } else {
                        lane
                    };
                    let value = Self::get_lane(&input, selector, bits);
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86ThreeDNow {
                dst,
                src1,
                src2,
                kind,
            } => {
                let first = Self::read_vec(ctx, *src1)[0];
                let second = Self::read_vec(ctx, *src2)[0];
                let mut result = [0u64; 16];
                result[0] = Self::x86_three_d_now_eval(*kind, first, second);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShift {
                dst,
                src,
                count,
                width,
                elem,
                shift,
            } => {
                let input = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let amount = ctx.read_vreg(*count);
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let shifted = if amount >= u64::from(bits) {
                        if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                            mask
                        } else {
                            0
                        }
                    } else {
                        match shift {
                            ShiftOp::Lsl => (value << amount) & mask,
                            ShiftOp::Lsr => value >> amount,
                            ShiftOp::Asr => {
                                let signed = if bits == 64 {
                                    value as i64
                                } else {
                                    ((value << (64 - bits)) as i64) >> (64 - bits)
                                };
                                ((signed >> amount) as u64) & mask
                            }
                            _ => unreachable!(),
                        }
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedShiftVariable {
                dst,
                src,
                count,
                mask: write_mask,
                width,
                elem,
                shift,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let counts = Self::read_vec(ctx, *count);
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let amount = Self::get_lane(&counts, lane, bits);
                    let shifted = if amount >= u64::from(bits) {
                        if *shift == ShiftOp::Asr && value & (1u64 << (bits - 1)) != 0 {
                            mask
                        } else {
                            0
                        }
                    } else {
                        match shift {
                            ShiftOp::Lsl => (value << amount) & mask,
                            ShiftOp::Lsr => value >> amount,
                            ShiftOp::Asr => {
                                let signed = if bits == 64 {
                                    value as i64
                                } else {
                                    ((value << (64 - bits)) as i64) >> (64 - bits)
                                };
                                ((signed >> amount) as u64) & mask
                            }
                            _ => unreachable!(),
                        }
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    write_mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedRotate {
                dst,
                src,
                count,
                mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let counts = count.map(|register| Self::read_vec(ctx, register));
                let mut result = [0u64; 16];
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let raw_count = counts.as_ref().map_or(u64::from(*amount), |values| {
                        Self::get_lane(values, lane, bits)
                    });
                    let reduced = (raw_count % u64::from(bits)) as u32;
                    let rotated = match (bits, left) {
                        (32, true) => u64::from((value as u32).rotate_left(reduced)),
                        (32, false) => u64::from((value as u32).rotate_right(reduced)),
                        (64, true) => value.rotate_left(reduced),
                        (64, false) => value.rotate_right(reduced),
                        _ => unreachable!(),
                    };
                    Self::set_lane(&mut result, lane, bits, rotated);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask,
                imm,
                width,
                elem,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let c = Self::read_vec(ctx, *src3);
                let mut result = [0u64; 16];
                for word in 0..(width.bytes() / 8) as usize {
                    let mut out = 0u64;
                    for index in 0..8u8 {
                        if imm & (1 << index) != 0 {
                            out |= if index & 4 != 0 { a[word] } else { !a[word] }
                                & if index & 2 != 0 { b[word] } else { !b[word] }
                                & if index & 1 != 0 { c[word] } else { !c[word] };
                        }
                    }
                    result[word] = out;
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFunnelShift {
                dst,
                src,
                fill,
                count,
                mask: write_mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let primary = Self::read_vec(ctx, *src);
                let secondary = Self::read_vec(ctx, *fill);
                let counts = count.map(|register| Self::read_vec(ctx, register));
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let value = Self::get_lane(&primary, lane, bits);
                    let fill_value = Self::get_lane(&secondary, lane, bits);
                    let raw_count = counts.as_ref().map_or(u64::from(*amount), |values| {
                        Self::get_lane(values, lane, bits)
                    });
                    let reduced = (raw_count % u64::from(bits)) as u32;
                    let shifted = if reduced == 0 {
                        value
                    } else if *left {
                        ((value << reduced) | (fill_value >> (bits - reduced))) & mask
                    } else {
                        (value >> reduced) | ((fill_value << (bits - reduced)) & mask)
                    };
                    Self::set_lane(&mut result, lane, bits, shifted);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    write_mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86MultiShiftQB {
                dst,
                control,
                source,
                mask,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let controls = Self::read_vec(ctx, *control);
                let data = Self::read_vec(ctx, *source);
                let mut result = [0u64; 16];
                for qword in 0..(width.bytes() / 8) as u8 {
                    let value = Self::get_lane(&data, qword, 64);
                    for byte in 0..8u8 {
                        let lane = qword * 8 + byte;
                        let shift = Self::get_lane(&controls, lane, 8) as u32 & 63;
                        Self::set_lane(&mut result, lane, 8, value.rotate_right(shift) & 0xFF);
                    }
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I8,
                );
                Self::write_vec(ctx, *dst, result);
            }

            _ => return self.execute_op_avx10(ctx, memory, op),
        }

        Ok(())
    }
}
