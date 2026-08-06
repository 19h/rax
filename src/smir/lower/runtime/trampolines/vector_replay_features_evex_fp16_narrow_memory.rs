//! Feature aggregation for EVEX `VCVTPS2PH` memory destinations.

use crate::smir::ir::{SmirBlock, SmirFunction};

use super::X86NativeReplayFeatureRequirements;

/// Accumulate one exact helper-backed EVEX `VCVTPS2PH` memory destination.
///
/// The architectural instruction requires AVX512F, plus AVX512VL for 128- and
/// 256-bit sources. Every form observes at most K[15:0], so the AVX512F KMOVW
/// bridge is exact and neither the bridge nor the instruction requires
/// AVX512BW. The semantic host guard shared with VEX `VCVTPS2PH` excludes hosts
/// that incorrectly flush FP16 denormal results under MXCSR.FTZ. Matching is
/// O(1) time and O(1) space.
pub(super) fn accumulate_evex_fp16_narrow_memory_replay_requirements(
    block: &SmirBlock,
    index: usize,
    func: &SmirFunction,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) -> Option<usize> {
    let sequence = super::super::x86_jit_evex_fp16_narrow_memory_sequence(
        block,
        index,
        true,
        &func.x86_instruction_bytes,
    )?;
    requirements.any = true;
    requirements.needs_avx = true;
    requirements.needs_vex_fp16_narrow = true;
    requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
    requirements.has_k16_opmask_span = true;
    *all_spans_support_avx_ymm16 = false;
    Some(sequence.consumed)
}
