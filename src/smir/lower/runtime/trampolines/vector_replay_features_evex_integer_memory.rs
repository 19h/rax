//! Feature aggregation for exact EVEX packed-integer memory replays.

use std::collections::HashMap;

use crate::smir::ir::types::VReg;
use crate::smir::ir::{SmirBlock, SmirFunction};

use super::X86NativeReplayFeatureRequirements;

/// Accumulate one exact unary-integer, `VPSADBW`, or `VP2INTERSECT` memory replay.
///
/// Both families require the full AVX-512 vector-state bridge and therefore
/// cannot use the AVX YMM0-YMM15 bridge. Matching is O(L), where L is at most
/// 64 packed lanes for the unary family, and uses O(1) auxiliary space.
#[allow(clippy::too_many_arguments)]
pub(super) fn accumulate_evex_integer_memory_replay_requirements(
    block: &SmirBlock,
    index: usize,
    func: &SmirFunction,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    if let Some(sequence) = super::super::x86_jit_evex_integer_unary_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        requirements.any = true;
        requirements.needs_avx = true;
        let lanes = sequence.encoding.width.lanes(sequence.encoding.elem);
        // KMOVW is exact when the operation cannot observe K[63:16]. Wider
        // byte/word forms use the full KMOVQ bridge.
        requirements.needs_avx512bw |= lanes > 16;
        requirements.has_k16_opmask_span |= lanes <= 16;
        requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
        requirements.needs_avx512cd |= sequence.encoding.needs_avx512cd;
        requirements.needs_avx512bitalg |= sequence.encoding.needs_avx512bitalg;
        requirements.needs_avx512vpopcntdq |= sequence.encoding.needs_avx512vpopcntdq;
        *all_spans_support_avx_ymm16 = false;
        return Some(sequence.consumed);
    }

    if let Some(sequence) = super::super::x86_jit_evex_psadbw_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        requirements.any = true;
        requirements.needs_avx = true;
        // VPSADBW and the full-width vector-state bridge require AVX-512BW.
        requirements.needs_avx512bw = true;
        requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
        *all_spans_support_avx_ymm16 = false;
        return Some(sequence.consumed);
    }

    let sequence = super::super::x86_jit_evex_vp2intersect_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    requirements.any = true;
    requirements.needs_avx = true;
    // VP2INTERSECT requires its dedicated extension; the full K0-K7/vector
    // helper bridge additionally uses AVX-512BW instructions.
    requirements.needs_avx512bw = true;
    requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
    requirements.needs_avx512vp2intersect = true;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
