//! Descriptor table instructions: LAR, LSL, Group 6.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;
use super::control_regs::{current_cpl, is_cpl0, raise_gp0};

#[derive(Clone, Copy)]
struct Descriptor {
    raw: u64,
}

impl Descriptor {
    fn present(self) -> bool {
        (self.raw >> 47) & 1 != 0
    }

    fn type_(self) -> u8 {
        ((self.raw >> 40) & 0x0F) as u8
    }

    fn is_code_or_data(self) -> bool {
        (self.raw >> 44) & 1 != 0
    }

    fn executable(self) -> bool {
        self.type_() & 0x8 != 0
    }

    fn conforming_code(self) -> bool {
        self.executable() && self.type_() & 0x4 != 0
    }

    fn dpl(self) -> u8 {
        ((self.raw >> 45) & 0x3) as u8
    }

    fn visible_from(self, selector: u16, cpl: u8) -> bool {
        if self.is_code_or_data() && self.conforming_code() {
            return true;
        }

        let rpl = (selector & 0x3) as u8;
        cpl <= self.dpl() && rpl <= self.dpl()
    }

    fn access_rights(self) -> u64 {
        ((self.raw >> 40) & 0xFFFF) << 8
    }

    fn limit(self) -> u64 {
        let mut limit = (self.raw & 0xFFFF) | (((self.raw >> 48) & 0x0F) << 16);
        if (self.raw >> 55) & 1 != 0 {
            limit = (limit << 12) | 0xFFF;
        }
        limit
    }

    fn can_lar(self, selector: u16, cpl: u8) -> bool {
        if !self.present() {
            return false;
        }

        let valid_type = self.is_code_or_data() || matches!(self.type_(), 0x2 | 0x9 | 0xB | 0xC);

        valid_type && self.visible_from(selector, cpl)
    }

    fn can_lsl(self, selector: u16, cpl: u8) -> bool {
        if !self.present() {
            return false;
        }

        let valid_type = self.is_code_or_data() || matches!(self.type_(), 0x2 | 0x9 | 0xB);

        valid_type && self.visible_from(selector, cpl)
    }

    fn can_verr(self, selector: u16, cpl: u8) -> bool {
        if !self.present() || !self.is_code_or_data() || !self.visible_from(selector, cpl) {
            return false;
        }

        !self.executable() || self.type_() & 0x2 != 0
    }

    fn can_verw(self, selector: u16, cpl: u8) -> bool {
        self.present()
            && self.is_code_or_data()
            && self.visible_from(selector, cpl)
            && !self.executable()
            && self.type_() & 0x2 != 0
    }
}

fn descriptor_for_selector(vcpu: &mut X86_64Vcpu, selector: u16) -> Result<Option<Descriptor>> {
    if selector & 0xFFFC == 0 {
        return Ok(None);
    }

    let ti = (selector & 0x4) != 0;
    let index = (selector >> 3) as u64;
    let (table_base, table_limit) = if ti {
        (vcpu.sregs.ldt.base, vcpu.sregs.ldt.limit as u64)
    } else {
        (vcpu.sregs.gdt.base, vcpu.sregs.gdt.limit as u64)
    };

    let offset = index * 8;
    if offset + 7 > table_limit {
        return Ok(None);
    }

    let raw = vcpu
        .mmu
        .read_u64_supervisor(table_base + offset, &vcpu.sregs)?;
    Ok(Some(Descriptor { raw }))
}

fn descriptor_for_lar(vcpu: &mut X86_64Vcpu, selector: u16) -> Result<Option<Descriptor>> {
    let cpl = current_cpl(vcpu);
    Ok(descriptor_for_selector(vcpu, selector)?.filter(|desc| desc.can_lar(selector, cpl)))
}

fn descriptor_for_lsl(vcpu: &mut X86_64Vcpu, selector: u16) -> Result<Option<Descriptor>> {
    let cpl = current_cpl(vcpu);
    Ok(descriptor_for_selector(vcpu, selector)?.filter(|desc| desc.can_lsl(selector, cpl)))
}

fn set_zf(vcpu: &mut X86_64Vcpu, set: bool) {
    vcpu.clear_lazy_flags();
    if set {
        vcpu.regs.rflags |= flags::bits::ZF;
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF;
    }
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
            // Privileged: loading the LDTR requires CPL 0.
            if !is_cpl0(vcpu) {
                return raise_gp0(vcpu);
            }
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
            // Privileged: loading the task register requires CPL 0.
            if !is_cpl0(vcpu) {
                return raise_gp0(vcpu);
            }
            let selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            vcpu.sregs.tr.selector = selector;

            // Load the TSS descriptor from the GDT
            // In 64-bit mode, TSS descriptor is 16 bytes (system segment descriptor)
            let gdt_base = vcpu.sregs.gdt.base;
            let index = (selector >> 3) as u64;
            let desc_addr = gdt_base + index * 8;

            // Read the 16-byte system segment descriptor
            let mut desc_bytes = [0u8; 16];
            vcpu.mmu.read(desc_addr, &mut desc_bytes, &vcpu.sregs)?;

            // Parse the descriptor (64-bit TSS descriptor format)
            // Bytes 0-7: legacy descriptor format
            // Bytes 8-15: upper 32 bits of base address + reserved
            let limit_low = u16::from_le_bytes([desc_bytes[0], desc_bytes[1]]) as u32;
            let base_low = u16::from_le_bytes([desc_bytes[2], desc_bytes[3]]) as u64;
            let base_mid = desc_bytes[4] as u64;
            let _type_attr = desc_bytes[5];
            let limit_high = (desc_bytes[6] & 0x0F) as u32;
            let base_high_byte = desc_bytes[7] as u64;
            let base_upper =
                u32::from_le_bytes([desc_bytes[8], desc_bytes[9], desc_bytes[10], desc_bytes[11]])
                    as u64;

            let limit = limit_low | (limit_high << 16);
            let base = base_low | (base_mid << 16) | (base_high_byte << 24) | (base_upper << 32);

            vcpu.sregs.tr.base = base;
            vcpu.sregs.tr.limit = limit;
        }
        // VERR - Verify Read (0x0F 0x00 /4)
        4 => {
            let selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            let cpl = current_cpl(vcpu);
            let readable = descriptor_for_selector(vcpu, selector)?
                .map(|desc| desc.can_verr(selector, cpl))
                .unwrap_or(false);
            set_zf(vcpu, readable);
        }
        // VERW - Verify Write (0x0F 0x00 /5)
        5 => {
            let selector = if is_memory {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u16(addr, &vcpu.sregs)?
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            let cpl = current_cpl(vcpu);
            let writable = descriptor_for_selector(vcpu, selector)?
                .map(|desc| desc.can_verw(selector, cpl))
                .unwrap_or(false);
            set_zf(vcpu, writable);
        }
        _ => {
            return vcpu.inject_undefined_instruction();
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

    if let Some(desc) = descriptor_for_lar(vcpu, selector)? {
        vcpu.set_reg(reg, desc.access_rights(), ctx.op_size);
        set_zf(vcpu, true); // Valid selector
    } else {
        set_zf(vcpu, false); // Null selector
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

    if let Some(desc) = descriptor_for_lsl(vcpu, selector)? {
        vcpu.set_reg(reg, desc.limit(), ctx.op_size);
        set_zf(vcpu, true); // Valid selector
    } else {
        set_zf(vcpu, false); // Null selector
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
