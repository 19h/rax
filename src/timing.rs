//! Global instruction-based timing for the emulator.
//!
//! All timing in the emulator is based on instruction count, not wall-clock time.
//! This ensures deterministic behavior and proper synchronization between:
//! - TSC (Time Stamp Counter)
//! - LAPIC timer
//! - PIT timer
//! - Any other time-dependent operations

use std::sync::atomic::{AtomicU64, Ordering};

/// Global instruction counter - incremented by step() for each instruction executed.
/// This is the single source of truth for all timing in the emulator.
static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Simulated CPU frequency in Hz (3 GHz - typical modern CPU)
pub const CPU_FREQUENCY_HZ: u64 = 3_000_000_000;

/// TSC ticks per instruction (assume 1 instruction = 1 TSC tick for simplicity)
/// This gives us a 3 GHz TSC frequency when running at native speed.
pub const TSC_TICKS_PER_INSN: u64 = 1;

/// LAPIC timer base frequency in Hz (typically 1 GHz for modern systems)
pub const LAPIC_TIMER_FREQ_HZ: u64 = 1_000_000_000;

/// PIT oscillator frequency (1.193182 MHz - fixed by hardware design)
pub const PIT_FREQUENCY_HZ: u64 = 1193182;

/// Increment the global instruction counter and return the new value.
#[inline(always)]
pub fn tick() -> u64 {
    INSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed) + 1
}

/// Get the current instruction count without incrementing.
#[inline(always)]
pub fn current() -> u64 {
    INSTRUCTION_COUNT.load(Ordering::Relaxed)
}

/// Get the current TSC value based on instruction count.
#[inline(always)]
pub fn tsc() -> u64 {
    current().wrapping_mul(TSC_TICKS_PER_INSN)
}

/// Convert instruction count to nanoseconds (for compatibility with time-based calculations).
/// At 3 GHz, 1 instruction = ~0.333 ns
#[inline(always)]
pub fn insn_to_nanos(insn_count: u64) -> u64 {
    // insn_count * 1_000_000_000 / CPU_FREQUENCY_HZ
    // To avoid overflow, divide first (less precise but safe)
    insn_count / 3 // ~0.333 ns per instruction at 3 GHz
}

/// Convert nanoseconds to instruction count.
#[inline(always)]
pub fn nanos_to_insn(nanos: u64) -> u64 {
    nanos.saturating_mul(3) // ~3 instructions per nanosecond at 3 GHz
}

/// Reset the instruction counter (used for testing or VM reset).
pub fn reset() {
    INSTRUCTION_COUNT.store(0, Ordering::Relaxed);
}
