//! Canonical fault-precise interpretation for far RET (`CA`/`CB`).

use super::system::{
    read_smir_u64, request_x86_descriptor_fault, x86_far_jump_descriptor_address,
    x86_system_segment_cache,
};
use crate::isa::x86_64::execute::control::{
    decode_x86_far_return_code, decode_x86_far_return_stack, validate_x86_far_call_target_offset,
};
use crate::isa::x86_64::execute::system::X86SystemDescriptorFault;
use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, X86SystemSegmentCache};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp, X86FarReturnOp};
use crate::smir::ir::types::*;

#[inline]
fn stack_range_is_canonical(address: u64, size: u64) -> bool {
    size != 0
        && address.checked_add(size - 1).is_some_and(|last| {
            crate::isa::x86_64::execute::system::is_canonical_48(address)
                && crate::isa::x86_64::execute::system::is_canonical_48(last)
        })
}

#[inline]
fn outer_rsp(loaded: u64, target_l: bool, stack_db: bool, pop_bytes: u16) -> u64 {
    if target_l {
        loaded.wrapping_add(u64::from(pop_bytes))
    } else if stack_db {
        u64::from((loaded as u32).wrapping_add(u32::from(pop_bytes)))
    } else {
        (loaded & !0xFFFF) | u64::from((loaded as u16).wrapping_add(pop_bytes))
    }
}

#[inline]
fn invalidate_data_segment(selector: &mut u16, cache: &mut X86SystemSegmentCache, new_cpl: u8) {
    if *selector & 0xFFFC == 0 {
        *selector = 0;
        cache.unusable = true;
        return;
    }
    let data = cache.type_ & 0x8 == 0;
    let nonconforming_code = cache.type_ & 0x8 != 0 && cache.type_ & 0x4 == 0;
    if cache.s && (data || nonconforming_code) && cache.dpl < new_cpl {
        *selector = 0;
        cache.unusable = true;
    }
}

impl SmirInterpreter {
    pub(crate) fn execute_op_far_return(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86FarReturn(ret) = &op.kind else {
            unreachable!("far-RET interpreter called for a different operation")
        };
        let X86FarReturnOp {
            target,
            offset_width,
            pop_bytes,
            requires_apx,
            next_pc,
        } = ret;
        let minimum_len = 1 + usize::from(*requires_apx) * 2 + usize::from(*pop_bytes != 0) * 2;
        let valid_shape = op.x86_hint.is_none()
            && next_pc
                .checked_sub(op.guest_pc)
                .is_some_and(|len| (minimum_len as u64..=15).contains(&len))
            && matches!(offset_width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
            && *target == VReg::Arch(ArchReg::X86(X86Reg::Rip));
        if !valid_shape {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let (cpl, gdtr_base, gdtr_limit, ldtr_selector, ldtr_cache, initial_rsp) =
            match &ctx.arch_regs {
                ArchRegState::X86_64(x86)
                    if x86.cr0 & 1 != 0
                        && x86.rflags & crate::isa::x86_64::flags::bits::VM == 0
                        && x86.efer & (1 << 10) != 0
                        && x86.cs_l
                        && (!*requires_apx || x86.apx_enabled) =>
                {
                    (
                        x86.cpl,
                        x86.gdtr_base,
                        x86.gdtr_limit,
                        x86.ldtr_selector,
                        x86.ldtr_cache.clone(),
                        x86.gpr[4],
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

        let mem_width = offset_width.to_mem_width();
        let width = u64::from(mem_width.bytes());
        if !stack_range_is_canonical(initial_rsp, width) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        let return_offset = self.load_memory(memory, initial_rsp, mem_width, SignExtend::Zero)?;
        let selector_address = initial_rsp.wrapping_add(width);
        if !stack_range_is_canonical(selector_address, width) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        let return_selector =
            self.load_memory(memory, selector_address, mem_width, SignExtend::Zero)? as u16;

        let mut descriptor_state = crate::smir::ir::context::X86RegState::new();
        descriptor_state.gdtr_base = gdtr_base;
        descriptor_state.gdtr_limit = gdtr_limit;
        descriptor_state.ldtr_selector = ldtr_selector;
        descriptor_state.ldtr_cache = ldtr_cache;
        let code_address =
            match x86_far_jump_descriptor_address(&descriptor_state, return_selector, 8) {
                Ok(address) => address,
                Err(fault) => {
                    request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                    return Ok(());
                }
            };
        let code_raw = read_smir_u64(memory, code_address)?;
        let target_state = match decode_x86_far_return_code(
            return_selector,
            code_raw,
            return_offset,
            *offset_width,
            cpl,
            false,
        ) {
            Ok(target) => target,
            Err(fault) => {
                request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                return Ok(());
            }
        };
        let target_cpl = (target_state.segment.selector & 3) as u8;
        let outer = target_cpl > cpl;

        let (final_rsp, stack_descriptor) = if !outer {
            if let Err(fault) = validate_x86_far_call_target_offset(&target_state) {
                request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                return Ok(());
            }
            (
                initial_rsp
                    .wrapping_add(width * 2)
                    .wrapping_add(u64::from(*pop_bytes)),
                None,
            )
        } else {
            let frame_size = width * 4 + u64::from(*pop_bytes);
            if !stack_range_is_canonical(initial_rsp, frame_size) {
                ctx.request_exit(ExitReason::StackSegment {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            let outer_rsp_address = initial_rsp
                .wrapping_add(width * 2)
                .wrapping_add(u64::from(*pop_bytes));
            let loaded_rsp =
                self.load_memory(memory, outer_rsp_address, mem_width, SignExtend::Zero)?;
            let outer_ss_address = outer_rsp_address.wrapping_add(width);
            let return_ss =
                self.load_memory(memory, outer_ss_address, mem_width, SignExtend::Zero)? as u16;

            let (stack_raw, stack_address) = if return_ss & 0xFFFC == 0 {
                (None, None)
            } else {
                let address = match x86_far_jump_descriptor_address(&descriptor_state, return_ss, 8)
                {
                    Ok(address) => address,
                    Err(fault) => {
                        request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                        return Ok(());
                    }
                };
                (Some(read_smir_u64(memory, address)?), Some(address))
            };
            let stack_state = match decode_x86_far_return_stack(
                return_ss,
                stack_raw,
                target_cpl,
                target_state.segment.l,
            ) {
                Ok(stack) => stack,
                Err(X86SystemDescriptorFault::SegmentNotPresent { error_code }) => {
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

            // Intel specifies SS validation before target-limit and
            // canonicality checks for an outer-privilege return.
            if let Err(fault) = validate_x86_far_call_target_offset(&target_state) {
                request_x86_descriptor_fault(ctx, op.guest_pc, fault);
                return Ok(());
            }
            let final_rsp = outer_rsp(
                loaded_rsp,
                target_state.segment.l,
                stack_state.segment.db,
                *pop_bytes,
            );
            let new_stack = Some((
                stack_address.map(|address| {
                    (
                        address,
                        stack_raw.expect("nonnull far-RET SS descriptor was read"),
                    )
                }),
                stack_state,
            ));
            (final_rsp, new_stack)
        };

        let code_write = code_raw != target_state.accessed_low;
        let stack_write = stack_descriptor
            .as_ref()
            .and_then(|(descriptor, stack)| {
                descriptor
                    .as_ref()
                    .map(|(address, raw)| (*address, *raw, stack))
            })
            .and_then(|(address, raw, stack)| {
                stack
                    .accessed_low
                    .filter(|accessed| *accessed != raw)
                    .map(|accessed| (address, accessed))
            });
        // Descriptor accessed-bit transitions are the final faulting
        // actions. Probe both before the first store so architectural
        // state and either descriptor remain unchanged on a fault.
        if code_write {
            memory.probe(code_address, 8, true)?;
        }
        if let Some((address, _)) = stack_write {
            memory.probe(address, 8, true)?;
        }
        if code_write {
            memory.write(code_address, &target_state.accessed_low.to_le_bytes())?;
        }
        if let Some((address, accessed)) = stack_write {
            memory.write(address, &accessed.to_le_bytes())?;
        }

        let target_offset = target_state.offset;
        let target_selector = target_state.segment.selector;
        let target_l = target_state.segment.l;
        let target_cache = x86_system_segment_cache(&target_state.segment);
        let new_ss = stack_descriptor.map(|(_, stack)| {
            (
                stack.segment.selector,
                x86_system_segment_cache(&stack.segment),
            )
        });
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!("validated x86 far-RET context changed")
        };
        if let Some((selector, cache)) = new_ss {
            x86.ss_selector = selector;
            x86.ss_cache = cache;
        }
        x86.gpr[4] = final_rsp;
        x86.cs_selector = target_selector;
        x86.cs_l = target_l;
        x86.cs_cache = target_cache;
        x86.cpl = target_cpl;
        x86.rip = target_offset;
        if outer {
            invalidate_data_segment(&mut x86.es_selector, &mut x86.es_cache, target_cpl);
            invalidate_data_segment(&mut x86.ds_selector, &mut x86.ds_cache, target_cpl);
            invalidate_data_segment(&mut x86.fs_selector, &mut x86.fs_cache, target_cpl);
            invalidate_data_segment(&mut x86.gs_selector, &mut x86.gs_cache, target_cpl);
        }
        Ok(())
    }
}
