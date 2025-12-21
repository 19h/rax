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
            // EAX: Stepping=1, Model=15, Family=6 => 0x6F1 (typical x86-64)
            let signature: u32 = 0x000006F1;
            let features_edx =
                0x00800001 | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 9) | (1 << 15) | (1 << 19);
            // ECX: SSE3(0), SSSE3(9), SSE4.1(19), SSE4.2(20), POPCNT(23), LAHF/SAHF(0)
            let features_ecx: u32 = (1 << 0) | (1 << 9) | (1 << 19) | (1 << 20) | (1 << 23);
            (signature, 0x00000000, features_ecx, features_edx)
        }
        2 => {
            // Cache and TLB information
            // AL = iteration count (always 1 for modern CPUs)
            // Format: each byte is a descriptor. 0 = null descriptor
            // Return a simple valid response
            (0x01, 0, 0, 0) // AL=1 = single iteration required
        }
        0x80000000 => {
            // Extended CPUID Information - max extended leaf
            (0x80000008u32, 0, 0, 0)
        }
        0x80000001 => {
            // Extended features - CRITICAL for efficient identity mapping
            // EAX: Same signature as leaf 1 (extended signature)
            let signature: u32 = 0x000006F1;
            let features_edx = (1u32 << 29)  // LM (Long Mode)
                             | (1u32 << 26)  // PDPE1GB (1GB huge pages in PDPTE)
                             | (1u32 << 20); // NX (No Execute)
            eprintln!("[CPUID] 0x80000001: Returning GBPAGES={}, LM={}, NX={} (EDX={:#x})",
                (features_edx >> 26) & 1, (features_edx >> 29) & 1, (features_edx >> 20) & 1, features_edx);
            (signature, 0, 0, features_edx)
        }
        // Brand string: "Rax Emulator" padded to 48 bytes (3 leaves x 16 bytes)
        0x80000002 => {
            // "Rax Emulato" (first 12 chars = 3x u32)
            (0x20786152, 0x6c756d45, 0x726f7461, 0x00000000) // "Rax Emulator\0\0\0\0"
        }
        0x80000003 => {
            (0, 0, 0, 0) // Second part (empty/null)
        }
        0x80000004 => {
            (0, 0, 0, 0) // Third part (empty/null)
        }
        0x80000008 => {
            // Address sizes: physical bits, linear bits, number of cores
            // Use 36 bits (64GB) for a reasonable physical address space
            let phys_bits: u32 = 36; // 64GB physical address space
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

/// Group 7 - SGDT, SIDT, LGDT, LIDT, SMSW, LMSW, INVLPG, etc. (0x0F 0x01)
pub fn group7(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;

    match reg_op {
        // SGDT m16&64 - Store Global Descriptor Table
        0 => {
            if modrm >> 6 == 3 {
                return Err(Error::Emulator(
                    "SGDT: requires memory operand".to_string(),
                ));
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
                return Err(Error::Emulator(
                    "SIDT: requires memory operand".to_string(),
                ));
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

/// CMC - Complement Carry Flag (0xF5)
pub fn cmc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags ^= flags::bits::CF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LAHF - Load AH from Flags (0x9F)
/// Loads SF, ZF, AF, PF, CF from RFLAGS into AH
pub fn lahf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // AH = SF:ZF:0:AF:0:PF:1:CF (bits 7:6:5:4:3:2:1:0)
    // These correspond to RFLAGS bits 7, 6, 4, 2, 0
    let flags_byte = (vcpu.regs.rflags & 0xFF) as u8;
    // Set AH (bits 8-15 of RAX)
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF00) | ((flags_byte as u64) << 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SAHF - Store AH into Flags (0x9E)
/// Stores AH into SF, ZF, AF, PF, CF of RFLAGS
pub fn sahf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // AH contains SF:ZF:0:AF:0:PF:1:CF
    let ah = ((vcpu.regs.rax >> 8) & 0xFF) as u64;
    // Mask for SF, ZF, AF, PF, CF (bits 7, 6, 4, 2, 0)
    let mask = 0xD5u64; // 1101_0101
    vcpu.regs.rflags = (vcpu.regs.rflags & !mask) | (ah & mask);
    // Bit 1 is always set
    vcpu.regs.rflags |= 0x2;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Return a simulated timestamp (just increment monotonically)
    use std::sync::atomic::{AtomicU64, Ordering};
    static TSC: AtomicU64 = AtomicU64::new(0);
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = tsc >> 32;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDTSCP - Read Time-Stamp Counter and Processor ID (0x0F 0x01 0xF9)
pub fn rdtscp(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Similar to RDTSC but also sets ECX to processor ID
    use std::sync::atomic::{AtomicU64, Ordering};
    static TSC: AtomicU64 = AtomicU64::new(0);
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = tsc >> 32;
    vcpu.regs.rcx = 0; // Processor ID = 0
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PAUSE - Spin-Loop Hint (F3 90)
pub fn pause(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Hint to the processor that this is a spin-wait loop
    // In emulation, we just treat it as NOP
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LFENCE - Load Fence (0x0F 0xAE /5)
pub fn lfence(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Memory fence - treat as NOP in emulation
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MFENCE - Memory Fence (0x0F 0xAE /6)
pub fn mfence(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Memory fence - treat as NOP in emulation
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SFENCE - Store Fence (0x0F 0xAE /7)
pub fn sfence(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Memory fence - treat as NOP in emulation
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

/// Group 6 - SLDT, STR, LLDT, LTR, VERR, VERW (0x0F 0x00)
pub fn group6(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let is_memory = modrm >> 6 != 3;

    match reg_op {
        // SLDT - Store Local Descriptor Table (0x0F 0x00 /0)
        0 => {
            let selector = vcpu.sregs.ldt.selector;
            if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.write_u16(addr, selector, &vcpu.sregs)?;
            } else {
                // Writing to register - zero-extends for 32/64-bit registers
                vcpu.set_reg(rm, selector as u64, ctx.op_size);
            }
        }
        // STR - Store Task Register (0x0F 0x00 /1)
        1 => {
            let selector = vcpu.sregs.tr.selector;
            if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.write_u16(addr, selector, &vcpu.sregs)?;
            } else {
                vcpu.set_reg(rm, selector as u64, ctx.op_size);
            }
        }
        // LLDT - Load Local Descriptor Table (0x0F 0x00 /2)
        2 => {
            let selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            vcpu.sregs.ldt.selector = selector;
            // In a real implementation, we'd load the descriptor from the GDT
            // For emulation purposes, just store the selector
        }
        // LTR - Load Task Register (0x0F 0x00 /3)
        3 => {
            let selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            vcpu.sregs.tr.selector = selector;
            // In a real implementation, we'd load the TSS descriptor from the GDT
        }
        // VERR - Verify Read (0x0F 0x00 /4)
        4 => {
            let _selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            // In real hardware, this checks if the selector is readable
            // For emulation, we'll just set ZF=1 (readable) for non-null selectors
            if _selector != 0 {
                vcpu.regs.rflags |= flags::bits::ZF;
            } else {
                vcpu.regs.rflags &= !flags::bits::ZF;
            }
        }
        // VERW - Verify Write (0x0F 0x00 /5)
        5 => {
            let _selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            // For emulation, set ZF=1 (writable) for non-null selectors
            if _selector != 0 {
                vcpu.regs.rflags |= flags::bits::ZF;
            } else {
                vcpu.regs.rflags &= !flags::bits::ZF;
            }
        }
        _ => {
            return Err(Error::Emulator(format!(
                "unimplemented 0F 00 /{} at RIP={:#x}",
                reg_op, vcpu.regs.rip
            )));
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LAR - Load Access Rights (0x0F 0x02)
pub fn lar(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg = ((modrm >> 3) & 0x07) | ctx.rex_r();
    let rm = (modrm & 0x07) | ctx.rex_b();
    let is_memory = modrm >> 6 != 3;

    let selector = if is_memory {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        vcpu.mmu.read_u16(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 2) as u16
    };

    // In a real implementation, we'd read the descriptor from GDT/LDT
    // For emulation, return a standard code/data segment access rights
    if selector != 0 {
        // Return typical access rights: present, ring 0, code/data segment
        let access_rights: u64 = 0x00CF9300; // Standard access rights
        vcpu.set_reg(reg, access_rights, ctx.op_size);
        vcpu.regs.rflags |= flags::bits::ZF; // Valid selector
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF; // Null selector
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LSL - Load Segment Limit (0x0F 0x03)
pub fn lsl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg = ((modrm >> 3) & 0x07) | ctx.rex_r();
    let rm = (modrm & 0x07) | ctx.rex_b();
    let is_memory = modrm >> 6 != 3;

    let selector = if is_memory {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        vcpu.mmu.read_u16(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 2) as u16
    };

    // For emulation, return max limit for valid selectors
    if selector != 0 {
        let limit: u64 = 0xFFFFFFFF; // Max 4GB limit (granularity bit set)
        vcpu.set_reg(reg, limit, ctx.op_size);
        vcpu.regs.rflags |= flags::bits::ZF; // Valid selector
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF; // Null selector
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
