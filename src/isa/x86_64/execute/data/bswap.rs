//! BSWAP instruction.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

/// BSWAP r16/r32/r64 (0x0F 0xC8-0xCF).
///
/// The architectural r16 result is undefined. RAX's deterministic direct
/// profile preserves the complete destination register for that form.
pub fn bswap(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode2: u8,
) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let reg = (opcode2 - 0xC8) | ctx.any_rex_b();
    let value = vcpu.get_reg(reg, op_size);
    let swapped = match op_size {
        4 => (value as u32).swap_bytes() as u64,
        8 => value.swap_bytes(),
        _ => value,
    };
    vcpu.set_reg(reg, swapped, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
