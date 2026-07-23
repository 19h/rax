//! EVEX AVX-512BW byte-transform and multiply-add validation frontiers.

use super::avx512::{IntOp, evex_int_arith, evex_palignr, evex_pshufb};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

fn evex_bw_encoding_is_reserved(ctx: &InsnContext) -> Result<bool> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("AVX-512BW operation requires EVEX prefix".to_string()))?;
    Ok(evex.ll == 3 || evex.broadcast || (evex.z && evex.aaa == 0))
}

/// Execute EVEX VPSHUFB after rejecting reserved fields before state or memory
/// access. EVEX.W is architecturally ignored.
pub fn evex_bw_pshufb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if evex_bw_encoding_is_reserved(ctx)? {
        return vcpu.inject_undefined_instruction();
    }
    evex_pshufb(vcpu, ctx)
}

/// Execute EVEX VPMADDUBSW after rejecting reserved fields before state or
/// memory access. EVEX.W is architecturally ignored.
pub fn evex_bw_pmaddubsw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if evex_bw_encoding_is_reserved(ctx)? {
        return vcpu.inject_undefined_instruction();
    }
    evex_int_arith(vcpu, ctx, IntOp::MaddUBSW)
}

/// Execute EVEX VPMADDWD after rejecting reserved fields before state or
/// memory access. EVEX.W is architecturally ignored.
pub fn evex_bw_pmaddwd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if evex_bw_encoding_is_reserved(ctx)? {
        return vcpu.inject_undefined_instruction();
    }
    evex_int_arith(vcpu, ctx, IntOp::MaddWD)
}

/// Execute EVEX VPALIGNR after rejecting reserved fields before state or
/// memory access. EVEX.W is architecturally ignored.
pub fn evex_bw_palignr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if evex_bw_encoding_is_reserved(ctx)? {
        return vcpu.inject_undefined_instruction();
    }
    evex_palignr(vcpu, ctx)
}

/// Execute EVEX VDBPSADBW after rejecting reserved fields before state or
/// memory access. EVEX.W must be zero.
pub fn evex_bw_dbpsadbw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("VDBPSADBW requires EVEX prefix".to_string()))?;
    if evex.w || evex_bw_encoding_is_reserved(ctx)? {
        return vcpu.inject_undefined_instruction();
    }
    vcpu.execute_vdbpsadbw(ctx)
}
