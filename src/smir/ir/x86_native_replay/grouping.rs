//! Shared single-pass grouping and fail-closed semantic-shape validation for
//! exact x86 source-byte replay.

use std::collections::HashMap;

use super::classifiers;
use super::{X86InstructionBytes, X86NativeReplaySpan};
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, Terminator};

fn count_virtual(map: &mut HashMap<VReg, usize>, reg: VReg) {
    if matches!(reg, VReg::Virtual(_)) {
        *map.entry(reg).or_insert(0) += 1;
    }
}

/// Count every virtual definition and use visible to this basic block. Replay
/// classifiers use these counts to prove that an elided temporary cannot
/// escape its exact semantic group through another operation, a phi, or the
/// terminator. Construction is O(N + P + T) time and O(V) space for N
/// operations, P phi operands, T terminator operands, and V virtual registers.
fn block_virtual_definition_use_counts(
    block: &SmirBlock,
) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for phi in &block.phis {
        count_virtual(&mut definitions, phi.dst);
        for (_, source) in &phi.sources {
            count_virtual(&mut uses, *source);
        }
    }
    for op in &block.ops {
        for destination in op.kind.dests() {
            count_virtual(&mut definitions, destination);
        }
        for source in op.kind.source_vregs() {
            count_virtual(&mut uses, source);
        }
    }
    match &block.terminator {
        Terminator::CondBranch { cond, .. } => count_virtual(&mut uses, *cond),
        Terminator::Switch { index, .. } => count_virtual(&mut uses, *index),
        Terminator::IndirectBranch { target, .. } => count_virtual(&mut uses, *target),
        Terminator::IndirectBranchMem { addr, .. } => {
            for reg in addr.regs() {
                count_virtual(&mut uses, reg);
            }
        }
        Terminator::Return { values } => {
            for value in values {
                count_virtual(&mut uses, *value);
            }
        }
        Terminator::Call { target, args, .. } | Terminator::TailCall { target, args } => {
            for reg in target.regs() {
                count_virtual(&mut uses, reg);
            }
            for argument in args {
                count_virtual(&mut uses, *argument);
            }
        }
        Terminator::Branch { .. } | Terminator::Trap { .. } | Terminator::Unreachable => {}
    }
    (definitions, uses)
}

pub(super) fn x86_native_replay_spans_where(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    classify: impl Fn(&X86InstructionBytes) -> Option<(bool, bool, bool)>,
) -> HashMap<usize, X86NativeReplaySpan> {
    let virtual_counts = std::cell::OnceCell::new();
    let mut groups = HashMap::<GuestAddr, (usize, usize, bool)>::new();
    for (index, op) in block.ops.iter().enumerate() {
        groups
            .entry(op.guest_pc)
            .and_modify(|(_, end, contiguous)| {
                if *end != index {
                    *contiguous = false;
                }
                *end = index + 1;
            })
            .or_insert((index, index + 1, true));
    }

    groups
        .into_iter()
        .filter_map(|(guest_pc, (start, end, contiguous))| {
            // Source replay executes the captured host instruction directly.
            // A register-form encoding must therefore never replace an IR
            // group that can access guest memory or enforce a memory-only
            // alignment fault, even when provenance and IR are malformed.
            if !contiguous
                || block.ops[start..end].iter().any(|op| {
                    op.kind.reads_memory()
                        || op.kind.writes_memory()
                        || matches!(
                            &op.kind,
                            OpKind::X86CheckAlignment { .. } | OpKind::X86CheckAlignmentAc { .. }
                        )
                })
            {
                return None;
            }
            let source_instruction = *instruction_bytes.get(&(block.id, guest_pc))?;
            let high_byte_multiply = source_instruction.legacy_high_byte_multiply_replay();
            let high_byte_crc32 = source_instruction.legacy_high_byte_crc32_replay();
            let high_byte_setcc = source_instruction.legacy_high_byte_setcc_replay();
            let replay_source = high_byte_multiply
                .map(|replay| replay.canonical_instruction)
                .unwrap_or(source_instruction);
            let (instruction, (needs_avx512vl, needs_avx512dq, needs_avx512fp16)) =
                if let Some(requirements) = classify(&replay_source) {
                    (replay_source, requirements)
                } else {
                    let canonical = replay_source.vex_scalar_l1_canonical_l0()?;
                    let requirements = classify(&canonical)?;
                    (canonical, requirements)
                };
            if let Some(replay) = high_byte_multiply {
                let temporary = classifiers::x86_legacy_high_byte_multiply_shape_temporary(
                    &block.ops[start..end],
                    replay,
                )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                if virtual_definitions.get(&temporary) != Some(&1)
                    || virtual_uses.get(&temporary) != Some(&1)
                {
                    return None;
                }
            }
            if let Some(replay) = high_byte_crc32 {
                let temporary = classifiers::x86_legacy_high_byte_crc32_shape_temporary(
                    &block.ops[start..end],
                    replay,
                )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                if virtual_definitions.get(&temporary) != Some(&1)
                    || virtual_uses.get(&temporary) != Some(&1)
                {
                    return None;
                }
            }
            if let Some(replay) = high_byte_setcc {
                let requirements =
                    classifiers::x86_legacy_high_byte_setcc_shape_virtual_requirements(
                        &block.ops[start..end],
                        replay,
                    )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                for (temporary, expected_uses) in requirements {
                    if virtual_definitions.get(&temporary) != Some(&1)
                        || virtual_uses.get(&temporary) != Some(&expected_uses)
                    {
                        return None;
                    }
                }
            }
            if let Some(replay) = source_instruction.legacy_register_aes_replay() {
                let requirements = classifiers::x86_legacy_aes_shape_virtual_requirements(
                    &block.ops[start..end],
                    replay,
                )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                for (temporary, expected_uses) in requirements {
                    if virtual_definitions.get(&temporary) != Some(&1)
                        || virtual_uses.get(&temporary) != Some(&expected_uses)
                    {
                        return None;
                    }
                }
            }
            if let Some(replay) = source_instruction.legacy_register_blend_replay() {
                let requirements = classifiers::x86_legacy_blend_shape_virtual_requirements(
                    &block.ops[start..end],
                    replay,
                )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                for (temporary, expected_definitions, expected_uses) in requirements {
                    if virtual_definitions.get(&temporary) != Some(&expected_definitions)
                        || virtual_uses.get(&temporary) != Some(&expected_uses)
                    {
                        return None;
                    }
                }
            }
            if let Some(replay) = source_instruction.legacy_register_packed_extend_replay() {
                let requirements =
                    classifiers::x86_legacy_packed_extend_shape_virtual_requirements(
                        &block.ops[start..end],
                        replay,
                    )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                for (temporary, expected_definitions, expected_uses) in requirements {
                    if virtual_definitions.get(&temporary) != Some(&expected_definitions)
                        || virtual_uses.get(&temporary) != Some(&expected_uses)
                    {
                        return None;
                    }
                }
            }
            if let Some(replay) = source_instruction.legacy_register_fp_flag_compare_replay()
                && !classifiers::x86_legacy_fp_flag_compare_shape_matches(
                    &block.ops[start..end],
                    replay,
                )
            {
                return None;
            }
            if let Some(replay) = source_instruction.legacy_register_sha_replay() {
                let requirements = classifiers::x86_legacy_sha_shape_virtual_requirements(
                    &block.ops[start..end],
                    replay,
                )?;
                let (virtual_definitions, virtual_uses) =
                    virtual_counts.get_or_init(|| block_virtual_definition_use_counts(block));
                for (temporary, expected_uses) in requirements {
                    if virtual_definitions.get(&temporary) != Some(&1)
                        || virtual_uses.get(&temporary) != Some(&expected_uses)
                    {
                        return None;
                    }
                }
            }
            // VPERMIL2 is VEX encoded but belongs to AMD's XOP feature
            // subset. Its dynamic guest-state guard must remain independently
            // lowered before exact register replay replaces the remaining
            // semantic graph.
            let replay_start = if instruction.is_vex_register_vpermil2() {
                if !matches!(block.ops[start].kind, OpKind::X86RequireXop)
                    || block.ops[start].x86_hint.is_some()
                {
                    return None;
                }
                start.checked_add(1).filter(|candidate| *candidate < end)?
            } else {
                start
            };
            Some((
                replay_start,
                X86NativeReplaySpan {
                    end,
                    instruction,
                    needs_avx512vl,
                    needs_avx512dq,
                    needs_avx512fp16,
                    preserve_mxcsr_de: instruction.evex_register_fp16_widen_preserves_mxcsr_de(),
                },
            ))
        })
        .collect()
}
