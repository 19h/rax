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
    use std::sync::atomic::{AtomicU64, Ordering};
    static CPUID_COUNT: AtomicU64 = AtomicU64::new(0);

    let leaf = vcpu.regs.rax as u32;
    let _subleaf = vcpu.regs.rcx as u32;

    let count = CPUID_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    // Always log CPUID 0x80000001 (GBPAGES detection) and first 100 of others
    if leaf == 0x80000001 || count <= 100 {
        eprintln!("[CPUID] leaf={:#x} subleaf={:#x} at RIP={:#x} (call #{})", leaf, _subleaf, vcpu.regs.rip, count);
    }

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
            (0x80000008u32, 0, 0, 0)
        }
        0x80000001 => {
            // Extended features - CRITICAL for efficient identity mapping
            let features_edx = (1u32 << 29)  // LM (Long Mode)
                             | (1u32 << 26)  // PDPE1GB (1GB huge pages in PDPTE)
                             | (1u32 << 20); // NX (No Execute)
            eprintln!("[CPUID] 0x80000001: Returning GBPAGES={}, LM={}, NX={} (EDX={:#x})",
                (features_edx >> 26) & 1, (features_edx >> 29) & 1, (features_edx >> 20) & 1, features_edx);
            (0, 0, 0, features_edx)
        }
        0x80000008 => {
            // Address sizes: physical bits, linear bits, number of cores
            // Use 29 bits (512MB) to minimize page table allocations
            // This limits identity mapping to 512MB + kernel size
            let phys_bits: u32 = 29; // 512MB physical address space
            let linear_bits: u32 = 48;
            (phys_bits | (linear_bits << 8), 0, 0, 0)
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
        3 => {
            tracing::debug!(
                old_cr3 = format!("{:#x}", vcpu.sregs.cr3),
                new_cr3 = format!("{:#x}", value),
                rip = format!("{:#x}", vcpu.regs.rip),
                "MOV CR3"
            );

            // Debug: Dump page table entries when switching to new page tables
            if value != vcpu.sregs.cr3 && value >= 0x5000000 {
                eprintln!("[MOV CR3] Switching from {:#x} to {:#x}", vcpu.sregs.cr3, value);

                // Dump PML4 entries
                let pml4_base = value & !0xFFF;
                eprintln!("[MOV CR3] PML4 at {:#x}:", pml4_base);
                for i in 0..4 {
                    let mut entry_buf = [0u8; 8];
                    if vcpu.mmu.read_phys(pml4_base + i * 8, &mut entry_buf).is_ok() {
                        let entry = u64::from_le_bytes(entry_buf);
                        if entry != 0 {
                            eprintln!("[MOV CR3]   PML4[{}] = {:#x}", i, entry);
                        }
                    }
                }

                // If PML4[0] is present, dump PDPT
                let mut pml4_0 = [0u8; 8];
                if vcpu.mmu.read_phys(pml4_base, &mut pml4_0).is_ok() {
                    let pml4e = u64::from_le_bytes(pml4_0);
                    if pml4e & 1 != 0 {
                        let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
                        eprintln!("[MOV CR3] PDPT at {:#x}:", pdpt_base);
                        for i in 0..4 {
                            let mut entry_buf = [0u8; 8];
                            if vcpu.mmu.read_phys(pdpt_base + i * 8, &mut entry_buf).is_ok() {
                                let entry = u64::from_le_bytes(entry_buf);
                                if entry != 0 {
                                    let is_1gb = (entry & (1 << 7)) != 0;
                                    eprintln!("[MOV CR3]   PDPT[{}] = {:#x} ({})", i, entry,
                                        if is_1gb { "1GB page" } else { "points to PDT" });
                                }
                            }
                        }

                        // If PDPT[0] points to PDT (not 1GB page), dump some PDT entries
                        let mut pdpt_0 = [0u8; 8];
                        if vcpu.mmu.read_phys(pdpt_base, &mut pdpt_0).is_ok() {
                            let pdpte = u64::from_le_bytes(pdpt_0);
                            if pdpte & 1 != 0 && (pdpte & (1 << 7)) == 0 {
                                let pdt_base = pdpte & 0x000F_FFFF_FFFF_F000;
                                eprintln!("[MOV CR3] PDT at {:#x}:", pdt_base);
                                // Dump PDT entries that should cover 0x5000000-0x5100000
                                // PDT index for 0x5000000 = (0x5000000 >> 21) & 0x1FF = 40
                                for i in 38..45 {
                                    let mut entry_buf = [0u8; 8];
                                    if vcpu.mmu.read_phys(pdt_base + i * 8, &mut entry_buf).is_ok() {
                                        let entry = u64::from_le_bytes(entry_buf);
                                        if entry != 0 {
                                            let is_2mb = (entry & (1 << 7)) != 0;
                                            let phys = entry & 0x000F_FFFF_FFE0_0000;
                                            eprintln!("[MOV CR3]   PDT[{}] = {:#x} (maps {:#x}, {})",
                                                i, entry, phys, if is_2mb { "2MB page" } else { "PT" });
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        eprintln!("[MOV CR3] WARNING: PML4[0] = {:#x} - not present!", pml4e);
                    }
                }
            }

            vcpu.sregs.cr3 = value;
        }
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
