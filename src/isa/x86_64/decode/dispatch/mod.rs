//! Instruction dispatch modules for the x86_64 CPU emulator.
//!
//! This module contains the opcode dispatch logic, split by encoding:
//! - `legacy`: Single-byte opcode dispatch
//! - `twobyte`: Two-byte (0x0F-prefixed) opcode dispatch
//! - `vex`: VEX-encoded (AVX) instruction dispatch
//! - `evex`: EVEX-encoded (AVX-512) instruction dispatch
//! - `resolver`: maps a decoded opcode to a fn-pointer handler for the
//!   decode-cache fast path

mod evex;
mod legacy;
mod resolver;
mod twobyte;
mod vex;

#[inline(always)]
fn f32_is_nan_bits(bits: u32) -> bool {
    bits & 0x7fff_ffff > 0x7f80_0000
}

#[inline(always)]
fn f64_is_nan_bits(bits: u64) -> bool {
    bits & 0x7fff_ffff_ffff_ffff > 0x7ff0_0000_0000_0000
}

#[inline(always)]
fn f32_quiet_nan_bits(bits: u32) -> u32 {
    bits | 0x0040_0000
}

#[inline(always)]
fn f64_quiet_nan_bits(bits: u64) -> u64 {
    bits | 0x0008_0000_0000_0000
}

#[inline(always)]
fn x86_mul_f32_bits(lhs: u32, rhs: u32) -> u32 {
    if f32_is_nan_bits(lhs) {
        f32_quiet_nan_bits(lhs)
    } else if f32_is_nan_bits(rhs) {
        f32_quiet_nan_bits(rhs)
    } else {
        (f32::from_bits(lhs) * f32::from_bits(rhs)).to_bits()
    }
}

#[inline(always)]
fn x86_mul_f64_bits(lhs: u64, rhs: u64) -> u64 {
    if f64_is_nan_bits(lhs) {
        f64_quiet_nan_bits(lhs)
    } else if f64_is_nan_bits(rhs) {
        f64_quiet_nan_bits(rhs)
    } else {
        (f64::from_bits(lhs) * f64::from_bits(rhs)).to_bits()
    }
}

#[inline(always)]
fn x86_add_f32_bits(lhs: u32, rhs: u32) -> u32 {
    if f32_is_nan_bits(lhs) {
        f32_quiet_nan_bits(lhs)
    } else if f32_is_nan_bits(rhs) {
        f32_quiet_nan_bits(rhs)
    } else {
        (f32::from_bits(lhs) + f32::from_bits(rhs)).to_bits()
    }
}

#[inline(always)]
fn x86_add_f64_bits(lhs: u64, rhs: u64) -> u64 {
    if f64_is_nan_bits(lhs) {
        f64_quiet_nan_bits(lhs)
    } else if f64_is_nan_bits(rhs) {
        f64_quiet_nan_bits(rhs)
    } else {
        (f64::from_bits(lhs) + f64::from_bits(rhs)).to_bits()
    }
}

#[inline(always)]
fn x86_dpps_lane_result_bits(lhs: [u32; 4], rhs: [u32; 4], in_mask: u8, lane: usize) -> u32 {
    let p0 = if in_mask & 0x01 != 0 {
        x86_mul_f32_bits(lhs[0], rhs[0])
    } else {
        0
    };
    let p1 = if in_mask & 0x02 != 0 {
        x86_mul_f32_bits(lhs[1], rhs[1])
    } else {
        0
    };
    let p2 = if in_mask & 0x04 != 0 {
        x86_mul_f32_bits(lhs[2], rhs[2])
    } else {
        0
    };
    let p3 = if in_mask & 0x08 != 0 {
        x86_mul_f32_bits(lhs[3], rhs[3])
    } else {
        0
    };
    let low_pair = x86_add_f32_bits(p0, p1);
    let high_pair = x86_add_f32_bits(p2, p3);
    if lane < 2 {
        x86_add_f32_bits(low_pair, high_pair)
    } else {
        x86_add_f32_bits(high_pair, low_pair)
    }
}

#[inline(always)]
fn x86_dppd_lane_result_bits(lhs: [u64; 2], rhs: [u64; 2], in_mask: u8, lane: usize) -> u64 {
    let p0 = if in_mask & 0x01 != 0 {
        x86_mul_f64_bits(lhs[0], rhs[0])
    } else {
        0
    };
    let p1 = if in_mask & 0x02 != 0 {
        x86_mul_f64_bits(lhs[1], rhs[1])
    } else {
        0
    };
    if lane == 0 {
        x86_add_f64_bits(p0, p1)
    } else {
        x86_add_f64_bits(p1, p0)
    }
}
