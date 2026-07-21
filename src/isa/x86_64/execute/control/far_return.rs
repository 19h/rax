//! Fault-precise IA-32e execution for far RET (`CA`/`CB`).

use crate::error::Error;
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{X86SystemDescriptorFault, is_canonical_48};
use crate::smir::ir::types::OpWidth;
use crate::vm::vcpu::{Segment, SystemRegisters};

use super::{
    X86FarJumpTarget, X86FarReturnStack, decode_x86_far_return_code, decode_x86_far_return_stack,
    validate_x86_far_call_target_offset,
};

/// Failure before the architectural CS:RIP:RSP[:SS] and data-segment commit.
/// Native-only preflight failures return to direct replay without exposing a
/// speculative descriptor write or privilege transition.
pub(in crate::isa::x86_64) enum X86FarReturnLoadFault {
    Architectural(X86SystemDescriptorFault),
    StackSegment { error_code: u32 },
    Memory(Error),
    NativeDeopt,
}

#[inline]
fn stack_range_is_canonical(address: u64, size: u64) -> bool {
    size != 0
        && address
            .checked_add(size - 1)
            .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last))
}

#[inline]
fn width_bytes(width: OpWidth) -> Option<u8> {
    match width {
        OpWidth::W16 => Some(2),
        OpWidth::W32 => Some(4),
        OpWidth::W64 => Some(8),
        OpWidth::W8 | OpWidth::W128 => None,
    }
}

#[inline]
fn outer_stack_pointer(
    loaded: u64,
    target: &X86FarJumpTarget,
    stack: &X86FarReturnStack,
    pop_bytes: u16,
) -> u64 {
    if target.segment.l {
        loaded.wrapping_add(u64::from(pop_bytes))
    } else if stack.segment.db {
        u64::from((loaded as u32).wrapping_add(u32::from(pop_bytes)))
    } else {
        (loaded & !0xFFFF) | u64::from((loaded as u16).wrapping_add(pop_bytes))
    }
}

#[inline]
fn invalidate_outer_data_segment(segment: &mut Segment, new_cpl: u8) {
    if segment.selector & 0xFFFC == 0 {
        segment.selector = 0;
        segment.unusable = true;
        return;
    }
    let data = segment.type_ & 0x8 == 0;
    let nonconforming_code = segment.type_ & 0x8 != 0 && segment.type_ & 0x4 == 0;
    if segment.s && (data || nonconforming_code) && segment.dpl < new_cpl {
        segment.selector = 0;
        segment.unusable = true;
    }
}

impl X86_64Vcpu {
    #[inline]
    fn far_return_supervisor_state(&self) -> SystemRegisters {
        let mut sregs = self.sregs.clone();
        sregs.cs.selector &= !3;
        sregs
    }

    fn read_far_return_stack_slot(
        &mut self,
        address: u64,
        width: u8,
        native_preflight: bool,
    ) -> Result<u64, X86FarReturnLoadFault> {
        if !stack_range_is_canonical(address, u64::from(width)) {
            return Err(X86FarReturnLoadFault::StackSegment { error_code: 0 });
        }
        if native_preflight && !self.far_jump_plain_read(address, usize::from(width), false) {
            return Err(X86FarReturnLoadFault::NativeDeopt);
        }
        self.read_mem(address, width)
            .map_err(X86FarReturnLoadFault::Memory)
    }

    fn read_far_return_descriptor(
        &mut self,
        selector: u16,
        native_preflight: bool,
    ) -> Result<(u64, u64), X86FarReturnLoadFault> {
        let address = self
            .far_jump_descriptor_address(selector, 8)
            .map_err(X86FarReturnLoadFault::Architectural)?;
        if native_preflight && !self.far_jump_plain_read(address, 8, true) {
            return Err(X86FarReturnLoadFault::NativeDeopt);
        }
        let raw = self
            .read_far_jump_descriptor_qword(address)
            .map_err(X86FarReturnLoadFault::Memory)?;
        Ok((raw, address))
    }

    fn preflight_far_return_descriptor_write(
        &mut self,
        address: u64,
        native_preflight: bool,
    ) -> Result<(), X86FarReturnLoadFault> {
        let supervisor = self.far_return_supervisor_state();
        self.mmu
            .preflight_write_range(address, 8, &supervisor)
            .map_err(X86FarReturnLoadFault::Memory)?;
        if native_preflight {
            let last = address
                .checked_add(7)
                .ok_or(X86FarReturnLoadFault::NativeDeopt)?;
            if self.mmu.is_code_page(address)
                || self.mmu.is_code_page(last)
                || !self.far_jump_plain_write(address, 8, true)
            {
                return Err(X86FarReturnLoadFault::NativeDeopt);
            }
        }
        Ok(())
    }

    fn commit_far_return(
        &mut self,
        code_address: u64,
        code_raw: u64,
        target: X86FarJumpTarget,
        new_stack: Option<(Option<(u64, u64)>, X86FarReturnStack)>,
        final_rsp: u64,
        native_preflight: bool,
    ) -> Result<(), X86FarReturnLoadFault> {
        let code_write = code_raw != target.accessed_low;
        let stack_write = new_stack
            .as_ref()
            .and_then(|(descriptor, stack)| {
                descriptor
                    .as_ref()
                    .map(|(address, raw)| (*address, *raw, stack))
            })
            .and_then(|(address, raw, stack)| {
                stack
                    .accessed_low
                    .filter(|accessed| raw != *accessed)
                    .map(|accessed| (address, raw, accessed))
            });

        if code_write {
            self.preflight_far_return_descriptor_write(code_address, native_preflight)?;
        }
        if let Some((address, _, _)) = stack_write {
            self.preflight_far_return_descriptor_write(address, native_preflight)?;
        }

        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        if native_preflight && self.jit_mem_log_active() && (code_write || stack_write.is_some()) {
            // Verification undoes stores before restoring the caller's CPL.
            // Descriptor-table writes are supervisor accesses and therefore
            // are reversible only when execution remains at CPL0.
            let new_cpl = target.segment.selector & 3;
            if self.sregs.cs.selector & 3 != 0 || new_cpl != 0 {
                return Err(X86FarReturnLoadFault::NativeDeopt);
            }
        }

        if code_write {
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            self.push_jit_mem_log((code_address, 8, code_raw));
            self.write_far_jump_descriptor_qword(code_address, target.accessed_low)
                .map_err(X86FarReturnLoadFault::Memory)?;
        }
        if let Some((address, raw, accessed)) = stack_write {
            #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
            self.push_jit_mem_log((address, 8, raw));
            self.write_far_jump_descriptor_qword(address, accessed)
                .map_err(X86FarReturnLoadFault::Memory)?;
        }

        let new_cpl = (target.segment.selector & 3) as u8;
        let outer = new_cpl > (self.sregs.cs.selector & 3) as u8;
        if let Some((_, stack)) = new_stack {
            self.sregs.ss = stack.segment;
        }
        self.regs.rsp = final_rsp;
        self.sregs.cs = target.segment;
        self.regs.rip = target.offset;
        if outer {
            invalidate_outer_data_segment(&mut self.sregs.es, new_cpl);
            invalidate_outer_data_segment(&mut self.sregs.ds, new_cpl);
            invalidate_outer_data_segment(&mut self.sregs.fs, new_cpl);
            invalidate_outer_data_segment(&mut self.sregs.gs, new_cpl);
        }
        Ok(())
    }

    /// Execute one protected IA-32e far RET issued from a 64-bit code segment.
    /// Every stack/descriptor read, outer-stack validation, accessed-bit write
    /// preflight, and target check precedes the single architectural commit.
    pub(in crate::isa::x86_64) fn return_far_long_mode(
        &mut self,
        offset_width: OpWidth,
        pop_bytes: u16,
        native_preflight: bool,
    ) -> Result<(), X86FarReturnLoadFault> {
        if !self.sregs.cs.l
            || self.sregs.efer & (1 << 10) == 0
            || self.sregs.cr0 & 1 == 0
            || self.regs.rflags & crate::isa::x86_64::flags::bits::VM != 0
        {
            return Err(X86FarReturnLoadFault::NativeDeopt);
        }
        let width = width_bytes(offset_width).ok_or(X86FarReturnLoadFault::NativeDeopt)?;
        let initial_rsp = self.regs.rsp;
        let return_offset =
            self.read_far_return_stack_slot(initial_rsp, width, native_preflight)?;
        let selector_address = initial_rsp.wrapping_add(u64::from(width));
        let return_selector =
            self.read_far_return_stack_slot(selector_address, width, native_preflight)? as u16;

        let (code_raw, code_address) =
            self.read_far_return_descriptor(return_selector, native_preflight)?;
        let cpl = (self.sregs.cs.selector & 3) as u8;
        let target = decode_x86_far_return_code(
            return_selector,
            code_raw,
            return_offset,
            offset_width,
            cpl,
            false,
        )
        .map_err(X86FarReturnLoadFault::Architectural)?;
        let target_cpl = (target.segment.selector & 3) as u8;

        if target_cpl == cpl {
            validate_x86_far_call_target_offset(&target)
                .map_err(X86FarReturnLoadFault::Architectural)?;
            let final_rsp = initial_rsp
                .wrapping_add(u64::from(width) * 2)
                .wrapping_add(u64::from(pop_bytes));
            return self.commit_far_return(
                code_address,
                code_raw,
                target,
                None,
                final_rsp,
                native_preflight,
            );
        }

        let frame_size = u64::from(width) * 4 + u64::from(pop_bytes);
        if !stack_range_is_canonical(initial_rsp, frame_size) {
            return Err(X86FarReturnLoadFault::StackSegment { error_code: 0 });
        }
        let outer_rsp_address = initial_rsp
            .wrapping_add(u64::from(width) * 2)
            .wrapping_add(u64::from(pop_bytes));
        let loaded_rsp =
            self.read_far_return_stack_slot(outer_rsp_address, width, native_preflight)?;
        let outer_ss_address = outer_rsp_address.wrapping_add(u64::from(width));
        let return_ss =
            self.read_far_return_stack_slot(outer_ss_address, width, native_preflight)? as u16;

        let (stack_raw, stack_address) = if return_ss & 0xFFFC == 0 {
            (None, None)
        } else {
            let (raw, address) = self.read_far_return_descriptor(return_ss, native_preflight)?;
            (Some(raw), Some(address))
        };
        let stack =
            match decode_x86_far_return_stack(return_ss, stack_raw, target_cpl, target.segment.l) {
                Ok(stack) => stack,
                Err(X86SystemDescriptorFault::SegmentNotPresent { error_code }) => {
                    return Err(X86FarReturnLoadFault::StackSegment { error_code });
                }
                Err(fault) => return Err(X86FarReturnLoadFault::Architectural(fault)),
            };

        validate_x86_far_call_target_offset(&target)
            .map_err(X86FarReturnLoadFault::Architectural)?;
        let final_rsp = outer_stack_pointer(loaded_rsp, &target, &stack, pop_bytes);
        let new_stack = Some((
            stack_address
                .map(|address| (address, stack_raw.expect("nonnull SS descriptor was read"))),
            stack,
        ));
        self.commit_far_return(
            code_address,
            code_raw,
            target,
            new_stack,
            final_rsp,
            native_preflight,
        )
    }
}
