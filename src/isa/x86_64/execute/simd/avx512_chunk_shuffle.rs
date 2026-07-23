//! EVEX shuffles operating at 128-bit chunk granularity.

use super::avx512::{
    apply_evex_mask, evex_scaled_disp8_addr, evex_three_op, load_mem_bytes, read_reg_bytes,
    vl_bytes_of, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute EVEX VSHUFF32x4/VSHUFF64x2/VSHUFI32x4/VSHUFI64x2.
pub fn evex_shuffle_128_lanes(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX 128-bit lane shuffle requires EVEX prefix".to_string())
    })?;

    // Intel SDM exception class E4NF and the opcode table admit only 256- and
    // 512-bit forms. EVEX.b is valid only for memory, and {z} with k0 is
    // reserved. Validate before effective-address calculation or state access.
    let modrm = ctx.peek_u8()?;
    if !matches!(evex.ll, 1 | 2) || (evex.z && evex.aaa == 0) || (evex.broadcast && modrm >> 6 == 3)
    {
        return vcpu.inject_undefined_instruction();
    }
    if !matches!(elem_size, 4 | 8) {
        return Err(Error::Emulator(
            "EVEX 128-bit lane shuffle requires dword or qword elements".to_string(),
        ));
    }

    let vl_bytes = vl_bytes_of(evex.ll);
    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let imm8 = ctx.consume_u8()?;
    let (dest, src1, src2_reg) = evex_three_op(&evex, reg, rm);
    let addr = if is_memory {
        let scale = if evex.broadcast { elem_size } else { vl_bytes };
        evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
    } else {
        addr
    };

    let src1_bytes = read_reg_bytes(vcpu, src1, vl_bytes);
    let src2_bytes = if is_memory {
        if evex.broadcast {
            let elem = vcpu.read_mem(addr, elem_size as u8)?.to_le_bytes();
            let mut data = [0u8; 64];
            for lane in 0..(vl_bytes / elem_size) {
                let base = lane * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, vl_bytes / elem_size)?
        }
    } else {
        read_reg_bytes(vcpu, src2_reg, vl_bytes)
    };

    let mut raw = [0u8; 64];
    let chunks = vl_bytes / 16;
    for destination_chunk in 0..chunks {
        let (source, source_chunk) = if chunks == 2 {
            if destination_chunk == 0 {
                (&src1_bytes, (imm8 & 1) as usize)
            } else {
                (&src2_bytes, ((imm8 >> 1) & 1) as usize)
            }
        } else {
            let selector = ((imm8 >> (destination_chunk * 2)) & 3) as usize;
            if destination_chunk < 2 {
                (&src1_bytes, selector)
            } else {
                (&src2_bytes, selector)
            }
        };

        let destination_base = destination_chunk * 16;
        let source_base = source_chunk * 16;
        raw[destination_base..destination_base + 16]
            .copy_from_slice(&source[source_base..source_base + 16]);
    }

    let result = apply_evex_mask(vcpu, &evex, dest, vl_bytes, elem_size, &raw);
    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
