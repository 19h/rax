//! Precise EVEX scalar floating-point comparisons that write RFLAGS.

use std::cmp::Ordering;

use super::avx512::{evex_rm_vec, evex_scaled_disp8_addr, f16_to_f32, read_reg_bytes};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VcpuExit;

const MXCSR_DAZ: u32 = 1 << 6;
const MXCSR_STATUS_MASK: u32 = 0x3F;
const CR4_OSXMMEXCPT: u64 = 1 << 10;

#[inline]
fn fp_masks(elem_size: usize) -> (u64, u64, u64, u64) {
    match elem_size {
        2 => (0x8000, 0x7C00, 0x03FF, 0x0200),
        4 => (0x8000_0000, 0x7F80_0000, 0x007F_FFFF, 0x0040_0000),
        8 => (
            0x8000_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x000F_FFFF_FFFF_FFFF,
            0x0008_0000_0000_0000,
        ),
        _ => unreachable!("validated EVEX COMI element size"),
    }
}

#[inline]
fn fp_is_nan(bits: u64, elem_size: usize) -> bool {
    let (_, exponent, fraction, _) = fp_masks(elem_size);
    bits & exponent == exponent && bits & fraction != 0
}

#[inline]
fn fp_is_snan(bits: u64, elem_size: usize) -> bool {
    let (_, _, _, quiet) = fp_masks(elem_size);
    fp_is_nan(bits, elem_size) && bits & quiet == 0
}

#[inline]
fn fp_is_denormal(bits: u64, elem_size: usize) -> bool {
    let (_, exponent, fraction, _) = fp_masks(elem_size);
    bits & exponent == 0 && bits & fraction != 0
}

#[inline]
fn apply_daz(bits: u64, elem_size: usize, mxcsr: u32) -> (u64, u32) {
    if !fp_is_denormal(bits, elem_size) {
        return (bits, 0);
    }

    // Intel SDM Vol. 2: AVX512-FP16 instructions always handle FP16
    // denormal inputs. DAZ applies only to the FP32/FP64 forms here.
    if elem_size != 2 && mxcsr & MXCSR_DAZ != 0 {
        let (sign, _, _, _) = fp_masks(elem_size);
        (bits & sign, 0)
    } else {
        (bits, 1 << 1)
    }
}

#[inline]
fn read_register_scalar(vcpu: &X86_64Vcpu, register: u8, elem_size: usize) -> u64 {
    let bytes = read_reg_bytes(vcpu, register, 16);
    let mut raw = [0u8; 8];
    raw[..elem_size].copy_from_slice(&bytes[..elem_size]);
    u64::from_le_bytes(raw)
}

#[inline]
fn fp_ordering(first: u64, second: u64, elem_size: usize) -> Option<Ordering> {
    if fp_is_nan(first, elem_size) || fp_is_nan(second, elem_size) {
        return None;
    }
    match elem_size {
        2 => f16_to_f32(first as u16).partial_cmp(&f16_to_f32(second as u16)),
        4 => f32::from_bits(first as u32).partial_cmp(&f32::from_bits(second as u32)),
        8 => f64::from_bits(first).partial_cmp(&f64::from_bits(second)),
        _ => unreachable!("validated EVEX COMI element size"),
    }
}

/// Execute EVEX VCOMISS/VCOMISD/VCOMISH or VUCOMISS/VUCOMISD/VUCOMISH.
///
/// The comparison is O(1) time and O(1) space. Reserved fields are rejected
/// before effective-address calculation. A memory fault precedes every SIMD
/// floating-point exception, and an unmasked SIMD exception updates MXCSR
/// status while leaving RFLAGS and RIP at the faulting instruction.
pub fn evex_comi(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    ordered: bool,
) -> Result<Option<VcpuExit>> {
    if !matches!(elem_size, 2 | 4 | 8) {
        return Err(Error::Emulator(format!(
            "EVEX COMI invalid element size {elem_size}"
        )));
    }
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX COMI requires EVEX prefix".to_string()))?;
    let modrm = ctx.peek_u8()?;
    let is_memory = modrm >> 6 != 3;
    if evex.vvvv != 0x0F
        || !evex.v_prime
        || evex.aaa != 0
        || evex.z
        || (!evex.broadcast && evex.ll == 3)
        || (evex.broadcast && is_memory)
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, decoded_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert_eq!(decoded_memory, is_memory);
    let addr = if is_memory {
        evex_scaled_disp8_addr(ctx, modrm_start, addr, elem_size)
    } else {
        addr
    };
    let source1 = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let source2 = evex_rm_vec(&evex, rm);
    let first_raw = read_register_scalar(vcpu, source1, elem_size);
    let second_raw = if is_memory {
        vcpu.read_mem(addr, elem_size as u8)?
    } else {
        read_register_scalar(vcpu, source2, elem_size)
    };

    let mxcsr_before = vcpu.mxcsr;
    let (first, first_status) = apply_daz(first_raw, elem_size, mxcsr_before);
    let (second, second_status) = apply_daz(second_raw, elem_size, mxcsr_before);
    let first_nan = fp_is_nan(first, elem_size);
    let second_nan = fp_is_nan(second, elem_size);
    let invalid = fp_is_snan(first, elem_size)
        || fp_is_snan(second, elem_size)
        || (ordered && (first_nan || second_nan));
    let status = first_status | second_status | u32::from(invalid);
    let suppress_exceptions = evex.broadcast;

    if !suppress_exceptions {
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

    let status_flags = flags::bits::ZF
        | flags::bits::PF
        | flags::bits::CF
        | flags::bits::OF
        | flags::bits::AF
        | flags::bits::SF;
    vcpu.regs.rflags &= !status_flags;
    match fp_ordering(first, second, elem_size) {
        None => vcpu.regs.rflags |= flags::bits::ZF | flags::bits::PF | flags::bits::CF,
        Some(Ordering::Less) => vcpu.regs.rflags |= flags::bits::CF,
        Some(Ordering::Equal) => vcpu.regs.rflags |= flags::bits::ZF,
        Some(Ordering::Greater) => {}
    }
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
