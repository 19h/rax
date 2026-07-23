//! Direct execution for scalar legacy MOVRS encodings.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// Execute `NOREP 0F 38 {8A,8B} !(11):rrr:bbb`.
///
/// MOVRS carries a read-shared cache hint, but its architectural operation is
/// exactly `DEST := SRC`. RAX does not expose cache-placement state, so the
/// transfer uses the ordinary checked guest-memory path.
pub fn movrs(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext, opcode: u8) -> Result<Option<VcpuExit>> {
    if !vcpu.sregs.cs.l || ctx.rep_prefix.is_some() {
        return vcpu.inject_undefined_instruction();
    }

    let modrm = ctx.peek_u8()?;
    if modrm >> 6 == 3 {
        return vcpu.inject_undefined_instruction();
    }

    let (reg, _, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert!(is_memory, "MOVRS excludes ModR/M register sources");
    let width = if opcode == 0x8A { 1 } else { ctx.op_size };
    let value = vcpu.read_mem(addr, width)?;
    if width == 1 {
        vcpu.set_reg8(reg, value, ctx.has_any_rex());
    } else {
        vcpu.set_reg(reg, value, width);
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
