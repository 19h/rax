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

/// Identify exact register-only legacy MMX/SSE packed conversion semantic
/// groups: `CVTPI2PS`, `CVTPS2PI`, `CVTTPS2PI`, `CVTPS2PD`, `CVTPI2PD`,
/// `CVTPD2PI`, `CVTTPD2PI`, and `CVTPD2PS`. Every form observes XMM state and
/// therefore uses the AVX YMM0-YMM15 bridge; the six MMX forms additionally
/// use the independent MMX/x87-tag bridge. Construction is O(N) time and O(P)
/// space for N operations and P unique guest PCs.
pub fn x86_legacy_packed_fp_convert_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_packed_fp_convert_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE/SSE2 scalar conversion semantic
/// groups: `CVTSI2SS`, `CVTSI2SD`, `CVTSS2SI`, `CVTSD2SI`, `CVTTSS2SI`,
/// `CVTTSD2SI`, `CVTSS2SD`, and `CVTSD2SS`, including both integer widths.
/// Every form observes XMM state and therefore uses the AVX YMM0-YMM15 bridge.
/// Construction is O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_legacy_scalar_fp_convert_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_scalar_fp_convert_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-destination legacy MMX/SSE scalar-extract groups:
/// `EXTRACTPS` and `PEXTRB/D/Q/W`. XMM forms use the AVX YMM0-YMM15 state
/// bridge; MMX `PEXTRW` uses the independent MMX/x87-tag bridge and retains
/// its leading `EnterMmx` marker. Construction is O(N) time and O(P + V)
/// space for N operations, P unique guest PCs, and V virtual registers.
pub fn x86_legacy_scalar_extract_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_scalar_extract_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-source legacy MMX/SSE scalar-insert groups:
/// `PINSRB/D/Q/W`. XMM forms use the AVX YMM0-YMM15 state bridge; MMX
/// `PINSRW` uses the independent MMX/x87-tag bridge and retains its leading
/// `EnterMmx` marker. Construction is O(N) time and O(P + V) space for N
/// operations, P unique guest PCs, and V virtual registers.
pub fn x86_legacy_scalar_insert_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_scalar_insert_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE2/SSE3 `MOVDDUP`, `MOVSHDUP`,
/// `MOVSLDUP`, `PSHUFD`, `PSHUFHW`, and `PSHUFLW` semantic groups. These
/// instructions preserve shared vector state above bit 127, so replay uses the
/// AVX YMM0-YMM15 state bridge. Construction is O(N) time and O(P + V) space
/// for N operations, P unique guest PCs, and V virtual registers.
pub fn x86_legacy_lane_shuffle_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_lane_shuffle_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSSE3 XMM `PALIGNR` semantic groups.
/// This instruction preserves shared vector state above bit 127, so replay
/// uses the AVX YMM0-YMM15 state bridge. Construction is O(N) time and
/// O(P + V) space for N operations, P unique guest PCs, and V virtual
/// registers.
pub fn x86_legacy_alignr_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_alignr_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE4.1 `ROUNDPS`, `ROUNDPD`,
/// `ROUNDSS`, and `ROUNDSD` semantic groups. Legacy ROUND preserves shared
/// vector state above bit 127, so replay uses the AVX YMM0-YMM15 bridge and
/// requires SSE4.1 host execution support. Construction is O(N) time and O(P)
/// space for N operations and P unique guest PCs.
pub fn x86_legacy_round_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_round_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE4.1 `DPPS` and `DPPD` semantic
/// groups. Legacy dot products preserve shared vector state above bit 127, so
/// replay uses the AVX YMM0-YMM15 bridge and requires SSE4.1 host execution
/// support. Construction is O(N) time and O(P + V) space for N operations, P
/// unique guest PCs, and V virtual registers.
pub fn x86_legacy_dot_product_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_dot_product_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE4.1 `INSERTPS` semantic groups.
/// Legacy INSERTPS preserves shared vector state above bit 127, so replay uses
/// the AVX YMM0-YMM15 bridge and requires SSE4.1 host execution support.
/// Construction is O(N) time and O(P + V) space for N operations, P unique
/// guest PCs, and V virtual registers.
pub fn x86_legacy_insertps_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_insertps_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy `PCLMULQDQ` semantic groups. The
/// source instruction preserves shared vector state above bit 127, so replay
/// uses the AVX YMM0-YMM15 bridge and requires PCLMULQDQ host execution
/// support. Construction is O(N) time and O(P + V) space for N operations, P
/// unique guest PCs, and V virtual registers.
pub fn x86_legacy_pclmulqdq_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_pclmulqdq_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy SSE4.1 `PTEST` semantic groups. The
/// source instruction reads XMM state without modifying any vector register,
/// so replay uses the AVX YMM0-YMM15 bridge and requires SSE4.1 host execution
/// support. Construction is O(N) time and O(P + V) space for N operations, P
/// unique guest PCs, and V virtual registers.
pub fn x86_legacy_ptest_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_ptest_replay()
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

/// Identify exact register-only legacy SSE2 packed-shift semantic groups.
/// These destructive XMM forms preserve shared vector state above bit 127,
/// so replay uses the AVX YMM0-YMM15 state bridge. Construction is O(N) time
/// and O(P + V) space for N operations, P unique guest PCs, and V virtual
/// registers.
pub fn x86_legacy_packed_shift_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_packed_shift_replay()
            .map(|_| (false, false, false))
    })
}

/// Identify exact register-only legacy MMX/SSE `PMULUDQ` and SSE4.1
/// `PMULDQ` semantic groups. The XMM forms preserve shared vector state above
/// bit 127 and therefore use the AVX YMM0-YMM15 bridge. The MMX form uses the
/// independent MMX/x87-tag bridge. Construction is O(N) time and O(P + V)
/// space for N operations, P unique guest PCs, and V virtual registers.
pub fn x86_legacy_widening_dword_multiply_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_register_widening_dword_multiply_replay()
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
