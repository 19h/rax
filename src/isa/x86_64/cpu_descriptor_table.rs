//! Shared direct, verifier, and native-helper support for descriptor registers.

use super::{Result, X86_64Vcpu};
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::isa::x86_64::execute::system::{decode_x86_ldt_descriptor, decode_x86_tss_descriptor};
use crate::vm::vcpu::Segment;

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

/// The implicit descriptor-register state that JIT verification must restore
/// before direct replay and compare before adopting the native result. CS,
/// LDTR, and TR can change inside helper/interpreter callouts even when the
/// surrounding native region only exposes their visible selectors.
#[derive(Clone)]
pub(super) struct DescriptorStateSnapshot {
    gdtr_base: u64,
    gdtr_limit: u16,
    idtr_base: u64,
    idtr_limit: u16,
    cs: Segment,
    ldtr: Segment,
    tr: Segment,
}

type SegmentFingerprint = (
    u64,
    u32,
    u16,
    u8,
    bool,
    u8,
    bool,
    bool,
    bool,
    bool,
    bool,
    bool,
);

fn segment_fingerprint(segment: &Segment) -> SegmentFingerprint {
    (
        segment.base,
        segment.limit,
        segment.selector,
        segment.type_,
        segment.present,
        segment.dpl,
        segment.db,
        segment.s,
        segment.l,
        segment.g,
        segment.avl,
        segment.unusable,
    )
}

impl DescriptorStateSnapshot {
    pub(super) fn restore(&self, vcpu: &mut X86_64Vcpu) {
        vcpu.sregs.gdt.base = self.gdtr_base;
        vcpu.sregs.gdt.limit = self.gdtr_limit;
        vcpu.sregs.idt.base = self.idtr_base;
        vcpu.sregs.idt.limit = self.idtr_limit;
        vcpu.sregs.cs = self.cs.clone();
        vcpu.sregs.ldt = self.ldtr.clone();
        vcpu.sregs.tr = self.tr.clone();
    }

    pub(super) fn append_verify_diffs(&self, vcpu: &X86_64Vcpu, diffs: &mut Vec<String>) {
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
        for (name, interp, native) in [
            ("cs", &vcpu.sregs.cs, &self.cs),
            ("ldtr", &vcpu.sregs.ldt, &self.ldtr),
            ("tr", &vcpu.sregs.tr, &self.tr),
        ] {
            let interp = segment_fingerprint(interp);
            let native = segment_fingerprint(native);
            if interp != native {
                diffs.push(format!("{name}: interp={interp:?} jit={native:?}"));
            }
        }
    }
}

impl X86_64Vcpu {
    pub(super) fn descriptor_state_snapshot(&self) -> DescriptorStateSnapshot {
        DescriptorStateSnapshot {
            gdtr_base: self.sregs.gdt.base,
            gdtr_limit: self.sregs.gdt.limit,
            idtr_base: self.sregs.idt.base,
            idtr_limit: self.sregs.idt.limit,
            cs: self.sregs.cs.clone(),
            ldtr: self.sregs.ldt.clone(),
            tr: self.sregs.tr.clone(),
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

/// Return the authoritative selector exposed by native SLDT/STR lowering.
/// Reading through the owning vCPU keeps a native region coherent with a prior
/// interpreter callout that executed LLDT or LTR.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_system_selector(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    selector: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_ref() }) else {
        return 0;
    };
    match selector {
        0 => u64::from(vcpu.sregs.ldt.selector),
        1 => u64::from(vcpu.sregs.tr.selector),
        _ => 0,
    }
}

/// JIT LLDT/LTR helper. Every architectural guard and the optional operand,
/// implicit GDT reads, and LTR busy-bit write are ordered before selector-state
/// commit. Failure returns zero so native code replays the direct instruction
/// at its original guest PC and delivers the precise architectural fault there.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_system_selector_load(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    operand: u64,
    encoding: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if encoding & !0x7 != 0
        || !vcpu.sregs.cs.l
        || vcpu.sregs.cr0 & 1 == 0
        || vcpu.regs.rflags & crate::isa::x86_64::flags::bits::VM != 0
        || vcpu.sregs.cs.selector & 3 != 0
        || encoding & 0x2 != 0 && !vcpu.apx_enabled()
    {
        return 0;
    }

    // A semantic descriptor failure deoptimizes and replays directly. Restrict
    // speculative accesses to ordinary RAM, then roll back buffered traces,
    // hooks, and verifier undo entries on failure so replay remains the sole
    // observable access. MMIO and translation faults deopt before data access.
    let saved_trace = vcpu.jit_mem_trace.clone();
    let saved_log = vcpu.jit_mem_log.clone();
    let mem_record_checkpoint = vcpu.mmu.mem_record_checkpoint();
    let loaded = (|| {
        let selector = if encoding & 1 != 0 {
            if !vcpu.mmu.read_range_is_plain_ram(operand, 2, &vcpu.sregs) {
                return false;
            }
            let Ok(value) = vcpu.read_mem(operand, 2) else {
                return false;
            };
            value as u16
        } else {
            operand as u16
        };

        let load_tr = encoding & 0x4 != 0;
        if selector & 0xFFFC == 0 {
            if load_tr {
                return false;
            }
            vcpu.sregs.ldt = Segment {
                selector,
                unusable: true,
                ..Segment::default()
            };
            return true;
        }
        if selector & 0x4 != 0 {
            return false;
        }

        let offset = u64::from(selector >> 3) * 8;
        if offset + 15 > u64::from(vcpu.sregs.gdt.limit) {
            return false;
        }
        let descriptor_addr = vcpu.sregs.gdt.base.wrapping_add(offset);
        if !vcpu
            .mmu
            .read_range_is_plain_ram(descriptor_addr, 16, &vcpu.sregs)
        {
            return false;
        }
        let Ok(low) = vcpu.read_mem(descriptor_addr, 8) else {
            return false;
        };
        let Ok(high) = vcpu.read_mem(descriptor_addr.wrapping_add(8), 8) else {
            return false;
        };

        if !load_tr {
            let Ok(segment) = decode_x86_ldt_descriptor(selector, low, Some(high), true) else {
                return false;
            };
            vcpu.sregs.ldt = segment;
            return true;
        }

        let Ok(descriptor) = decode_x86_tss_descriptor(selector, low, Some(high), true, true)
        else {
            return false;
        };
        let Some(descriptor_last) = descriptor_addr.checked_add(7) else {
            return false;
        };
        if vcpu.mmu.is_code_page(descriptor_addr)
            || vcpu.mmu.is_code_page(descriptor_last)
            || !vcpu
                .mmu
                .write_range_is_plain_ram(descriptor_addr, 8, &vcpu.sregs)
        {
            return false;
        }
        vcpu.commit_tr_descriptor(descriptor_addr, low, descriptor)
            .is_ok()
    })();
    if loaded {
        1
    } else {
        vcpu.jit_mem_trace = saved_trace;
        vcpu.jit_mem_log = saved_log;
        vcpu.mmu
            .restore_mem_record_checkpoint(mem_record_checkpoint);
        0
    }
}
