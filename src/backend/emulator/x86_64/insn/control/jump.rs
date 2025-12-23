//! Jump instructions: JMP, Jcc.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// JMP rel8 (0xEB)
pub fn jmp_rel8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    Ok(None)
}

/// JMP rel32 (0xE9)
pub fn jmp_rel32(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u32()? as i32 as i64;
    vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    Ok(None)
}

/// Jcc rel8 (0x70-0x7F)
pub fn jcc_rel8(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    cc: u8,
) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    if vcpu.check_condition(cc) {
        vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    } else {
        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}

/// Jcc rel32 (0x0F 0x80-0x8F)
pub fn jcc_rel32(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    cc: u8,
) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u32()? as i32 as i64;
    if vcpu.check_condition(cc) {
        vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    } else {
        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}

/// JMP FAR ptr16:16/ptr16:32 (0xEA)
/// Far jump with immediate pointer - loads segment:offset from instruction
/// Note: This opcode is invalid in 64-bit mode but we emulate it for compatibility.
/// Uses inverted operand size: 16-bit default, 66h prefix gives 32-bit.
pub fn jmp_far_ptr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // 0xEA is a legacy instruction - use inverted operand size logic:
    // - Without 66h prefix (op_size=4 in 64-bit mode): use 16-bit offset
    // - With 66h prefix (op_size=2 in 64-bit mode): use 32-bit offset
    // This matches compatibility mode behavior (CS.D=0)
    let offset = if ctx.operand_size_override {
        ctx.consume_u32()? as u64
    } else {
        ctx.consume_u16()? as u64
    };
    let selector = ctx.consume_u16()?;

    // Load CS:IP (simplified - just set the registers)
    // Full implementation would validate segment descriptor
    vcpu.set_sreg(1, selector); // CS is segment register index 1
    vcpu.regs.rip = offset;
    Ok(None)
}

/// JMP FAR m16:16/m16:32/m16:64 (0xFF /5)
/// Far jump with memory indirect - loads segment:offset from memory
/// Uses inverted operand size for legacy compatibility:
/// - Without prefix: 16-bit offset
/// - With 66h prefix: 32-bit offset
/// - With REX.W: 64-bit offset
pub fn jmp_far_mem(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let _modrm = ctx.consume_u8()?;

    // Get memory address
    let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;

    // Determine offset size using inverted logic (like legacy 16-bit mode)
    // REX.W → 64-bit, 66h → 32-bit, default → 16-bit
    let offset_size: u8 = if ctx.rex_w() {
        8
    } else if ctx.operand_size_override {
        4
    } else {
        2
    };

    // Read offset and selector from memory
    let offset = vcpu.read_mem(addr, offset_size)?;
    let selector = vcpu.mmu.read_u16(addr + offset_size as u64, &vcpu.sregs)?;

    // Load CS:IP
    vcpu.set_sreg(1, selector); // CS is segment register index 1
    vcpu.regs.rip = offset;
    Ok(None)
}
