//! Feature aggregation for exact EVEX extraction-to-memory replays.

use std::collections::HashMap;

use crate::smir::ir::types::VReg;
use crate::smir::ir::{SmirBlock, SmirFunction};

use super::X86NativeReplayFeatureRequirements;

/// Accumulate one exact scalar-lane or vector-chunk extraction to memory.
///
/// Both forms require the full ZMM/K helper bridge. Classification is O(L)
/// for at most eight chunk lanes and O(V) for the precomputed virtual maps.
#[allow(clippy::too_many_arguments)]
pub(super) fn accumulate_evex_extract_memory_replay_requirements(
    block: &SmirBlock,
    index: usize,
    func: &SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::super::x86_jit_evex_extract_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    requirements.any = true;
    requirements.needs_avx = true;
    // Every source may be ZMM16-ZMM31. The complete ZMM/K helper bridge uses
    // AVX-512BW even when the extraction itself requires only F or DQ.
    requirements.needs_avx512bw = true;
    requirements.needs_avx512vl |= sequence.needs_avx512vl();
    requirements.needs_avx512dq |= sequence.needs_avx512dq();
    requirements.has_k16_opmask_span |=
        sequence.writemask().is_some() && sequence.mask_lanes() <= 16;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed())
}
