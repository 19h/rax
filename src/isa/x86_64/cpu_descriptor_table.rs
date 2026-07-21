//! Shared direct, verifier, and native-helper support for descriptor registers.

use super::{Result, X86_64Vcpu};
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::isa::x86_64::execute::system::{
    X86SegmentLoadTarget, decode_x86_ldt_descriptor, decode_x86_tss_descriptor, is_canonical_48,
};
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
/// before direct replay and compare before adopting the native result. CS, SS,
/// DS, ES, FS, GS, LDTR, and TR can change inside helper/interpreter callouts
/// even when the surrounding native region only exposes their visible
/// selectors.
#[derive(Clone)]
pub(super) struct DescriptorStateSnapshot {
    gdtr_base: u64,
    gdtr_limit: u16,
    idtr_base: u64,
    idtr_limit: u16,
    cs: Segment,
    ss: Segment,
    es: Segment,
    ds: Segment,
    fs: Segment,
    gs: Segment,
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
        vcpu.sregs.ss = self.ss.clone();
        vcpu.sregs.es = self.es.clone();
        vcpu.sregs.ds = self.ds.clone();
        vcpu.sregs.fs = self.fs.clone();
        vcpu.sregs.gs = self.gs.clone();
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
            ("ss", &vcpu.sregs.ss, &self.ss),
            ("es", &vcpu.sregs.es, &self.es),
            ("ds", &vcpu.sregs.ds, &self.ds),
            ("fs", &vcpu.sregs.fs, &self.fs),
            ("gs", &vcpu.sregs.gs, &self.gs),
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
            ss: self.sregs.ss.clone(),
            es: self.sregs.es.clone(),
            ds: self.sregs.ds.clone(),
            fs: self.sregs.fs.clone(),
            gs: self.sregs.gs.clone(),
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

/// Return one authoritative guest selector for native SLDT/STR, MOV r/m,Sreg,
/// or PUSH FS/GS lowering. Reading through the owning vCPU keeps a native
/// region coherent with prior interpreter callouts that changed selector state.
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
        2 => u64::from(vcpu.sregs.es.selector),
        3 => u64::from(vcpu.sregs.cs.selector),
        4 => u64::from(vcpu.sregs.ss.selector),
        5 => u64::from(vcpu.sregs.ds.selector),
        6 => u64::from(vcpu.sregs.fs.selector),
        7 => u64::from(vcpu.sregs.gs.selector),
        _ => 0,
    }
}

/// JIT LLDT/LTR, `MOV Sreg,r/m`, and `POP FS/GS` helper. Every architectural
/// guard, source read, descriptor read, and implicit descriptor write is
/// ordered before selector/cache commit. The native caller commits POP's RSP
/// increment only after success. Failure returns zero so native code replays
/// the direct instruction at its original guest PC.
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
    let memory_source = encoding & 1 != 0;
    let requires_apx = encoding & 0x2 != 0;
    let selector_id = (encoding >> 2) & 7;
    let memory64 = encoding & 0x20 != 0;
    let stack_source = encoding & 0x40 != 0;
    let ordinary_target = match selector_id {
        2 => Some(X86SegmentLoadTarget::Es),
        4 => Some(X86SegmentLoadTarget::Ss),
        5 => Some(X86SegmentLoadTarget::Ds),
        6 => Some(X86SegmentLoadTarget::Fs),
        7 => Some(X86SegmentLoadTarget::Gs),
        _ => None,
    };
    let system_selector = selector_id <= 1;
    if encoding & !0x7F != 0
        || !(system_selector || ordinary_target.is_some())
        || memory64 && (!memory_source || system_selector)
        || stack_source
            && (!memory_source
                || system_selector
                || !matches!(
                    ordinary_target,
                    Some(X86SegmentLoadTarget::Fs | X86SegmentLoadTarget::Gs)
                )
                || vcpu.sregs.efer & (1 << 10) == 0)
        || !vcpu.sregs.cs.l
        || vcpu.sregs.cr0 & 1 == 0
        || vcpu.regs.rflags & crate::isa::x86_64::flags::bits::VM != 0
        || system_selector && vcpu.sregs.cs.selector & 3 != 0
        || requires_apx && !vcpu.apx_enabled()
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
        let source_width: usize = if memory64 { 8 } else { 2 };
        let selector = if memory_source {
            if stack_source {
                let Some(last) = operand.checked_add(source_width as u64 - 1) else {
                    return false;
                };
                if !is_canonical_48(operand) || !is_canonical_48(last) {
                    return false;
                }
            }
            if !vcpu
                .mmu
                .read_range_is_plain_ram(operand, source_width, &vcpu.sregs)
            {
                return false;
            }
            let Ok(value) = vcpu.read_mem(operand, source_width as u8) else {
                return false;
            };
            value as u16
        } else {
            operand as u16
        };

        if let Some(target) = ordinary_target {
            return vcpu.load_segment_selector(target, selector, true).is_ok();
        }

        let load_tr = selector_id == 1;
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
        state.fs_base = vcpu.sregs.fs.base;
        state.gs_base = vcpu.sregs.gs.base;
        state.interrupt_inhibit = u64::from(vcpu.interrupt_inhibit);
        1
    } else {
        vcpu.jit_mem_trace = saved_trace;
        vcpu.jit_mem_log = saved_log;
        vcpu.mmu
            .restore_mem_record_checkpoint(mem_record_checkpoint);
        0
    }
}
