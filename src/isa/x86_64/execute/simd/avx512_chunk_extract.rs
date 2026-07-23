//! EVEX 128-bit and 256-bit vector-chunk extraction validation frontiers.

use super::avx512::evex_extract_chunk;
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute EVEX VEXTRACTF*/VEXTRACTI* after rejecting reserved fields before
/// effective-address calculation, memory access, or architectural state
/// access.
pub fn evex_extract_chunk_validated(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    chunk_bytes: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX vector-chunk extract requires EVEX prefix".to_string())
    })?;
    let modrm = ctx.peek_u8()?;
    let is_memory = modrm >> 6 != 3;
    let valid_shape =
        matches!(elem_size, 4 | 8) && matches!((chunk_bytes, evex.ll), (16, 1 | 2) | (32, 2));
    if !valid_shape
        || evex.broadcast
        || evex.vvvv != 0xF
        || !evex.v_prime
        || (evex.z && (evex.aaa == 0 || is_memory))
    {
        return vcpu.inject_undefined_instruction();
    }
    evex_extract_chunk(vcpu, ctx, elem_size, chunk_bytes)
}
