//! Legacy SSE MIN/MAX preserve source-selection, DAZ, and MXCSR semantics.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::smir::interpret::X86SimdArithmetic;
use crate::vm::vcpu::VcpuExit;

pub fn sse_min(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    super::fp_arithmetic::execute(vcpu, ctx, X86SimdArithmetic::Minimum)
}

pub fn sse_max(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    super::fp_arithmetic::execute(vcpu, ctx, X86SimdArithmetic::Maximum)
}
