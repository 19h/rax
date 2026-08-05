//! Host-feature contract for helper-backed EVEX `VFPCLASS*` memory replay.

use std::collections::HashMap;

use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86JitEvexFpClassMemoryFeatureSpan {
    pub(super) consumed: usize,
    pub(super) needs_avx512vl: bool,
    pub(super) needs_avx512dq: bool,
    pub(super) needs_avx512fp16: bool,
}

/// Recognize one exact `VFPCLASS*` memory replay and return its complete host
/// feature contract. The shared full-width ZMM/K bridge separately contributes
/// AVX-512F and AVX-512BW.
#[allow(clippy::too_many_arguments)]
pub(super) fn x86_jit_evex_fp_class_memory_feature_span(
    block: &SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpClassMemoryFeatureSpan> {
    let sequence = super::x86_jit_evex_fp_class_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexFpClassMemoryFeatureSpan {
        consumed: sequence.consumed,
        needs_avx512vl: sequence.encoding.needs_avx512vl,
        needs_avx512dq: sequence.encoding.needs_avx512dq,
        needs_avx512fp16: sequence.encoding.needs_avx512fp16,
    })
}
