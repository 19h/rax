//! Feature accumulation for exact EVEX duplicate-move memory replays.

use std::collections::HashMap;

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::VReg;

use super::vector_replay_features::X86NativeReplayFeatureRequirements;

pub(super) fn accumulate_evex_duplicate_move_memory_requirements(
    block: &SmirBlock,
    index: usize,
    function: &crate::smir::ir::SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::x86_jit_evex_duplicate_move_memory_sequence(
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
    // Every F32/F64 duplicate shape observes at most 16 destination mask bits;
    // this remains vacuously true for an unmasked encoding. KMOVW therefore
    // avoids imposing an AVX512BW dependency on this AVX512F family.
    requirements.has_k16_opmask_span = true;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
