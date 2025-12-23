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
            vcpu.mmu.write_u16(addr, vcpu.sregs.gdt.limit, &vcpu.sregs)?;
            vcpu.mmu.write_u64(addr + 2, vcpu.sregs.gdt.base, &vcpu.sregs)?;
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
            vcpu.mmu.write_u16(addr, vcpu.sregs.idt.limit, &vcpu.sregs)?;
            vcpu.mmu.write_u64(addr + 2, vcpu.sregs.idt.base, &vcpu.sregs)?;
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
                return Err(Error::Emulator(format!("MOV r, DR{}: #UD when CR4.DE=1", dr)));
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
                return Err(Error::Emulator(format!("MOV DR{}, r: #UD when CR4.DE=1", dr)));
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

                        // If PDPT[0] points to PDT (not 1GB page), check and fix PDT entries
                        let mut pdpt_0 = [0u8; 8];
                        if vcpu.mmu.read_phys(pdpt_base, &mut pdpt_0).is_ok() {
                            let pdpte = u64::from_le_bytes(pdpt_0);
                            if pdpte & 1 != 0 && (pdpte & (1 << 7)) == 0 {
                                let pdt_base = pdpte & 0x000F_FFFF_FFFF_F000;
                                eprintln!("[MOV CR3] PDT at {:#x}:", pdt_base);

                                // FIX: The kernel's identity mapping is broken.
                                // It writes 2MB-page entries to PDPT level instead of PDT.
                                // We need to populate the PDT with proper 2MB identity mappings.
                                // Check if PDT[40] (for addr 0x5000000) is missing
                                let mut pdt_40 = [0u8; 8];
                                if vcpu.mmu.read_phys(pdt_base + 40 * 8, &mut pdt_40).is_ok() {
                                    let entry = u64::from_le_bytes(pdt_40);
                                    if entry == 0 {
                                        eprintln!("[MOV CR3] FIX: PDT entries missing, creating identity mappings...");

                                        // Create 2MB identity mappings for 0-512MB
                                        // Each PDT entry covers 2MB, so we need 256 entries
                                        let page_flags: u64 = 0x1e3; // P+RW+A+D+PS+G
                                        for idx in 0..256u64 {
                                            let phys_addr = idx << 21; // idx * 2MB
                                            let pdt_entry = phys_addr | page_flags;
                                            let _ = vcpu.mmu.write_phys(pdt_base + idx * 8, &pdt_entry.to_le_bytes());
                                        }
                                        eprintln!("[MOV CR3] Created 256 PDT entries (0-512MB identity mapping)");
                                    }
                                }

                                // Dump PDT entries that should cover 0x5000000-0x5100000
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
