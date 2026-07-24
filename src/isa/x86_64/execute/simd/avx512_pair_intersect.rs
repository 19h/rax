//! AVX-512 VP2INTERSECTD/Q direct execution.

use super::avx512::{
    evex_rm_vec, evex_scaled_disp8_addr, load_mem_bytes, read_reg_bytes, vl_bytes_of,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

#[inline]
fn read_element(bytes: &[u8; 64], lane: usize, elem_size: usize) -> u64 {
    let base = lane * elem_size;
    let mut raw = [0u8; 8];
    raw[..elem_size].copy_from_slice(&bytes[base..base + elem_size]);
    u64::from_le_bytes(raw)
}

/// Execute EVEX VP2INTERSECTD/VP2INTERSECTQ.
///
/// Intel specifies an even/odd opmask destination pair: ModR/M.reg bit 0 is
/// ignored when selecting the first register. Reserved EVEX fields are
/// rejected before effective-address calculation or architectural state
/// access. Memory operands use the E4NF full-vector or broadcast tuple scale;
/// both destination masks commit only after every source access succeeds.
pub fn evex_p2intersect(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX VP2INTERSECT* requires EVEX prefix".to_string()))?;
    if !matches!(elem_size, 4 | 8) {
        return Err(Error::Emulator(
            "EVEX VP2INTERSECT* requires dword or qword elements".to_string(),
        ));
    }

    let modrm = ctx.peek_u8()?;
    let register_source = modrm >> 6 == 3;
    if evex.ll == 3
        || evex.aaa != 0
        || evex.z
        || !evex.r
        || !evex.r_prime
        || (evex.broadcast && register_source)
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let destination_base = usize::from(reg & 0x06);
    let src1 = (evex.vvvv ^ 0x0F) | if evex.v_prime { 0 } else { 16 };
    let src2_reg = evex_rm_vec(&evex, rm);
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
            let elem = vcpu.read_mem(addr, elem_size as u8)?.to_le_bytes();
            let mut data = [0u8; 64];
            for lane in 0..num_elems {
                let base = lane * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, num_elems)?
        }
    } else {
        read_reg_bytes(vcpu, src2_reg, vl_bytes)
    };

    let mut mask1 = 0u64;
    let mut mask2 = 0u64;
    for lane1 in 0..num_elems {
        let value1 = read_element(&src1_bytes, lane1, elem_size);
        for lane2 in 0..num_elems {
            if read_element(&src2_bytes, lane2, elem_size) == value1 {
                mask1 |= 1u64 << lane1;
                mask2 |= 1u64 << lane2;
            }
        }
    }

    vcpu.regs.k[destination_base] = mask1;
    vcpu.regs.k[destination_base + 1] = mask2;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
