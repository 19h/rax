//! Feature accumulation for exact EVEX `VMOVNTDQA` memory replays.

use std::collections::HashMap;

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::VReg;

use super::vector_replay_features::X86NativeReplayFeatureRequirements;

pub(super) fn accumulate_evex_movntdqa_memory_requirements(
    block: &SmirBlock,
    index: usize,
    function: &crate::smir::ir::SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::x86_jit_evex_movntdqa_memory_sequence(
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
    // VMOVNTDQA is unmasked. This is vacuously a K[15:0]-only span, allowing
    // the AVX512F KMOVW state bridge without imposing AVX512BW.
    requirements.has_k16_opmask_span = true;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
