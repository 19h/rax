//! Byte-validated native replay metadata for x86 instructions.
//!
//! These classifiers accept exact register-only instruction shapes whose
//! source bytes can safely replace the contiguous semantic SMIR group emitted
//! for the same guest instruction.

use std::collections::HashMap;

use super::SmirBlock;
use super::types::{BlockId, GuestAddr};

/// Exact bytes of one x86 instruction. Architectural x86 instructions are at
/// most 15 bytes; keeping a fixed-size value makes function provenance cheap to
/// clone and prevents metadata from carrying an unbounded byte sequence into a
/// native lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86InstructionBytes {
    bytes: [u8; 15],
    len: u8,
}

impl X86InstructionBytes {
    /// Capture one complete x86 instruction.
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > 15 {
            return None;
        }
        let mut captured = [0u8; 15];
        captured[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            bytes: captured,
            len: bytes.len() as u8,
        })
    }

    /// Return the complete instruction byte sequence.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

mod classifiers;

/// A contiguous semantic-op group that may be replaced by one exact native x86
/// instruction after byte-level validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86NativeReplaySpan {
    /// Exclusive semantic-op end index.
    pub end: usize,
    /// Exact source instruction to emit.
    pub instruction: X86InstructionBytes,
    /// Whether native execution requires AVX-512VL.
    pub needs_avx512vl: bool,
    /// Whether native execution requires AVX-512DQ.
    pub needs_avx512dq: bool,
    /// Whether native execution requires AVX-512-FP16.
    pub needs_avx512fp16: bool,
}

/// Compatibility name for the first replay family.
pub type X86EvexFpReplaySpan = X86NativeReplaySpan;

fn x86_evex_replay_spans_where(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    classify: impl Fn(&X86InstructionBytes) -> Option<(bool, bool, bool)>,
) -> HashMap<usize, X86NativeReplaySpan> {
    let mut groups = HashMap::<GuestAddr, (usize, usize, bool)>::new();
    for (index, op) in block.ops.iter().enumerate() {
        groups
            .entry(op.guest_pc)
            .and_modify(|(_, end, contiguous)| {
                if *end != index {
                    *contiguous = false;
                }
                *end = index + 1;
            })
            .or_insert((index, index + 1, true));
    }

    groups
        .into_iter()
        .filter_map(|(guest_pc, (start, end, contiguous))| {
            if !contiguous {
                return None;
            }
            let instruction = *instruction_bytes.get(&(block.id, guest_pc))?;
            let (needs_avx512vl, needs_avx512dq, needs_avx512fp16) = classify(&instruction)?;
            Some((
                start,
                X86NativeReplaySpan {
                    end,
                    instruction,
                    needs_avx512vl,
                    needs_avx512dq,
                    needs_avx512fp16,
                },
            ))
        })
        .collect()
}

/// Identify valid register-only EVEX floating-point replay groups in `block`.
/// Construction is O(N) time and O(P) space for N SMIR operations and P unique
/// guest PCs. A guest PC occurring in multiple non-contiguous groups is
/// rejected, preventing one source instruction from replacing reordered or
/// fabricated semantic fragments.
pub fn x86_evex_fp_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86EvexFpReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_fp_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX logical replay groups in `block` in O(N)
/// time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_logic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_logic_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX packed integer arithmetic replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_integer_arithmetic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX shared-count shift replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_shared_count_shift_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_shared_count_shift_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX immediate-count shift replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_immediate_count_shift_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_immediate_count_shift_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed FMA replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX scalar FMA replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_scalar_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed binary16 FMA replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_fp16_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_fp16_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX scalar binary16 FMA replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_scalar_fp16_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fp16_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX scalar binary16 arithmetic and
/// square-root replay groups in `block` in O(N) time and O(P) space for N
/// operations and P unique guest PCs.
pub fn x86_evex_scalar_fp16_arithmetic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fp16_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX packed integer min/max replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_integer_minmax_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_minmax_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer multiply replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_integer_multiply_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_multiply_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX packed integer interleave replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_integer_interleave_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_interleave_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX signed/unsigned saturating pack replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_integer_pack_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_pack_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer absolute-value replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_packed_abs_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_abs_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX rounded unsigned packed average replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_packed_average_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_average_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer test replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_test_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_test_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer compare replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_compare_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_compare_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX opmask-selector blend replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_mask_blend_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_mask_blend_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX vector-to-opmask conversion replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_vector_to_mask_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_vector_to_mask_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX opmask-to-vector conversion replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_mask_to_vector_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_mask_to_vector_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX opmask-to-vector broadcast replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_mask_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_mask_broadcast_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX one-source lane-shuffle replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_lane_shuffle_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_lane_shuffle_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX floating shuffle/interleave replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_fp_shuffle_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_fp_shuffle_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only AVX-512F dword/qword full-vector and in-lane
/// permute replay groups in `block` in O(N) time and O(P) space for N
/// operations and P unique guest PCs.
pub fn x86_evex_avx512f_permute_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_avx512f_permute_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed-move replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_move_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_move_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed sign/zero-extension replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_packed_extend_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_extend_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-source EVEX 32/64-bit broadcast replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_broadcast_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-source EVEX byte/word broadcast replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_narrow_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_narrow_broadcast_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid EVEX GPR-source integer broadcast replay groups in `block`
/// in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_gpr_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_gpr_broadcast_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify every validated native EVEX replay group in one O(N)-time,
/// O(P)-space block pass. Classifiers are intentionally disjoint and ordered
/// explicitly so adding a replay family does not add another scan of the SMIR
/// operation stream.
pub fn x86_evex_native_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        if let Some(needs_vl) = instruction.evex_register_fp_arithmetic_needs_vl() {
            return Some((needs_vl, false, false));
        }
        if let Some(requirements) = instruction.evex_register_logic_requirements() {
            return Some((requirements.0, requirements.1, false));
        }
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
            .or_else(|| {
                instruction
                    .evex_register_shared_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_immediate_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_arithmetic_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_minmax_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_multiply_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_interleave_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_pack_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_abs_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_average_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_test_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_compare_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_blend_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_vector_to_mask_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_to_vector_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_lane_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_avx512f_permute_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_move_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_extend_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_broadcast_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_narrow_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_gpr_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
    })
}

#[cfg(test)]
#[path = "x86_native_replay_tests.rs"]
mod tests;
