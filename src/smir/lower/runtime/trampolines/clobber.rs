//! trampolines::clobber tests

use super::*;
use crate::smir::ir::ops::{
    X86LmswSource, X86MonitorMwaitOp, X86SelectorVerifySource, X86SmswTarget,
    X86SystemSelectorSource, X86SystemSelectorTarget,
};
use crate::smir::lower::runtime::*;
use crate::smir::lower::x86_64::{
    x86_cli_shape_valid, x86_clts_shape_valid, x86_far_call_shape_valid,
    x86_far_call_terminal_shape_valid, x86_far_jump_shape_valid, x86_far_jump_terminal_shape_valid,
    x86_far_return_shape_valid, x86_far_return_terminal_shape_valid, x86_lmsw_shape_valid,
    x86_read_control_shape_valid, x86_read_debug_shape_valid, x86_selector_verify_shape_valid,
    x86_smsw_shape_valid, x86_sti_shape_valid, x86_system_selector_load_shape_valid,
    x86_system_selector_store_shape_valid, x86_write_control_shape_valid,
    x86_write_debug_shape_valid,
};

/// Admit only scalar MMU-helper transfers that the x86-64 state-backed
/// lowerer can reconstruct without allocator-owned values. The subsequent
/// generic clobber checks reject RSP/RBP destinations; those registers remain
/// valid address components and store sources because helpers read them from
/// `GuestRegs` rather than the host stack/frame registers.
pub(crate) fn x86_jit_scalar_mem_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, MemWidth, VReg};

    let state_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let scalar_width = |width: &MemWidth| {
        matches!(
            width,
            MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8
        )
    };

    match op {
        OpKind::Load {
            dst, addr, width, ..
        } => state_gpr(dst) && scalar_width(width) && x86_jit_mem_address_shape_valid(addr),
        OpKind::Store { src, addr, width } => {
            (state_gpr(src) || matches!(src, VReg::Imm(_)))
                && scalar_width(width)
                && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    }
}

/// Decide whether a lifted function is safe to execute through the native tier
/// under the 1:1 identity register map.
///
/// The identity map (guest GPR `N` ⇒ host GPR `N`) is what makes native
/// execution marshal-free, but it leaves *every* host GPR holding live guest
/// state — there is no free scratch register. So any value the block writes to a
/// `VReg::Virtual` (a non-architectural temporary the lifter introduced) would
/// be allocated onto a guest-occupied host register and silently corrupt guest
/// state on write-back. Such a block must NOT be promoted; the interpreter runs
/// it instead.
///
/// Exemptions are virtual values that the lowerer proves it never materializes:
/// a trailing `TestCondition` whose `dst` feeds the block's `CondBranch`, and
/// (when MMU helpers are enabled) single-use temporaries in exact x86 memory
/// source pairs, MMX `MASKMOVQ`, or the pre-decrement RSP snapshot in PUSH RSP.
/// The former folds to a direct `Jcc`; the memory forms fold to exact
/// helper-backed operations.
///
/// Pure architectural-register blocks (counter/pointer loops, ALU chains,
/// guest-conditional branches) pass — which is the bulk of hot code. Validated
/// Validated MOV/ADD/SUB forms involving guest RSP/RBP use their state-backed
/// slots rather than the corresponding host stack/frame registers.
pub fn is_native_clobber_safe(func: &crate::smir::ir::SmirFunction) -> bool {
    is_native_clobber_safe_excluding(func, &std::collections::HashMap::new(), false)
}
/// Like [`is_native_clobber_safe`] but skips blocks in `excluded` (block-id ⇒
/// resume PC, i.e. the native-exit stubs). Those blocks are lowered to exit
/// stubs and never execute natively, so their ops can't clobber guest state —
/// excluding them lets the JIT accept regions whose loop is clobber-safe even
/// when an exit/continuation block uses a virtual temporary.
pub fn is_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    let flag_live_in = x86_flag_live_in(func, excluded);
    func.blocks
        .iter()
        .filter(|b| !excluded.contains_key(&b.id))
        .all(|b| {
            let flags_live_out = x86_block_flag_live_out(b, excluded, &flag_live_in);
            block_is_clobber_safe(b, &func.x86_instruction_bytes, allow_mem, flags_live_out)
        })
}
/// True if every op in `block` is safe to execute natively under the JIT:
///   (1) it is on the fail-safe register-only whitelist (`SmirOp::is_jit_safe`)
///       — so it touches no memory and is validated bit-exact vs KVM; and
///   (2) it writes only architectural registers (no virtual temp, which would
///       alias a guest GPR under the identity register map).
/// A trailing `TestCondition` feeding the block's `CondBranch`, exact
/// helper-backed scalar/CRC memory sequences, and exact POP/PUSH stack
/// temporaries are exempt because the lowerer never materializes their virtual
/// destinations.
pub(crate) fn block_is_clobber_safe(
    block: &crate::smir::ir::SmirBlock,
    x86_instruction_bytes: &std::collections::HashMap<
        (
            crate::smir::ir::types::BlockId,
            crate::smir::ir::types::GuestAddr,
        ),
        crate::smir::ir::X86InstructionBytes,
    >,
    allow_mem: bool,
    flags_live_out: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    // A native host trap cannot stand in for a guest architectural exception:
    // it would signal the emulator process rather than producing an exact
    // guest exit. Frontier blocks explicitly listed in `excluded` never reach
    // this function, so rejecting terminal traps here does not constrain the
    // existing native-exit mechanism.
    if matches!(
        block.terminator,
        Terminator::Trap { .. } | Terminator::Unreachable
    ) {
        return false;
    }

    // The native trampoline runs the region on the HOST stack: guest RSP is
    // never loaded into the host RSP, and the lowerer's prologue repurposes RBP
    // as the frame pointer. Validated MOV forms use GuestRegs slots and keep the
    // prologue's saved guest RBP coherent; other RSP/RBP operations would still
    // compute against host stack state and therefore remain ineligible.
    let touches_sp_bp = |v: &VReg| {
        matches!(
            v,
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)) | VReg::Arch(ArchReg::X86(X86Reg::Rbp))
        )
    };

    let n = block.ops.len();
    let far_control_count = block
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op.kind,
                OpKind::X86FarJump(..) | OpKind::X86FarCall(..) | OpKind::X86FarReturn(..)
            )
        })
        .count();
    if far_control_count != 0
        && (far_control_count != 1
            || !(x86_far_jump_terminal_shape_valid(block)
                || x86_far_call_terminal_shape_valid(block)
                || x86_far_return_terminal_shape_valid(block)))
    {
        return false;
    }
    let native_replay_spans =
        crate::smir::ir::x86_evex_native_replay_spans(block, x86_instruction_bytes);
    // Count virtual definitions and uses once. Exact helper-sequence validation
    // then remains O(1) per candidate and the complete gate remains O(N).
    let mut virtual_definitions = std::collections::HashMap::new();
    let mut virtual_uses = std::collections::HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *virtual_definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *virtual_uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }

    // Generic flag-suppressed ADC/SBB lowering cannot retain a live carry and
    // is rejected by `x86_block_preserves_live_flags`. An exact helper-backed
    // memory RMW sequence is different: its compute is wrapped by PUSHFQ/POPFQ
    // and its post-store replay consumes the same incoming carry. Exempt only
    // that validated compute index from the generic clobber check.
    let mut preserved_clobber_exceptions = std::collections::HashSet::new();
    let mut scan = 0;
    while scan < n {
        if let Some(consumed) = x86_jit_mem_alu_rmw_sequence_len(
            block,
            scan,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            preserved_clobber_exceptions.insert(scan + 1);
            scan += consumed;
        } else {
            scan += 1;
        }
    }
    if !x86_block_preserves_live_flags(block, flags_live_out, &preserved_clobber_exceptions) {
        return false;
    }

    let mut i = 0;
    while i < n {
        if let Some(span) = native_replay_spans.get(&i) {
            i = span.end;
            continue;
        }
        if let Some(consumed) = x86_jit_maskmovdqu_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mmx_maskmovq_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mmx_scalar_memory_transfer_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mmx_memory_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_shift_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_unary_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_alu_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_update_rmw_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_alu_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_cmove_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_extend_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_widening_mul_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_unsigned_div_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_signed_div_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_high_byte_unsigned_div_source_sequence_len(
            block,
            i,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_high_byte_signed_div_source_sequence_len(
            block,
            i,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_test_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_bit_scan_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) = x86_jit_mem_count_source_sequence_len(
            block,
            i,
            allow_mem,
            &virtual_definitions,
            &virtual_uses,
        ) {
            i += consumed;
            continue;
        }
        if let Some(consumed) =
            x86_jit_pop2_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_pop2_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_push2_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_push2_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_pop_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_pop_candidate(block, i) {
            return false;
        }
        if let Some(consumed) =
            x86_jit_push_sequence_len(block, i, allow_mem, &virtual_definitions, &virtual_uses)
        {
            i += consumed;
            continue;
        }
        if x86_jit_push_candidate(block, i) {
            return false;
        }
        if x86_mem_crc32_pair_valid(block, i, allow_mem, &virtual_definitions, &virtual_uses) {
            i += 2;
            continue;
        }
        let op = &block.ops[i];
        if i + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    i += 1;
                    continue;
                }
            }
        }
        // (1) fail-safe whitelist: any non-whitelisted op (div, general FP/SIMD,
        // syscall, unvalidated) makes the whole region ineligible. When memory
        // JIT is enabled, register-destination Load/Store are additionally
        // allowed (they lower to MMU helper calls with fault-bail); RMW forms
        // still bail via the virtual-temp check below, and RSP/RBP-based
        // addresses via check (3). Explicitly admitted x86 scalar/vector
        // families receive exact shape checks below and, where needed, separate
        // host-feature gates before execution.
        let cldemote_ok = matches!(
            &op.kind,
            OpKind::X86CacheControl {
                addr,
                kind: crate::smir::ir::ops::X86CacheControlKind::Cldemote,
            } if x86_jit_mem_address_shape_valid(addr)
        );
        let alignment_ok = matches!(
            &op.kind,
            OpKind::X86CheckAlignment { addr, alignment }
                if matches!(alignment, 16 | 32 | 64) && x86_jit_mem_address_shape_valid(addr)
        );
        let vector_mem_ok = allow_mem && x86_jit_vector_mem_shape_valid(&op.kind);
        let mmx_mem_ok = allow_mem && x86_jit_mmx_mem_shape_valid(op);
        let stack_mov_ok = x86_state_backed_stack_mov_valid(&op.kind);
        let stack_alu_ok = x86_state_backed_stack_alu_valid(&op.kind);
        let state_extend_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_extend_valid(op);
        let state_cmove_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_cmove_valid(op);
        let state_setcc_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_setcc_valid(op);
        let state_not_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_not_valid(op);
        let state_neg_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_neg_valid(op);
        let state_inc_dec_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_inc_dec_valid(op);
        let state_rotate_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_rotate_valid(op);
        let state_shift_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_shift_valid(op);
        let state_carry_rotate_ok =
            crate::smir::lower::x86_64::x86_state_backed_gpr_carry_rotate_valid(op);
        let state_double_shift_ok =
            crate::smir::lower::x86_64::x86_state_backed_gpr_double_shift_valid(op);
        let state_count_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_count_valid(op);
        let state_bit_scan_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_bit_scan_valid(op);
        let state_bit_test_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_bit_test_valid(op);
        let state_crc32_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_crc32_valid(op);
        let state_and_not_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_and_not_valid(op);
        let state_bextr_bzhi_ok =
            crate::smir::lower::x86_64::x86_state_backed_gpr_bextr_bzhi_valid(op);
        let state_bls_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_bls_valid(op);
        let state_adx_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_adx_valid(op);
        let state_pdep_pext_ok =
            crate::smir::lower::x86_64::x86_state_backed_gpr_pdep_pext_valid(op);
        let state_bswap_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_bswap_valid(op);
        let state_xchg_ok = crate::smir::lower::x86_64::x86_state_backed_gpr_xchg_valid(op);
        let fsgsbase_ok = crate::smir::lower::x86_64::x86_fsgsbase_shape_valid(&op.kind);
        let read_control_ok = x86_read_control_shape_valid(&op.kind);
        let smsw_ok = match &op.kind {
            OpKind::X86Smsw(smsw) if x86_smsw_shape_valid(&op.kind) => match &smsw.target {
                X86SmswTarget::Register { .. } => true,
                X86SmswTarget::Memory { addr } => {
                    allow_mem && x86_jit_mem_address_shape_valid(addr)
                }
            },
            _ => false,
        };
        let selector_store_ok = match &op.kind {
            OpKind::X86SystemSelectorStore(store) if x86_system_selector_store_shape_valid(op) => {
                match &store.target {
                    X86SystemSelectorTarget::Register { .. } => true,
                    X86SystemSelectorTarget::Memory { addr } => {
                        allow_mem && x86_jit_mem_address_shape_valid(addr)
                    }
                    X86SystemSelectorTarget::Stack { .. } => allow_mem,
                }
            }
            _ => false,
        };
        let selector_load_ok = match &op.kind {
            OpKind::X86SystemSelectorLoad(load) if x86_system_selector_load_shape_valid(op) => {
                allow_mem
                    && match &load.source {
                        X86SystemSelectorSource::Register { .. } => true,
                        X86SystemSelectorSource::Memory { addr, .. } => {
                            x86_jit_mem_address_shape_valid(addr)
                        }
                        X86SystemSelectorSource::Stack { .. } => true,
                        X86SystemSelectorSource::FarPointer { addr, .. } => {
                            x86_jit_mem_address_shape_valid(addr)
                        }
                    }
            }
            _ => false,
        };
        let selector_verify_ok = match &op.kind {
            OpKind::X86SelectorVerify(verify) if x86_selector_verify_shape_valid(op) => {
                allow_mem
                    && match &verify.source {
                        X86SelectorVerifySource::Register { .. } => true,
                        X86SelectorVerifySource::Memory { addr, .. } => {
                            x86_jit_mem_address_shape_valid(addr)
                        }
                    }
            }
            _ => false,
        };
        let lmsw_ok = match &op.kind {
            OpKind::X86Lmsw(lmsw) if x86_lmsw_shape_valid(op) => match &lmsw.source {
                X86LmswSource::Register { .. } => true,
                X86LmswSource::Memory { addr } => {
                    allow_mem && x86_jit_mem_address_shape_valid(addr)
                }
            },
            _ => false,
        };
        let descriptor_store_ok = match &op.kind {
            OpKind::X86DescriptorTableStore(store)
                if crate::smir::lower::x86_64::x86_descriptor_table_store_shape_valid(op) =>
            {
                allow_mem && x86_jit_mem_address_shape_valid(&store.addr)
            }
            _ => false,
        };
        let descriptor_load_ok = match &op.kind {
            OpKind::X86DescriptorTableLoad(load)
                if crate::smir::lower::x86_64::x86_descriptor_table_load_shape_valid(op) =>
            {
                allow_mem && x86_jit_mem_address_shape_valid(&load.addr)
            }
            _ => false,
        };
        let far_jump_ok = matches!(
            &op.kind,
            OpKind::X86FarJump(jump)
                if allow_mem
                    && x86_far_jump_shape_valid(op)
                    && x86_jit_mem_address_shape_valid(&jump.addr)
        );
        let far_call_ok = matches!(
            &op.kind,
            OpKind::X86FarCall(call)
                if allow_mem
                    && x86_far_call_shape_valid(op)
                    && x86_jit_mem_address_shape_valid(&call.addr)
        );
        let far_return_ok = allow_mem && x86_far_return_shape_valid(op);
        let read_debug_ok = x86_read_debug_shape_valid(&op.kind);
        let write_control_ok = x86_write_control_shape_valid(op);
        let write_debug_ok = x86_write_debug_shape_valid(&op.kind);
        let cli_ok = x86_cli_shape_valid(op);
        let sti_ok = x86_sti_shape_valid(op);
        let swapgs_ok = crate::smir::lower::x86_64::x86_swapgs_shape_valid(&op.kind);
        let monitor_mwait_ok = match &op.kind {
            OpKind::X86MonitorMwait(X86MonitorMwaitOp { addr, .. })
                if crate::smir::lower::x86_64::x86_monitor_mwait_shape_valid(&op.kind) =>
            {
                addr.as_ref().map_or(true, |addr| {
                    allow_mem && x86_jit_mem_address_shape_valid(addr)
                })
            }
            _ => false,
        };
        let pkru_ok = crate::smir::lower::x86_64::x86_pkru_shape_valid(&op.kind);
        let stack_state_ok = stack_mov_ok
            || stack_alu_ok
            || state_extend_ok
            || state_cmove_ok
            || state_setcc_ok
            || state_not_ok
            || state_neg_ok
            || state_inc_dec_ok
            || state_rotate_ok
            || state_shift_ok
            || state_carry_rotate_ok
            || state_double_shift_ok
            || state_count_ok
            || state_bit_scan_ok
            || state_bit_test_ok
            || state_crc32_ok
            || state_and_not_ok
            || state_bextr_bzhi_ok
            || state_bls_ok
            || state_adx_ok
            || state_pdep_pext_ok
            || state_bswap_ok
            || state_xchg_ok
            || fsgsbase_ok
            || read_control_ok
            || smsw_ok
            || selector_store_ok
            || selector_load_ok
            || selector_verify_ok
            || lmsw_ok
            || descriptor_store_ok
            || descriptor_load_ok
            || read_debug_ok
            || write_control_ok
            || write_debug_ok;
        if (crate::smir::lower::x86_64::x86_state_backed_gpr_extend_candidate(op)
            && !state_extend_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_cmove_candidate(op)
                && !state_cmove_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_setcc_candidate(op)
                && !state_setcc_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_not_candidate(op) && !state_not_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_neg_candidate(op) && !state_neg_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_inc_dec_candidate(op)
                && !state_inc_dec_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_rotate_candidate(op)
                && !state_rotate_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_shift_candidate(op)
                && !state_shift_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_carry_rotate_candidate(op)
                && !state_carry_rotate_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_double_shift_candidate(op)
                && !state_double_shift_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_count_candidate(op)
                && !state_count_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_bit_scan_candidate(op)
                && !state_bit_scan_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_bit_test_candidate(op)
                && !state_bit_test_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_crc32_candidate(op)
                && !state_crc32_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_and_not_candidate(op)
                && !state_and_not_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_bextr_bzhi_candidate(op)
                && !state_bextr_bzhi_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_bls_candidate(op) && !state_bls_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_adx_candidate(op) && !state_adx_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_pdep_pext_candidate(op)
                && !state_pdep_pext_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_bswap_candidate(op)
                && !state_bswap_ok)
            || (crate::smir::lower::x86_64::x86_state_backed_gpr_xchg_candidate(op)
                && !state_xchg_ok)
        {
            return false;
        }
        let unsigned_div_ok = x86_jit_unsigned_div_register_shape_valid(op);
        let signed_div_ok = x86_jit_signed_div_register_shape_valid(op);
        let guarded_div_ok = unsigned_div_ok || signed_div_ok;
        let mem_ok = (allow_mem && x86_jit_scalar_mem_shape_valid(&op.kind))
            || cldemote_ok
            || alignment_ok
            || vector_mem_ok
            || mmx_mem_ok
            || descriptor_store_ok
            || descriptor_load_ok
            || far_jump_ok
            || far_call_ok
            || far_return_ok
            || selector_load_ok
            || selector_verify_ok
            || matches!(
                &op.kind,
                OpKind::X86SystemSelectorStore(store)
                    if selector_store_ok
                        && matches!(
                            &store.target,
                            X86SystemSelectorTarget::Memory { .. }
                                | X86SystemSelectorTarget::Stack { .. }
                        )
            );
        let scalar_ok = matches!(
            op.kind,
            OpKind::AndNot { .. }
                | OpKind::X86Bls { .. }
                | OpKind::X86Adx { .. }
                | OpKind::X86XTest
        ) || guarded_div_ok;
        let vector_ok = x86_native_vector_smir_op(op);
        let mmx_ok = is_x86_native_mmx_op(op) || mmx_mem_ok;
        if !op.is_jit_safe() && !mem_ok && !scalar_ok && !vector_ok && !mmx_ok {
            return false;
        }
        if x86_movx_uses_ambiguous_high_byte_source(op) {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg))
            && !x86_legacy_high_byte_movx_shape_valid(op)
        {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::Mulx)) && !x86_mulx_shape_valid(op) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::MulU {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            } | OpKind::MulS {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            }
        ) && !x86_word_full_mul_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bsf { .. } | OpKind::Bsr { .. })
            && !x86_bit_scan_shape_valid(&op.kind)
            && !state_bit_scan_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::Crc32C { .. })
            && !x86_crc32_shape_valid(&op.kind)
            && !state_crc32_ok
        {
            return false;
        }
        if let OpKind::X86ReadPid { dst } = &op.kind {
            if !x86_native_rdpid_gpr(dst) {
                return false;
            }
        }
        if matches!(op.kind, OpKind::X86Random { .. }) && !x86_random_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86CacheControl { .. }) && !cldemote_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86CheckAlignment { .. }) && !alignment_ok {
            return false;
        }
        if matches!(op.kind, OpKind::VLoad { .. } | OpKind::VStore { .. })
            && !vector_mem_ok
            && !mmx_mem_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86XGetBv { .. }) && !x86_xgetbv_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86Clts) && !x86_clts_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86Msr(..))
            && !crate::smir::lower::x86_64::x86_msr_shape_valid(op)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86ReadControl { .. }) && !read_control_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Smsw(..)) && !smsw_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86SystemSelectorStore(..)) && !selector_store_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86SystemSelectorLoad(..)) && !selector_load_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86SelectorVerify(..)) && !selector_verify_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Lmsw(..)) && !lmsw_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86DescriptorTableStore(..)) && !descriptor_store_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86DescriptorTableLoad(..)) && !descriptor_load_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86FarJump(..)) && !far_jump_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86FarCall(..)) && !far_call_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86FarReturn(..)) && !far_return_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86ReadDebug { .. }) && !read_debug_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86WriteControl { .. }) && !write_control_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86WriteDebug { .. }) && !write_debug_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Cli { .. }) && !cli_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Sti { .. }) && !sti_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Cpuid { .. })
            && !crate::smir::lower::x86_64::x86_cpuid_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86ReadTsc(..))
            && !crate::smir::lower::x86_64::x86_read_tsc_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86ReadPmc(..))
            && !crate::smir::lower::x86_64::x86_read_pmc_shape_valid(op)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86FsGsBase { .. }) && !fsgsbase_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86SwapGs { .. }) && !swapgs_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86MonitorMwait(..)) && !monitor_mwait_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86Pkru { .. }) && !pkru_ok {
            return false;
        }
        if matches!(op.kind, OpKind::X86XSetBv { .. }) {
            if !x86_xsetbv_shape_valid(&op.kind)
                || !block.ops[i + 1..]
                    .iter()
                    .any(|next| next.guest_pc != op.guest_pc)
            {
                return false;
            }
        }
        if let OpKind::Fence { kind } = &op.kind {
            if !matches!(
                *kind,
                crate::smir::ir::types::FenceKind::LoadLoad
                    | crate::smir::ir::types::FenceKind::Full
                    | crate::smir::ir::types::FenceKind::StoreStore
                    | crate::smir::ir::types::FenceKind::InstructionSerialize
            ) {
                return false;
            }
        }
        if matches!(
            op.kind,
            OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. }
        ) && !x86_bit_test_shape_valid(&op.kind)
            && !state_bit_test_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::Cwd { .. }) && !x86_cwd_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Rcl { .. } | OpKind::Rcr { .. })
            && !x86_carry_rotate_shape_valid(&op.kind)
            && !state_carry_rotate_ok
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::AndNot { .. }
                | OpKind::Bextr { .. }
                | OpKind::Bzhi { .. }
                | OpKind::X86Bls { .. }
                | OpKind::Pdep { .. }
                | OpKind::Pext { .. }
        ) && !x86_bmi_shape_valid(&op.kind)
            && !state_and_not_ok
            && !state_bextr_bzhi_ok
            && !state_bls_ok
            && !state_pdep_pext_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86Adx { .. })
            && !x86_adx_shape_valid(&op.kind)
            && !state_adx_ok
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Clz { .. }
                | OpKind::Ctz { .. }
                | OpKind::Popcnt { .. }
                | OpKind::X86Count { .. }
        ) && !x86_count_shape_valid(&op.kind)
            && !state_count_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bswap { .. })
            && !x86_bswap_shape_valid(&op.kind)
            && !state_bswap_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::Xchg { .. })
            && !x86_xchg_shape_valid(&op.kind)
            && !state_xchg_ok
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86NddDoubleShift { .. })
            && !x86_ndd_double_shift_shape_valid(&op.kind)
            && !state_double_shift_ok
        {
            return false;
        }
        // (2) no virtual-temp writes (would clobber a guest GPR).
        if op
            .kind
            .dests()
            .iter()
            .any(|d| matches!(d, VReg::Virtual(_)))
        {
            return false;
        }
        // (3) guest RSP/RBP. Validated MOV/MOVX/CMOV/SETcc/NOT/NEG/INC/DEC/ROL/ROR/RCL/RCR/SHL/SHR/SAR/SHLD/SHRD (including APX NDD)/count/bit-scan/bit-test/CRC32/BMI/ADX/PDEP/PEXT/BSWAP/XCHG/ADD/SUB reads/writes are state-backed.
        // Other writes are not modeled and bail. A read is additionally valid
        // as an operand of a mem-JIT Load/Store (an address base/index, or a stored value): the MMU
        // helper reads the value from the GuestRegs struct — the current guest
        // RSP/RBP — not the host RSP/RBP. CLDEMOTE is also safe because
        // its architecturally ignorable hint never materializes the address;
        // X86CheckAlignment snapshots live GPRs and computes from GuestRegs.
        // Any OTHER op reading RSP/RBP would use the host frame pointer / host
        // stack (wrong) → bail.
        if !stack_state_ok && op.kind.dests().iter().any(touches_sp_bp) {
            return false;
        }
        if !mem_ok
            && !stack_state_ok
            && !guarded_div_ok
            && op.kind.source_vregs().iter().any(touches_sp_bp)
        {
            return false;
        }
        i += 1;
    }
    true
}
pub(crate) fn x86_native_op_would_clobber_preserved_flags(
    op: &crate::smir::ir::ops::OpKind,
) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::Adc {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Sbb {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shld {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shrd {
            flags: FlagUpdate::None,
            ..
        }
    )
}
