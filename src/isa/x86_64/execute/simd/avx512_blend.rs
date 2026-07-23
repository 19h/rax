//! AVX-512 opmask-selector blend instruction implementations.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::avx512::{
    evex_mask, evex_scaled_disp8_addr, evex_three_op, load_mem_bytes, read_reg_bytes, vl_bytes_of,
    write_vec_vl,
};

/// Execute VBLENDMPS/PD or VPBLENDMB/MW/MD/MQ.
///
/// The opmask is an element selector, not a writemask: selector bit 1 chooses
/// the ModR/M source, while selector bit 0 chooses EVEX.vvvv or zero when
/// EVEX.z is set.
pub fn evex_blend_select(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX blend requires EVEX prefix".to_string()))?;

    // L'L=3 is reserved. Zeroing requires a real selector mask because k0
    // denotes "no control mask". EVEX.b is defined only for 32-bit/64-bit
    // memory broadcasts and is reserved for register and byte/word forms.
    let modrm = ctx.peek_u8()?;
    let register_source = modrm >> 6 == 3;
    if evex.ll == 3
        || (evex.z && evex.aaa == 0)
        || (evex.broadcast && (register_source || elem_size < 4))
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let (dest, src1, src2_reg) = evex_three_op(&evex, reg, rm);

    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let addr = if is_memory {
        let scale = if evex.broadcast && matches!(elem_size, 4 | 8) {
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
        if evex.broadcast && matches!(elem_size, 4 | 8) {
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

    let selector = evex_mask(vcpu, evex.aaa, num_elems);
    let mut result = [0u8; 64];
    for lane in 0..num_elems {
        let base = lane * elem_size;
        if (selector >> lane) & 1 != 0 {
            result[base..base + elem_size].copy_from_slice(&src2_bytes[base..base + elem_size]);
        } else if !evex.z {
            result[base..base + elem_size].copy_from_slice(&src1_bytes[base..base + elem_size]);
        }
    }

    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
