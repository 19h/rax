//! EVEX 128-bit and 256-bit vector-chunk insertion validation frontiers.

use super::avx512::evex_insert_chunk;
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute EVEX VINSERTF*/VINSERTI* after rejecting reserved fields before
/// effective-address calculation, memory access, or architectural state
/// access.
pub fn evex_insert_chunk_validated(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    chunk_bytes: usize,
) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX vector-chunk insert requires EVEX prefix".to_string())
    })?;
    let valid_shape =
        matches!(elem_size, 4 | 8) && matches!((chunk_bytes, evex.ll), (16, 1 | 2) | (32, 2));
    if !valid_shape || evex.broadcast || (evex.z && evex.aaa == 0) {
        return vcpu.inject_undefined_instruction();
    }
    evex_insert_chunk(vcpu, ctx, elem_size, chunk_bytes)
}
