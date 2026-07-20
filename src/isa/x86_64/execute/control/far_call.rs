//! Fault-precise IA-32e execution for indirect far CALL (`FF /3`).

use crate::error::Error;
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{X86SystemDescriptorFault, is_canonical_48};
use crate::smir::ir::types::OpWidth;
use crate::vm::vcpu::{Segment, SystemRegisters};

use super::{
    X86FarJumpDescriptor, X86FarJumpTarget, decode_x86_far_call_descriptor,
    decode_x86_far_call_gate_target, selector_error_code, validate_x86_far_call_target_offset,
    x86_far_jump_is_ia32e_call_gate,
};

/// Failure before the architectural CS:RIP:RSP[:SS] commit. `Memory` retains
/// MMU faults for the owning instruction dispatcher; selector-derived faults
/// retain their exact exception class and error code.
pub(in crate::isa::x86_64) enum X86FarCallLoadFault {
    Architectural(X86SystemDescriptorFault),
    StackSegment { error_code: u32 },
    InvalidTss { error_code: u32 },
    Memory(Error),
    NativeDeopt,
}

#[derive(Clone, Copy)]
struct X86FarCallStackWrite {
    address: u64,
    width: u8,
    value: u64,
}

struct X86FarCallFrame {
    writes: Vec<X86FarCallStackWrite>,
    final_rsp: u64,
    access_cpl: u8,
    new_ss: Option<Segment>,
}

#[inline]
fn stack_write_range_is_canonical(address: u64, width: u8) -> bool {
    address
        .checked_add(u64::from(width) - 1)
        .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last))
}

fn build_far_call_frame(
    initial_rsp: u64,
    access_cpl: u8,
    values: &[(u8, u64)],
    new_ss: Option<Segment>,
) -> Result<X86FarCallFrame, X86FarCallLoadFault> {
    let mut rsp = initial_rsp;
    let mut writes = Vec::with_capacity(values.len());
    for &(width, value) in values {
        debug_assert!(matches!(width, 2 | 4 | 8));
        rsp = rsp.wrapping_sub(u64::from(width));
        if !stack_write_range_is_canonical(rsp, width) {
            return Err(X86FarCallLoadFault::StackSegment { error_code: 0 });
        }
        writes.push(X86FarCallStackWrite {
            address: rsp,
            width,
            value,
        });
    }
    Ok(X86FarCallFrame {
        writes,
        final_rsp: rsp,
        access_cpl,
        new_ss,
    })
}

#[inline]
fn null_long_mode_stack_segment(cpl: u8) -> Segment {
    Segment {
        base: 0,
        limit: 0xFFFF_FFFF,
        selector: u16::from(cpl),
        type_: 0x3,
        present: true,
        dpl: cpl,
        db: true,
        s: true,
        l: false,
        g: true,
        avl: false,
        unusable: false,
    }
}

impl X86_64Vcpu {
    #[inline]
    fn far_call_access_state(&self, cpl: u8) -> SystemRegisters {
        let mut sregs = self.sregs.clone();
        sregs.cs.selector = (sregs.cs.selector & !3) | u16::from(cpl);
        sregs
    }

    fn far_call_plain_read(&mut self, address: u64, size: usize, cpl: u8) -> bool {
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            let sregs = self.far_call_access_state(cpl);
            return self.mmu.read_range_is_plain_ram(address, size, &sregs);
        }
        #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
        {
            let _ = (address, size, cpl);
            false
        }
    }

    fn far_call_plain_write(&mut self, address: u64, size: usize, cpl: u8) -> bool {
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        {
            let sregs = self.far_call_access_state(cpl);
            return self.mmu.write_range_is_plain_ram(address, size, &sregs);
        }
        #[cfg(not(all(feature = "smir-jit", target_arch = "x86_64")))]
        {
            let _ = (address, size, cpl);
            false
        }
    }

    fn read_far_call_descriptor(
        &mut self,
        selector: u16,
        size: u64,
        native_preflight: bool,
    ) -> Result<(u64, Option<u64>, u64), X86FarCallLoadFault> {
        let address = self
            .far_jump_descriptor_address(selector, size)
            .map_err(X86FarCallLoadFault::Architectural)?;
        if native_preflight && !self.far_call_plain_read(address, size as usize, 0) {
            return Err(X86FarCallLoadFault::NativeDeopt);
        }
        let low = self
            .read_far_jump_descriptor_qword(address)
            .map_err(X86FarCallLoadFault::Memory)?;
        let high = if size == 16 {
            Some(
                self.read_far_jump_descriptor_qword(address.wrapping_add(8))
                    .map_err(X86FarCallLoadFault::Memory)?,
            )
        } else {
            None
        };
        Ok((low, high, address))
    }

    fn read_far_call_tss_rsp(
        &mut self,
        target_cpl: u8,
        native_preflight: bool,
    ) -> Result<u64, X86FarCallLoadFault> {
        let tr_error = selector_error_code(self.sregs.tr.selector);
        if self.sregs.tr.selector & 0xFFFC == 0
            || self.sregs.tr.unusable
            || !self.sregs.tr.present
            || self.sregs.tr.s
            || !matches!(self.sregs.tr.type_ & 0xF, 0x9 | 0xB)
        {
            return Err(X86FarCallLoadFault::InvalidTss {
                error_code: tr_error,
            });
        }
        let offset = 4_u64 + u64::from(target_cpl) * 8;
        if offset + 7 > u64::from(self.sregs.tr.limit) {
            return Err(X86FarCallLoadFault::InvalidTss {
                error_code: tr_error,
            });
        }
        let Some(address) = self.sregs.tr.base.checked_add(offset) else {
            return Err(X86FarCallLoadFault::InvalidTss {
                error_code: tr_error,
            });
        };
        if !stack_write_range_is_canonical(address, 8) {
            return Err(X86FarCallLoadFault::InvalidTss {
                error_code: tr_error,
            });
        }
        if native_preflight && !self.far_call_plain_read(address, 8, 0) {
            return Err(X86FarCallLoadFault::NativeDeopt);
        }
        self.read_far_jump_descriptor_qword(address)
            .map_err(X86FarCallLoadFault::Memory)
    }

    fn preflight_far_call_writes(
        &mut self,
        descriptor_address: u64,
        old_low: u64,
        target: &X86FarJumpTarget,
        frame: &X86FarCallFrame,
        native_preflight: bool,
    ) -> Result<(), X86FarCallLoadFault> {
        if old_low != target.accessed_low {
            let supervisor = self.far_call_access_state(0);
            self.mmu
                .preflight_write_range(descriptor_address, 8, &supervisor)
                .map_err(X86FarCallLoadFault::Memory)?;
            if native_preflight {
                let last = descriptor_address
                    .checked_add(7)
                    .ok_or(X86FarCallLoadFault::NativeDeopt)?;
                if self.mmu.is_code_page(descriptor_address)
                    || self.mmu.is_code_page(last)
                    || !self.far_call_plain_write(descriptor_address, 8, 0)
                {
                    return Err(X86FarCallLoadFault::NativeDeopt);
                }
                // Verification restores writes through the caller's original
                // privilege. A supervisor-only accessed-bit write is therefore
                // not safely reversible from CPL1-3.
                #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
                if self.jit_mem_log_active() && self.sregs.cs.selector & 3 != 0 {
                    return Err(X86FarCallLoadFault::NativeDeopt);
                }
            }
        }

        let access = self.far_call_access_state(frame.access_cpl);
        for write in &frame.writes {
            self.mmu
                .preflight_write_range(write.address, usize::from(write.width), &access)
                .map_err(X86FarCallLoadFault::Memory)?;
            if native_preflight {
                let last = write
                    .address
                    .checked_add(u64::from(write.width) - 1)
                    .ok_or(X86FarCallLoadFault::NativeDeopt)?;
                if self.mmu.is_code_page(write.address)
                    || self.mmu.is_code_page(last)
                    || !self.far_call_plain_write(
                        write.address,
                        usize::from(write.width),
                        frame.access_cpl,
                    )
                {
                    return Err(X86FarCallLoadFault::NativeDeopt);
                }
            }
        }
        Ok(())
    }

    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    fn log_far_call_frame_for_verify(
        &mut self,
        descriptor_address: u64,
        old_low: u64,
        target: &X86FarJumpTarget,
        frame: &X86FarCallFrame,
    ) -> Result<(), X86FarCallLoadFault> {
        if !self.jit_mem_log_active() {
            return Ok(());
        }
        if frame.new_ss.is_some() {
            // The verifier restores memory after restoring the caller CPL; a
            // frame written on a more-privileged stack is not reversible there.
            return Err(X86FarCallLoadFault::NativeDeopt);
        }
        if old_low != target.accessed_low {
            self.push_jit_mem_log((descriptor_address, 8, old_low));
        }

        let checkpoint = self.mmu.mem_record_checkpoint();
        let access = self.far_call_access_state(frame.access_cpl);
        for write in &frame.writes {
            let mut bytes = [0_u8; 8];
            if self
                .mmu
                .read(
                    write.address,
                    &mut bytes[..usize::from(write.width)],
                    &access,
                )
                .is_err()
            {
                self.mmu.restore_mem_record_checkpoint(checkpoint);
                return Err(X86FarCallLoadFault::NativeDeopt);
            }
            self.push_jit_mem_log((write.address, write.width, u64::from_le_bytes(bytes)));
        }
        self.mmu.restore_mem_record_checkpoint(checkpoint);
        Ok(())
    }

    fn commit_far_call(
        &mut self,
        descriptor_address: u64,
        old_low: u64,
        target: X86FarJumpTarget,
        frame: X86FarCallFrame,
        native_preflight: bool,
    ) -> Result<(), X86FarCallLoadFault> {
        self.preflight_far_call_writes(
            descriptor_address,
            old_low,
            &target,
            &frame,
            native_preflight,
        )?;
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        if native_preflight {
            self.log_far_call_frame_for_verify(descriptor_address, old_low, &target, &frame)?;
        }

        if old_low != target.accessed_low {
            self.write_far_jump_descriptor_qword(descriptor_address, target.accessed_low)
                .map_err(X86FarCallLoadFault::Memory)?;
        }

        let access = self.far_call_access_state(frame.access_cpl);
        for write in &frame.writes {
            let bytes = write.value.to_le_bytes();
            self.mmu
                .write(write.address, &bytes[..usize::from(write.width)], &access)
                .map_err(X86FarCallLoadFault::Memory)?;
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            self.push_jit_mem_trace((1, write.address, write.width, write.value));
        }

        if let Some(ss) = frame.new_ss {
            self.sregs.ss = ss;
        }
        self.regs.rsp = frame.final_rsp;
        self.sregs.cs = target.segment;
        self.regs.rip = target.offset;
        Ok(())
    }

    /// Execute the protected IA-32e, current-64-bit-code form of `FF /3`.
    /// Pointer and descriptor reads, call-gate privilege checks, optional TSS
    /// stack selection, every stack write preflight, and the code-descriptor
    /// accessed-bit transition precede one architectural state commit.
    pub(in crate::isa::x86_64) fn call_far_long_mode(
        &mut self,
        pointer_address: u64,
        offset_width: OpWidth,
        stack_segment: bool,
        return_pc: u64,
        native_preflight: bool,
    ) -> Result<(), X86FarCallLoadFault> {
        if !self.sregs.cs.l || self.sregs.efer & (1 << 10) == 0 {
            return Err(X86FarCallLoadFault::NativeDeopt);
        }
        let pointer_size: usize = match offset_width {
            OpWidth::W16 => 4,
            OpWidth::W32 => 6,
            OpWidth::W64 => 10,
            OpWidth::W8 | OpWidth::W128 => return Err(X86FarCallLoadFault::NativeDeopt),
        };
        let canonical_pointer = pointer_address
            .checked_add(pointer_size as u64 - 1)
            .is_some_and(|last| is_canonical_48(pointer_address) && is_canonical_48(last));
        if !canonical_pointer {
            return Err(if stack_segment {
                X86FarCallLoadFault::StackSegment { error_code: 0 }
            } else {
                X86FarCallLoadFault::Architectural(X86SystemDescriptorFault::GeneralProtection {
                    error_code: 0,
                })
            });
        }
        let cpl = (self.sregs.cs.selector & 3) as u8;
        if native_preflight && !self.far_call_plain_read(pointer_address, pointer_size, cpl) {
            return Err(X86FarCallLoadFault::NativeDeopt);
        }

        let offset_size = offset_width.bits() as u8 / 8;
        let pointer_offset = self
            .read_mem(pointer_address, offset_size)
            .map_err(X86FarCallLoadFault::Memory)?;
        let selector = self
            .read_mem(pointer_address.wrapping_add(u64::from(offset_size)), 2)
            .map_err(X86FarCallLoadFault::Memory)? as u16;

        let (mut selected_low, _, mut selected_address) =
            self.read_far_call_descriptor(selector, 8, native_preflight)?;
        let selected_high = if x86_far_jump_is_ia32e_call_gate(selected_low, true) {
            selected_address = self
                .far_jump_descriptor_address(selector, 16)
                .map_err(X86FarCallLoadFault::Architectural)?;
            if native_preflight && !self.far_call_plain_read(selected_address, 16, 0) {
                return Err(X86FarCallLoadFault::NativeDeopt);
            }
            Some(
                self.read_far_jump_descriptor_qword(selected_address.wrapping_add(8))
                    .map_err(X86FarCallLoadFault::Memory)?,
            )
        } else {
            None
        };
        let descriptor = decode_x86_far_call_descriptor(
            selector,
            selected_low,
            selected_high,
            pointer_offset,
            offset_width,
            cpl,
            true,
        )
        .map_err(X86FarCallLoadFault::Architectural)?;

        let old_cs = self.sregs.cs.selector;
        let old_ss = self.sregs.ss.selector;
        let old_rsp = self.regs.rsp;
        let (target, frame) = match descriptor {
            X86FarJumpDescriptor::Code(target) => {
                let width = offset_size;
                let frame = build_far_call_frame(
                    old_rsp,
                    cpl,
                    &[(width, u64::from(old_cs)), (width, return_pc)],
                    None,
                )?;
                (target, frame)
            }
            X86FarJumpDescriptor::CallGate(gate) => {
                let (target_low, _, target_address) =
                    self.read_far_call_descriptor(gate.selector, 8, native_preflight)?;
                selected_low = target_low;
                selected_address = target_address;
                let target =
                    decode_x86_far_call_gate_target(gate.selector, target_low, gate.offset, cpl)
                        .map_err(X86FarCallLoadFault::Architectural)?;
                let target_cpl = (target.segment.selector & 3) as u8;
                if target_cpl < cpl {
                    let new_rsp = self.read_far_call_tss_rsp(target_cpl, native_preflight)?;
                    let frame = build_far_call_frame(
                        new_rsp,
                        target_cpl,
                        &[
                            (8, u64::from(old_ss)),
                            (8, old_rsp),
                            (8, u64::from(old_cs)),
                            (8, return_pc),
                        ],
                        Some(null_long_mode_stack_segment(target_cpl)),
                    )?;
                    (target, frame)
                } else {
                    let frame = build_far_call_frame(
                        old_rsp,
                        cpl,
                        &[(8, u64::from(old_cs)), (8, return_pc)],
                        None,
                    )?;
                    (target, frame)
                }
            }
        };
        validate_x86_far_call_target_offset(&target).map_err(X86FarCallLoadFault::Architectural)?;

        self.commit_far_call(
            selected_address,
            selected_low,
            target,
            frame,
            native_preflight,
        )
    }
}
