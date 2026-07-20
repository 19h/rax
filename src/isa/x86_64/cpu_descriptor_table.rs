//! Shared direct, verifier, and native-helper support for descriptor tables.

use super::{Result, X86_64Vcpu};

impl X86_64Vcpu {
    /// Read a complete descriptor-table pseudo-descriptor before exposing
    /// either field. Long mode reads 10 bytes (16-bit limit + 64-bit base).
    /// Legacy and compatibility modes read 6 bytes; a 16-bit operand retains
    /// only the low 24 base bits, while a 32-bit operand retains all 32.
    #[inline]
    pub(in crate::isa::x86_64) fn read_descriptor_table_mem(
        &mut self,
        addr: u64,
        operand_size: u8,
    ) -> Result<(u16, u64)> {
        let long_mode = self.sregs.cs.l;
        let len = if long_mode { 10 } else { 6 };
        let mut payload = [0u8; 10];
        self.mmu.read(addr, &mut payload[..len], &self.sregs)?;

        let limit = u16::from_le_bytes(payload[..2].try_into().unwrap());
        let (base, _traced_base, _traced_size) = if long_mode {
            let base = u64::from_le_bytes(payload[2..10].try_into().unwrap());
            (base, base, 8)
        } else {
            let raw = u32::from_le_bytes(payload[2..6].try_into().unwrap());
            let base = if operand_size == 2 {
                raw & 0x00FF_FFFF
            } else {
                raw
            };
            (u64::from(base), u64::from(raw), 4)
        };

        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            self.push_jit_mem_trace((0, addr, 2, u64::from(limit)));
            self.push_jit_mem_trace((0, addr.wrapping_add(2), _traced_size, _traced_base));
        }
        Ok((limit, base))
    }

    /// Store a descriptor-table register as one MMU transaction. In 64-bit
    /// mode the payload is the fixed 10-byte limit:base form; legacy and
    /// compatibility modes use the 6-byte limit:base form. The logical trace
    /// remains split into scalar widths supported by JIT verification.
    #[inline]
    pub(in crate::isa::x86_64) fn write_descriptor_table_mem(
        &mut self,
        addr: u64,
        limit: u16,
        base: u64,
    ) -> Result<()> {
        let mut payload = [0u8; 10];
        payload[..2].copy_from_slice(&limit.to_le_bytes());
        let (len, base_size, traced_base) = if self.sregs.cs.l {
            payload[2..].copy_from_slice(&base.to_le_bytes());
            (10, 8, base)
        } else {
            let base32 = base as u32;
            payload[2..6].copy_from_slice(&base32.to_le_bytes());
            (6, 4, u64::from(base32))
        };
        let result = self.mmu.write(addr, &payload[..len], &self.sregs);
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        if result.is_ok() {
            self.push_jit_mem_trace((1, addr, 2, u64::from(limit)));
            self.push_jit_mem_trace((1, addr.wrapping_add(2), base_size, traced_base));
        }
        result
    }
}

/// The implicit descriptor-table state that JIT verification must restore
/// before direct replay and compare before adopting the native result.
#[derive(Clone, Copy)]
pub(super) struct DescriptorTableSnapshot {
    gdtr_base: u64,
    gdtr_limit: u16,
    idtr_base: u64,
    idtr_limit: u16,
}

impl DescriptorTableSnapshot {
    pub(super) fn restore(self, vcpu: &mut X86_64Vcpu) {
        vcpu.sregs.gdt.base = self.gdtr_base;
        vcpu.sregs.gdt.limit = self.gdtr_limit;
        vcpu.sregs.idt.base = self.idtr_base;
        vcpu.sregs.idt.limit = self.idtr_limit;
    }

    pub(super) fn append_verify_diffs(self, vcpu: &X86_64Vcpu, diffs: &mut Vec<String>) {
        for (name, interp, native) in [
            ("gdtr_base", vcpu.sregs.gdt.base, self.gdtr_base),
            (
                "gdtr_limit",
                u64::from(vcpu.sregs.gdt.limit),
                u64::from(self.gdtr_limit),
            ),
            ("idtr_base", vcpu.sregs.idt.base, self.idtr_base),
            (
                "idtr_limit",
                u64::from(vcpu.sregs.idt.limit),
                u64::from(self.idtr_limit),
            ),
        ] {
            if interp != native {
                diffs.push(format!("{name}: interp={interp:#x} jit={native:#x}"));
            }
        }
    }
}

impl X86_64Vcpu {
    pub(super) fn descriptor_table_snapshot(&self) -> DescriptorTableSnapshot {
        DescriptorTableSnapshot {
            gdtr_base: self.sregs.gdt.base,
            gdtr_limit: self.sregs.gdt.limit,
            idtr_base: self.sregs.idt.base,
            idtr_limit: self.sregs.idt.limit,
        }
    }
}

/// JIT SGDT/SIDT helper. The complete 10-byte long-mode payload is submitted
/// to the MMU in one transaction. Verify-mode undo records retain the existing
/// scalar log ABI as an adjacent 2-byte limit and 8-byte base pair.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_descriptor_table_store(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    table: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if !vcpu.sregs.cs.l {
        return 0;
    }
    let Some(last) = addr.checked_add(9) else {
        return 0;
    };
    if vcpu.mmu.is_code_page(addr) || vcpu.mmu.is_code_page(last) {
        return 0;
    }
    let (limit, base) = match table {
        0 => (vcpu.sregs.gdt.limit, vcpu.sregs.gdt.base),
        1 => (vcpu.sregs.idt.limit, vcpu.sregs.idt.base),
        _ => return 0,
    };

    if vcpu.jit_mem_log.is_some() {
        match vcpu.read_bytes(addr, 10) {
            Ok(old) => {
                vcpu.push_jit_mem_log((
                    addr,
                    2,
                    u64::from(u16::from_le_bytes(old[..2].try_into().unwrap())),
                ));
                if vcpu.jit_mem_log.is_some() {
                    vcpu.push_jit_mem_log((
                        addr.wrapping_add(2),
                        8,
                        u64::from_le_bytes(old[2..].try_into().unwrap()),
                    ));
                }
            }
            Err(_) => vcpu.jit_mem_log = None,
        }
    }

    u64::from(vcpu.write_descriptor_table_mem(addr, limit, base).is_ok())
}

/// JIT LGDT/LIDT helper. The complete 10-byte long-mode pseudo-descriptor is
/// read before either GDTR/IDTR field is committed. Faults and malformed table
/// selectors leave both descriptor-table registers unchanged.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_descriptor_table_load(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    table: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if !vcpu.sregs.cs.l || addr.checked_add(9).is_none() || table > 1 {
        return 0;
    }
    let Ok((limit, base)) = vcpu.read_descriptor_table_mem(addr, 8) else {
        return 0;
    };
    match table {
        0 => {
            vcpu.sregs.gdt.limit = limit;
            vcpu.sregs.gdt.base = base;
        }
        1 => {
            vcpu.sregs.idt.limit = limit;
            vcpu.sregs.idt.base = base;
        }
        _ => unreachable!("descriptor-table selector validated above"),
    }
    1
}
