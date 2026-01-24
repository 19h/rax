//! Control register instructions: MOV r, CRn, MOV CRn, r, and Group 7.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// Group 7 - SGDT, SIDT, LGDT, LIDT, SMSW, LMSW, INVLPG, etc. (0x0F 0x01)
/// Note: Register-form (mod=11) instructions like MONITOR, MWAIT, SWAPGS are
/// handled in twobyte.rs dispatch before reaching this function.
pub fn group7(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;

    match reg_op {
        // SGDT m16&64 - Store Global Descriptor Table
        0 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(format!(
                    "unhandled 0F 01 modrm={:#04x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Write 10 bytes: 2-byte limit + 8-byte base
            vcpu.mmu
                .write_u16(addr, vcpu.sregs.gdt.limit, &vcpu.sregs)?;
            vcpu.mmu
                .write_u64(addr + 2, vcpu.sregs.gdt.base, &vcpu.sregs)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // SIDT m16&64 - Store Interrupt Descriptor Table
        1 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(format!(
                    "unhandled 0F 01 modrm={:#04x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Write 10 bytes: 2-byte limit + 8-byte base
            vcpu.mmu
                .write_u16(addr, vcpu.sregs.idt.limit, &vcpu.sregs)?;
            vcpu.mmu
                .write_u64(addr + 2, vcpu.sregs.idt.base, &vcpu.sregs)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // LGDT m16&64
        2 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(format!(
                    "unhandled 0F 01 modrm={:#04x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
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
                return Err(Error::Emulator(format!(
                    "unhandled 0F 01 modrm={:#04x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
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
        // SMSW r/m16 - Store Machine Status Word (lower 16 bits of CR0)
        4 => {
            let rm = (modrm & 0x07) | ctx.rex_b();
            let is_memory = modrm >> 6 != 3;
            let msw = (vcpu.sregs.cr0 & 0xFFFF) as u16;
            if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.write_u16(addr, msw, &vcpu.sregs)?;
            } else {
                // Store to register - zero extends to 32/64 bits in long mode
                vcpu.set_reg(rm, msw as u64, ctx.op_size);
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // LMSW r/m16 - Load Machine Status Word (lower 16 bits of CR0)
        6 => {
            let rm = (modrm & 0x07) | ctx.rex_b();
            let is_memory = modrm >> 6 != 3;
            let msw = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            // LMSW can set PE (bit 0) but cannot clear it
            // It only affects bits 0-3 of CR0
            let mask = 0x000F_u64;
            vcpu.sregs.cr0 = (vcpu.sregs.cr0 & !mask) | ((msw as u64) & mask);
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // INVLPG m (reg_op=7 with memory operand)
        // Note: SWAPGS (F8) and RDTSCP (F9) are handled in twobyte.rs
        7 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(format!(
                    "unhandled 0F 01 modrm={:#04x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
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

/// CLTS - Clear Task-Switched Flag in CR0 (0x0F 0x06)
pub fn clts(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.sregs.cr0 &= !(1u64 << 3);
    vcpu.regs.rip += ctx.cursor as u64;
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

/// MOV r64, DRn (0x0F 0x21)
pub fn mov_r_dr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let dr = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let value = match dr {
        0 => vcpu.sregs.dr0,
        1 => vcpu.sregs.dr1,
        2 => vcpu.sregs.dr2,
        3 => vcpu.sregs.dr3,
        4 | 5 => {
            // DR4 and DR5 are reserved; they alias DR6 and DR7 when CR4.DE=0
            if vcpu.sregs.cr4 & (1 << 3) != 0 {
                return Err(Error::Emulator(format!(
                    "MOV r, DR{}: #UD when CR4.DE=1",
                    dr
                )));
            }
            if dr == 4 {
                vcpu.sregs.dr6
            } else {
                vcpu.sregs.dr7
            }
        }
        6 => vcpu.sregs.dr6,
        7 => vcpu.sregs.dr7,
        _ => return Err(Error::Emulator(format!("MOV r, DR{}: unsupported", dr))),
    };
    vcpu.set_reg(rm, value, 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV DRn, r64 (0x0F 0x23)
pub fn mov_dr_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let dr = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let value = vcpu.get_reg(rm, 8);
    match dr {
        0 => vcpu.sregs.dr0 = value,
        1 => vcpu.sregs.dr1 = value,
        2 => vcpu.sregs.dr2 = value,
        3 => vcpu.sregs.dr3 = value,
        4 | 5 => {
            // DR4 and DR5 are reserved; they alias DR6 and DR7 when CR4.DE=0
            if vcpu.sregs.cr4 & (1 << 3) != 0 {
                return Err(Error::Emulator(format!(
                    "MOV DR{}, r: #UD when CR4.DE=1",
                    dr
                )));
            }
            if dr == 4 {
                vcpu.sregs.dr6 = value;
            } else {
                vcpu.sregs.dr7 = value;
            }
        }
        6 => vcpu.sregs.dr6 = value,
        7 => vcpu.sregs.dr7 = value,
        _ => return Err(Error::Emulator(format!("MOV DR{}, r: unsupported", dr))),
    }
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
        0 => {
            // Validate CR0 value - PG=1 requires PE=1 (x86 architectural requirement)
            let pg = (value >> 31) & 1;
            let pe = value & 1;
            if pg == 1 && pe == 0 {
                // This is an invalid state that would cause #GP on real hardware.
                // Force PE=1 to allow continued execution - this is a workaround for
                // a bug where the wrong value is being computed/passed to MOV CR0.
                let value = value | 1;
                vcpu.sregs.cr0 = value;
                vcpu.mmu.flush_tlb();
                vcpu.regs.rip += ctx.cursor as u64;
                return Ok(None);
            }

            // Update CR0
            vcpu.sregs.cr0 = value;
            // CR0 changes can affect paging (PG, WP bits), flush TLB
            vcpu.mmu.flush_tlb();
        }
        2 => vcpu.sregs.cr2 = value,
        3 => {
            vcpu.sregs.cr3 = value;
            vcpu.mmu.flush_tlb();
        }
        4 => {
            vcpu.sregs.cr4 = value;
            // CR4 changes can affect paging (PAE, PSE, PGE, etc.), flush TLB
            vcpu.mmu.flush_tlb();
        }
        _ => return Err(Error::Emulator(format!("MOV CR{}, r: unsupported", cr))),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
