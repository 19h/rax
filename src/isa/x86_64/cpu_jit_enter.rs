//! Fault-precise helper-backed native x86 `ENTER` transaction.

use super::{JIT_VERIFY_MEM_LOG_LIMIT, JIT_VERIFY_MEM_TRACE_LIMIT, X86_64Vcpu};
use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::isa::x86_64::memory::AccessType;
use crate::smir::lower::runtime::GuestRegs;

struct EnterUndo {
    chunks: Vec<(u64, Vec<u8>)>,
}

fn observation_buffers_have_capacity(vcpu: &X86_64Vcpu, nesting_level: u32) -> bool {
    let stores = if nesting_level == 0 {
        1
    } else {
        nesting_level as usize + 1
    };
    let reads = nesting_level.saturating_sub(1) as usize;
    let trace_accesses = stores + reads;
    let trace_fits = vcpu.jit_mem_trace.as_ref().is_none_or(|trace| {
        trace
            .len()
            .checked_add(trace_accesses)
            .is_some_and(|end| end <= JIT_VERIFY_MEM_TRACE_LIMIT)
    });
    let log_fits = vcpu.jit_mem_log.as_ref().is_none_or(|log| {
        log.len()
            .checked_add(stores)
            .is_some_and(|end| end <= JIT_VERIFY_MEM_LOG_LIMIT)
    });
    trace_fits && log_fits
}

fn canonical_range(address: u64, size: u32) -> bool {
    address
        .checked_add(u64::from(size) - 1)
        .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last))
}

fn code_range(vcpu: &X86_64Vcpu, address: u64, size: u32) -> bool {
    let last = address + u64::from(size) - 1;
    vcpu.mmu.is_code_page(address) || vcpu.mmu.is_code_page(last)
}

fn rollback_enter(
    vcpu: &mut X86_64Vcpu,
    undo: &[EnterUndo],
    trace_checkpoint: Option<usize>,
    log_checkpoint: Option<usize>,
    mem_record_checkpoint: usize,
) {
    for entry in undo.iter().rev() {
        for (physical_address, old) in entry.chunks.iter().rev() {
            let _ = vcpu.mmu.write_phys(*physical_address, old);
        }
    }
    // A page-table alias may have populated a translation after an earlier
    // ENTER store changed its backing PTE. The physical rollback restores the
    // PTE bytes; discard every translation derived from the transient image.
    vcpu.mmu.flush_tlb();
    vcpu.mmu
        .restore_mem_record_checkpoint(mem_record_checkpoint);
    if let Some(checkpoint) = trace_checkpoint {
        if let Some(trace) = &mut vcpu.jit_mem_trace {
            trace.truncate(checkpoint);
        }
    } else {
        vcpu.jit_mem_trace = None;
    }
    if let Some(checkpoint) = log_checkpoint {
        if let Some(log) = &mut vcpu.jit_mem_log {
            log.truncate(checkpoint);
        }
    } else {
        vcpu.jit_mem_log = None;
    }
}

fn snapshot_store(vcpu: &mut X86_64Vcpu, address: u64, size: u32) -> Option<(u64, EnterUndo)> {
    if code_range(vcpu, address, size)
        || !vcpu
            .mmu
            .write_range_is_plain_ram(address, size as usize, &vcpu.sregs)
    {
        return None;
    }

    let mut old_bytes = [0_u8; 8];
    let mut chunks = Vec::with_capacity(2);
    let mut current = address;
    let mut offset = 0_usize;
    while offset != size as usize {
        let chunk = (size as usize - offset).min((0x1000 - (current & 0xFFF)) as usize);
        let physical = vcpu
            .mmu
            .translate(current, AccessType::Write, &vcpu.sregs)
            .ok()?;
        vcpu.mmu
            .read_phys(physical, &mut old_bytes[offset..offset + chunk])
            .ok()?;
        chunks.push((physical, old_bytes[offset..offset + chunk].to_vec()));
        offset += chunk;
        current = current.checked_add(chunk as u64)?;
    }
    Some((u64::from_le_bytes(old_bytes), EnterUndo { chunks }))
}

fn enter_store(
    vcpu: &mut X86_64Vcpu,
    undo: &mut Vec<EnterUndo>,
    address: u64,
    value: u64,
    size: u32,
) -> bool {
    let Some((old, snapshot)) = snapshot_store(vcpu, address, size) else {
        return false;
    };
    undo.push(snapshot);
    if vcpu.jit_mem_log_active() {
        vcpu.push_jit_mem_log((address, size as u8, old));
    }
    vcpu.write_mem(address, value, size as u8).is_ok()
}

/// Execute one complete long-mode ENTER transaction against ordinary guest
/// RAM. Runtime and temporary space are both O(N), where N is the masked
/// nesting level (0..=31). Zero requests exact direct replay without retaining
/// helper-visible state, trace, undo-log, or memory changes.
pub(super) unsafe extern "C" fn rax_jit_enter(
    state: *mut GuestRegs,
    allocation_size: u32,
    nesting_level: u32,
    width: u32,
    requires_apx: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if allocation_size > u32::from(u16::MAX)
        || nesting_level >= 32
        || !matches!(width, 2 | 8)
        || requires_apx > 1
        || state.efer & (1 << 10) == 0
        || state.cs_l == 0
        || (requires_apx != 0 && state.apx_enabled == 0)
    {
        return 0;
    }
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if vcpu.sregs.efer & (1 << 10) == 0
        || !vcpu.sregs.cs.l
        || (requires_apx != 0 && !vcpu.apx_enabled())
        || !observation_buffers_have_capacity(vcpu, nesting_level)
    {
        return 0;
    }

    let old_rsp = state.gpr[4];
    let old_rbp = state.gpr[5];
    let delta = u64::from(width);
    let mut stack_pointer = old_rsp.wrapping_sub(delta);

    // Validate every deterministic linear range without translating it. Each
    // access is revalidated against current page tables immediately before it
    // occurs; a later failure rolls physical RAM back before direct replay.
    if !canonical_range(stack_pointer, width) || code_range(vcpu, stack_pointer, width) {
        return 0;
    }
    for index in 1..nesting_level {
        let parent = old_rbp.wrapping_sub(u64::from(index) * delta);
        if !canonical_range(parent, width) {
            return 0;
        }
        stack_pointer = stack_pointer.wrapping_sub(delta);
        if !canonical_range(stack_pointer, width) || code_range(vcpu, stack_pointer, width) {
            return 0;
        }
    }
    if nesting_level != 0 {
        stack_pointer = stack_pointer.wrapping_sub(delta);
        if !canonical_range(stack_pointer, width) || code_range(vcpu, stack_pointer, width) {
            return 0;
        }
    }
    let final_rsp = stack_pointer.wrapping_sub(u64::from(allocation_size));
    if !canonical_range(final_rsp, 1) {
        return 0;
    }

    let trace_checkpoint = vcpu.jit_mem_trace.as_ref().map(Vec::len);
    let log_checkpoint = vcpu.jit_mem_log.as_ref().map(Vec::len);
    let mem_record_checkpoint = vcpu.mmu.mem_record_checkpoint();
    let mut undo = Vec::with_capacity(nesting_level as usize + 1);
    stack_pointer = old_rsp.wrapping_sub(delta);
    if !enter_store(vcpu, &mut undo, stack_pointer, old_rbp, width) {
        rollback_enter(
            vcpu,
            &undo,
            trace_checkpoint,
            log_checkpoint,
            mem_record_checkpoint,
        );
        return 0;
    }
    let frame_pointer = stack_pointer;

    for index in 1..nesting_level {
        let parent_address = old_rbp.wrapping_sub(u64::from(index) * delta);
        if !vcpu
            .mmu
            .read_range_is_plain_ram(parent_address, width as usize, &vcpu.sregs)
        {
            rollback_enter(
                vcpu,
                &undo,
                trace_checkpoint,
                log_checkpoint,
                mem_record_checkpoint,
            );
            return 0;
        }
        let Ok(parent) = vcpu.read_mem(parent_address, width as u8) else {
            rollback_enter(
                vcpu,
                &undo,
                trace_checkpoint,
                log_checkpoint,
                mem_record_checkpoint,
            );
            return 0;
        };
        stack_pointer = stack_pointer.wrapping_sub(delta);
        if !enter_store(vcpu, &mut undo, stack_pointer, parent, width) {
            rollback_enter(
                vcpu,
                &undo,
                trace_checkpoint,
                log_checkpoint,
                mem_record_checkpoint,
            );
            return 0;
        }
    }
    if nesting_level != 0 {
        stack_pointer = stack_pointer.wrapping_sub(delta);
        if !enter_store(vcpu, &mut undo, stack_pointer, frame_pointer, width) {
            rollback_enter(
                vcpu,
                &undo,
                trace_checkpoint,
                log_checkpoint,
                mem_record_checkpoint,
            );
            return 0;
        }
    }

    // Repeat the non-writing final probe at its architectural position. This
    // catches a translation changed by an aliased display store; rollback
    // makes direct replay the sole observer of that exceptional execution.
    if vcpu
        .mmu
        .preflight_write_range(final_rsp, 1, &vcpu.sregs)
        .is_err()
    {
        rollback_enter(
            vcpu,
            &undo,
            trace_checkpoint,
            log_checkpoint,
            mem_record_checkpoint,
        );
        return 0;
    }

    state.gpr[4] = final_rsp;
    state.gpr[5] = if width == 2 {
        (old_rbp & !0xFFFF) | (frame_pointer & 0xFFFF)
    } else {
        frame_pointer
    };
    1
}
