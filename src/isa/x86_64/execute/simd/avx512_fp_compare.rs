//! AVX-512 floating-point comparisons that write an opmask destination.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::avx512::{
    evex_mask, evex_rm_vec, evex_scaled_disp8_addr, f16_to_f32, load_mem_bytes, read_fp_elem,
    read_reg_bytes, vl_bytes_of,
};
use super::compare::{cmp_predicate_f32, cmp_predicate_f64};

/// EVEX VCMPPS/PD/PH and VCMPSD/SS/SH: compare FP elements into a k-mask.
pub fn evex_fp_cmp(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    scalar: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FP compare requires EVEX prefix".to_string()))?;

    // Opmask destinations cannot consume EVEX.R/R'. Packed no-SAE L'L=11b is
    // reserved. Register-source packed SAE and scalar SAE ignore all four L'L
    // control values; scalar no-SAE LLIG accepts 00b..10b.
    if evex.z
        || !evex.r
        || !evex.r_prime
        || (!scalar && !evex.broadcast && evex.ll == 3)
        || (scalar && !evex.broadcast && evex.ll == 3)
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let imm = ctx.consume_u8()?;
    let packed_sae = !scalar && evex.broadcast && !is_memory;
    if imm & !0x1F != 0
        || (scalar && evex.broadcast && is_memory)
        || (!scalar && !packed_sae && evex.ll == 3)
    {
        return vcpu.inject_undefined_instruction();
    }

    let k_dst = (reg & 0x07) as usize;
    let src1 = (evex.vvvv ^ 0xF) | if evex.v_prime { 0 } else { 16 };
    let src2_reg = evex_rm_vec(&evex, rm);
    let vl_bytes = if packed_sae {
        64
    } else if scalar {
        16
    } else {
        vl_bytes_of(evex.ll)
    };
    let num_elems = if scalar { 1 } else { vl_bytes / elem_size };
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

    let src1_bytes = read_reg_bytes(vcpu, src1, vl_bytes);
    let src2_bytes = if is_memory {
        if evex.broadcast && !scalar {
            let elem = vcpu.read_mem(addr, elem_size as u8)?;
            let elem_le = elem.to_le_bytes();
            let mut data = [0u8; 64];
            for lane in 0..num_elems {
                let base = lane * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem_le[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, num_elems)?
        }
    } else {
        read_reg_bytes(vcpu, src2_reg, vl_bytes)
    };

    let writemask = evex_mask(vcpu, evex.aaa, num_elems);
    let mut result = 0u64;
    for lane in 0..num_elems {
        if (writemask >> lane) & 1 == 0 {
            continue;
        }
        let cond = match elem_size {
            2 => cmp_predicate_f32(
                f16_to_f32(read_fp_elem(&src1_bytes, lane, elem_size) as u16),
                f16_to_f32(read_fp_elem(&src2_bytes, lane, elem_size) as u16),
                imm,
            ),
            4 => cmp_predicate_f32(
                f32::from_bits(read_fp_elem(&src1_bytes, lane, elem_size) as u32),
                f32::from_bits(read_fp_elem(&src2_bytes, lane, elem_size) as u32),
                imm,
            ),
            8 => cmp_predicate_f64(
                f64::from_bits(read_fp_elem(&src1_bytes, lane, elem_size)),
                f64::from_bits(read_fp_elem(&src2_bytes, lane, elem_size)),
                imm,
            ),
            _ => false,
        };
        if cond {
            result |= 1u64 << lane;
        }
    }

    vcpu.regs.k[k_dst] = result;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
