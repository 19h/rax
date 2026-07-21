//! Fault-precise LLDT/LTR, `MOV Sreg,r/m`, `POP FS/GS`, and `LSS/LFS/LGS`
//! interpretation.

use crate::isa::x86_64::execute::system::{
    X86SegmentLoadTarget, X86SelectorVerifyAccess, X86SystemDescriptorFault,
    decode_x86_ldt_descriptor, decode_x86_segment_load_descriptor, decode_x86_tss_descriptor,
    is_canonical_48, x86_real_mode_segment, x86_selector_verifies,
};
use crate::smir::interpret::*;
use crate::smir::ir::context::{
    ArchRegState, ExitReason, SmirContext, X86RegState, X86SystemSegmentCache,
};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    SmirOp, X86SelectorVerifyKind, X86SelectorVerifyOp, X86SelectorVerifySource, X86SystemSelector,
    X86SystemSelectorLoadOp, X86SystemSelectorSource,
};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg};

use super::system::{
    read_smir_u64, request_x86_descriptor_fault, x86_far_jump_descriptor_address,
    x86_system_segment_cache,
};

fn ordinary_target(selector: X86SystemSelector) -> Option<X86SegmentLoadTarget> {
    Some(match selector {
        X86SystemSelector::Es => X86SegmentLoadTarget::Es,
        X86SystemSelector::Ss => X86SegmentLoadTarget::Ss,
        X86SystemSelector::Ds => X86SegmentLoadTarget::Ds,
        X86SystemSelector::Fs => X86SegmentLoadTarget::Fs,
        X86SystemSelector::Gs => X86SegmentLoadTarget::Gs,
        X86SystemSelector::Ldtr | X86SystemSelector::Tr | X86SystemSelector::Cs => return None,
    })
}

fn selector_load_shape_valid(op: &SmirOp, load: &X86SystemSelectorLoadOp) -> bool {
    let system = matches!(
        load.selector,
        X86SystemSelector::Ldtr | X86SystemSelector::Tr
    );
    let ordinary = ordinary_target(load.selector).is_some();
    let instruction_len = load.next_pc.checked_sub(op.guest_pc);
    let length_valid = if system {
        matches!(instruction_len, Some(3..=15))
    } else {
        matches!(instruction_len, Some(2..=15))
    };
    if op.x86_hint.is_some() || !(system || ordinary) || !length_valid {
        return false;
    }

    match &load.source {
        X86SystemSelectorSource::Register { src } => matches!(
            src,
            VReg::Arch(ArchReg::X86(reg))
                if reg.gpr_index().is_some_and(|index| index < 16 || load.requires_apx)
        ),
        X86SystemSelectorSource::Memory { addr, width, .. } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            let width_valid = *width == MemWidth::B2 || ordinary && *width == MemWidth::B8;
            addr.is_x86_state_backed_shape() && (!uses_egpr || load.requires_apx) && width_valid
        }
        X86SystemSelectorSource::Stack {
            stack_pointer,
            width,
        } => {
            let minimum_len = 2 + u64::from(load.requires_apx) + u64::from(*width == MemWidth::B2);
            *stack_pointer == VReg::Arch(ArchReg::X86(crate::smir::ir::types::X86Reg::Rsp))
                && matches!(width, MemWidth::B2 | MemWidth::B8)
                && matches!(load.selector, X86SystemSelector::Fs | X86SystemSelector::Gs)
                && instruction_len.is_some_and(|length| (minimum_len..=15).contains(&length))
        }
        X86SystemSelectorSource::FarPointer {
            addr,
            dst,
            offset_width,
            ..
        } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            let Some(dst_index) = (match dst {
                VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
                _ => None,
            }) else {
                return false;
            };
            let minimum_len = 3
                + u64::from(load.requires_apx)
                + u64::from(*offset_width == OpWidth::W16)
                + u64::from(*offset_width == OpWidth::W64 && !load.requires_apx);
            addr.is_x86_state_backed_shape()
                && ((!uses_egpr && dst_index < 16) || load.requires_apx)
                && matches!(offset_width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                && matches!(
                    load.selector,
                    X86SystemSelector::Ss | X86SystemSelector::Fs | X86SystemSelector::Gs
                )
                && instruction_len.is_some_and(|length| (minimum_len..=15).contains(&length))
        }
    }
}

fn selector_verify_shape_valid(op: &SmirOp, verify: &X86SelectorVerifyOp) -> bool {
    if op.x86_hint.is_some() || !matches!(verify.next_pc.checked_sub(op.guest_pc), Some(3..=15)) {
        return false;
    }

    match &verify.source {
        X86SelectorVerifySource::Register { src } => matches!(
            src,
            VReg::Arch(ArchReg::X86(reg))
                if reg.gpr_index().is_some_and(|index| index < 16 || verify.requires_apx)
        ),
        X86SelectorVerifySource::Memory { addr, .. } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            addr.is_x86_state_backed_shape() && (!uses_egpr || verify.requires_apx)
        }
    }
}

fn selector_verify_descriptor_address(x86: &X86RegState, selector: u16) -> Option<u64> {
    if selector & 0xFFFC == 0 {
        return None;
    }
    let ti = selector & 0x4 != 0;
    if ti && (x86.ldtr_selector & 0xFFFC == 0 || x86.ldtr_cache.unusable) {
        return None;
    }
    let (base, limit) = if ti {
        (x86.ldtr_cache.base, u64::from(x86.ldtr_cache.limit))
    } else {
        (x86.gdtr_base, u64::from(x86.gdtr_limit))
    };
    let offset = u64::from(selector >> 3) * 8;
    if offset.checked_add(7).is_none_or(|last| last > limit) {
        return None;
    }
    let address = base.checked_add(offset)?;
    let last = address.checked_add(7)?;
    if x86.efer & (1 << 10) != 0 && (!is_canonical_48(address) || !is_canonical_48(last)) {
        return None;
    }
    Some(address)
}

fn commit_ordinary_segment(
    x86: &mut X86RegState,
    target: X86SegmentLoadTarget,
    segment: crate::vm::vcpu::Segment,
) {
    let cache = x86_system_segment_cache(&segment);
    match target {
        X86SegmentLoadTarget::Es => {
            x86.es_selector = segment.selector;
            x86.es_cache = cache;
        }
        X86SegmentLoadTarget::Ss => {
            x86.ss_selector = segment.selector;
            x86.ss_cache = cache;
            x86.interrupt_inhibit = true;
        }
        X86SegmentLoadTarget::Ds => {
            x86.ds_selector = segment.selector;
            x86.ds_cache = cache;
        }
        X86SegmentLoadTarget::Fs => {
            x86.fs_selector = segment.selector;
            x86.fs_base = segment.base;
            x86.fs_cache = cache;
        }
        X86SegmentLoadTarget::Gs => {
            x86.gs_selector = segment.selector;
            x86.gs_base = segment.base;
            x86.gs_cache = cache;
        }
    }
}

impl SmirInterpreter {
    pub(super) fn execute_x86_selector_verify(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
        verify: &X86SelectorVerifyOp,
    ) -> Result<(), MemoryError> {
        if !selector_verify_shape_valid(op, verify) {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let (ia32e_active, cpl) = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => {
                if verify.requires_apx && !x86.apx_enabled
                    || x86.cr0 & 1 == 0
                    || x86.rflags & crate::isa::x86_64::flags::bits::VM != 0
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                (x86.efer & (1 << 10) != 0, x86.cpl)
            }
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: op.guest_pc,
                    opcode: 0,
                });
                return Ok(());
            }
        };

        let selector = match &verify.source {
            X86SelectorVerifySource::Register { src } => ctx.read_vreg(*src) as u16,
            X86SelectorVerifySource::Memory {
                addr,
                stack_segment,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let canonical = effective_addr.checked_add(1).is_some_and(|last| {
                    !ia32e_active || is_canonical_48(effective_addr) && is_canonical_48(last)
                });
                if !canonical {
                    ctx.request_exit(if *stack_segment {
                        ExitReason::StackSegment {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    } else {
                        ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    });
                    return Ok(());
                }
                self.load_memory(memory, effective_addr, MemWidth::B2, SignExtend::Zero)? as u16
            }
        };

        let verified = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => match selector_verify_descriptor_address(x86, selector) {
                Some(address) => {
                    let raw = read_smir_u64(memory, address)?;
                    let access = match verify.kind {
                        X86SelectorVerifyKind::Read => X86SelectorVerifyAccess::Read,
                        X86SelectorVerifyKind::Write => X86SelectorVerifyAccess::Write,
                    };
                    x86_selector_verifies(selector, raw, cpl, access)
                }
                None => false,
            },
            _ => unreachable!("validated x86 selector-verification state changed"),
        };

        ctx.flags.materialize_all();
        ctx.flags.materialized.zf = verified;
        ctx.flags.lazy = None;
        Ok(())
    }

    pub(super) fn execute_x86_system_selector_load(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
        load: &X86SystemSelectorLoadOp,
    ) -> Result<(), MemoryError> {
        if !selector_load_shape_valid(op, load) {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let system = matches!(
            load.selector,
            X86SystemSelector::Ldtr | X86SystemSelector::Tr
        );
        let (protected, virtual_8086, cpl, long_mode, ia32e_active) = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => {
                if load.requires_apx && !x86.apx_enabled {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                (
                    x86.cr0 & 1 != 0,
                    x86.rflags & crate::isa::x86_64::flags::bits::VM != 0,
                    x86.cpl,
                    x86.cs_l,
                    x86.efer & (1 << 10) != 0,
                )
            }
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: op.guest_pc,
                    opcode: 0,
                });
                return Ok(());
            }
        };

        // LLDT/LTR mode and privilege faults precede source-memory access.
        if system && (!protected || virtual_8086) {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }
        if system && cpl != 0 {
            ctx.request_exit(ExitReason::GeneralProtection {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }

        let (selector, stack_commit, far_pointer_commit) = match &load.source {
            X86SystemSelectorSource::Register { src } => (ctx.read_vreg(*src) as u16, None, None),
            X86SystemSelectorSource::Memory {
                addr,
                width,
                stack_segment,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let width_bytes = width.bytes() as u64;
                let canonical = effective_addr
                    .checked_add(width_bytes - 1)
                    .is_some_and(|last| {
                        !ia32e_active || is_canonical_48(effective_addr) && is_canonical_48(last)
                    });
                if !canonical {
                    ctx.request_exit(if *stack_segment {
                        ExitReason::StackSegment {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    } else {
                        ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    });
                    return Ok(());
                }
                (
                    self.load_memory(memory, effective_addr, *width, SignExtend::Zero)? as u16,
                    None,
                    None,
                )
            }
            X86SystemSelectorSource::Stack {
                stack_pointer,
                width,
            } => {
                if !protected || virtual_8086 || !ia32e_active || !long_mode {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let rsp = ctx.read_vreg(*stack_pointer);
                let width_bytes = u64::from(width.bytes());
                let canonical = rsp
                    .checked_add(width_bytes - 1)
                    .is_some_and(|last| is_canonical_48(rsp) && is_canonical_48(last));
                if !canonical {
                    ctx.request_exit(ExitReason::StackSegment {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                (
                    self.load_memory(memory, rsp, *width, SignExtend::Zero)? as u16,
                    Some((*stack_pointer, rsp.wrapping_add(width_bytes))),
                    None,
                )
            }
            X86SystemSelectorSource::FarPointer {
                addr,
                dst,
                offset_width,
                stack_segment,
            } => {
                if !protected || virtual_8086 || !ia32e_active || !long_mode {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let effective_addr = self.compute_address(ctx, addr);
                let offset_bytes = u64::from(offset_width.bytes());
                let pointer_bytes = offset_bytes + 2;
                let canonical = effective_addr
                    .checked_add(pointer_bytes - 1)
                    .is_some_and(|last| is_canonical_48(effective_addr) && is_canonical_48(last));
                if !canonical {
                    ctx.request_exit(if *stack_segment {
                        ExitReason::StackSegment {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    } else {
                        ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        }
                    });
                    return Ok(());
                }
                let offset = self.load_memory(
                    memory,
                    effective_addr,
                    offset_width.to_mem_width(),
                    SignExtend::Zero,
                )?;
                let selector = self.load_memory(
                    memory,
                    effective_addr + offset_bytes,
                    MemWidth::B2,
                    SignExtend::Zero,
                )? as u16;
                (selector, None, Some((*dst, offset, *offset_width)))
            }
        };

        if let Some(target) = ordinary_target(load.selector) {
            if !protected || virtual_8086 {
                let segment = x86_real_mode_segment(selector, virtual_8086);
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!("validated x86 selector-load state changed")
                };
                commit_ordinary_segment(x86, target, segment);
                if let Some((stack_pointer, value)) = stack_commit {
                    ctx.write_vreg(stack_pointer, value);
                }
                if let Some((dst, value, width)) = far_pointer_commit {
                    Self::write_gpr(ctx, dst, value, width);
                }
                return Ok(());
            }

            if selector & 0xFFFC == 0 {
                if target == X86SegmentLoadTarget::Ss
                    && (!long_mode || cpl == 3 || (selector & 3) as u8 != cpl)
                {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                let segment = crate::vm::vcpu::Segment {
                    selector,
                    dpl: cpl,
                    unusable: true,
                    ..crate::vm::vcpu::Segment::default()
                };
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!("validated x86 selector-load state changed")
                };
                commit_ordinary_segment(x86, target, segment);
                if let Some((stack_pointer, value)) = stack_commit {
                    ctx.write_vreg(stack_pointer, value);
                }
                if let Some((dst, value, width)) = far_pointer_commit {
                    Self::write_gpr(ctx, dst, value, width);
                }
                return Ok(());
            }

            let descriptor_addr = {
                let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                    unreachable!("validated x86 selector-load state changed")
                };
                match x86_far_jump_descriptor_address(x86, selector, 8) {
                    Ok(address) => address,
                    Err(fault) => {
                        request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                        return Ok(());
                    }
                }
            };
            let low = read_smir_u64(memory, descriptor_addr)?;
            let descriptor = match decode_x86_segment_load_descriptor(target, selector, low, cpl) {
                Ok(descriptor) => descriptor,
                Err(X86SystemDescriptorFault::SegmentNotPresent { error_code })
                    if target == X86SegmentLoadTarget::Ss =>
                {
                    ctx.request_exit(ExitReason::StackSegment {
                        addr: op.guest_pc,
                        error_code,
                    });
                    return Ok(());
                }
                Err(fault) => {
                    request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                    return Ok(());
                }
            };
            if low != descriptor.accessed_low {
                memory.write(descriptor_addr, &descriptor.accessed_low.to_le_bytes())?;
            }
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!("validated x86 selector-load state changed")
            };
            commit_ordinary_segment(x86, target, descriptor.segment);
            if let Some((stack_pointer, value)) = stack_commit {
                ctx.write_vreg(stack_pointer, value);
            }
            if let Some((dst, value, width)) = far_pointer_commit {
                Self::write_gpr(ctx, dst, value, width);
            }
            return Ok(());
        }

        debug_assert!(stack_commit.is_none());
        debug_assert!(far_pointer_commit.is_none());

        if selector & 0xFFFC == 0 {
            if load.selector == X86SystemSelector::Tr {
                ctx.request_exit(ExitReason::GeneralProtection {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                unreachable!("validated x86 selector-load state changed")
            };
            x86.ldtr_selector = selector;
            x86.ldtr_cache = X86SystemSegmentCache {
                unusable: true,
                ..X86SystemSegmentCache::default()
            };
            return Ok(());
        }

        if selector & 4 != 0 {
            ctx.request_exit(ExitReason::GeneralProtection {
                addr: op.guest_pc,
                error_code: u32::from(selector & 0xFFFC),
            });
            return Ok(());
        }
        let descriptor_size = if long_mode { 16 } else { 8 };
        let descriptor_addr = {
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!("validated x86 selector-load state changed")
            };
            match x86_far_jump_descriptor_address(x86, selector, descriptor_size) {
                Ok(address) => address,
                Err(fault) => {
                    request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                    return Ok(());
                }
            }
        };
        let low = read_smir_u64(memory, descriptor_addr)?;
        let high = if long_mode {
            Some(read_smir_u64(memory, descriptor_addr.wrapping_add(8))?)
        } else {
            None
        };
        let decoded = match load.selector {
            X86SystemSelector::Ldtr => decode_x86_ldt_descriptor(selector, low, high, long_mode)
                .map(|segment| (segment, None)),
            X86SystemSelector::Tr => {
                decode_x86_tss_descriptor(selector, low, high, long_mode, ia32e_active)
                    .map(|descriptor| (descriptor.segment, Some(descriptor.busy_low)))
            }
            _ => unreachable!("validated system selector-load kind changed"),
        };
        let (segment, busy_low) = match decoded {
            Ok(decoded) => decoded,
            Err(fault) => {
                request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                return Ok(());
            }
        };
        if let Some(busy_low) = busy_low {
            memory.write(descriptor_addr, &busy_low.to_le_bytes())?;
        }

        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!("validated x86 selector-load state changed")
        };
        let cache = x86_system_segment_cache(&segment);
        match load.selector {
            X86SystemSelector::Ldtr => {
                x86.ldtr_selector = segment.selector;
                x86.ldtr_cache = cache;
            }
            X86SystemSelector::Tr => {
                x86.tr_selector = segment.selector;
                x86.tr_type = segment.type_;
                x86.tr_cache = cache;
            }
            _ => unreachable!("validated system selector-load kind changed"),
        }
        Ok(())
    }
}
