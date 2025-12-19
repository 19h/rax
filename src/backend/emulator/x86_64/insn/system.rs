//! System instructions: CPUID, RDMSR, WRMSR, LGDT, LIDT, CLI, STI, etc.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// CLI - Clear Interrupt Flag (0xFA)
pub fn cli(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags &= !flags::bits::IF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STI - Set Interrupt Flag (0xFB)
pub fn sti(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags |= flags::bits::IF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CLC - Clear Carry Flag (0xF8)
pub fn clc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags &= !flags::bits::CF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STC - Set Carry Flag (0xF9)
pub fn stc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags |= flags::bits::CF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CLD - Clear Direction Flag (0xFC)
pub fn cld(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags &= !flags::bits::DF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STD - Set Direction Flag (0xFD)
pub fn std(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags |= flags::bits::DF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PUSHF (0x9C)
pub fn pushf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.push64(vcpu.regs.rflags)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// POPF (0x9D)
pub fn popf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let flags_val = vcpu.pop64()?;
    // Preserve reserved bits
    vcpu.regs.rflags = (flags_val & 0x00000000_00257FD5) | 0x2;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CPUID (0x0F 0xA2)
pub fn cpuid(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let leaf = vcpu.regs.rax as u32;
    let _subleaf = vcpu.regs.rcx as u32;

    let (eax, ebx, ecx, edx) = match leaf {
        0 => {
            // Return max leaf and vendor string "RaxEmulato"
            (0x01, 0x45786152, 0x6f74616c, 0x756d456d)
        }
        1 => {
            // Processor signature and features
            let features_edx =
                0x00800001 | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 9) | (1 << 15) | (1 << 19);
            let features_ecx = 0;
            (0x00000000, 0x00000000, features_ecx, features_edx)
        }
        0x80000000 => {
            // Extended CPUID Information - max extended leaf
            (0x80000001u32, 0, 0, 0)
        }
        0x80000001 => {
            // Extended features
            let features_edx = 1u32 << 29; // LM (Long Mode)
            (0, 0, 0, features_edx)
        }
        _ => (0, 0, 0, 0),
    };

    vcpu.regs.rax = eax as u64;
    vcpu.regs.rbx = ebx as u64;
    vcpu.regs.rcx = ecx as u64;
    vcpu.regs.rdx = edx as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// WRMSR (0x0F 0x30)
pub fn wrmsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ecx = vcpu.regs.rcx as u32;
    let value = ((vcpu.regs.rdx & 0xFFFF_FFFF) << 32) | (vcpu.regs.rax & 0xFFFF_FFFF);

    match ecx {
        0xC0000080 => vcpu.sregs.efer = value, // EFER
        0xC0000100 => vcpu.sregs.fs.base = value, // FS.base
        0xC0000101 => vcpu.sregs.gs.base = value, // GS.base
        0xC0000102 => {}                       // KernelGSbase - ignore
        _ => {}                                // Ignore unknown MSRs
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDMSR (0x0F 0x32)
pub fn rdmsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ecx = vcpu.regs.rcx as u32;

    let value = match ecx {
        0xC0000080 => vcpu.sregs.efer, // EFER
        0xC0000100 => vcpu.sregs.fs.base, // FS.base
        0xC0000101 => vcpu.sregs.gs.base, // GS.base
        0xC0000102 => 0,               // KernelGSbase
        _ => 0,                        // Return 0 for unknown MSRs
    };

    vcpu.regs.rax = (value & 0xFFFF_FFFF) as u64;
    vcpu.regs.rdx = (value >> 32) as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 7 - LGDT, LIDT, INVLPG, etc. (0x0F 0x01)
pub fn group7(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;

    match reg_op {
        // LGDT m16&64
        2 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(
                    "LGDT: requires memory operand".to_string(),
                ));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Read 10 bytes: 2-byte limit + 8-byte base
            let limit = vcpu.mmu.read_u16(addr, &vcpu.sregs)?;
            let base = vcpu.mmu.read_u64(addr + 2, &vcpu.sregs)?;
            vcpu.sregs.gdt.limit = limit;
            vcpu.sregs.gdt.base = base;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // LIDT m16&64
        3 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(
                    "LIDT: requires memory operand".to_string(),
                ));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Read 10 bytes: 2-byte limit + 8-byte base
            let limit = vcpu.mmu.read_u16(addr, &vcpu.sregs)?;
            let base = vcpu.mmu.read_u64(addr + 2, &vcpu.sregs)?;
            vcpu.sregs.idt.limit = limit;
            vcpu.sregs.idt.base = base;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // INVLPG m
        7 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(
                    "INVLPG: requires memory operand".to_string(),
                ));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Invalidate TLB entry for address
            vcpu.mmu.invlpg(addr);
            vcpu.regs.rip += ctx.cursor as u64;
        }
        _ => {
            return Err(Error::Emulator(format!(
                "unimplemented 0F 01 /{} at RIP={:#x}",
                reg_op, vcpu.regs.rip
            )));
        }
    }
    Ok(None)
}

/// MOV r64, CRn (0x0F 0x20)
pub fn mov_r_cr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let cr = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let value = match cr {
        0 => vcpu.sregs.cr0,
        2 => vcpu.sregs.cr2,
        3 => vcpu.sregs.cr3,
        4 => vcpu.sregs.cr4,
        _ => return Err(Error::Emulator(format!("MOV r, CR{}: unsupported", cr))),
    };
    vcpu.set_reg(rm, value, 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV CRn, r64 (0x0F 0x22)
pub fn mov_cr_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let cr = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let value = vcpu.get_reg(rm, 8);
    match cr {
        0 => vcpu.sregs.cr0 = value,
        2 => vcpu.sregs.cr2 = value,
        3 => vcpu.sregs.cr3 = value,
        4 => vcpu.sregs.cr4 = value,
        _ => return Err(Error::Emulator(format!("MOV CR{}, r: unsupported", cr))),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ENDBR64/ENDBR32 (0x0F 0x1E) - CET instructions, treat as NOP
pub fn endbr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Skip the ModR/M byte (FA=ENDBR64, FB=ENDBR32)
    ctx.cursor += 1;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Multi-byte NOP (0x0F 0x1F)
pub fn nop_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    // Skip any additional bytes for memory operand
    if modrm >> 6 != 3 {
        let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
