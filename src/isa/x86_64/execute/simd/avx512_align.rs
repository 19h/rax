//! EVEX doubleword and quadword vector alignment.

use super::avx512::{
    evex_mask, evex_scaled_disp8_addr, evex_three_op, load_mem_bytes, read_reg_bytes, vl_bytes_of,
    write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute EVEX VALIGND/Q. The low half of the concatenation is source 2 and
/// the high half is source 1; imm8 selects an element-aligned right shift.
pub fn evex_valign(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX valign requires EVEX prefix".to_string()))?;

    // Intel SDM VALIGND/Q and exception class E4NF: L'L=3 and {z} with k0
    // are reserved. EVEX.b is valid only for a memory source. Validate before
    // address calculation so invalid encodings cannot access memory or commit.
    let modrm = ctx.peek_u8()?;
    if evex.ll == 3 || (evex.z && evex.aaa == 0) || (evex.broadcast && modrm >> 6 == 3) {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let (dest, src1, src2_reg) = evex_three_op(&evex, reg, rm);
    let imm = ctx.consume_u8()?;

    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let addr = if is_memory {
        let scale = if evex.broadcast { elem_size } else { vl_bytes };
        evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
    } else {
        addr
    };
    let src1_bytes = read_reg_bytes(vcpu, src1, vl_bytes);
    let src2_bytes = if is_memory {
        if evex.broadcast {
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

    // KL is a power of two for every defined vector/element width. Intel's
    // pseudocode selects imm8[log2(KL)-1:0], including only imm8[0] for
    // 128-bit VALIGNQ.
    let shift = usize::from(imm) & (num_elems - 1);
    let dest_old = read_reg_bytes(vcpu, dest, vl_bytes);
    let mask = evex_mask(vcpu, evex.aaa, num_elems);
    let mut raw = [0u8; 64];
    for lane in 0..num_elems {
        let source_lane = lane + shift;
        let destination_base = lane * elem_size;
        if source_lane < num_elems {
            let source_base = source_lane * elem_size;
            raw[destination_base..destination_base + elem_size]
                .copy_from_slice(&src2_bytes[source_base..source_base + elem_size]);
        } else {
            let source_base = (source_lane - num_elems) * elem_size;
            raw[destination_base..destination_base + elem_size]
                .copy_from_slice(&src1_bytes[source_base..source_base + elem_size]);
        }
    }

    let mut result = [0u8; 64];
    for lane in 0..num_elems {
        let base = lane * elem_size;
        if (mask >> lane) & 1 != 0 {
            result[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if !evex.z {
            result[base..base + elem_size].copy_from_slice(&dest_old[base..base + elem_size]);
        }
    }

    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
