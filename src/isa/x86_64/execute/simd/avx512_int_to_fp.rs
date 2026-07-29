//! EVEX packed integer-to-floating-point conversion frontiers.

use super::avx512::evex_packed_int_to_fp;
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute `VCVTDQ2PD` or `VCVTUDQ2PD`.
///
/// Intel defines register-source `EVEX.b=1` as an ignored attempt to encode
/// embedded rounding. It implies a 512-bit operation and ignores every `L'L`
/// value. A memory source instead uses `EVEX.b` for broadcast and therefore
/// retains the ordinary `L'L=11b` reservation.
pub fn evex_packed_i32_to_f64(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    signed: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX packed I32-to-F64 conversion requires EVEX prefix".to_string())
    })?;
    let is_memory = ctx.peek_u8()? >> 6 != 3;

    if evex.vvvv != 0x0F
        || !evex.v_prime
        || (evex.z && evex.aaa == 0)
        || (evex.ll == 3 && (is_memory || !evex.broadcast))
    {
        return vcpu.inject_undefined_instruction();
    }

    if evex.broadcast && !is_memory {
        // All I32 values are exactly representable as F64. Normalize the
        // ignored L'L field before delegating while retaining EVEX.b so the
        // shared conversion path selects the implied 512-bit vector length.
        ctx.evex
            .as_mut()
            .expect("validated EVEX packed I32-to-F64 prefix")
            .ll = 0;
    }

    evex_packed_int_to_fp(vcpu, ctx, 4, 8, signed)
}
