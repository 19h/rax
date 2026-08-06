//! Feature aggregation for exact EVEX high/low half-move memory replays.

use std::collections::HashMap;

use crate::smir::ir::types::VReg;
use crate::smir::ir::{SmirBlock, SmirFunction};

use super::X86NativeReplayFeatureRequirements;

/// Accumulate one exact Type-E9NF EVEX.128 high/low half-move memory source.
///
/// The instruction itself requires AVX-512F and not AVX-512VL. XMM16-XMM31
/// operands require the full ZMM/K state bridge, whose implementation also
/// uses AVX-512BW.
#[allow(clippy::too_many_arguments)]
pub(super) fn accumulate_evex_half_move_memory_replay_requirements(
    block: &SmirBlock,
    index: usize,
    func: &SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::super::x86_jit_evex_half_move_memory_sequence(
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
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
