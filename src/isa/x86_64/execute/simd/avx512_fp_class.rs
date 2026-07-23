//! EVEX floating-point classification with precise masked-memory accesses.

use super::avx512::{evex_mask, evex_rm_vec, evex_scaled_disp8_addr, read_reg_bytes, vl_bytes_of};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

#[inline]
fn class_mask(bits: u64, elem_size: usize, daz: bool) -> u8 {
    let (sign_mask, exponent_mask, fraction_mask, quiet_mask) = match elem_size {
        2 => (0x8000, 0x7C00, 0x03FF, 0x0200),
        4 => (0x8000_0000, 0x7F80_0000, 0x007F_FFFF, 0x0040_0000),
        8 => (
            0x8000_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x000F_FFFF_FFFF_FFFF,
            0x0008_0000_0000_0000,
        ),
        _ => unreachable!("validated FPCLASS element size"),
    };
    let sign = bits & sign_mask != 0;
    let exponent = bits & exponent_mask;
    let fraction = bits & fraction_mask;

    if exponent == exponent_mask {
        return if fraction == 0 {
            1 << if sign { 4 } else { 3 }
        } else if fraction & quiet_mask != 0 {
            1 << 0
        } else {
            1 << 7
        };
    }
    if exponent == 0 {
        if fraction == 0 || (daz && elem_size != 2) {
            return 1 << if sign { 2 } else { 1 };
        }
        return (1 << 5) | if sign { 1 << 6 } else { 0 };
    }
    if sign { 1 << 6 } else { 0 }
}

#[inline]
fn read_element(bytes: &[u8; 64], lane: usize, elem_size: usize) -> u64 {
    let base = lane * elem_size;
    let mut raw = [0u8; 8];
    raw[..elem_size].copy_from_slice(&bytes[base..base + elem_size]);
    u64::from_le_bytes(raw)
}

/// Execute VFPCLASSPS/PD/PH or VFPCLASSSS/SD/SH.
///
/// Reserved EVEX fields are rejected before effective-address calculation or
/// architectural state access. Memory forms perform only the data accesses
/// selected by the writemask, in increasing lane order, and commit the
/// destination opmask only after every selected access succeeds.
pub fn evex_fpclass(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    scalar: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FP classify requires EVEX prefix".to_string()))?;
    let modrm = ctx.peek_u8()?;
    let is_memory = modrm >> 6 != 3;
    let valid_shape = matches!(elem_size, 2 | 4 | 8) && (scalar || evex.ll != 3);
    if !valid_shape
        || evex.vvvv != 0xF
        || !evex.v_prime
        || evex.z
        || !evex.r
        || !evex.r_prime
        || (evex.broadcast && (scalar || !is_memory))
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, decoded_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert_eq!(decoded_memory, is_memory);
    let immediate = ctx.consume_u8()?;
    let destination = (reg & 0x07) as usize;
    let vl_bytes = if scalar { 16 } else { vl_bytes_of(evex.ll) };
    let lanes = if scalar { 1 } else { vl_bytes / elem_size };
    let writemask = evex_mask(vcpu, evex.aaa, lanes);
    let daz = elem_size != 2 && vcpu.mxcsr & (1 << 6) != 0;
    let addr = if is_memory {
        let scale = if evex.broadcast || scalar {
            elem_size
        } else {
            vl_bytes
        };
        evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
    } else {
        addr
    };

    let mut result = 0u64;
    if is_memory {
        if evex.broadcast {
            if writemask != 0 {
                let bits = vcpu.read_mem(addr, elem_size as u8)?;
                if class_mask(bits, elem_size, daz) & immediate != 0 {
                    result = writemask;
                }
            }
        } else {
            for lane in 0..lanes {
                if writemask >> lane & 1 == 0 {
                    continue;
                }
                let lane_addr = addr.wrapping_add((lane * elem_size) as u64);
                let bits = vcpu.read_mem(lane_addr, elem_size as u8)?;
                if class_mask(bits, elem_size, daz) & immediate != 0 {
                    result |= 1 << lane;
                }
            }
        }
    } else {
        let source = evex_rm_vec(&evex, rm);
        let bytes = read_reg_bytes(vcpu, source, vl_bytes);
        for lane in 0..lanes {
            if writemask >> lane & 1 != 0
                && class_mask(read_element(&bytes, lane, elem_size), elem_size, daz) & immediate
                    != 0
            {
                result |= 1 << lane;
            }
        }
    }

    vcpu.regs.k[destination] = result;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
