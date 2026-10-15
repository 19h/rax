//! Source-register metadata for vector and packed operations.

use crate::smir::ir::ops::{OpKind, X86RepMode, X86StringKind};
use crate::smir::ir::types::{SrcOperand, VReg};

impl OpKind {
    pub(super) fn vector_source_vregs(&self) -> Vec<VReg> {
        let mut result = Vec::new();
        match self {
            // AVX10 operations - extract source registers
            OpKind::VMin { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VFma {
                src1, src2, acc, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*acc);
            }

            OpKind::X86Fma(fma) => result.extend(fma.source_vregs()),

            OpKind::X86FP16Fma {
                src1,
                src2,
                src3,
                mask,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
                result.extend(mask.iter().copied());
            }

            OpKind::VDotProductBF16 {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VDotProduct {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VMultiplyAdd52 {
                acc,
                src1,
                src2,
                mask,
                ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::VCvtBF16ToFP32 { src, .. } => {
                result.push(*src);
            }

            OpKind::VConflict { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::VPopcnt { src, mask, .. } | OpKind::VLeadingZeros { src, mask, .. } => {
                result.push(*src);
                result.extend(mask.iter().copied());
            }

            OpKind::VPermute {
                src1,
                src2,
                indices,
                ..
            } => {
                result.push(*src1);
                if let Some(s2) = src2 {
                    result.push(*s2);
                }
                result.push(*indices);
            }

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                zeroing,
                ..
            } => {
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
                result.push(*table1);
                result.extend(table2.iter().copied());
                result.push(*indices);
                result.extend(mask.iter().copied());
            }

            OpKind::VShuffleBitQM {
                src, indices, mask, ..
            } => {
                result.push(*src);
                result.push(*indices);
                result.extend(mask.iter().copied());
            }

            OpKind::VCompress {
                dst,
                src,
                mask,
                zeroing,
                ..
            }
            | OpKind::VExpand {
                dst,
                src,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
                if !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::X86NarrowInt {
                dst,
                src,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
                if !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src1);
                if let Some(s2) = src2 {
                    result.push(*s2);
                }
                result.extend(mask.iter().copied());
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::VMinMax { src1, src2, .. } | OpKind::VSadBytes { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
                if mask.is_some() && !zeroing {
                    result.push(*dst);
                }
            }

            OpKind::X86Aes { src1, src2, .. } => {
                result.push(*src1);
                result.extend(src2.iter().copied());
            }

            OpKind::X86DotProduct { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86FpBinary {
                src1, src2, mask, ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(mask.iter().copied());
            }

            OpKind::X86Sha32 { src1, src2, wk, .. } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(wk.iter().copied());
            }

            OpKind::X86PackedStringCompare {
                dst,
                src1,
                src2,
                len1,
                len2,
                kind,
                zero_upper,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.extend(len1.iter().copied());
                result.extend(len2.iter().copied());
                // Legacy mask forms overwrite only XMM0 bits 127:0. VEX mask
                // forms clear the shared architectural vector value above bit
                // 127 and therefore do not read the old destination.
                if kind.returns_mask() && !zero_upper {
                    result.push(*dst);
                }
            }

            OpKind::X86Sha512Msg1 { dst, src } | OpKind::X86Sha512Msg2 { dst, src } => {
                result.push(*dst);
                result.push(*src);
            }

            OpKind::X86Sha512Rounds2 { dst, state, wk } => {
                result.push(*dst);
                result.push(*state);
                result.push(*wk);
            }

            OpKind::X86Sm3Msg1 {
                dst, src1, src2, ..
            }
            | OpKind::X86Sm3Msg2 {
                dst, src1, src2, ..
            } => {
                result.push(*dst);
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Sm3Rounds2 {
                dst, state, words, ..
            } => {
                result.push(*dst);
                result.push(*state);
                result.push(*words);
            }

            OpKind::X86Sm4 { src1, src2, .. } => {
                result.push(*src1);
                result.push(*src2);
            }

            OpKind::X86Convert16ToFp32 { src, .. } => {
                result.push(*src);
            }

            OpKind::X86PackedShiftImm { src, .. } => {
                result.push(*src);
            }

            OpKind::X86PackedAlignRight { high, low, .. } => {
                result.push(*high);
                result.push(*low);
            }

            OpKind::X86PackedShuffleImm { src, .. } => {
                result.push(*src);
            }

            OpKind::X86PackedShift { src, count, .. } => {
                result.push(*src);
                result.push(*count);
            }
            OpKind::X86PackedShiftVariable {
                src, count, mask, ..
            } => {
                result.push(*src);
                result.push(*count);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86PackedRotate {
                src, count, mask, ..
            } => {
                result.push(*src);
                if let Some(count) = count {
                    result.push(*count);
                }
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86XopPackedBit { src, count, .. } => {
                result.push(*src);
                if let SrcOperand::Reg(register) = count {
                    result.push(*register);
                }
            }

            OpKind::X86TernaryLogic {
                src1,
                src2,
                src3,
                mask,
                ..
            } => {
                result.push(*src1);
                result.push(*src2);
                result.push(*src3);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86PackedFunnelShift {
                src,
                fill,
                count,
                mask,
                ..
            } => {
                result.push(*src);
                result.push(*fill);
                if let Some(count) = count {
                    result.push(*count);
                }
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::X86MultiShiftQB {
                control,
                source,
                mask,
                ..
            } => {
                result.push(*control);
                result.push(*source);
                if let Some(mask) = mask {
                    result.push(*mask);
                }
            }

            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask,
                zeroing,
                ..
            } => {
                result.push(*src);
                if let Some(mask) = mask {
                    result.push(*mask);
                    if !*zeroing {
                        result.push(*dst);
                    }
                }
            }

            OpKind::VDotProductExt {
                acc, src1, src2, ..
            } => {
                result.push(*acc);
                result.push(*src1);
                result.push(*src2);
            }

            // Carry-less multiply: src1/src2 are SrcOperands; the `_acc` forms
            // also read the existing dst/dst_hi (XOR target).
            OpKind::ClMul {
                src1,
                src2,
                dst,
                dst_hi,
                acc,
                ..
            } => {
                if let SrcOperand::Reg(r) = src1 {
                    result.push(*r);
                }
                if let SrcOperand::Reg(r) = src2 {
                    result.push(*r);
                }
                if *acc {
                    result.push(*dst);
                    if let Some(hi) = dst_hi {
                        result.push(*hi);
                    }
                }
            }

            OpKind::Crc32C { crc, data, .. } => {
                result.push(*crc);
                result.push(*data);
            }

            // Wide complex multiply: reads both halves of the Rss and Rtt pairs.
            OpKind::CmpyW128Sat {
                rss_lo,
                rss_hi,
                rtt_lo,
                rtt_hi,
                ..
            } => {
                result.push(*rss_lo);
                result.push(*rss_hi);
                result.push(*rtt_lo);
                result.push(*rtt_hi);
            }

            // Register-amount saturating shift: src and amount are SrcOperands.
            OpKind::SatOrigShl { src, amount, .. } => {
                if let SrcOperand::Reg(r) = src {
                    result.push(*r);
                }
                if let SrcOperand::Reg(r) = amount {
                    result.push(*r);
                }
            }

            OpKind::X86String {
                kind,
                rep,
                accumulator,
                src_index,
                dst_index,
                count,
                src_segment,
                ..
            } => {
                match kind {
                    X86StringKind::Movs => result.extend([*src_index, *dst_index]),
                    X86StringKind::Stos => result.extend([*accumulator, *dst_index]),
                    X86StringKind::Lods => result.extend([*accumulator, *src_index]),
                    X86StringKind::Scas => result.extend([*accumulator, *dst_index]),
                    X86StringKind::Cmps => result.extend([*src_index, *dst_index]),
                }
                if *rep != X86RepMode::None {
                    result.push(*count);
                }
                if let Some(segment) = src_segment {
                    result.push(*segment);
                }
            }
            _ => {} // The exhaustive sources dispatcher owns all other variants.
        }
        result
    }
}
