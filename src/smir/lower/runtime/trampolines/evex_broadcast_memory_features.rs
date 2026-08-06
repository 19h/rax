//! Feature accumulation for exact EVEX memory broadcasts.

use std::collections::HashMap;

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::VReg;

use super::vector_replay_features::X86NativeReplayFeatureRequirements;

pub(super) fn accumulate_evex_broadcast_memory_requirements(
    block: &SmirBlock,
    index: usize,
    function: &crate::smir::ir::SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::x86_jit_evex_broadcast_memory_sequence(
        block,
        index,
        true,
        &function.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    requirements.any = true;
    requirements.needs_avx = true;
    requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
    requirements.needs_avx512bw |= sequence.encoding.needs_avx512bw;
    requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
    // Broadcast masks never observe more than one bit per destination lane.
    // Non-BW shapes have at most 16 lanes and can therefore use AVX512F
    // KMOVW for the helper bridge without adding an AVX512BW dependency.
    requirements.has_k16_opmask_span |= sequence.encoding.width.lanes(sequence.encoding.elem) <= 16;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
