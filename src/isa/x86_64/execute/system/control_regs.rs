//! Control register instructions: MOV r, CRn, MOV CRn, r, and Group 7.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

const CR0_HIGH_RESERVED_MASK: u64 = 0xFFFF_FFFF_0000_0000;
const CR4_UMIP: u64 = 1 << 11;
const CR4_HIGH_RESERVED_MASK: u64 = 0xFFFF_FFFF_0000_0000;

/// Current Privilege Level of the executing code.
///
/// The CPL is the low two bits of the CS selector in protected mode, except
/// that virtual-8086 mode always executes with effective CPL 3. In real mode
/// (CR0.PE=0) there is no privilege concept and the processor effectively runs
/// as ring 0; many rax test fixtures also leave CS unset (selector 0) while
/// exercising privileged instructions, so a non-protected-mode vCPU must be
/// treated as CPL 0 to avoid spurious faults.
#[inline]
pub(super) fn current_cpl(vcpu: &X86_64Vcpu) -> u8 {
    // CR0.PE (bit 0) distinguishes protected mode from real mode.
    if vcpu.sregs.cr0 & 1 == 0 {
        return 0;
    }
    // RFLAGS.VM (bit 17) identifies virtual-8086 mode. CS.RPL is not an
    // authoritative privilege indicator there.
    if vcpu.regs.rflags & (1 << 17) != 0 {
        return 3;
    }
    (vcpu.sregs.cs.selector & 0x3) as u8
}

/// Returns true if the current code is privileged (CPL == 0).
#[inline]
pub(super) fn is_cpl0(vcpu: &X86_64Vcpu) -> bool {
    current_cpl(vcpu) == 0
}

#[inline]
pub(super) fn umip_blocks_user_instruction(vcpu: &X86_64Vcpu) -> bool {
    vcpu.sregs.cr4 & CR4_UMIP != 0 && current_cpl(vcpu) > 0
}

fn write_descriptor_table(vcpu: &mut X86_64Vcpu, addr: u64, limit: u16, base: u64) -> Result<()> {
    vcpu.mmu.write_u16(addr, limit, &vcpu.sregs)?;
    if vcpu.sregs.cs.l {
        vcpu.mmu.write_u64(addr + 2, base, &vcpu.sregs)?;
    } else {
        vcpu.mmu.write_u32(addr + 2, base as u32, &vcpu.sregs)?;
    }
    Ok(())
}

fn read_descriptor_table(vcpu: &mut X86_64Vcpu, addr: u64) -> Result<(u16, u64)> {
    let limit = vcpu.mmu.read_u16(addr, &vcpu.sregs)?;
    let base = if vcpu.sregs.cs.l {
        vcpu.mmu.read_u64(addr + 2, &vcpu.sregs)?
    } else {
        u64::from(vcpu.mmu.read_u32(addr + 2, &vcpu.sregs)?)
    };
    Ok((limit, base))
}

/// Inject a #GP(0) (General Protection fault, vector 13, error code 0).
///
/// Exception delivery sets RIP to the fault handler, so callers MUST return
/// without advancing RIP past the faulting instruction.
#[inline]
pub(super) fn raise_gp0(vcpu: &mut X86_64Vcpu) -> Result<Option<VcpuExit>> {
    vcpu.inject_exception(13, Some(0))?;
    Ok(None)
}

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
                return vcpu.inject_undefined_instruction();
            }
            if umip_blocks_user_instruction(vcpu) {
                return raise_gp0(vcpu);
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            write_descriptor_table(vcpu, addr, vcpu.sregs.gdt.limit, vcpu.sregs.gdt.base)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // SIDT m16&64 - Store Interrupt Descriptor Table
        1 => {
            if modrm >> 6 == 3 {
                return vcpu.inject_undefined_instruction();
            }
            if umip_blocks_user_instruction(vcpu) {
                return raise_gp0(vcpu);
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            write_descriptor_table(vcpu, addr, vcpu.sregs.idt.limit, vcpu.sregs.idt.base)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // LGDT m16&64
        2 => {
            // Privileged: loading the GDTR requires CPL 0.
            if !is_cpl0(vcpu) {
                return raise_gp0(vcpu);
            }
            if modrm >> 6 == 3 {
                return vcpu.inject_undefined_instruction();
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            let (limit, base) = read_descriptor_table(vcpu, addr)?;
            vcpu.sregs.gdt.limit = limit;
            vcpu.sregs.gdt.base = base;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // LIDT m16&64
        3 => {
            // Privileged: loading the IDTR requires CPL 0.
            if !is_cpl0(vcpu) {
                return raise_gp0(vcpu);
            }
            if modrm >> 6 == 3 {
                return vcpu.inject_undefined_instruction();
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            let (limit, base) = read_descriptor_table(vcpu, addr)?;
            vcpu.sregs.idt.limit = limit;
            vcpu.sregs.idt.base = base;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        // SMSW r/m16 - Store Machine Status Word (lower 16 bits of CR0)
        4 => {
            if umip_blocks_user_instruction(vcpu) {
                return raise_gp0(vcpu);
            }
            let rm = (modrm & 0x07) | ctx.rex_b();
            let is_memory = modrm >> 6 != 3;
            if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu
                    .write_u16(addr, (vcpu.sregs.cr0 & 0xFFFF) as u16, &vcpu.sregs)?;
            } else {
                let value = match ctx.op_size {
                    2 => vcpu.sregs.cr0 & 0xFFFF,
                    4 => vcpu.sregs.cr0 & 0xFFFF_FFFF,
                    8 => vcpu.sregs.cr0,
                    _ => unreachable!(),
                };
                vcpu.set_reg(rm, value, ctx.op_size);
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
                return vcpu.inject_undefined_instruction();
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
            // Invalidate TLB entry for address
            vcpu.mmu.invlpg(addr);
            vcpu.regs.rip += ctx.cursor as u64;
        }
        _ => {
            return vcpu.inject_undefined_instruction();
        }
    }
    Ok(None)
}

/// CLTS - Clear Task-Switched Flag in CR0 (0x0F 0x06)
pub fn clts(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Real-address mode is explicitly permitted. Protected, compatibility,
    // and 64-bit modes require CPL0; virtual-8086 mode has effective CPL3.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }
    vcpu.sregs.cr0 &= !(1u64 << 3);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r32/r64, CRn (0x0F 0x20)
pub fn mov_r_cr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // REX2/APX does not define an extended MOV-from-control-register form.
    // Decode-time #UD conditions take precedence over the dynamic CPL check.
    if ctx.rex2.is_some() {
        return vcpu.inject_undefined_instruction();
    }
    let modrm = ctx.consume_u8()?;
    let cr = ((modrm >> 3) & 0x07) | ctx.rex_r();
    let rm = (modrm & 0x07) | ctx.rex_b();
    if !matches!(cr, 0 | 2 | 3 | 4 | 8) || (cr == 8 && !vcpu.sregs.cs.l) {
        return vcpu.inject_undefined_instruction();
    }
    // Real-address mode is permitted. Protected, compatibility, and 64-bit
    // modes require CPL0; virtual-8086 mode has effective CPL3.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }
    let value = match cr {
        0 => vcpu.sregs.cr0,
        2 => vcpu.sregs.cr2,
        3 => vcpu.sregs.cr3,
        4 => vcpu.sregs.cr4,
        8 => vcpu.sregs.cr8,
        _ => unreachable!("validated readable control register changed"),
    };
    // Outside 64-bit mode this instruction always has a 32-bit operand; 66H
    // is ignored. A 32-bit GPR write zero-extends in the canonical register
    // state, while 64-bit mode writes the complete r64 destination.
    vcpu.set_reg(rm, value, if vcpu.sregs.cs.l { 8 } else { 4 });
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r64, DRn (0x0F 0x21)
pub fn mov_r_dr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Privileged: accessing debug registers requires CPL 0.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }
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
                return vcpu.inject_undefined_instruction();
            }
            if dr == 4 {
                vcpu.sregs.dr6
            } else {
                vcpu.sregs.dr7
            }
        }
        6 => vcpu.sregs.dr6,
        7 => vcpu.sregs.dr7,
        _ => return vcpu.inject_undefined_instruction(),
    };
    vcpu.set_reg(rm, value, 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV DRn, r64 (0x0F 0x23)
pub fn mov_dr_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Privileged: writing debug registers requires CPL 0.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }
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
                return vcpu.inject_undefined_instruction();
            }
            if dr == 4 {
                vcpu.sregs.dr6 = value;
            } else {
                vcpu.sregs.dr7 = value;
            }
        }
        6 => vcpu.sregs.dr6 = value,
        7 => vcpu.sregs.dr7 = value,
        _ => return vcpu.inject_undefined_instruction(),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV CRn, r64 (0x0F 0x22)
pub fn mov_cr_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Privileged: writing control registers requires CPL 0.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }
    let modrm = ctx.consume_u8()?;
    let cr = ((modrm >> 3) & 0x07) | ctx.rex_r();
    let rm = (modrm & 0x07) | ctx.rex_b();
    let value = vcpu.get_reg(rm, 8);

    match cr {
        0 => {
            if value & CR0_HIGH_RESERVED_MASK != 0 {
                return raise_gp0(vcpu);
            }

            // Validate CR0 value - PG=1 requires PE=1 (x86 architectural requirement).
            // An invalid intermediate (PG=1, PE=0) would #GP on real hardware; force
            // PE=1 so a guest computing the wrong transient can continue.
            let mut value = value;
            if (value >> 31) & 1 == 1 && value & 1 == 0 {
                value |= 1;
            }

            // Update CR0.
            vcpu.sregs.cr0 = value;

            // EFER.LMA is set by the processor (not software) when CR0.PG is
            // enabled while EFER.LME=1 — the protected→long transition. The far
            // jump that follows then establishes 64-bit vs compatibility mode
            // from CS.L. Without setting LMA here, a guest enabling paging to
            // enter long mode (e.g. TempleOS, which then runs compatibility-mode
            // code under its 4-level page tables) leaves LMA clear and the paging
            // walker rejects its long-mode page tables.
            //
            // LMA is only *set*, never cleared: disabling paging in long mode
            // #GPs on real hardware (no real OS does it), and some test fixtures
            // legitimately run long mode with CR0.PG=0 (physical==virtual) and
            // must keep LMA across a CR0 write that leaves PG clear.
            const EFER_LME: u64 = 1 << 8; // Long Mode Enable
            const EFER_LMA: u64 = 1 << 10; // Long Mode Active
            let pg = (value >> 31) & 1 != 0;
            let lme = vcpu.sregs.efer & EFER_LME != 0;
            if pg && lme {
                vcpu.sregs.efer |= EFER_LMA;
            }

            // CR0 changes can affect paging (PG, WP bits), flush TLB
            vcpu.mmu.flush_tlb();
        }
        2 => vcpu.sregs.cr2 = value,
        3 => {
            // With CR4.PCIDE set, CR3 bit 63 requests no TLB flush on write; it
            // is not part of the architecturally readable CR3 value.
            vcpu.sregs.cr3 = value & !(1u64 << 63);
            vcpu.mmu.flush_tlb();
        }
        4 => {
            if value & CR4_HIGH_RESERVED_MASK != 0 {
                return raise_gp0(vcpu);
            }
            vcpu.sregs.cr4 = value;
            // CR4 changes can affect paging (PAE, PSE, PGE, etc.), flush TLB
            vcpu.mmu.flush_tlb();
        }
        8 => {
            if value & !0xF != 0 {
                return raise_gp0(vcpu);
            }
            vcpu.sregs.cr8 = value;
        }
        _ => return vcpu.inject_undefined_instruction(),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
