//! Precise EVEX packed FP16 widening conversions.

use super::avx512::{
    evex_mask, evex_reg_vec, evex_rm_vec, evex_scaled_disp8_addr, read_reg_bytes, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

const MXCSR_STATUS_MASK: u32 = 0x3F;
const MXCSR_INVALID: u32 = 1 << 0;
const MXCSR_DENORMAL: u32 = 1 << 1;
const CR4_OSXMMEXCPT: u64 = 1 << 10;

/// Architecturally distinct packed FP16 widening families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fp16WidenKind {
    /// `VCVTPH2PD`: FP16 to FP64, with denormal-input reporting.
    ToF64,
    /// Legacy-map `VCVTPH2PS`: FP16 to FP32 without broadcast or DE.
    ToF32,
    /// MAP6 `VCVTPH2PSX`: FP16 to FP32 with memory broadcast support.
    ToF32X,
}

impl Fp16WidenKind {
    #[inline]
    fn destination_element_bytes(self) -> usize {
        match self {
            Self::ToF64 => 8,
            Self::ToF32 | Self::ToF32X => 4,
        }
    }

    #[inline]
    fn permits_memory_broadcast(self) -> bool {
        matches!(self, Self::ToF64 | Self::ToF32X)
    }

    #[inline]
    fn reports_denormal(self, is_memory: bool, broadcast: bool) -> bool {
        matches!(self, Self::ToF64) || (matches!(self, Self::ToF32X) && is_memory && broadcast)
    }
}

#[inline]
fn operation_bytes(ll: u8, register_sae: bool) -> usize {
    if register_sae {
        64
    } else {
        match ll {
            0 => 16,
            1 => 32,
            2 => 64,
            _ => unreachable!("validated EVEX FP16 widening L'L"),
        }
    }
}

#[inline]
fn fp16_is_denormal(bits: u16) -> bool {
    bits & 0x7C00 == 0 && bits & 0x03FF != 0
}

#[inline]
fn fp16_is_snan(bits: u16) -> bool {
    bits & 0x7C00 == 0x7C00 && bits & 0x03FF != 0 && bits & 0x0200 == 0
}

/// Convert one binary16 value to binary32 exactly, preserving NaN sign and
/// payload while quieting signaling NaNs.
#[inline]
fn fp16_to_fp32_bits(bits: u16) -> u32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1F;
    let fraction = bits & 0x03FF;
    match (exponent, fraction) {
        (0, 0) => sign,
        (0, _) => {
            let leading = 15 - fraction.leading_zeros() as i32;
            let unbiased = -24 + leading;
            let normalized = (u32::from(fraction) << (10 - leading)) & 0x03FF;
            sign | (((unbiased + 127) as u32) << 23) | (normalized << 13)
        }
        (0x1F, 0) => sign | 0x7F80_0000,
        (0x1F, _) => sign | 0x7F80_0000 | (u32::from(fraction) << 13) | 0x0040_0000,
        _ => sign | (u32::from(exponent + 112) << 23) | (u32::from(fraction) << 13),
    }
}

/// Convert one binary16 value to binary64 exactly, preserving NaN sign and
/// payload while quieting signaling NaNs.
#[inline]
fn fp16_to_fp64_bits(bits: u16) -> u64 {
    let sign = u64::from(bits & 0x8000) << 48;
    let exponent = (bits >> 10) & 0x1F;
    let fraction = bits & 0x03FF;
    match (exponent, fraction) {
        (0, 0) => sign,
        (0, _) => {
            let leading = 15 - fraction.leading_zeros() as i32;
            let unbiased = -24 + leading;
            let normalized = (u64::from(fraction) << (10 - leading)) & 0x03FF;
            sign | (((unbiased + 1023) as u64) << 52) | (normalized << 42)
        }
        (0x1F, 0) => sign | 0x7FF0_0000_0000_0000,
        (0x1F, _) => {
            sign | 0x7FF0_0000_0000_0000 | (u64::from(fraction) << 42) | 0x0008_0000_0000_0000
        }
        _ => sign | (u64::from(exponent + 1008) << 52) | (u64::from(fraction) << 42),
    }
}

/// Execute `VCVTPH2PD`, EVEX `VCVTPH2PS`, or `VCVTPH2PSX`.
///
/// Reserved fields are rejected before effective-address calculation. Active
/// memory lanes are read before floating-point status is committed, preserving
/// memory-fault priority. The algorithm is O(VL / destination-width) time
/// (at most 16 lanes) and O(1) space.
pub fn evex_fp16_widen(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    kind: Fp16WidenKind,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FP16 widening requires EVEX prefix".to_string()))?;
    let modrm = ctx.peek_u8()?;
    let is_memory = modrm >> 6 != 3;
    let register_sae = evex.broadcast && !is_memory;

    if evex.vvvv != 0x0F
        || !evex.v_prime
        || (evex.z && evex.aaa == 0)
        || (!register_sae && evex.ll == 3)
        || (evex.broadcast && is_memory && !kind.permits_memory_broadcast())
    {
        return vcpu.inject_undefined_instruction();
    }

    let destination_element_bytes = kind.destination_element_bytes();
    let destination_bytes = operation_bytes(evex.ll, register_sae);
    let lanes = destination_bytes / destination_element_bytes;
    let source_bytes = lanes * 2;
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
            if evex.broadcast { 2 } else { source_bytes },
        )
    } else {
        addr
    };

    // Read every active source before deriving or committing FP status. Masked
    // memory lanes suppress their accesses; a broadcast performs one access if
    // and only if at least one destination lane is active.
    let mut source = [0u16; 16];
    if is_memory {
        if evex.broadcast {
            if mask != 0 {
                let value = vcpu.read_mem(addr, 2)? as u16;
                for lane in 0..lanes {
                    if mask & (1u64 << lane) != 0 {
                        source[lane] = value;
                    }
                }
            }
        } else {
            for lane in 0..lanes {
                if mask & (1u64 << lane) != 0 {
                    source[lane] = vcpu.read_mem(addr.wrapping_add((lane * 2) as u64), 2)? as u16;
                }
            }
        }
    } else {
        let source_container_bytes = if source_bytes <= 16 { 16 } else { 32 };
        let raw = read_reg_bytes(vcpu, source_register, source_container_bytes);
        for lane in 0..lanes {
            source[lane] = u16::from_le_bytes([raw[lane * 2], raw[lane * 2 + 1]]);
        }
    }

    let old = read_reg_bytes(vcpu, destination, destination_bytes);
    let report_denormal = kind.reports_denormal(is_memory, evex.broadcast);
    let mut result = [0u8; 64];
    let mut status = 0u32;
    for lane in 0..lanes {
        let base = lane * destination_element_bytes;
        if mask & (1u64 << lane) == 0 {
            if !evex.z {
                result[base..base + destination_element_bytes]
                    .copy_from_slice(&old[base..base + destination_element_bytes]);
            }
            continue;
        }

        let value = source[lane];
        if fp16_is_snan(value) {
            status |= MXCSR_INVALID;
        }
        if report_denormal && fp16_is_denormal(value) {
            status |= MXCSR_DENORMAL;
        }
        if destination_element_bytes == 4 {
            result[base..base + 4].copy_from_slice(&fp16_to_fp32_bits(value).to_le_bytes());
        } else {
            result[base..base + 8].copy_from_slice(&fp16_to_fp64_bits(value).to_le_bytes());
        }
    }

    if !register_sae {
        let mxcsr_before = vcpu.mxcsr;
        vcpu.mxcsr |= status;
        let masks = (mxcsr_before >> 7) & MXCSR_STATUS_MASK;
        if status & !masks != 0 {
            let vector = if vcpu.sregs.cr4 & CR4_OSXMMEXCPT != 0 {
                19 // #XM: SIMD floating-point exception
            } else {
                6 // #UD when CR4.OSXMMEXCPT is clear
            };
            vcpu.inject_exception(vector, None)?;
            return Ok(None);
        }
    }

    write_vec_vl(vcpu, destination, destination_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::x86_64::execute::simd::avx512::f16_to_f32;

    #[test]
    fn raw_widening_is_exhaustive_for_every_binary16_encoding() {
        for raw in u16::MIN..=u16::MAX {
            let exponent = raw & 0x7C00;
            let fraction = raw & 0x03FF;
            let sign32 = u32::from(raw & 0x8000) << 16;
            let sign64 = u64::from(raw & 0x8000) << 48;
            if exponent == 0x7C00 && fraction != 0 {
                assert_eq!(
                    fp16_to_fp32_bits(raw),
                    sign32 | 0x7F80_0000 | (u32::from(fraction) << 13) | 0x0040_0000,
                    "FP32 0x{raw:04X}"
                );
                assert_eq!(
                    fp16_to_fp64_bits(raw),
                    sign64
                        | 0x7FF0_0000_0000_0000
                        | (u64::from(fraction) << 42)
                        | 0x0008_0000_0000_0000,
                    "FP64 0x{raw:04X}"
                );
            } else {
                let as_f32 = f16_to_f32(raw);
                assert_eq!(fp16_to_fp32_bits(raw), as_f32.to_bits(), "0x{raw:04X}");
                assert_eq!(
                    fp16_to_fp64_bits(raw),
                    f64::from(as_f32).to_bits(),
                    "0x{raw:04X}"
                );
            }
        }
    }
}
