//! AVX-512 vector/opmask conversion instruction implementations.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::avx512::{read_reg_bytes, vl_bytes_of, write_vec_vl};

/// EVEX VPMOVM2B/W/D/Q: expand mask bits into all-ones/all-zero vector elements.
pub fn evex_mask_to_vec(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX mask-to-vector requires EVEX prefix".to_string()))?;

    if evex.vvvv != 0xF {
        return vcpu.inject_undefined_instruction();
    }

    let (reg, rm, is_memory, _, _) = vcpu.decode_modrm(ctx)?;
    if is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let dest = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let src_mask = vcpu.regs.k[(rm & 0x07) as usize];
    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let mut result = [0u8; 64];

    for lane in 0..num_elems {
        let base = lane * elem_size;
        if (src_mask >> lane) & 1 != 0 {
            result[base..base + elem_size].fill(0xff);
        }
    }

    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// EVEX VPMOVB/W/D/Q2M: collect vector element sign bits into a k-mask.
pub fn evex_vec_to_mask(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX vector-to-mask requires EVEX prefix".to_string()))?;

    // Type E7NM is register-only and has no writemask/broadcast controls.
    // EVEX.vvvv/V' and both ModR/M.reg extension bits are reserved. Validate
    // all fields before ModR/M decoding so a reserved memory form cannot
    // perform address calculation or commit architectural state.
    let modrm = ctx.peek_u8()?;
    if evex.aaa != 0
        || evex.z
        || evex.broadcast
        || evex.ll == 3
        || evex.vvvv != 0xF
        || !evex.v_prime
        || !evex.r
        || !evex.r_prime
        || modrm >> 6 != 3
    {
        return vcpu.inject_undefined_instruction();
    }

    let (reg, rm, _, _, _) = vcpu.decode_modrm(ctx)?;
    let dest_mask = (reg & 0x07) as usize;
    let src = (rm & 0x07) | if evex.b { 0 } else { 8 } | if evex.x { 0 } else { 16 };
    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let src_bytes = read_reg_bytes(vcpu, src, vl_bytes);
    let mut result = 0u64;

    for lane in 0..num_elems {
        let msb = src_bytes[lane * elem_size + elem_size - 1] & 0x80;
        if msb != 0 {
            result |= 1u64 << lane;
        }
    }

    vcpu.regs.k[dest_mask] = result;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
