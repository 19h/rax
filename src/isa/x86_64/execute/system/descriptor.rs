//! Descriptor table instructions: LAR, LSL, Group 6.

use crate::error::{Error, Result};
use crate::vm::vcpu::{Segment, VcpuExit};

use super::control_regs::{current_cpl, is_cpl0, raise_gp0, umip_blocks_user_instruction};
use super::msr::is_canonical_48;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;

/// Architectural descriptor-validation faults shared by direct execution and
/// standalone SMIR interpretation. Selector-derived error codes clear RPL but
/// retain the selector index and TI bit, as required by x86 exception format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86SystemDescriptorFault {
    GeneralProtection { error_code: u32 },
    SegmentNotPresent { error_code: u32 },
}

/// LLDT failure before any LDTR field commits. Native lowering treats every
/// variant as a precise deoptimization; direct execution delivers the encoded
/// architectural exception or propagates the original memory fault.
#[derive(Debug)]
pub(in crate::isa::x86_64) enum X86LdtrLoadFault {
    Architectural(X86SystemDescriptorFault),
    Memory(Error),
}

#[inline]
fn selector_error_code(selector: u16) -> u32 {
    u32::from(selector & 0xFFFC)
}

/// Decode and validate one LDT system descriptor after the owning GDT bytes
/// have been read. In 64-bit mode the descriptor is 16 bytes: the upper base
/// dword is followed by a reserved dword, and the legacy L/D attribute bits
/// are reserved for this system-descriptor format.
pub(crate) fn decode_x86_ldt_descriptor(
    selector: u16,
    low: u64,
    high: Option<u64>,
    long_mode: bool,
) -> std::result::Result<Segment, X86SystemDescriptorFault> {
    let error_code = selector_error_code(selector);
    let type_ = ((low >> 40) & 0x0F) as u8;
    let system = (low >> 44) & 1 == 0;
    if !system || type_ != 0x2 {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }

    let mut base =
        ((low >> 16) & 0xFFFF) | (((low >> 32) & 0xFF) << 16) | (((low >> 56) & 0xFF) << 24);
    if long_mode {
        let Some(high) = high else {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        };
        // Figure 9-4 of Intel SDM Vol. 3A reserves descriptor bits 127:96 and
        // lower-descriptor L/D. AMD specifies #GP(selector) for nonzero
        // extended attributes in 64-bit mode.
        if high >> 32 != 0 || (low >> 53) & 0x3 != 0 {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        }
        base |= (high & 0xFFFF_FFFF) << 32;
        if !is_canonical_48(base) {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        }
    }

    if (low >> 47) & 1 == 0 {
        return Err(X86SystemDescriptorFault::SegmentNotPresent { error_code });
    }

    let raw_limit = ((low & 0xFFFF) | (((low >> 48) & 0x0F) << 16)) as u32;
    let g = (low >> 55) & 1 != 0;
    let limit = if g {
        (raw_limit << 12) | 0xFFF
    } else {
        raw_limit
    };
    Ok(Segment {
        base,
        limit,
        selector,
        type_,
        present: true,
        dpl: ((low >> 45) & 0x3) as u8,
        db: false,
        s: false,
        l: false,
        g,
        avl: (low >> 52) & 1 != 0,
        unusable: false,
    })
}

impl X86_64Vcpu {
    /// Validate and load the complete visible/hidden LDTR state. All descriptor
    /// bytes are read before commit. A null selector loads an unusable LDTR
    /// without consulting descriptor memory.
    pub(in crate::isa::x86_64) fn load_ldtr_selector(
        &mut self,
        selector: u16,
    ) -> std::result::Result<(), X86LdtrLoadFault> {
        if selector & 0xFFFC == 0 {
            self.sregs.ldt = Segment {
                selector,
                unusable: true,
                ..Segment::default()
            };
            return Ok(());
        }

        let error_code = selector_error_code(selector);
        if selector & 0x4 != 0 {
            return Err(X86LdtrLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let offset = u64::from(selector >> 3) * 8;
        let descriptor_size = if self.sregs.cs.l { 16_u64 } else { 8 };
        if offset + descriptor_size - 1 > u64::from(self.sregs.gdt.limit) {
            return Err(X86LdtrLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let address = self.sregs.gdt.base.wrapping_add(offset);
        let low = self
            .read_mem(address, 8)
            .map_err(X86LdtrLoadFault::Memory)?;
        let high = if self.sregs.cs.l {
            Some(
                self.read_mem(address.wrapping_add(8), 8)
                    .map_err(X86LdtrLoadFault::Memory)?,
            )
        } else {
            None
        };
        let segment = decode_x86_ldt_descriptor(selector, low, high, self.sregs.cs.l)
            .map_err(X86LdtrLoadFault::Architectural)?;
        self.sregs.ldt = segment;
        Ok(())
    }
}

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
    // Every Group-6 instruction is recognized only in protected mode and is
    // invalid in virtual-8086 mode. Reject before decoding or touching an
    // operand so #UD has priority over UMIP, privilege, and memory faults.
    if vcpu.sregs.cr0 & 1 == 0 || vcpu.regs.rflags & flags::bits::VM != 0 {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.any_rex_b();
    let is_memory = modrm >> 6 != 3;

    match reg_op {
        // SLDT - Store Local Descriptor Table (0x0F 0x00 /0)
        0 => {
            if umip_blocks_user_instruction(vcpu) {
                return raise_gp0(vcpu);
            }
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
            if umip_blocks_user_instruction(vcpu) {
                return raise_gp0(vcpu);
            }
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
                vcpu.read_mem(addr, 2)? as u16
            } else {
                vcpu.get_reg(rm, 2) as u16
            };
            match vcpu.load_ldtr_selector(selector) {
                Ok(()) => {}
                Err(X86LdtrLoadFault::Architectural(
                    X86SystemDescriptorFault::GeneralProtection { error_code },
                )) => {
                    vcpu.inject_exception(13, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86LdtrLoadFault::Architectural(
                    X86SystemDescriptorFault::SegmentNotPresent { error_code },
                )) => {
                    vcpu.inject_exception(11, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86LdtrLoadFault::Memory(error)) => return Err(error),
            }
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
