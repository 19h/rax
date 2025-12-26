//! Wall-clock based timing for the emulator.
//!
//! Timing is based on actual wall-clock time, not instruction count.
//! This allows TSC-based delays to complete in real time rather than
//! being tied to emulator execution speed.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Start time of the emulator - all timing is relative to this
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Instruction counter - still useful for debugging/profiling
static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Flag indicating a timer interrupt is pending from the timer thread
static TIMER_PENDING: AtomicBool = AtomicBool::new(false);

/// Simulated CPU frequency in Hz (3 GHz - typical modern CPU)
pub const CPU_FREQUENCY_HZ: u64 = 3_000_000_000;

/// LAPIC timer base frequency in Hz (typically 1 GHz for modern systems)
pub const LAPIC_TIMER_FREQ_HZ: u64 = 1_000_000_000;

/// PIT oscillator frequency (1.193182 MHz - fixed by hardware design)
pub const PIT_FREQUENCY_HZ: u64 = 1193182;

/// Initialize timing (call once at startup)
pub fn init() {
    START_TIME.get_or_init(Instant::now);
}

/// Get elapsed time since emulator start in nanoseconds
#[inline(always)]
pub fn elapsed_nanos() -> u64 {
    let start = START_TIME.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

/// Get the current TSC value based on wall-clock time.
/// At 3 GHz, TSC increments 3 billion times per second.
#[inline(always)]
pub fn tsc() -> u64 {
    // TSC = elapsed_nanos * CPU_FREQUENCY_HZ / 1_000_000_000
    // Simplify: TSC = elapsed_nanos * 3 (for 3 GHz)
    elapsed_nanos().saturating_mul(3)
}

/// Increment the instruction counter (for profiling/debugging)
#[inline(always)]
pub fn tick() -> u64 {
    INSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed) + 1
}

/// Get current instruction count
#[inline(always)]
pub fn instruction_count() -> u64 {
    INSTRUCTION_COUNT.load(Ordering::Relaxed)
}

/// Signal that a timer interrupt is pending
pub fn set_timer_pending() {
    TIMER_PENDING.store(true, Ordering::Release);
}

/// Check and clear timer pending flag
pub fn take_timer_pending() -> bool {
    TIMER_PENDING.swap(false, Ordering::AcqRel)
}

/// Check if timer is pending (without clearing)
pub fn is_timer_pending() -> bool {
    TIMER_PENDING.load(Ordering::Acquire)
}

/// Convert nanoseconds to PIT ticks
#[inline(always)]
pub fn nanos_to_pit_ticks(nanos: u64) -> u64 {
    // ticks = nanos * PIT_FREQUENCY_HZ / 1_000_000_000
    // Use 128-bit math to avoid overflow
    ((nanos as u128 * PIT_FREQUENCY_HZ as u128) / 1_000_000_000) as u64
}

/// Reset timing (for VM reset)
pub fn reset() {
    INSTRUCTION_COUNT.store(0, Ordering::Relaxed);
    TIMER_PENDING.store(false, Ordering::Release);
    // Note: START_TIME cannot be reset (OnceLock)
}

// Legacy compatibility - keep these for code that still uses instruction-based timing
pub fn current() -> u64 {
    instruction_count()
}

pub fn insn_to_nanos(insn_count: u64) -> u64 {
    insn_count / 3
}

pub fn nanos_to_insn(nanos: u64) -> u64 {
    nanos.saturating_mul(3)
}
