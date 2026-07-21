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

/// LLDT/LTR failure before any visible or hidden selector state commits.
/// Native lowering treats every variant as a precise deoptimization; direct
/// execution delivers the encoded architectural exception or propagates the
/// original memory fault.
#[derive(Debug)]
pub(in crate::isa::x86_64) enum X86SystemSelectorLoadFault {
    Architectural(X86SystemDescriptorFault),
    Memory(Error),
}

#[inline]
fn selector_error_code(selector: u16) -> u32 {
    u32::from(selector & 0xFFFC)
}

/// Ordinary segment register admitted by `MOV Sreg,r/m` (`8E /r`). CS has no
/// encoding in this direction and is deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86SegmentLoadTarget {
    Es,
    Ss,
    Ds,
    Fs,
    Gs,
}

/// A validated code/data descriptor and the descriptor qword after the
/// architecturally implicit accessed-bit transition.
#[derive(Debug)]
pub(crate) struct X86SegmentLoadDescriptor {
    pub(crate) segment: Segment,
    pub(crate) accessed_low: u64,
}

/// Direct/JIT failures before an ordinary segment-selector commit. Native-only
/// preflight failures are replayed by the direct engine so MMIO, translation,
/// and architectural faults are observed exactly once at the guest frontier.
#[derive(Debug)]
pub(in crate::isa::x86_64) enum X86SegmentSelectorLoadFault {
    Architectural(X86SystemDescriptorFault),
    StackSegment { error_code: u32 },
    Memory(Error),
    NativeDeopt,
}

#[inline]
fn segment_descriptor_base(raw: u64) -> u64 {
    ((raw >> 16) & 0xFFFF) | (((raw >> 32) & 0xFF) << 16) | (((raw >> 56) & 0xFF) << 24)
}

#[inline]
fn segment_descriptor_limit(raw: u64) -> u32 {
    let raw_limit = ((raw & 0xFFFF) | (((raw >> 48) & 0x0F) << 16)) as u32;
    if raw >> 55 & 1 != 0 {
        (raw_limit << 12) | 0xFFF
    } else {
        raw_limit
    }
}

/// Decode and validate one non-null data/stack selector. Type and privilege
/// checks precede presence, matching the architectural exception priority.
pub(crate) fn decode_x86_segment_load_descriptor(
    target: X86SegmentLoadTarget,
    selector: u16,
    raw: u64,
    cpl: u8,
) -> std::result::Result<X86SegmentLoadDescriptor, X86SystemDescriptorFault> {
    let error_code = selector_error_code(selector);
    if selector & 0xFFFC == 0 {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0 });
    }

    let type_ = ((raw >> 40) & 0x0F) as u8;
    let code_or_data = raw >> 44 & 1 != 0;
    let executable = type_ & 0x8 != 0;
    let readable_or_writable = type_ & 0x2 != 0;
    let conforming = executable && type_ & 0x4 != 0;
    let dpl = ((raw >> 45) & 3) as u8;
    let rpl = (selector & 3) as u8;

    let valid = if target == X86SegmentLoadTarget::Ss {
        code_or_data && !executable && readable_or_writable && rpl == cpl && dpl == cpl
    } else {
        let readable = !executable || readable_or_writable;
        let privilege_ok = conforming || (cpl <= dpl && rpl <= dpl);
        code_or_data && readable && privilege_ok
    };
    if !valid {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }
    if raw >> 47 & 1 == 0 {
        return Err(X86SystemDescriptorFault::SegmentNotPresent { error_code });
    }

    Ok(X86SegmentLoadDescriptor {
        segment: Segment {
            base: segment_descriptor_base(raw),
            limit: segment_descriptor_limit(raw),
            selector,
            type_: type_ | 1,
            present: true,
            dpl,
            db: raw >> 54 & 1 != 0,
            s: true,
            l: raw >> 53 & 1 != 0,
            g: raw >> 55 & 1 != 0,
            avl: raw >> 52 & 1 != 0,
            unusable: false,
        },
        accessed_low: raw | (1_u64 << 40),
    })
}

/// Real and virtual-8086 modes load selector-derived 64 KiB segments without
/// consulting a descriptor table. Virtual-8086 caches carry DPL 3.
pub(crate) fn x86_real_mode_segment(selector: u16, virtual_8086: bool) -> Segment {
    Segment {
        base: u64::from(selector) << 4,
        limit: 0xFFFF,
        selector,
        type_: 0x3,
        present: true,
        dpl: if virtual_8086 { 3 } else { 0 },
        db: false,
        s: true,
        l: false,
        g: false,
        avl: false,
        unusable: false,
    }
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

/// Fully decoded available TSS descriptor and the low descriptor qword after
/// its architecturally required available-to-busy transition.
pub(crate) struct X86TssDescriptor {
    pub(crate) segment: Segment,
    pub(crate) busy_low: u64,
}

/// Decode and validate one TSS descriptor after all architecturally selected
/// bytes have been read. IA-32e mode admits only the available 32/64-bit TSS
/// type (9); legacy protected mode additionally admits an available 16-bit TSS
/// (type 1). A successful decode reports the busy descriptor image that LTR
/// must commit to the GDT before exposing the new task-register state.
pub(crate) fn decode_x86_tss_descriptor(
    selector: u16,
    low: u64,
    high: Option<u64>,
    long_mode: bool,
    ia32e_active: bool,
) -> std::result::Result<X86TssDescriptor, X86SystemDescriptorFault> {
    let error_code = selector_error_code(selector);
    let type_ = ((low >> 40) & 0x0F) as u8;
    let system = (low >> 44) & 1 == 0;
    let available_tss = type_ == 0x9 || !ia32e_active && type_ == 0x1;
    if !system || !available_tss {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }

    let mut base =
        ((low >> 16) & 0xFFFF) | (((low >> 32) & 0xFF) << 16) | (((low >> 56) & 0xFF) << 24);
    if long_mode {
        let Some(high) = high else {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        };
        // Intel Figure 9-4 reserves descriptor bits 127:96 and the legacy L/D
        // attributes. AMD specifies #GP(selector) for nonzero extended
        // attributes in 64-bit mode.
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
    let busy_type = type_ | 0x2;
    Ok(X86TssDescriptor {
        segment: Segment {
            base,
            limit,
            selector,
            type_: busy_type,
            present: true,
            dpl: ((low >> 45) & 0x3) as u8,
            db: false,
            s: false,
            l: false,
            g,
            avl: (low >> 52) & 1 != 0,
            unusable: false,
        },
        busy_low: low | (1_u64 << 41),
    })
}

impl X86_64Vcpu {
    /// Commit the non-faulting portion of LTR after descriptor validation and
    /// any native-helper MMIO/permission preflight. The GDT update precedes TR
    /// exposure and supplies verifier undo data when native differential mode
    /// is active.
    pub(in crate::isa::x86_64) fn commit_tr_descriptor(
        &mut self,
        address: u64,
        old_low: u64,
        descriptor: X86TssDescriptor,
    ) -> Result<()> {
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        self.push_jit_mem_log((address, 8, old_low));
        self.write_mem(address, descriptor.busy_low, 8)?;
        self.sregs.tr = descriptor.segment;
        Ok(())
    }

    /// Validate and load the complete visible/hidden LDTR state. All descriptor
    /// bytes are read before commit. A null selector loads an unusable LDTR
    /// without consulting descriptor memory.
    pub(in crate::isa::x86_64) fn load_ldtr_selector(
        &mut self,
        selector: u16,
    ) -> std::result::Result<(), X86SystemSelectorLoadFault> {
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
            return Err(X86SystemSelectorLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let offset = u64::from(selector >> 3) * 8;
        let descriptor_size = if self.sregs.cs.l { 16_u64 } else { 8 };
        if offset + descriptor_size - 1 > u64::from(self.sregs.gdt.limit) {
            return Err(X86SystemSelectorLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let address = self.sregs.gdt.base.wrapping_add(offset);
        let low = self
            .read_mem(address, 8)
            .map_err(X86SystemSelectorLoadFault::Memory)?;
        let high = if self.sregs.cs.l {
            Some(
                self.read_mem(address.wrapping_add(8), 8)
                    .map_err(X86SystemSelectorLoadFault::Memory)?,
            )
        } else {
            None
        };
        let segment = decode_x86_ldt_descriptor(selector, low, high, self.sregs.cs.l)
            .map_err(X86SystemSelectorLoadFault::Architectural)?;
        self.sregs.ldt = segment;
        Ok(())
    }

    /// Validate an available TSS descriptor, mark it busy in the GDT, then
    /// load the complete visible/hidden task-register state. Descriptor reads
    /// and the busy write all precede TR commit, so any fault is noncommitting.
    pub(in crate::isa::x86_64) fn load_tr_selector(
        &mut self,
        selector: u16,
    ) -> std::result::Result<(), X86SystemSelectorLoadFault> {
        let error_code = selector_error_code(selector);
        if selector & 0xFFFC == 0 {
            return Err(X86SystemSelectorLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
            ));
        }
        if selector & 0x4 != 0 {
            return Err(X86SystemSelectorLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let offset = u64::from(selector >> 3) * 8;
        let long_mode = self.sregs.cs.l;
        let descriptor_size = if long_mode { 16_u64 } else { 8 };
        if offset + descriptor_size - 1 > u64::from(self.sregs.gdt.limit) {
            return Err(X86SystemSelectorLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code },
            ));
        }

        let address = self.sregs.gdt.base.wrapping_add(offset);
        let low = self
            .read_mem(address, 8)
            .map_err(X86SystemSelectorLoadFault::Memory)?;
        let high = if long_mode {
            Some(
                self.read_mem(address.wrapping_add(8), 8)
                    .map_err(X86SystemSelectorLoadFault::Memory)?,
            )
        } else {
            None
        };
        let descriptor = decode_x86_tss_descriptor(
            selector,
            low,
            high,
            long_mode,
            self.sregs.efer & (1 << 10) != 0,
        )
        .map_err(X86SystemSelectorLoadFault::Architectural)?;

        self.commit_tr_descriptor(address, low, descriptor)
            .map_err(X86SystemSelectorLoadFault::Memory)?;
        Ok(())
    }

    fn commit_segment_selector(&mut self, target: X86SegmentLoadTarget, segment: Segment) {
        match target {
            X86SegmentLoadTarget::Es => self.sregs.es = segment,
            X86SegmentLoadTarget::Ss => {
                self.sregs.ss = segment;
                // MOV SS and POP SS inhibit maskable interrupts and selected
                // debug traps through the boundary following the next
                // instruction.
                self.interrupt_inhibit = true;
            }
            X86SegmentLoadTarget::Ds => self.sregs.ds = segment,
            X86SegmentLoadTarget::Fs => self.sregs.fs = segment,
            X86SegmentLoadTarget::Gs => self.sregs.gs = segment,
        }
    }

    /// Validate and load ES/SS/DS/FS/GS for `MOV Sreg,r/m`, `POP Sreg`, or a
    /// far-pointer segment load (`LES/LDS/LSS/LFS/LGS`).
    /// Descriptor reads and the implicit accessed-bit store precede
    /// selector/cache exposure. `native_preflight` excludes accesses that
    /// cannot be speculated exactly once before direct replay.
    pub(in crate::isa::x86_64) fn load_segment_selector(
        &mut self,
        target: X86SegmentLoadTarget,
        selector: u16,
        native_preflight: bool,
    ) -> std::result::Result<(), X86SegmentSelectorLoadFault> {
        let virtual_8086 = self.regs.rflags & flags::bits::VM != 0;
        if self.sregs.cr0 & 1 == 0 || virtual_8086 {
            self.commit_segment_selector(target, x86_real_mode_segment(selector, virtual_8086));
            return Ok(());
        }

        let cpl = current_cpl(self);
        if selector & 0xFFFC == 0 {
            if target == X86SegmentLoadTarget::Ss
                && (!self.sregs.cs.l || cpl == 3 || (selector & 3) as u8 != cpl)
            {
                return Err(X86SegmentSelectorLoadFault::Architectural(
                    X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
                ));
            }
            self.commit_segment_selector(
                target,
                Segment {
                    selector,
                    dpl: cpl,
                    unusable: true,
                    ..Segment::default()
                },
            );
            return Ok(());
        }

        let descriptor_address = self
            .far_jump_descriptor_address(selector, 8)
            .map_err(X86SegmentSelectorLoadFault::Architectural)?;
        if native_preflight && !self.far_jump_plain_read(descriptor_address, 8, true) {
            return Err(X86SegmentSelectorLoadFault::NativeDeopt);
        }
        let low = self
            .read_far_jump_descriptor_qword(descriptor_address)
            .map_err(X86SegmentSelectorLoadFault::Memory)?;
        let descriptor = match decode_x86_segment_load_descriptor(target, selector, low, cpl) {
            Ok(descriptor) => descriptor,
            Err(X86SystemDescriptorFault::SegmentNotPresent { error_code })
                if target == X86SegmentLoadTarget::Ss =>
            {
                return Err(X86SegmentSelectorLoadFault::StackSegment { error_code });
            }
            Err(fault) => return Err(X86SegmentSelectorLoadFault::Architectural(fault)),
        };

        if low != descriptor.accessed_low {
            if native_preflight {
                let Some(last) = descriptor_address.checked_add(7) else {
                    return Err(X86SegmentSelectorLoadFault::NativeDeopt);
                };
                if self.mmu.is_code_page(descriptor_address)
                    || self.mmu.is_code_page(last)
                    || !self.far_jump_plain_write(descriptor_address, 8, true)
                {
                    return Err(X86SegmentSelectorLoadFault::NativeDeopt);
                }
            }
            // Verification restores logged stores through the guest access
            // path. A user-mode supervisor descriptor cannot be undone there;
            // replay directly and perform the transition exactly once.
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            if native_preflight && self.jit_mem_log_active() && cpl != 0 {
                return Err(X86SegmentSelectorLoadFault::NativeDeopt);
            }
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            self.push_jit_mem_log((descriptor_address, 8, low));
            self.write_far_jump_descriptor_qword(descriptor_address, descriptor.accessed_low)
                .map_err(X86SegmentSelectorLoadFault::Memory)?;
        }

        self.commit_segment_selector(target, descriptor.segment);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Descriptor {
    raw: u64,
}

/// Access class selected by VERR/VERW. This is deliberately distinct from a
/// segment-register load: Intel and AMD define verification in terms of type
/// and privilege only, so a clear descriptor Present bit does not make an
/// otherwise admissible selector fail verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86SelectorVerifyAccess {
    Read,
    Write,
}

/// Descriptor value selected by LAR/LSL after the non-faulting selector,
/// descriptor-type, and privilege checks have succeeded. Descriptor presence
/// is deliberately not a validity predicate for either instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86SelectorQueryAccess {
    AccessRights,
    Limit,
}

/// Direct/JIT failures while LAR/LSL read a selector descriptor. Null,
/// out-of-bounds, invalid-type, and invisible selectors are represented by a
/// successful `None`, because the architecture reports those cases with ZF=0.
#[derive(Debug)]
pub(in crate::isa::x86_64) enum X86SelectorQueryFault {
    Memory(Error),
    NativeDeopt,
}

/// Evaluate the descriptor/type/privilege portion of VERR/VERW after selector
/// nullness, table selection, bounds, and the descriptor read have succeeded.
/// Neither instruction raises a selector-derived protection exception.
pub(crate) fn x86_selector_verifies(
    selector: u16,
    raw: u64,
    cpl: u8,
    access: X86SelectorVerifyAccess,
) -> bool {
    let descriptor = Descriptor { raw };
    if !descriptor.is_code_or_data() || !descriptor.visible_from(selector, cpl) {
        return false;
    }

    match access {
        X86SelectorVerifyAccess::Read => !descriptor.executable() || descriptor.type_() & 0x2 != 0,
        X86SelectorVerifyAccess::Write => !descriptor.executable() && descriptor.type_() & 0x2 != 0,
    }
}

impl Descriptor {
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
}

fn selector_query_type_valid(
    descriptor: Descriptor,
    ia32e_active: bool,
    access: X86SelectorQueryAccess,
) -> bool {
    if descriptor.is_code_or_data() {
        return true;
    }

    match (ia32e_active, access) {
        (false, X86SelectorQueryAccess::AccessRights) => {
            matches!(
                descriptor.type_(),
                0x1 | 0x2 | 0x3 | 0x4 | 0x5 | 0x9 | 0xB | 0xC
            )
        }
        (false, X86SelectorQueryAccess::Limit) => {
            matches!(descriptor.type_(), 0x1 | 0x2 | 0x3 | 0x9 | 0xB)
        }
        (true, X86SelectorQueryAccess::AccessRights) => {
            matches!(descriptor.type_(), 0x2 | 0x9 | 0xB | 0xC)
        }
        (true, X86SelectorQueryAccess::Limit) => {
            matches!(descriptor.type_(), 0x2 | 0x9 | 0xB)
        }
    }
}

/// Whether an otherwise type-valid LAR/LSL system descriptor occupies the
/// 16-byte IA-32e format. Code/data descriptors always remain 8 bytes.
pub(crate) fn x86_selector_query_needs_high(
    raw: u64,
    ia32e_active: bool,
    access: X86SelectorQueryAccess,
) -> bool {
    let descriptor = Descriptor { raw };
    ia32e_active
        && !descriptor.is_code_or_data()
        && selector_query_type_valid(descriptor, ia32e_active, access)
}

/// Evaluate the type, IA-32e extended-descriptor, and privilege portion of
/// LAR/LSL after selector nullness, table selection, bounds, and descriptor
/// memory accesses have completed. Intel defines LAR result bits 19:16 as
/// undefined; this implementation retains the existing deterministic choice
/// of copying the corresponding descriptor bits.
pub(crate) fn x86_selector_query(
    selector: u16,
    raw: u64,
    high: Option<u64>,
    cpl: u8,
    ia32e_active: bool,
    access: X86SelectorQueryAccess,
) -> Option<u64> {
    let descriptor = Descriptor { raw };
    if !selector_query_type_valid(descriptor, ia32e_active, access)
        || !descriptor.visible_from(selector, cpl)
    {
        return None;
    }
    if x86_selector_query_needs_high(raw, ia32e_active, access)
        && high.is_none_or(|upper| (upper >> 40) & 0x1F != 0)
    {
        return None;
    }

    Some(match access {
        X86SelectorQueryAccess::AccessRights => descriptor.access_rights(),
        X86SelectorQueryAccess::Limit => descriptor.limit(),
    })
}

impl X86_64Vcpu {
    /// Read and evaluate one LAR/LSL descriptor without turning selector-
    /// derived failures into exceptions. Native preflight admits only plain
    /// RAM reads so direct replay can reproduce MMIO and translation effects
    /// exactly once at the original guest frontier.
    pub(in crate::isa::x86_64) fn query_selector_descriptor(
        &mut self,
        selector: u16,
        access: X86SelectorQueryAccess,
        native_preflight: bool,
    ) -> std::result::Result<Option<u64>, X86SelectorQueryFault> {
        if selector & 0xFFFC == 0 {
            return Ok(None);
        }

        let ti = selector & 0x4 != 0;
        if ti && (self.sregs.ldt.selector & 0xFFFC == 0 || self.sregs.ldt.unusable) {
            return Ok(None);
        }
        let (table_base, table_limit) = if ti {
            (self.sregs.ldt.base, u64::from(self.sregs.ldt.limit))
        } else {
            (self.sregs.gdt.base, u64::from(self.sregs.gdt.limit))
        };
        let offset = u64::from(selector >> 3) * 8;
        if offset.checked_add(7).is_none_or(|last| last > table_limit) {
            return Ok(None);
        }
        let Some(address) = table_base.checked_add(offset) else {
            return Ok(None);
        };
        let Some(last) = address.checked_add(7) else {
            return Ok(None);
        };
        let ia32e_active = self.sregs.efer & (1 << 10) != 0;
        if ia32e_active && (!is_canonical_48(address) || !is_canonical_48(last)) {
            return Ok(None);
        }
        if native_preflight && !self.far_jump_plain_read(address, 8, true) {
            return Err(X86SelectorQueryFault::NativeDeopt);
        }
        let raw = self
            .read_far_jump_descriptor_qword(address)
            .map_err(X86SelectorQueryFault::Memory)?;

        let high = if x86_selector_query_needs_high(raw, ia32e_active, access) {
            if offset
                .checked_add(15)
                .is_none_or(|last_offset| last_offset > table_limit)
            {
                return Ok(None);
            }
            let Some(high_address) = address.checked_add(8) else {
                return Ok(None);
            };
            let Some(high_last) = high_address.checked_add(7) else {
                return Ok(None);
            };
            if !is_canonical_48(high_address) || !is_canonical_48(high_last) {
                return Ok(None);
            }
            if native_preflight && !self.far_jump_plain_read(high_address, 8, true) {
                return Err(X86SelectorQueryFault::NativeDeopt);
            }
            Some(
                self.read_far_jump_descriptor_qword(high_address)
                    .map_err(X86SelectorQueryFault::Memory)?,
            )
        } else {
            None
        };

        Ok(x86_selector_query(
            selector,
            raw,
            high,
            current_cpl(self),
            ia32e_active,
            access,
        ))
    }
}

fn descriptor_for_verification(vcpu: &mut X86_64Vcpu, selector: u16) -> Result<Option<Descriptor>> {
    if selector & 0xFFFC == 0 {
        return Ok(None);
    }

    let ti = selector & 0x4 != 0;
    if ti && (vcpu.sregs.ldt.selector & 0xFFFC == 0 || vcpu.sregs.ldt.unusable) {
        return Ok(None);
    }
    let (table_base, table_limit) = if ti {
        (vcpu.sregs.ldt.base, u64::from(vcpu.sregs.ldt.limit))
    } else {
        (vcpu.sregs.gdt.base, u64::from(vcpu.sregs.gdt.limit))
    };
    let offset = u64::from(selector >> 3) * 8;
    if offset.checked_add(7).is_none_or(|last| last > table_limit) {
        return Ok(None);
    }
    let Some(address) = table_base.checked_add(offset) else {
        return Ok(None);
    };
    let raw = vcpu.mmu.read_u64_supervisor(address, &vcpu.sregs)?;
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    vcpu.push_jit_mem_trace((0, address, 8, raw));
    Ok(Some(Descriptor { raw }))
}

fn set_zf(vcpu: &mut X86_64Vcpu, set: bool) {
    vcpu.materialize_flags();
    if set {
        vcpu.regs.rflags |= flags::bits::ZF;
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF;
    }
}

fn read_fixed_selector_source(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    modrm_start: usize,
    modrm: u8,
    rm: u8,
) -> Result<Option<u16>> {
    if modrm >> 6 == 3 {
        return Ok(Some(vcpu.get_reg(rm, 2) as u16));
    }
    let (addr, extra, stack_segment) =
        vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;
    let canonical = addr.checked_add(1).is_some_and(|last| {
        vcpu.sregs.efer & (1 << 10) == 0 || is_canonical_48(addr) && is_canonical_48(last)
    });
    if !canonical {
        vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
        return Ok(None);
    }
    Ok(Some(vcpu.read_mem(addr, 2)? as u16))
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
            let Some(selector) = read_fixed_selector_source(vcpu, ctx, modrm_start, modrm, rm)?
            else {
                return Ok(None);
            };
            match vcpu.load_ldtr_selector(selector) {
                Ok(()) => {}
                Err(X86SystemSelectorLoadFault::Architectural(
                    X86SystemDescriptorFault::GeneralProtection { error_code },
                )) => {
                    vcpu.inject_exception(13, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86SystemSelectorLoadFault::Architectural(
                    X86SystemDescriptorFault::SegmentNotPresent { error_code },
                )) => {
                    vcpu.inject_exception(11, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86SystemSelectorLoadFault::Memory(error)) => return Err(error),
            }
        }
        // LTR - Load Task Register (0x0F 0x00 /3)
        3 => {
            // Privileged: loading the task register requires CPL 0.
            if !is_cpl0(vcpu) {
                return raise_gp0(vcpu);
            }
            let Some(selector) = read_fixed_selector_source(vcpu, ctx, modrm_start, modrm, rm)?
            else {
                return Ok(None);
            };
            match vcpu.load_tr_selector(selector) {
                Ok(()) => {}
                Err(X86SystemSelectorLoadFault::Architectural(
                    X86SystemDescriptorFault::GeneralProtection { error_code },
                )) => {
                    vcpu.inject_exception(13, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86SystemSelectorLoadFault::Architectural(
                    X86SystemDescriptorFault::SegmentNotPresent { error_code },
                )) => {
                    vcpu.inject_exception(11, Some(u64::from(error_code)))?;
                    return Ok(None);
                }
                Err(X86SystemSelectorLoadFault::Memory(error)) => return Err(error),
            }
        }
        // VERR - Verify Read (0x0F 0x00 /4)
        4 => {
            let Some(selector) = read_fixed_selector_source(vcpu, ctx, modrm_start, modrm, rm)?
            else {
                return Ok(None);
            };
            let cpl = current_cpl(vcpu);
            let readable = descriptor_for_verification(vcpu, selector)?
                .map(|desc| {
                    x86_selector_verifies(selector, desc.raw, cpl, X86SelectorVerifyAccess::Read)
                })
                .unwrap_or(false);
            set_zf(vcpu, readable);
        }
        // VERW - Verify Write (0x0F 0x00 /5)
        5 => {
            let Some(selector) = read_fixed_selector_source(vcpu, ctx, modrm_start, modrm, rm)?
            else {
                return Ok(None);
            };
            let cpl = current_cpl(vcpu);
            let writable = descriptor_for_verification(vcpu, selector)?
                .map(|desc| {
                    x86_selector_verifies(selector, desc.raw, cpl, X86SelectorVerifyAccess::Write)
                })
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

fn lar_lsl(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    access: X86SelectorQueryAccess,
) -> Result<Option<VcpuExit>> {
    // Both instructions are recognized only in protected mode and are invalid
    // in virtual-8086 mode. Reject before decoding or touching the source.
    if vcpu.sregs.cr0 & 1 == 0 || vcpu.regs.rflags & flags::bits::VM != 0 {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg = ((modrm >> 3) & 0x07) | ctx.any_rex_r();
    let rm = (modrm & 0x07) | ctx.any_rex_b();
    let Some(selector) = read_fixed_selector_source(vcpu, ctx, modrm_start, modrm, rm)? else {
        return Ok(None);
    };

    let value = match vcpu.query_selector_descriptor(selector, access, false) {
        Ok(value) => value,
        Err(X86SelectorQueryFault::Memory(error)) => return Err(error),
        Err(X86SelectorQueryFault::NativeDeopt) => {
            unreachable!("direct LAR/LSL descriptor query cannot request native replay")
        }
    };
    if let Some(value) = value {
        vcpu.set_reg(reg, value, ctx.op_size);
    }
    set_zf(vcpu, value.is_some());

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LAR - Load Access Rights (0x0F 0x02)
pub fn lar(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    lar_lsl(vcpu, ctx, X86SelectorQueryAccess::AccessRights)
}

/// LSL - Load Segment Limit (0x0F 0x03)
pub fn lsl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    lar_lsl(vcpu, ctx, X86SelectorQueryAccess::Limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(type_: u8, dpl: u8, present: bool, system: bool) -> u64 {
        0xBCDE_u64
            | (0x5000_u64 << 16)
            | (0x34_u64 << 32)
            | (u64::from(type_ & 0xF) << 40)
            | (u64::from(!system) << 44)
            | (u64::from(dpl & 3) << 45)
            | (u64::from(present) << 47)
            | (0xA_u64 << 48)
            | (1 << 52)
            | (1 << 54)
            | (1 << 55)
            | (0x12_u64 << 56)
    }

    #[test]
    fn ordinary_segment_descriptor_decode_preserves_cache_and_sets_accessed() {
        let raw = descriptor(0x2, 0, true, false);
        for target in [
            X86SegmentLoadTarget::Es,
            X86SegmentLoadTarget::Ss,
            X86SegmentLoadTarget::Ds,
            X86SegmentLoadTarget::Fs,
            X86SegmentLoadTarget::Gs,
        ] {
            let decoded = decode_x86_segment_load_descriptor(target, 0x10, raw, 0).unwrap();
            assert_eq!(decoded.segment.selector, 0x10);
            assert_eq!(decoded.segment.base, 0x1234_5000);
            assert_eq!(decoded.segment.limit, 0xA_BCDE_FFF);
            assert_eq!(decoded.segment.type_, 0x3);
            assert_eq!(decoded.segment.dpl, 0);
            assert!(decoded.segment.present);
            assert!(decoded.segment.db);
            assert!(decoded.segment.s);
            assert!(decoded.segment.g);
            assert!(decoded.segment.avl);
            assert!(!decoded.segment.unusable);
            assert_eq!(decoded.accessed_low, raw | (1 << 40));
        }
    }

    #[test]
    fn selector_verification_uses_type_and_privilege_but_ignores_presence() {
        for present in [false, true] {
            let read_only = descriptor(0x0, 3, present, false);
            let writable = descriptor(0x2, 3, present, false);
            let execute_only = descriptor(0x8, 3, present, false);
            let readable_code = descriptor(0xA, 3, present, false);
            let conforming_readable = descriptor(0xE, 0, present, false);
            let system = descriptor(0x2, 3, present, true);

            assert!(x86_selector_verifies(
                0x13,
                read_only,
                3,
                X86SelectorVerifyAccess::Read
            ));
            assert!(!x86_selector_verifies(
                0x13,
                read_only,
                3,
                X86SelectorVerifyAccess::Write
            ));
            assert!(x86_selector_verifies(
                0x13,
                writable,
                3,
                X86SelectorVerifyAccess::Write
            ));
            assert!(!x86_selector_verifies(
                0x13,
                execute_only,
                3,
                X86SelectorVerifyAccess::Read
            ));
            assert!(x86_selector_verifies(
                0x13,
                readable_code,
                3,
                X86SelectorVerifyAccess::Read
            ));
            assert!(!x86_selector_verifies(
                0x13,
                readable_code,
                3,
                X86SelectorVerifyAccess::Write
            ));
            assert!(x86_selector_verifies(
                0x13,
                conforming_readable,
                3,
                X86SelectorVerifyAccess::Read
            ));
            assert!(!x86_selector_verifies(
                0x13,
                system,
                3,
                X86SelectorVerifyAccess::Read
            ));
        }

        let dpl_two = descriptor(0x2, 2, true, false);
        assert!(x86_selector_verifies(
            0x12,
            dpl_two,
            2,
            X86SelectorVerifyAccess::Write
        ));
        assert!(!x86_selector_verifies(
            0x13,
            dpl_two,
            2,
            X86SelectorVerifyAccess::Write
        ));
        assert!(!x86_selector_verifies(
            0x12,
            dpl_two,
            3,
            X86SelectorVerifyAccess::Write
        ));
    }

    #[test]
    fn selector_query_type_matrices_privilege_and_presence_are_exact() {
        for present in [false, true] {
            let data = descriptor(0x2, 3, present, false);
            for access in [
                X86SelectorQueryAccess::AccessRights,
                X86SelectorQueryAccess::Limit,
            ] {
                assert!(x86_selector_query(0x13, data, None, 3, false, access).is_some());
                assert!(x86_selector_query(0x13, data, None, 3, true, access).is_some());
            }
        }

        for (access, legacy_valid, ia32e_valid) in [
            (
                X86SelectorQueryAccess::AccessRights,
                &[0x1, 0x2, 0x3, 0x4, 0x5, 0x9, 0xB, 0xC][..],
                &[0x2, 0x9, 0xB, 0xC][..],
            ),
            (
                X86SelectorQueryAccess::Limit,
                &[0x1, 0x2, 0x3, 0x9, 0xB][..],
                &[0x2, 0x9, 0xB][..],
            ),
        ] {
            for type_ in 0..=0xF {
                let raw = descriptor(type_, 3, false, true);
                assert_eq!(
                    x86_selector_query(0x13, raw, None, 3, false, access).is_some(),
                    legacy_valid.contains(&type_),
                    "legacy {access:?} type {type_:#x}"
                );
                let high = ia32e_valid.contains(&type_).then_some(0);
                assert_eq!(
                    x86_selector_query(0x13, raw, high, 3, true, access).is_some(),
                    ia32e_valid.contains(&type_),
                    "IA-32e {access:?} type {type_:#x}"
                );
            }
        }

        let dpl_two = descriptor(0x2, 2, false, false);
        assert!(
            x86_selector_query(0x12, dpl_two, None, 2, true, X86SelectorQueryAccess::Limit)
                .is_some()
        );
        assert!(
            x86_selector_query(0x13, dpl_two, None, 2, true, X86SelectorQueryAccess::Limit)
                .is_none()
        );
        let conforming = descriptor(0xC, 0, false, false);
        assert!(
            x86_selector_query(
                0x13,
                conforming,
                None,
                3,
                true,
                X86SelectorQueryAccess::AccessRights
            )
            .is_some()
        );
    }

    #[test]
    fn selector_query_extended_descriptor_and_values_are_exact() {
        let system = descriptor(0x2, 0, false, true);
        for access in [
            X86SelectorQueryAccess::AccessRights,
            X86SelectorQueryAccess::Limit,
        ] {
            assert!(x86_selector_query_needs_high(system, true, access));
            assert!(x86_selector_query(0x10, system, None, 0, true, access).is_none());
            assert!(x86_selector_query(0x10, system, Some(1 << 40), 0, true, access).is_none());
            assert!(x86_selector_query(0x10, system, Some(0), 0, true, access).is_some());
            assert!(!x86_selector_query_needs_high(system, false, access));
        }

        let data = descriptor(0x2, 0, false, false);
        assert!(!x86_selector_query_needs_high(
            data,
            true,
            X86SelectorQueryAccess::AccessRights
        ));
        assert_eq!(
            x86_selector_query(
                0x10,
                data,
                None,
                0,
                true,
                X86SelectorQueryAccess::AccessRights
            ),
            Some(((data >> 40) & 0xFFFF) << 8)
        );
        assert_eq!(
            x86_selector_query(0x10, data, None, 0, true, X86SelectorQueryAccess::Limit),
            Some(0xA_BCDE_FFF)
        );
    }

    #[test]
    fn ordinary_data_descriptor_type_privilege_and_presence_faults_are_exact() {
        for (name, raw, selector, cpl, expected) in [
            (
                "system",
                descriptor(0x2, 0, true, true),
                0x10,
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x10 },
            ),
            (
                "unreadable code",
                descriptor(0x8, 0, true, false),
                0x10,
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x10 },
            ),
            (
                "RPL",
                descriptor(0x2, 0, true, false),
                0x13,
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x10 },
            ),
            (
                "CPL",
                descriptor(0x2, 2, true, false),
                0x12,
                3,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x10 },
            ),
            (
                "not present",
                descriptor(0x2, 0, false, false),
                0x10,
                0,
                X86SystemDescriptorFault::SegmentNotPresent { error_code: 0x10 },
            ),
        ] {
            assert_eq!(
                decode_x86_segment_load_descriptor(X86SegmentLoadTarget::Ds, selector, raw, cpl,)
                    .expect_err(name),
                expected,
                "{name}"
            );
        }

        // Readable conforming code is loadable independently of DPL.
        assert!(
            decode_x86_segment_load_descriptor(
                X86SegmentLoadTarget::Ds,
                0x13,
                descriptor(0xE, 0, true, false),
                3,
            )
            .is_ok()
        );
    }

    #[test]
    fn stack_descriptor_requires_writable_data_and_exact_cpl_rpl_dpl() {
        for (raw, selector, cpl) in [
            (descriptor(0x0, 0, true, false), 0x10, 0),
            (descriptor(0x2, 1, true, false), 0x10, 0),
            (descriptor(0x2, 0, true, false), 0x11, 0),
            (descriptor(0xA, 0, true, false), 0x10, 0),
        ] {
            assert!(matches!(
                decode_x86_segment_load_descriptor(X86SegmentLoadTarget::Ss, selector, raw, cpl,),
                Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0x10 })
            ));
        }
        assert!(matches!(
            decode_x86_segment_load_descriptor(
                X86SegmentLoadTarget::Ss,
                0x10,
                descriptor(0x2, 0, false, false),
                0,
            ),
            Err(X86SystemDescriptorFault::SegmentNotPresent { error_code: 0x10 })
        ));
    }

    #[test]
    fn real_and_virtual_8086_segment_images_are_selector_derived() {
        for (virtual_8086, dpl) in [(false, 0), (true, 3)] {
            let segment = x86_real_mode_segment(0xF123, virtual_8086);
            assert_eq!(segment.base, 0xF_1230);
            assert_eq!(segment.limit, 0xFFFF);
            assert_eq!(segment.selector, 0xF123);
            assert_eq!(segment.type_, 0x3);
            assert_eq!(segment.dpl, dpl);
            assert!(segment.present && segment.s);
            assert!(!segment.db);
            assert!(!segment.l);
            assert!(!segment.g);
            assert!(!segment.unusable);
        }
    }
}
