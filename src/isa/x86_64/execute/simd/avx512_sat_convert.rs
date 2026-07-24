//! Precise AVX10.2 MAP5 saturating floating-point-to-integer conversions.

use super::avx512::{
    evex_mask, evex_reg_vec, evex_rm_vec, evex_scaled_disp8_addr, read_reg_bytes, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

const MXCSR_STATUS_MASK: u32 = 0x3F;
const MXCSR_INVALID: u32 = 1 << 0;
const MXCSR_PRECISION: u32 = 1 << 5;
const MXCSR_DAZ: u32 = 1 << 6;
const CR4_OSXMMEXCPT: u64 = 1 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SatFpToIntKind {
    F32ToI8 { signed: bool },
    F64ToI64 { signed: bool },
}

impl SatFpToIntKind {
    fn fp_bytes(self) -> usize {
        match self {
            Self::F32ToI8 { .. } => 4,
            Self::F64ToI64 { .. } => 8,
        }
    }

    fn int_bits(self) -> u32 {
        match self {
            Self::F32ToI8 { .. } => 8,
            Self::F64ToI64 { .. } => 64,
        }
    }

    fn signed(self) -> bool {
        match self {
            Self::F32ToI8 { signed } | Self::F64ToI64 { signed } => signed,
        }
    }

    fn w(self) -> bool {
        matches!(self, Self::F64ToI64 { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConversionResult {
    bits: u64,
    status: u32,
}

fn fp_layout(fp_bytes: usize) -> (u32, u32, i32) {
    match fp_bytes {
        4 => (8, 23, 127),
        8 => (11, 52, 1023),
        _ => unreachable!("validated AVX10.2 source element width"),
    }
}

fn is_denormal(bits: u64, fp_bytes: usize) -> bool {
    let (exponent_bits, fraction_bits, _) = fp_layout(fp_bytes);
    let exponent = (bits >> fraction_bits) & ((1u64 << exponent_bits) - 1);
    let fraction = bits & ((1u64 << fraction_bits) - 1);
    exponent == 0 && fraction != 0
}

fn saturation_endpoint(signed: bool, negative: bool, int_bits: u32) -> u64 {
    match (signed, negative, int_bits) {
        (true, true, 64) => 1u64 << 63,
        (true, true, _) => 1u64 << (int_bits - 1),
        (true, false, 64) => i64::MAX as u64,
        (true, false, _) => (1u64 << (int_bits - 1)) - 1,
        (false, true, _) => 0,
        (false, false, 64) => u64::MAX,
        (false, false, _) => (1u64 << int_bits) - 1,
    }
}

fn truncate_saturating(
    bits: u64,
    fp_bytes: usize,
    int_bits: u32,
    signed: bool,
) -> ConversionResult {
    let (exponent_bits, fraction_bits, bias) = fp_layout(fp_bytes);
    let sign_mask = 1u64 << (fp_bytes * 8 - 1);
    let negative = bits & sign_mask != 0;
    let exponent_mask = (1u64 << exponent_bits) - 1;
    let exponent_field = (bits >> fraction_bits) & exponent_mask;
    let fraction = bits & ((1u64 << fraction_bits) - 1);

    if exponent_field == exponent_mask {
        return ConversionResult {
            bits: if fraction == 0 {
                saturation_endpoint(signed, negative, int_bits)
            } else {
                0
            },
            status: MXCSR_INVALID,
        };
    }
    if exponent_field == 0 && fraction == 0 {
        return ConversionResult { bits: 0, status: 0 };
    }

    let (significand, exponent) = if exponent_field == 0 {
        (u128::from(fraction), 1 - bias - fraction_bits as i32)
    } else {
        (
            (1u128 << fraction_bits) | u128::from(fraction),
            exponent_field as i32 - bias - fraction_bits as i32,
        )
    };
    let (magnitude, inexact) = if exponent >= 0 {
        let shift = exponent as u32;
        if shift >= u128::BITS || significand > (u128::MAX >> shift) {
            return ConversionResult {
                bits: saturation_endpoint(signed, negative, int_bits),
                status: MXCSR_INVALID,
            };
        }
        (significand << shift, false)
    } else {
        let drop = (-exponent) as u32;
        if drop >= u128::BITS {
            (0, significand != 0)
        } else {
            (
                significand >> drop,
                significand & ((1u128 << drop) - 1) != 0,
            )
        }
    };

    let mask = if int_bits == 64 {
        u64::MAX
    } else {
        (1u64 << int_bits) - 1
    };
    let valid = if signed {
        let negative_limit = 1u128 << (int_bits - 1);
        if negative {
            magnitude <= negative_limit
        } else {
            magnitude < negative_limit
        }
    } else {
        (!negative || magnitude == 0) && magnitude <= u128::from(mask)
    };
    if !valid {
        return ConversionResult {
            bits: saturation_endpoint(signed, negative, int_bits),
            status: MXCSR_INVALID,
        };
    }

    ConversionResult {
        bits: if negative {
            0u128.wrapping_sub(magnitude) as u64 & mask
        } else {
            magnitude as u64 & mask
        },
        status: if inexact { MXCSR_PRECISION } else { 0 },
    }
}

pub fn evex_saturating_fp_to_int(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    kind: SatFpToIntKind,
) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("AVX10.2 saturating conversion requires EVEX prefix".to_string())
    })?;
    let modrm = ctx.peek_u8()?;
    let is_memory = modrm >> 6 != 3;
    let register_sae = evex.broadcast && !is_memory;
    if evex.mm != 5
        || evex.pp != 1
        || evex.w != kind.w()
        || evex.vvvv != 0x0F
        || !evex.v_prime
        || evex.ll == 3
        || (evex.z && evex.aaa == 0)
        || (register_sae && evex.ll != 0)
    {
        return vcpu.inject_undefined_instruction();
    }

    let width = if register_sae { 64 } else { 16usize << evex.ll };
    let fp_bytes = kind.fp_bytes();
    let lanes = width / fp_bytes;
    let mask = evex_mask(vcpu, evex.aaa, lanes);
    let modrm_start = ctx.cursor;
    let (reg, rm, decoded_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert_eq!(decoded_memory, is_memory);
    let destination = evex_reg_vec(&evex, reg);
    let source_register = evex_rm_vec(&evex, rm);
    let addr = if is_memory {
        evex_scaled_disp8_addr(
            ctx,
            modrm_start,
            addr,
            if evex.broadcast { fp_bytes } else { width },
        )
    } else {
        addr
    };

    // Resolve every active memory source before changing MXCSR or destination
    // state. A broadcast is one architectural read; masked full tuples read
    // only active elements.
    let mut source = [0u8; 64];
    if is_memory {
        if evex.broadcast {
            if mask != 0 {
                let value = vcpu.read_mem(addr, fp_bytes as u8)?.to_le_bytes();
                for lane in 0..lanes {
                    if mask & (1u64 << lane) != 0 {
                        let start = lane * fp_bytes;
                        source[start..start + fp_bytes].copy_from_slice(&value[..fp_bytes]);
                    }
                }
            }
        } else {
            for lane in 0..lanes {
                if mask & (1u64 << lane) != 0 {
                    let value = vcpu
                        .read_mem(addr.wrapping_add((lane * fp_bytes) as u64), fp_bytes as u8)?
                        .to_le_bytes();
                    let start = lane * fp_bytes;
                    source[start..start + fp_bytes].copy_from_slice(&value[..fp_bytes]);
                }
            }
        }
    } else {
        source = read_reg_bytes(vcpu, source_register, width);
    }

    let old = read_reg_bytes(vcpu, destination, width);
    let mut result = [0u8; 64];
    let mut status = 0;
    for lane in 0..lanes {
        let base = lane * fp_bytes;
        if mask & (1u64 << lane) == 0 {
            if !evex.z {
                result[base..base + fp_bytes].copy_from_slice(&old[base..base + fp_bytes]);
            }
            continue;
        }

        let mut raw = [0u8; 8];
        raw[..fp_bytes].copy_from_slice(&source[base..base + fp_bytes]);
        let mut bits = u64::from_le_bytes(raw);
        if vcpu.mxcsr & MXCSR_DAZ != 0 && is_denormal(bits, fp_bytes) {
            bits &= 1u64 << (fp_bytes * 8 - 1);
        }
        let converted = truncate_saturating(bits, fp_bytes, kind.int_bits(), kind.signed());
        status |= converted.status;
        if fp_bytes == 4 {
            // Byte results occupy bits 7:0 of their corresponding dword;
            // bytes 3:1 remain zero.
            result[base] = converted.bits as u8;
        } else {
            result[base..base + 8].copy_from_slice(&converted.bits.to_le_bytes());
        }
    }

    if !register_sae {
        let mxcsr_before = vcpu.mxcsr;
        let masks = (mxcsr_before >> 7) & MXCSR_STATUS_MASK;
        // Invalid is a pre-computation exception, while Precision is a
        // post-computation exception. An unmasked pre-computation exception
        // faults before the post-computation phase can accrue PE.
        let pre_status = status & MXCSR_INVALID;
        let reported_status = if pre_status & !masks != 0 {
            pre_status
        } else {
            status
        };
        vcpu.mxcsr |= reported_status;
        if reported_status & !masks != 0 {
            let vector = if vcpu.sregs.cr4 & CR4_OSXMMEXCPT != 0 {
                19 // #XM: SIMD floating-point exception
            } else {
                6 // #UD when CR4.OSXMMEXCPT is clear
            };
            vcpu.inject_exception(vector, None)?;
            return Ok(None);
        }
    }

    write_vec_vl(vcpu, destination, width, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_scalar_helper_covers_signed_unsigned_boundaries() {
        for (bits, fp_bytes, int_bits, signed, expected, status) in [
            (u64::from(f32::NAN.to_bits()), 4, 8, true, 0, MXCSR_INVALID),
            (
                u64::from(f32::INFINITY.to_bits()),
                4,
                8,
                true,
                0x7F,
                MXCSR_INVALID,
            ),
            (
                u64::from((-128.9f32).to_bits()),
                4,
                8,
                true,
                0x80,
                MXCSR_PRECISION,
            ),
            (
                9_223_372_036_854_775_808.0f64.to_bits(),
                8,
                64,
                true,
                i64::MAX as u64,
                MXCSR_INVALID,
            ),
            (
                18_446_744_073_709_551_616.0f64.to_bits(),
                8,
                64,
                false,
                u64::MAX,
                MXCSR_INVALID,
            ),
        ] {
            assert_eq!(
                truncate_saturating(bits, fp_bytes, int_bits, signed),
                ConversionResult {
                    bits: expected,
                    status,
                }
            );
        }
    }
}
