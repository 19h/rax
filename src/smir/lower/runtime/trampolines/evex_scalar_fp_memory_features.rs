//! Shared feature classification for helper-backed EVEX scalar FP memory spans.

use std::collections::HashMap;

use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86JitEvexScalarFpMemoryFeatureSpan {
    pub(super) consumed: usize,
    pub(super) needs_avx512bw: bool,
    pub(super) needs_avx512dq: bool,
    pub(super) needs_avx512er: bool,
    pub(super) needs_avx512fp16: bool,
    pub(super) uses_k16_opmasks: bool,
}

/// Recognize an exact scalar floating-point memory replay and return its
/// common feature contract.
#[allow(clippy::too_many_arguments)]
pub(super) fn x86_jit_evex_scalar_fp_memory_feature_span(
    block: &SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpMemoryFeatureSpan> {
    if let Some(sequence) = super::x86_jit_evex_scalar_move_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(X86JitEvexScalarFpMemoryFeatureSpan {
            consumed: sequence.consumed,
            // The native trampoline's complete K0-K7 bridge uses KMOVQ.
            needs_avx512bw: true,
            needs_avx512dq: false,
            needs_avx512er: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            uses_k16_opmasks: false,
        });
    }
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
            needs_avx512bw: true,
            needs_avx512dq: false,
            needs_avx512er: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            uses_k16_opmasks: false,
        });
    }
    if let Some(sequence) = super::x86_jit_evex_fp_flag_compare_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(X86JitEvexScalarFpMemoryFeatureSpan {
            consumed: sequence.consumed,
            // The full XMM0-XMM31 state bridge uses KMOVQ for K0-K7.
            needs_avx512bw: true,
            needs_avx512dq: false,
            needs_avx512er: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            uses_k16_opmasks: false,
        });
    }
    if let Some(sequence) = super::x86_jit_evex_scalar_fp_compare_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(X86JitEvexScalarFpMemoryFeatureSpan {
            consumed: sequence.consumed,
            needs_avx512bw: true,
            needs_avx512dq: false,
            needs_avx512er: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            uses_k16_opmasks: false,
        });
    }
    if let Some(sequence) = super::x86_jit_evex_scalar_fp_convert_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(X86JitEvexScalarFpMemoryFeatureSpan {
            consumed: sequence.consumed,
            needs_avx512bw: true,
            needs_avx512dq: false,
            needs_avx512er: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            uses_k16_opmasks: false,
        });
    }
    let sequence = super::x86_jit_evex_scalar_fp_unary_memory_sequence(
        block,
        index,
        true,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )?;
    let uses_k16_opmasks = matches!(
        sequence.encoding.kind,
        crate::smir::ir::X86EvexScalarFpUnaryMemoryKind::Recip14
            | crate::smir::ir::X86EvexScalarFpUnaryMemoryKind::Rsqrt14
            | crate::smir::ir::X86EvexScalarFpUnaryMemoryKind::Recip28
            | crate::smir::ir::X86EvexScalarFpUnaryMemoryKind::Rsqrt28
    );
    Some(X86JitEvexScalarFpMemoryFeatureSpan {
        consumed: sequence.consumed,
        // VRCP14/VRSQRT14 and VRCP28/VRSQRT28 observe at most 16 mask bits,
        // so their existing narrow KMOVW bridge needs AVX-512F but not BW.
        // FP16 approximation and the other scalar replay families retain the
        // full KMOVQ bridge and therefore require AVX-512BW.
        needs_avx512bw: !uses_k16_opmasks,
        needs_avx512dq: sequence.encoding.needs_avx512dq,
        needs_avx512er: sequence.encoding.needs_avx512er,
        needs_avx512fp16: sequence.encoding.needs_avx512fp16,
        uses_k16_opmasks,
    })
}
