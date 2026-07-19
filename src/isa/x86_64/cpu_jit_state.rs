//! Native x86 guest-region metadata shared by compilation and execution.

/// A compiled native hot-block region. The lowered code is register-state
/// independent (it marshals guest state in/out per run), so one `JitRegion` is
/// cached by (RIP, mode_tag) and re-run for every later entry to that RIP until
/// the underlying guest code page is written (SMC invalidation).
pub(super) struct JitRegion {
    pub(super) exec: crate::smir::lower::runtime::ExecMem,
    pub(super) entry_offset: usize,
    /// Whether the entry trampoline must marshal ZMM0-ZMM31 and K0-K7.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_vector: bool,
    /// Whether exact helper-backed XMM masked stores need source state copied
    /// into `GuestRegs` without activating the native vector entry bridge.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_xmm_state: bool,
    /// Whether vector state can use AVX512F KMOVW while retaining K[63:16] in
    /// memory. False selects the general AVX512BW KMOVQ path.
    #[cfg(target_arch = "x86_64")]
    pub(super) narrow_vector_opmasks: bool,
    /// Whether the native entry bridge must marshal MM0-MM7 and guest x87 tags.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_mmx: bool,
}
