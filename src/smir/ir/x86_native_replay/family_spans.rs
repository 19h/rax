//! Register-only replay-span selectors grouped by instruction family.

use super::*;

/// Identify exact register-only legacy AES-NI semantic groups. The source
/// instruction preserves shared vector state above bit 127, so validated replay
/// uses the AVX YMM0-YMM15 state bridge without requiring AVX-512 state.
/// Construction is O(N) time and O(P + V) space for N operations, P unique
/// guest PCs, and V virtual registers.
pub fn x86_legacy_aes_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_aes_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SHA-NI semantic groups. The source
/// instruction preserves shared vector state above bit 127, so validated
/// replay uses the AVX YMM0-YMM15 state bridge without requiring AVX-512
/// state. Construction is O(N) time and O(P + V) space for N operations, P
/// unique guest PCs, and V virtual registers.
pub fn x86_legacy_sha_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_sha_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE4.1 packed sign/zero-extension
/// semantic groups. Legacy PMOVSX*/PMOVZX* preserve shared vector state above
/// bit 127, so replay uses the AVX YMM0-YMM15 state bridge without requiring
/// AVX-512 state. Construction is O(N) time and O(P + V) space for N
/// operations, P unique guest PCs, and V virtual registers.
pub fn x86_legacy_packed_extend_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_packed_extend_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy `COMISS`, `UCOMISS`, `COMISD`, and
/// `UCOMISD` semantic groups. The source instruction preserves vector state,
/// so validated replay uses the AVX YMM0-YMM15 state bridge without requiring
/// AVX-512 state. Construction is O(N) time and O(P) space for N operations
/// and P unique guest PCs.
pub fn x86_legacy_fp_flag_compare_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_fp_flag_compare_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify baseline scalar register instructions that name AH, CH, DH, or BH
/// and therefore require exact source-byte replay rather than virtual-register
/// materialization under the x86 identity map. Documented Group 2 forms carry
/// a deterministic undefined-status wrapper at lowering. Construction is O(N)
/// time and O(P) space for N operations and P unique guest PCs.
pub fn x86_legacy_high_byte_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .is_legacy_high_byte_register_replay()
            .then_some((false, false, false))
    })
}

/// Identify valid register-only EVEX GFNI replay groups in `block` in O(N)
/// time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_gfni_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_gfni_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only VEX GFNI replay groups in `block` in O(N)
/// time and O(P) space for N operations and P unique guest PCs. Memory forms
/// remain at the precise SMIR interpreter boundary.
pub fn x86_vex_gfni_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .vex_register_gfni_uses_ymm()
            .map(|_| (false, false, false))
    })
}

/// Identify valid register-only EVEX VPCLMULQDQ replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_vpclmulqdq_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_vpclmulqdq_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only VEX VPCLMULQDQ replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs. Memory
/// forms remain at the precise SMIR interpreter boundary.
pub fn x86_vex_vpclmulqdq_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .vex_register_vpclmulqdq_uses_ymm()
            .map(|_| (false, false, false))
    })
}

/// Identify valid register-only EVEX VP2INTERSECTD/Q replay groups in `block`
/// in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_vp2intersect_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_vp2intersect_needs_vl()
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

/// Identify every validated native EVEX replay group in O(N) time and O(P)
/// space while preserving the established EVEX-only API.
pub fn x86_evex_native_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    let mut spans = x86_native_replay_spans(block, instruction_bytes);
    spans.retain(|_, span| span.instruction.as_slice().first() == Some(&0x62));
    spans
}

/// Identify register-only AVX VEX packed-string comparison replay groups in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_vex_packed_string_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .is_vex_register_packed_string_compare()
            .then_some((false, false, false))
    })
}
