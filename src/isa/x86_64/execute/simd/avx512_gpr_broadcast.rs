//! EVEX GPR-source integer broadcasts.

use super::avx512::{
    apply_evex_mask, evex_reg_vec, evex_rm_gpr, scalar_low_bytes, vl_bytes_of, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute EVEX VPBROADCASTB/W/D/Q GPR-source forms (0F38.7A/7B/7C).
pub fn evex_broadcast_gpr(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX GPR broadcast requires EVEX prefix".to_string()))?;

    // Intel SDM Tables 2-41 through 2-43: EVEX.vvvv/V' are reserved,
    // EVEX.b is unsupported, L'L=3 is reserved, and zeroing requires a real
    // writemask. ModR/M.r/m is a GPR, so EVEX.X is architecturally ignored;
    // EVEX.B remains the ordinary bit-3 GPR extension. Validate before ModR/M
    // decoding so invalid memory forms cannot calculate an address or commit.
    let modrm = ctx.peek_u8()?;
    if evex.vvvv != 0xF
        || !evex.v_prime
        || evex.broadcast
        || evex.ll == 3
        || (evex.z && evex.aaa == 0)
        || modrm >> 6 != 3
    {
        return vcpu.inject_undefined_instruction();
    }

    let (reg, rm, _, _, _) = vcpu.decode_modrm(ctx)?;
    let dest = evex_reg_vec(&evex, reg);
    let src = evex_rm_gpr(&evex, rm);
    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let source_size = if elem_size == 8 { 8 } else { 4 };
    let elem = scalar_low_bytes(vcpu.get_reg(src, source_size), elem_size);

    let mut raw = [0u8; 64];
    for lane in 0..num_elems {
        let base = lane * elem_size;
        raw[base..base + elem_size].copy_from_slice(&elem[..elem_size]);
    }

    let result = apply_evex_mask(vcpu, &evex, dest, vl_bytes, elem_size, &raw);
    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
