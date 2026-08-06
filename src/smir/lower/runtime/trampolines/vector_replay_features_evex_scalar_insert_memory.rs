//! Feature aggregation for exact EVEX scalar-insert memory replays.

use std::collections::HashMap;

use crate::smir::ir::types::VReg;
use crate::smir::ir::{SmirBlock, SmirFunction};

use super::X86NativeReplayFeatureRequirements;

/// Accumulate one exact Type-E9NF scalar insertion from memory.
///
/// The instruction itself requires AVX-512F, BW, or DQ according to its
/// opcode. The full ZMM/K helper bridge conservatively requires AVX-512BW for
/// every form; none of these fixed EVEX.128 forms requires AVX-512VL.
#[allow(clippy::too_many_arguments)]
pub(super) fn accumulate_evex_scalar_insert_memory_replay_requirements(
    block: &SmirBlock,
    index: usize,
    func: &SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::super::x86_jit_evex_scalar_insert_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    requirements.any = true;
    requirements.needs_avx = true;
    requirements.needs_avx512bw = true;
    requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
