//! Integer division and remainder primitives of the direct eBPF ALU.
//!
//! Each function is exactly the Rust operator on its operands. Callers have
//! already handled a zero divisor and, for the signed forms, the overflowing
//! `MIN / -1` case, so no call can panic. The operators are separate
//! functions so that the formal tier (`tools/ebpf-formal`) can replace them
//! with one functionally consistent uninterpreted operator shared with its
//! specification: bit-level SAT cannot prove two divider circuits equal.

#[inline(always)]
pub(crate) fn udiv32(dividend: u32, divisor: u32) -> u32 {
    dividend / divisor
}

#[inline(always)]
pub(crate) fn urem32(dividend: u32, divisor: u32) -> u32 {
    dividend % divisor
}

#[inline(always)]
pub(crate) fn udiv64(dividend: u64, divisor: u64) -> u64 {
    dividend / divisor
}

#[inline(always)]
pub(crate) fn urem64(dividend: u64, divisor: u64) -> u64 {
    dividend % divisor
}

#[inline(always)]
pub(crate) fn sdiv32(dividend: i32, divisor: i32) -> i32 {
    dividend / divisor
}

#[inline(always)]
pub(crate) fn srem32(dividend: i32, divisor: i32) -> i32 {
    dividend % divisor
}

#[inline(always)]
pub(crate) fn sdiv64(dividend: i64, divisor: i64) -> i64 {
    dividend / divisor
}

#[inline(always)]
pub(crate) fn srem64(dividend: i64, divisor: i64) -> i64 {
    dividend % divisor
}
