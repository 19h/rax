//! Shared feature classification for helper-backed EVEX scalar FP memory spans.

use std::collections::HashMap;

use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86JitEvexScalarFpMemoryFeatureSpan {
    pub(super) consumed: usize,
    pub(super) needs_avx512fp16: bool,
}

/// Recognize either exact scalar floating-point arithmetic or precision-
/// conversion memory replay and return their common feature contract.
#[allow(clippy::too_many_arguments)]
pub(super) fn x86_jit_evex_scalar_fp_memory_feature_span(
    block: &SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpMemoryFeatureSpan> {
    if let Some(sequence) = super::x86_jit_evex_scalar_fp_arithmetic_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(X86JitEvexScalarFpMemoryFeatureSpan {
            consumed: sequence.consumed,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
        });
    }
    let sequence = super::x86_jit_evex_scalar_fp_convert_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexScalarFpMemoryFeatureSpan {
        consumed: sequence.consumed,
        needs_avx512fp16: sequence.encoding.needs_avx512fp16,
    })
}
