//! Descriptor validation and state commit for far control transfers.

use crate::error::Error;
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::execute::system::X86SystemDescriptorFault;
use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::smir::ir::types::OpWidth;
use crate::vm::vcpu::Segment;

#[inline]
pub(super) fn selector_error_code(selector: u16) -> u32 {
    u32::from(selector & 0xFFFC)
}

/// A validated code-segment target. `accessed_low` is the descriptor image
/// after the architecturally implicit accessed-bit transition; callers must
/// commit that write before exposing either CS or RIP.
pub(crate) struct X86FarJumpTarget {
    pub(crate) segment: Segment,
    pub(crate) offset: u64,
    pub(crate) accessed_low: u64,
}

/// The target selector and 64-bit offset carried by an IA-32e call gate.
pub(crate) struct X86FarJumpCallGate {
    pub(crate) selector: u16,
    pub(crate) offset: u64,
}

/// Result of decoding the descriptor selected by the memory far pointer.
pub(crate) enum X86FarJumpDescriptor {
    Code(X86FarJumpTarget),
    CallGate(X86FarJumpCallGate),
}

/// Direct/JIT execution failures before a far-JMP state commit. Native-only
/// preflight failures deliberately carry no architectural classification: the
/// JIT restores its speculative bookkeeping and replays the instruction in the
/// direct interpreter, which delivers the exact exception or MMIO behavior.
pub(in crate::isa::x86_64) enum X86FarJumpLoadFault {
    Architectural(X86SystemDescriptorFault),
    StackSegment,
    Memory(Error),
    NativeDeopt,
}

#[inline]
fn normalized_far_offset(offset: u64, width: OpWidth) -> Option<u64> {
    Some(match width {
        OpWidth::W16 => offset & 0xFFFF,
        OpWidth::W32 => offset & 0xFFFF_FFFF,
        OpWidth::W64 => offset,
        OpWidth::W8 | OpWidth::W128 => return None,
    })
}

#[inline]
fn descriptor_limit(raw: u64) -> u32 {
    let raw_limit = ((raw & 0xFFFF) | (((raw >> 48) & 0x0F) << 16)) as u32;
    if raw >> 55 & 1 != 0 {
        (raw_limit << 12) | 0xFFF
    } else {
        raw_limit
    }
}

#[inline]
fn descriptor_base(raw: u64) -> u64 {
    ((raw >> 16) & 0xFFFF) | (((raw >> 32) & 0xFF) << 16) | (((raw >> 56) & 0xFF) << 24)
}

/// Decode a code descriptor reached directly or through a call gate. The
/// privilege predicates differ only in whether the target selector's RPL is
/// consulted; a gate-provided selector is not the caller-supplied selector.
fn decode_x86_far_jump_code(
    selector: u16,
    raw: u64,
    offset: u64,
    offset_width: OpWidth,
    cpl: u8,
    ia32e_active: bool,
    through_call_gate: bool,
    call_gate_may_lower_privilege: bool,
    validate_target_offset: bool,
) -> Result<X86FarJumpTarget, X86SystemDescriptorFault> {
    let error_code = selector_error_code(selector);
    let type_ = ((raw >> 40) & 0x0F) as u8;
    let executable = raw >> 44 & 1 != 0 && type_ & 0x8 != 0;
    if !executable {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }

    let l = raw >> 53 & 1 != 0;
    let db = raw >> 54 & 1 != 0;
    if (ia32e_active && l && db) || (through_call_gate && ia32e_active && !l) {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }

    let dpl = ((raw >> 45) & 3) as u8;
    let conforming = type_ & 0x4 != 0;
    let rpl = (selector & 3) as u8;
    let privilege_invalid = if conforming {
        dpl > cpl
    } else if through_call_gate && call_gate_may_lower_privilege {
        dpl > cpl
    } else {
        dpl != cpl || !through_call_gate && rpl > cpl
    };
    if privilege_invalid {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }
    if raw >> 47 & 1 == 0 {
        return Err(X86SystemDescriptorFault::SegmentNotPresent { error_code });
    }

    let Some(mut offset) = normalized_far_offset(offset, offset_width) else {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    };
    // An IA-32e transfer into a compatibility code segment loads EIP, even
    // when a REX.W far pointer supplied 64 offset bits. Operand-size 16 then
    // applies the narrower IP mask as usual.
    if ia32e_active && !l {
        offset &= 0xFFFF_FFFF;
    }
    let target = X86FarJumpTarget {
        segment: Segment {
            base: if ia32e_active && l {
                0
            } else {
                descriptor_base(raw)
            },
            limit: descriptor_limit(raw),
            selector: (selector & !3)
                | u16::from(
                    if through_call_gate && call_gate_may_lower_privilege && !conforming {
                        dpl
                    } else {
                        cpl
                    },
                ),
            type_: type_ | 1,
            present: true,
            dpl,
            db,
            s: true,
            l,
            g: raw >> 55 & 1 != 0,
            avl: raw >> 52 & 1 != 0,
            unusable: false,
        },
        offset,
        accessed_low: raw | (1_u64 << 40),
    };
    if validate_target_offset {
        validate_x86_far_call_target_offset(&target)?;
    }
    Ok(target)
}

/// Validate the target offset after a far CALL has checked the complete return
/// frame. Intel gives stack/TSS faults priority over a target-limit or
/// canonicality fault, whereas far JMP validates the same target immediately.
pub(crate) fn validate_x86_far_call_target_offset(
    target: &X86FarJumpTarget,
) -> Result<(), X86SystemDescriptorFault> {
    if (!target.segment.l && target.offset > u64::from(target.segment.limit))
        || (target.segment.l && !is_canonical_48(target.offset))
    {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0 });
    }
    Ok(())
}

/// Whether a selected descriptor requires the upper qword of an IA-32e
/// call-gate descriptor before it can be validated.
pub(crate) fn x86_far_jump_is_ia32e_call_gate(raw: u64, ia32e_active: bool) -> bool {
    ia32e_active && raw >> 44 & 1 == 0 && (raw >> 40) & 0x0F == 0x0C
}

/// Decode the descriptor selected by the memory far pointer. IA-32e admits a
/// conforming/nonconforming code segment or a 16-byte 64-bit call gate. The
/// offset embedded in a far pointer is ignored for the call-gate case.
fn decode_x86_far_control_descriptor(
    selector: u16,
    low: u64,
    high: Option<u64>,
    pointer_offset: u64,
    offset_width: OpWidth,
    cpl: u8,
    ia32e_active: bool,
    validate_target_offset: bool,
) -> Result<X86FarJumpDescriptor, X86SystemDescriptorFault> {
    let error_code = selector_error_code(selector);
    if selector & 0xFFFC == 0 {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0 });
    }

    if low >> 44 & 1 != 0 {
        return decode_x86_far_jump_code(
            selector,
            low,
            pointer_offset,
            offset_width,
            cpl,
            ia32e_active,
            false,
            false,
            validate_target_offset,
        )
        .map(X86FarJumpDescriptor::Code);
    }

    if !x86_far_jump_is_ia32e_call_gate(low, ia32e_active) {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }
    let Some(high) = high else {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    };
    // Descriptor bits 108:104 are the IA-32e upper type-consistency field and
    // must be zero. Other reserved fields have no specified JMP exception and
    // therefore are not promoted into an invented architectural fault here.
    if (high >> 40) & 0x1F != 0 {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }

    let gate_dpl = ((low >> 45) & 3) as u8;
    let gate_rpl = (selector & 3) as u8;
    if gate_dpl < cpl || gate_dpl < gate_rpl {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
    }
    if low >> 47 & 1 == 0 {
        return Err(X86SystemDescriptorFault::SegmentNotPresent { error_code });
    }

    let target_selector = ((low >> 16) & 0xFFFF) as u16;
    if target_selector & 0xFFFC == 0 {
        return Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0 });
    }
    let target_offset =
        (low & 0xFFFF) | (((low >> 48) & 0xFFFF) << 16) | ((high & 0xFFFF_FFFF) << 32);
    Ok(X86FarJumpDescriptor::CallGate(X86FarJumpCallGate {
        selector: target_selector,
        offset: target_offset,
    }))
}

/// Decode the selector chosen by a far JMP, including immediate target-offset
/// validation because the operation has no stack fault with higher priority.
pub(crate) fn decode_x86_far_jump_descriptor(
    selector: u16,
    low: u64,
    high: Option<u64>,
    pointer_offset: u64,
    offset_width: OpWidth,
    cpl: u8,
    ia32e_active: bool,
) -> Result<X86FarJumpDescriptor, X86SystemDescriptorFault> {
    decode_x86_far_control_descriptor(
        selector,
        low,
        high,
        pointer_offset,
        offset_width,
        cpl,
        ia32e_active,
        true,
    )
}

/// Decode the selector chosen by a far CALL while deferring target-offset
/// validation until after the return-frame/TSS checks required by the
/// architectural exception-priority order.
pub(crate) fn decode_x86_far_call_descriptor(
    selector: u16,
    low: u64,
    high: Option<u64>,
    pointer_offset: u64,
    offset_width: OpWidth,
    cpl: u8,
    ia32e_active: bool,
) -> Result<X86FarJumpDescriptor, X86SystemDescriptorFault> {
    decode_x86_far_control_descriptor(
        selector,
        low,
        high,
        pointer_offset,
        offset_width,
        cpl,
        ia32e_active,
        false,
    )
}

/// Validate the code descriptor selected by an already-validated IA-32e call
/// gate. IA-32e gates may target only a 64-bit code segment (L=1, D=0).
pub(crate) fn decode_x86_far_jump_call_gate_target(
    selector: u16,
    raw: u64,
    offset: u64,
    cpl: u8,
) -> Result<X86FarJumpTarget, X86SystemDescriptorFault> {
    decode_x86_far_jump_code(
        selector,
        raw,
        offset,
        OpWidth::W64,
        cpl,
        true,
        true,
        false,
        true,
    )
}

/// Validate the code segment selected by an IA-32e far-CALL gate. A
/// nonconforming target may lower CPL to its DPL; a conforming target retains
/// the caller's CPL. The returned segment selector already carries that final
/// CPL in its RPL bits.
pub(crate) fn decode_x86_far_call_gate_target(
    selector: u16,
    raw: u64,
    offset: u64,
    cpl: u8,
) -> Result<X86FarJumpTarget, X86SystemDescriptorFault> {
    decode_x86_far_jump_code(
        selector,
        raw,
        offset,
        OpWidth::W64,
        cpl,
        true,
        true,
        true,
        false,
    )
}

impl X86_64Vcpu {
    pub(super) fn far_jump_descriptor_address(
        &self,
        selector: u16,
        size: u64,
    ) -> Result<u64, X86SystemDescriptorFault> {
        let error_code = selector_error_code(selector);
        if selector & 0xFFFC == 0 {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code: 0 });
        }

        let ti = selector & 4 != 0;
        if ti && (self.sregs.ldt.selector & 0xFFFC == 0 || self.sregs.ldt.unusable) {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        }
        let (base, limit) = if ti {
            (self.sregs.ldt.base, u64::from(self.sregs.ldt.limit))
        } else {
            (self.sregs.gdt.base, u64::from(self.sregs.gdt.limit))
        };
        let offset = u64::from(selector >> 3) * 8;
        let Some(last_offset) = offset.checked_add(size - 1) else {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        };
        if last_offset > limit {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        }
        let Some(address) = base.checked_add(offset) else {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        };
        let Some(last) = address.checked_add(size - 1) else {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        };
        if !is_canonical_48(address) || !is_canonical_48(last) {
            return Err(X86SystemDescriptorFault::GeneralProtection { error_code });
        }
        Ok(address)
    }

    fn far_jump_plain_read(&mut self, address: u64, size: usize, supervisor: bool) -> bool {
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            let mut sregs = self.sregs.clone();
            if supervisor {
                sregs.cs.selector &= !3;
            }
            return self.mmu.read_range_is_plain_ram(address, size, &sregs);
        }
        #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
        {
            let _ = (address, size, supervisor);
            false
        }
    }

    fn far_jump_plain_write(&mut self, address: u64, size: usize, supervisor: bool) -> bool {
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            let mut sregs = self.sregs.clone();
            if supervisor {
                sregs.cs.selector &= !3;
            }
            return self.mmu.write_range_is_plain_ram(address, size, &sregs);
        }
        #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
        {
            let _ = (address, size, supervisor);
            false
        }
    }

    pub(super) fn read_far_jump_descriptor_qword(&mut self, address: u64) -> Result<u64, Error> {
        let value = self.mmu.read_u64_supervisor(address, &self.sregs)?;
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        self.push_jit_mem_trace((0, address, 8, value));
        Ok(value)
    }

    pub(super) fn write_far_jump_descriptor_qword(
        &mut self,
        address: u64,
        value: u64,
    ) -> Result<(), Error> {
        self.mmu.write_u64_supervisor(address, value, &self.sregs)?;
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        self.push_jit_mem_trace((1, address, 8, value));
        Ok(())
    }

    fn read_far_jump_descriptor(
        &mut self,
        selector: u16,
        size: u64,
        native_preflight: bool,
    ) -> Result<(u64, Option<u64>, u64), X86FarJumpLoadFault> {
        let address = self
            .far_jump_descriptor_address(selector, size)
            .map_err(X86FarJumpLoadFault::Architectural)?;
        if native_preflight && !self.far_jump_plain_read(address, size as usize, true) {
            return Err(X86FarJumpLoadFault::NativeDeopt);
        }
        let low = self
            .read_far_jump_descriptor_qword(address)
            .map_err(X86FarJumpLoadFault::Memory)?;
        let high = if size == 16 {
            Some(
                self.read_far_jump_descriptor_qword(address.wrapping_add(8))
                    .map_err(X86FarJumpLoadFault::Memory)?,
            )
        } else {
            None
        };
        Ok((low, high, address))
    }

    fn commit_far_jump_target(
        &mut self,
        descriptor_address: u64,
        old_low: u64,
        target: X86FarJumpTarget,
        native_preflight: bool,
    ) -> Result<(), X86FarJumpLoadFault> {
        if old_low != target.accessed_low {
            if native_preflight {
                let Some(last) = descriptor_address.checked_add(7) else {
                    return Err(X86FarJumpLoadFault::NativeDeopt);
                };
                if self.mmu.is_code_page(descriptor_address)
                    || self.mmu.is_code_page(last)
                    || !self.far_jump_plain_write(descriptor_address, 8, true)
                {
                    return Err(X86FarJumpLoadFault::NativeDeopt);
                }
            }
            // Verification currently restores logged stores through the guest
            // access path. At CPL3 a supervisor-only descriptor would therefore
            // be impossible to undo safely; fail closed and let direct replay
            // perform the implicit accessed-bit transition once.
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            if native_preflight && self.jit_mem_log_active() && self.sregs.cs.selector & 3 != 0 {
                return Err(X86FarJumpLoadFault::NativeDeopt);
            }
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            self.push_jit_mem_log((descriptor_address, 8, old_low));
            self.write_far_jump_descriptor_qword(descriptor_address, target.accessed_low)
                .map_err(X86FarJumpLoadFault::Memory)?;
        }

        self.sregs.cs = target.segment;
        self.regs.rip = target.offset;
        Ok(())
    }

    /// Execute the IA-32e 64-bit-code-segment form of `FF /5`. Pointer,
    /// descriptor, call-gate, and implicit accessed-bit memory effects all
    /// precede the atomic CS:RIP commit. `stack_segment` selects #SS(0) for a
    /// noncanonical SS-based pointer range. `native_preflight` excludes MMIO,
    /// translation faults, and code-page descriptor writes before a helper can
    /// create an observable access that direct replay would duplicate.
    pub(in crate::isa::x86_64) fn jump_far_long_mode(
        &mut self,
        pointer_address: u64,
        offset_width: OpWidth,
        stack_segment: bool,
        native_preflight: bool,
    ) -> Result<(), X86FarJumpLoadFault> {
        if !self.sregs.cs.l || self.sregs.efer & (1 << 10) == 0 {
            return Err(X86FarJumpLoadFault::NativeDeopt);
        }
        let pointer_size: usize = match offset_width {
            OpWidth::W16 => 4,
            OpWidth::W32 => 6,
            OpWidth::W64 => 10,
            OpWidth::W8 | OpWidth::W128 => return Err(X86FarJumpLoadFault::NativeDeopt),
        };
        let canonical_range = pointer_address
            .checked_add(pointer_size as u64 - 1)
            .is_some_and(|last| is_canonical_48(pointer_address) && is_canonical_48(last));
        if !canonical_range {
            return Err(if stack_segment {
                X86FarJumpLoadFault::StackSegment
            } else {
                X86FarJumpLoadFault::Architectural(X86SystemDescriptorFault::GeneralProtection {
                    error_code: 0,
                })
            });
        }
        if native_preflight && !self.far_jump_plain_read(pointer_address, pointer_size, false) {
            return Err(X86FarJumpLoadFault::NativeDeopt);
        }
        let offset_size = offset_width.bits() as u8 / 8;
        let pointer_offset = self
            .read_mem(pointer_address, offset_size)
            .map_err(X86FarJumpLoadFault::Memory)?;
        let selector = self
            .read_mem(pointer_address.wrapping_add(u64::from(offset_size)), 2)
            .map_err(X86FarJumpLoadFault::Memory)? as u16;
        if selector & 0xFFFC == 0 {
            return Err(X86FarJumpLoadFault::Architectural(
                X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
            ));
        }

        let cpl = (self.sregs.cs.selector & 3) as u8;
        let (mut low, _, mut descriptor_address) =
            self.read_far_jump_descriptor(selector, 8, native_preflight)?;
        let high = if x86_far_jump_is_ia32e_call_gate(low, true) {
            descriptor_address = self
                .far_jump_descriptor_address(selector, 16)
                .map_err(X86FarJumpLoadFault::Architectural)?;
            if native_preflight && !self.far_jump_plain_read(descriptor_address, 16, true) {
                return Err(X86FarJumpLoadFault::NativeDeopt);
            }
            Some(
                self.read_far_jump_descriptor_qword(descriptor_address.wrapping_add(8))
                    .map_err(X86FarJumpLoadFault::Memory)?,
            )
        } else {
            None
        };
        let descriptor = decode_x86_far_jump_descriptor(
            selector,
            low,
            high,
            pointer_offset,
            offset_width,
            cpl,
            true,
        )
        .map_err(X86FarJumpLoadFault::Architectural)?;
        let target = match descriptor {
            X86FarJumpDescriptor::Code(target) => target,
            X86FarJumpDescriptor::CallGate(gate) => {
                let (target_low, _, target_address) =
                    self.read_far_jump_descriptor(gate.selector, 8, native_preflight)?;
                descriptor_address = target_address;
                low = target_low;
                decode_x86_far_jump_call_gate_target(gate.selector, low, gate.offset, cpl)
                    .map_err(X86FarJumpLoadFault::Architectural)?
            }
        };

        self.commit_far_jump_target(descriptor_address, low, target, native_preflight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_descriptor(
        dpl: u8,
        present: bool,
        conforming: bool,
        l: bool,
        db: bool,
        limit: u32,
    ) -> u64 {
        assert!(limit <= 0xF_FFFF);
        u64::from(limit & 0xFFFF)
            | ((0x8 | u64::from(conforming) << 2) << 40)
            | (1 << 44)
            | (u64::from(dpl & 3) << 45)
            | (u64::from(present) << 47)
            | (u64::from((limit >> 16) & 0xF) << 48)
            | (u64::from(l) << 53)
            | (u64::from(db) << 54)
    }

    fn call_gate(target_selector: u16, target_offset: u64, dpl: u8, present: bool) -> (u64, u64) {
        let low = (target_offset & 0xFFFF)
            | (u64::from(target_selector) << 16)
            | (0xC << 40)
            | (u64::from(dpl & 3) << 45)
            | (u64::from(present) << 47)
            | (((target_offset >> 16) & 0xFFFF) << 48);
        let high = (target_offset >> 32) & 0xFFFF_FFFF;
        (low, high)
    }

    #[test]
    fn direct_code_descriptor_enforces_privilege_presence_mode_and_offset() {
        let valid = code_descriptor(3, true, false, true, false, 0);
        let X86FarJumpDescriptor::Code(target) = decode_x86_far_jump_descriptor(
            0x1B,
            valid,
            None,
            0x1234_5678_9ABC,
            OpWidth::W64,
            3,
            true,
        )
        .expect("valid same-CPL 64-bit code target") else {
            panic!("code descriptor decoded as a gate")
        };
        assert_eq!(target.segment.selector, 0x1B);
        assert!(target.segment.l);
        assert!(!target.segment.db);
        assert_eq!(target.segment.type_ & 1, 1);
        assert_eq!(target.offset, 0x1234_5678_9ABC);
        assert_eq!(target.accessed_low, valid | (1 << 40));

        for (name, selector, raw, offset, expected) in [
            (
                "null",
                3,
                valid,
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
            ),
            (
                "RPL",
                0x1B,
                code_descriptor(0, true, false, true, false, 0),
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x18 },
            ),
            (
                "not present",
                0x1B,
                code_descriptor(3, false, false, true, false, 0),
                0,
                X86SystemDescriptorFault::SegmentNotPresent { error_code: 0x18 },
            ),
            (
                "L and D",
                0x1B,
                code_descriptor(3, true, false, true, true, 0),
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x18 },
            ),
            (
                "noncanonical target",
                0x1B,
                valid,
                0x0000_8000_0000_0000,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
            ),
        ] {
            let actual =
                decode_x86_far_jump_descriptor(selector, raw, None, offset, OpWidth::W64, 3, true)
                    .err()
                    .unwrap_or_else(|| panic!("{name} unexpectedly decoded"));
            assert_eq!(actual, expected, "{name}");
        }

        let conforming = code_descriptor(1, true, true, true, false, 0);
        assert!(
            decode_x86_far_jump_descriptor(0x18, conforming, None, 0x55, OpWidth::W64, 3, true,)
                .is_ok()
        );
        let compat = code_descriptor(3, true, false, false, true, 0x1234);
        assert_eq!(
            decode_x86_far_jump_descriptor(0x1B, compat, None, 0x1235, OpWidth::W32, 3, true,)
                .err(),
            Some(X86SystemDescriptorFault::GeneralProtection { error_code: 0 })
        );
    }

    #[test]
    fn ia32e_call_gate_enforces_reserved_privilege_presence_and_target_contracts() {
        let offset = 0xFFFF_8000_1234_5678;
        let (low, high) = call_gate(0x28, offset, 3, true);
        let X86FarJumpDescriptor::CallGate(gate) =
            decode_x86_far_jump_descriptor(0x33, low, Some(high), u64::MAX, OpWidth::W16, 3, true)
                .expect("valid 64-bit call gate")
        else {
            panic!("call gate decoded as code")
        };
        assert_eq!(gate.selector, 0x28);
        assert_eq!(gate.offset, offset);

        for (name, selected, gate_low, gate_high, cpl, expected) in [
            (
                "gate DPL",
                0x30,
                call_gate(0x28, offset, 2, true).0,
                high,
                3,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x30 },
            ),
            (
                "gate RPL",
                0x33,
                call_gate(0x28, offset, 2, true).0,
                high,
                0,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x30 },
            ),
            (
                "gate absent",
                0x33,
                call_gate(0x28, offset, 3, false).0,
                high,
                3,
                X86SystemDescriptorFault::SegmentNotPresent { error_code: 0x30 },
            ),
            (
                "upper type",
                0x33,
                low,
                high | (1 << 40),
                3,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0x30 },
            ),
            (
                "null target",
                0x33,
                call_gate(3, offset, 3, true).0,
                high,
                3,
                X86SystemDescriptorFault::GeneralProtection { error_code: 0 },
            ),
        ] {
            assert_eq!(
                decode_x86_far_jump_descriptor(
                    selected,
                    gate_low,
                    Some(gate_high),
                    0,
                    OpWidth::W64,
                    cpl,
                    true,
                )
                .err(),
                Some(expected),
                "{name}"
            );
        }

        let target = decode_x86_far_jump_call_gate_target(
            gate.selector,
            code_descriptor(3, true, false, true, false, 0),
            gate.offset,
            3,
        )
        .expect("valid gate target");
        assert_eq!(target.offset, offset);
        assert_eq!(target.segment.selector, 0x2B);
        assert_eq!(
            decode_x86_far_jump_call_gate_target(
                gate.selector,
                code_descriptor(3, true, false, false, true, 0xF_FFFF),
                gate.offset,
                3,
            )
            .err(),
            Some(X86SystemDescriptorFault::GeneralProtection { error_code: 0x28 })
        );
    }

    #[test]
    fn far_call_gate_target_allows_only_architectural_privilege_transitions() {
        let offset = 0xFFFF_8000_1234_5678;
        let ring0 = code_descriptor(0, true, false, true, false, 0);
        assert_eq!(
            decode_x86_far_jump_call_gate_target(0x30, ring0, offset, 3).err(),
            Some(X86SystemDescriptorFault::GeneralProtection { error_code: 0x30 }),
            "far JMP must not use a gate to lower CPL"
        );
        let call_target = decode_x86_far_call_gate_target(0x30, ring0, offset, 3)
            .expect("far CALL may enter a more-privileged nonconforming segment");
        assert_eq!(call_target.segment.selector, 0x30);
        assert_eq!(call_target.segment.dpl, 0);

        let conforming = code_descriptor(0, true, true, true, false, 0);
        let conforming_target =
            decode_x86_far_call_gate_target(0x30, conforming, offset, 3).unwrap();
        assert_eq!(
            conforming_target.segment.selector, 0x33,
            "conforming target retains caller CPL"
        );
        assert_eq!(
            decode_x86_far_call_gate_target(
                0x33,
                code_descriptor(3, true, false, true, false, 0),
                offset,
                0,
            )
            .err(),
            Some(X86SystemDescriptorFault::GeneralProtection { error_code: 0x30 })
        );
    }

    #[test]
    fn ia32e_compatibility_target_loads_eip_even_from_m16_64_pointer() {
        let compat = code_descriptor(3, true, false, false, true, 0xF_FFFF);
        let X86FarJumpDescriptor::Code(target) = decode_x86_far_jump_descriptor(
            0x1B,
            compat,
            None,
            0xFFFF_FFFF_0001_2345,
            OpWidth::W64,
            3,
            true,
        )
        .expect("REX.W far pointer truncates to compatibility EIP") else {
            panic!("compatibility descriptor decoded as a gate")
        };
        assert_eq!(target.offset, 0x0001_2345);
        assert!(!target.segment.l);
        assert!(target.segment.db);
    }
}
