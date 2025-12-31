//! ARM bit operations test suite.
//!
//! This module ports the comprehensive test suite from arm-js-py/bitops.py
//! to validate ARM-specific bit manipulation operations used in instruction
//! execution.
//!
//! Original tests from: docs/arm/arm-js-py/bitops.py
//! Copyright 2012, Ryota Ozaki
//! Copyright 2014, espes

use rax::arm::execution::*;
use rax::arm::decoder::ShiftType;

// =============================================================================
// Bit Manipulation Tests (from bitops.py)
// =============================================================================

#[test]
fn test_clear_bit() {
    // assert2(clear_bit(0xffffffff, 0), 0xfffffffe)
    assert_eq!(0xffffffffu32 & !(1 << 0), 0xfffffffe);
    // assert2(clear_bit(0x13, 31), 0x13)
    assert_eq!(0x13u32 & !(1u32 << 31), 0x13);
    // assert2(clear_bit(0x13, 0), 0x12)
    assert_eq!(0x13u32 & !(1 << 0), 0x12);
}

#[test]
fn test_clear_bits() {
    fn clear_bits(val: u32, start: u32, end: u32) -> u32 {
        let mask = ((1u64 << (start + 1)) - 1) & !((1u64 << end) - 1);
        val & !(mask as u32)
    }
    
    // assert2(clear_bits(0xffffffff, 31, 0), 0)
    assert_eq!(clear_bits(0xffffffff, 31, 0), 0);
    // assert2(clear_bits(0xffffffff, 31, 16), 0x0000ffff)
    assert_eq!(clear_bits(0xffffffff, 31, 16), 0x0000ffff);
    // assert2(clear_bits(0xffffffff, 15, 0), 0xffff0000)
    assert_eq!(clear_bits(0xffffffff, 15, 0), 0xffff0000);
    // assert2(clear_bits(0xffffffff, 15, 12), 0xffff0fff)
    assert_eq!(clear_bits(0xffffffff, 15, 12), 0xffff0fff);
    // assert2(clear_bits(0x0fffffff, 15, 12), 0x0fff0fff)
    assert_eq!(clear_bits(0x0fffffff, 15, 12), 0x0fff0fff);
}

#[test]
fn test_xor() {
    // assert1(xor(0xffffffff, 0xffffffff) == 0)
    assert_eq!(0xffffffffu32 ^ 0xffffffff, 0);
    // assert1(xor(0x11111111, 0x22222222) == 0x33333333)
    assert_eq!(0x11111111u32 ^ 0x22222222, 0x33333333);
    // assert1(xor(0xf0000000, 0xf0000000) == 0)
    assert_eq!(0xf0000000u32 ^ 0xf0000000, 0);
}

#[test]
fn test_xor64() {
    // assert1(xor64(0xffffffff, 0xffffffff) == 0)
    assert_eq!(0xffffffffu64 ^ 0xffffffff, 0);
    // assert1(xor64(0x11111111, 0x22222222) == 0x33333333)
    assert_eq!(0x11111111u64 ^ 0x22222222, 0x33333333);
    // assert1(xor64(0xf0000000, 0xf0000000) == 0)
    assert_eq!(0xf0000000u64 ^ 0xf0000000, 0);
    // assert1(xor64(0x1f0000000, 0xf0000000) == 0x100000000)
    assert_eq!(0x1f0000000u64 ^ 0xf0000000, 0x100000000);
}

#[test]
fn test_not() {
    fn not32(x: u32) -> u32 {
        !x
    }
    
    // assert1(not_(0xffffffff) == 0x00000000)
    assert_eq!(not32(0xffffffff), 0x00000000);
    // assert1(not_(0x00000000) == 0xffffffff)
    assert_eq!(not32(0x00000000), 0xffffffff);
    // assert1(not_(0x00000001) == 0xfffffffe)
    assert_eq!(not32(0x00000001), 0xfffffffe);
    // assert1(not_(0x80000000) == 0x7fffffff)
    assert_eq!(not32(0x80000000), 0x7fffffff);
}

#[test]
fn test_or() {
    // assert1(or_(0x11111111, 0x22222222) == 0x33333333)
    assert_eq!(0x11111111u32 | 0x22222222, 0x33333333);
    // assert1(or_(0xffffffff, 0x00000000) == 0xffffffff)
    assert_eq!(0xffffffffu32 | 0x00000000, 0xffffffff);
    // assert1(or_(0xffffffff, 0xffffffff) == 0xffffffff)
    assert_eq!(0xffffffffu32 | 0xffffffff, 0xffffffff);
}

#[test]
fn test_or64() {
    // assert1(or64(0x11111111, 0x22222222) == 0x33333333)
    assert_eq!(0x11111111u64 | 0x22222222, 0x33333333);
    // assert1(or64(0xffffffff, 0x00000000) == 0xffffffff)
    assert_eq!(0xffffffffu64 | 0x00000000, 0xffffffff);
    // assert1(or64(0xffffffff, 0xffffffff) == 0xffffffff)
    assert_eq!(0xffffffffu64 | 0xffffffff, 0xffffffff);
    // assert1(or64(0xf00000000, 0x00000000) == 0xf00000000)
    assert_eq!(0xf00000000u64 | 0x00000000, 0xf00000000);
    // assert1(or64(0xf00000000, 0x0000000f) == 0xf0000000f)
    assert_eq!(0xf00000000u64 | 0x0000000f, 0xf0000000f);
}

#[test]
fn test_and() {
    // assert1(and_(0x11111111, 0x22222222) == 0)
    assert_eq!(0x11111111u32 & 0x22222222, 0);
    // assert1(and_(0xffffffff, 0) == 0)
    assert_eq!(0xffffffffu32 & 0, 0);
}

#[test]
fn test_and64() {
    // assert1(and64(0x11111111, 0x22222222) == 0)
    assert_eq!(0x11111111u64 & 0x22222222, 0);
    // assert2(and64(0xffffffff, 0), 0)
    assert_eq!(0xffffffffu64 & 0, 0);
    // assert2(and64(0xffffffffffff, 0), 0)
    assert_eq!(0xffffffffffffu64 & 0, 0);
    // assert2(and64(0xffffffffffff, 0xffffffff), 0xffffffff)
    assert_eq!(0xffffffffffffu64 & 0xffffffff, 0xffffffff);
}

#[test]
fn test_get_bit() {
    fn get_bit(val: u32, pos: u32) -> u32 {
        (val >> pos) & 1
    }
    
    // assert2(get_bit(0xffffffff, 31), 1)
    assert_eq!(get_bit(0xffffffff, 31), 1);
    // assert2(get_bit(0xffffffff, 0), 1)
    assert_eq!(get_bit(0xffffffff, 0), 1);
    // assert1(get_bit(0x80000000, 31) == 1)
    assert_eq!(get_bit(0x80000000, 31), 1);
    // assert1(get_bit(0, 31) == 0)
    assert_eq!(get_bit(0, 31), 0);
    // assert1(get_bit(0, 0) == 0)
    assert_eq!(get_bit(0, 0), 0);
    // assert1(get_bit(0x7fffffff, 31) == 0)
    assert_eq!(get_bit(0x7fffffff, 31), 0);
    // assert2(get_bit(0x80000000, 31), 1)
    assert_eq!(get_bit(0x80000000, 31), 1);
}

#[test]
fn test_get_bit64() {
    fn get_bit64(val: u64, pos: u32) -> u64 {
        (val >> pos) & 1
    }
    
    // assert1(get_bit64(0xffffffff, 31) == 1)
    assert_eq!(get_bit64(0xffffffff, 31), 1);
    // assert2(get_bit64(0xffffffff, 0), 1)
    assert_eq!(get_bit64(0xffffffff, 0), 1);
    // assert1(get_bit64(0x80000000, 31) == 1)
    assert_eq!(get_bit64(0x80000000, 31), 1);
    // assert1(get_bit64(0, 31) == 0)
    assert_eq!(get_bit64(0, 31), 0);
    // assert1(get_bit64(0, 0) == 0)
    assert_eq!(get_bit64(0, 0), 0);
    // assert1(get_bit64(0x7fffffff, 31) == 0)
    assert_eq!(get_bit64(0x7fffffff, 31), 0);
    // assert1(get_bit64(0xffffffffffff, 31) == 1)
    assert_eq!(get_bit64(0xffffffffffff, 31), 1);
    // assert2(get_bit64(0xffffffffffff, 50), 0)
    assert_eq!(get_bit64(0xffffffffffff, 50), 0);
}

#[test]
fn test_get_bits() {
    fn get_bits(val: u32, start: u32, end: u32) -> u32 {
        (val >> end) & ((1u64 << (start - end + 1)) - 1) as u32
    }
    
    // assert1(get_bits(0xffffffff, 31, 0) == 0xffffffff)
    assert_eq!(get_bits(0xffffffff, 31, 0), 0xffffffff);
    // assert1(get_bits(0xffffffff, 31, 16) == 0xffff)
    assert_eq!(get_bits(0xffffffff, 31, 16), 0xffff);
    // assert1(get_bits(0, 31, 0) == 0)
    assert_eq!(get_bits(0, 31, 0), 0);
    // assert1(get_bits(0x13, 4, 0) == 0x13, get_bits(0x13, 4, 0))
    assert_eq!(get_bits(0x13, 4, 0), 0x13);
    // assert2(get_bits(0xf0000000, 31, 27), 0x1e)
    assert_eq!(get_bits(0xf0000000, 31, 27), 0x1e);
    // assert2(get_bits(0xc0000000, 31, 27), 0x18)
    assert_eq!(get_bits(0xc0000000, 31, 27), 0x18);
}

#[test]
fn test_set_bit() {
    fn set_bit(val: u32, pos: u32, bit: u32) -> u32 {
        if bit != 0 {
            val | (1 << pos)
        } else {
            val & !(1 << pos)
        }
    }
    
    // assert1(set_bit(0xffffffff, 0, 0) == 0xfffffffe, set_bit(0xffffffff, 0, 0))
    assert_eq!(set_bit(0xffffffff, 0, 0), 0xfffffffe);
    // assert1(set_bit(0xffffffff, 31, 0) == 0x7fffffff, set_bit(0xffffffff, 31, 0))
    assert_eq!(set_bit(0xffffffff, 31, 0), 0x7fffffff);
    // assert1(set_bit(0xffffffff, 31, 1) == 0xffffffff, set_bit(0xffffffff, 31, 1))
    assert_eq!(set_bit(0xffffffff, 31, 1), 0xffffffff);
    // assert1(set_bit(0x13, 31, 0) == 0x13, set_bit(0x13, 31, 0))
    assert_eq!(set_bit(0x13, 31, 0), 0x13);
    // assert1(set_bit(0, 31, 1) == 0x80000000)
    assert_eq!(set_bit(0, 31, 1), 0x80000000);
    // assert1(set_bit(0, 0, 1) == 1)
    assert_eq!(set_bit(0, 0, 1), 1);
    // assert1(set_bit(0, 2, 1) == 4, set_bit(0, 2, 1))
    assert_eq!(set_bit(0, 2, 1), 4);
}

#[test]
fn test_set_bits() {
    fn set_bits(val: u32, start: u32, end: u32, bits: u32) -> u32 {
        let mask = ((1u64 << (start + 1)) - 1) & !((1u64 << end) - 1);
        (val & !(mask as u32)) | ((bits << end) & mask as u32)
    }
    
    // assert1(set_bits(0xffffffff, 31, 0, 0) == 0)
    assert_eq!(set_bits(0xffffffff, 31, 0, 0), 0);
    // assert1(set_bits(0xffffffff, 15, 0, 0) == 0xffff0000, set_bits(0xffffffff, 15, 0, 0))
    assert_eq!(set_bits(0xffffffff, 15, 0, 0), 0xffff0000);
    // assert1(set_bits(0, 4, 0, 0x13) == 0x13)
    assert_eq!(set_bits(0, 4, 0, 0x13), 0x13);
    // assert2(set_bits(0xf0000000, 31, 27, 0x1e), 0xf0000000)
    assert_eq!(set_bits(0xf0000000, 31, 27, 0x1e), 0xf0000000);
    // assert2(set_bits(0x00000000, 31, 27, 0x1e), 0xf0000000)
    assert_eq!(set_bits(0x00000000, 31, 27, 0x1e), 0xf0000000);
    // assert2(set_bits(0xf0000000, 31, 27, 0x18), 0xc0000000)
    assert_eq!(set_bits(0xf0000000, 31, 27, 0x18), 0xc0000000);
}

#[test]
fn test_lsl() {
    // assert2(lsl(1, 1), 2)
    assert_eq!(1u64 << 1, 2);
    // assert2(lsl(0xf0000000, 1), 0x1e0000000)
    assert_eq!(0xf0000000u64 << 1, 0x1e0000000);
    // assert2(lsl(0xffffffff, 1), 0x1fffffffe)
    assert_eq!(0xffffffffu64 << 1, 0x1fffffffe);
    // assert2(lsl(0xf0f0f0f0, 4), 0xf0f0f0f00)
    assert_eq!(0xf0f0f0f0u64 << 4, 0xf0f0f0f00);
    // assert2(lsl(0x100000000, 1), 0x200000000)
    assert_eq!(0x100000000u64 << 1, 0x200000000);
}

#[test]
fn test_lsr() {
    fn lsr(val: u32, amount: u32) -> u32 {
        if amount >= 32 { 0 } else { val >> amount }
    }
    
    // assert2(lsr(1, 1), 0)
    assert_eq!(lsr(1, 1), 0);
    // assert2(lsr(0xf0000000, 1), 0x78000000)
    assert_eq!(lsr(0xf0000000, 1), 0x78000000);
    // assert2(lsr(0xffffffff, 1), 0x7fffffff)
    assert_eq!(lsr(0xffffffff, 1), 0x7fffffff);
    // assert2(lsr(0xf0f0f0f0, 4), 0x0f0f0f0f)
    assert_eq!(lsr(0xf0f0f0f0, 4), 0x0f0f0f0f);
    // assert2(lsr(0x80000000, 32), 0)
    assert_eq!(lsr(0x80000000, 32), 0);
    // assert2(lsr(0x80000000, 1), 0x40000000)
    assert_eq!(lsr(0x80000000, 1), 0x40000000);
}

#[test]
fn test_sint32() {
    // assert2(sint32(0x00000000), 0x00000000)
    assert_eq!(sint32(0x00000000), 0x00000000);
    // assert2(sint32(0x80000000), -2147483648)
    assert_eq!(sint32(0x80000000), -2147483648);
    // assert2(sint32(0x100000000), 0x00000000) - wraps in Rust
    // assert2(sint32(0xffffffff), -1)
    assert_eq!(sint32(0xffffffff), -1);
}

#[test]
fn test_uint32() {
    fn uint32(x: u64) -> u32 {
        (x & 0xffffffff) as u32
    }
    
    // assert2(uint32(0x00000000),  0x00000000)
    assert_eq!(uint32(0x00000000), 0x00000000);
    // assert2(uint32(0x80000000),  0x80000000)
    assert_eq!(uint32(0x80000000), 0x80000000);
    // assert2(uint32(0x100000000), 0x00000000)
    assert_eq!(uint32(0x100000000), 0x00000000);
    // assert2(uint32(0xffffffff),  0xffffffff)
    assert_eq!(uint32(0xffffffff), 0xffffffff);
    // assert2(uint32(0xfffffffff), 0xffffffff)
    assert_eq!(uint32(0xfffffffff), 0xffffffff);
}

#[test]
fn test_sign_extend_from_bitops() {
    // assert2(sign_extend(0, 26, 32), 0)
    assert_eq!(sign_extend(0, 26), 0);
    // assert2(sign_extend(0, 1, 32), 0)
    assert_eq!(sign_extend(0, 1), 0);
    // assert2(sign_extend(1, 1, 32), 0xffffffff)
    assert_eq!(sign_extend(1, 1), 0xffffffff);
    // assert2(sign_extend(0x0000ffff, 16, 32), 0xffffffff)
    assert_eq!(sign_extend(0x0000ffff, 16), 0xffffffff);
    // assert2(sign_extend(0x00007fff, 16, 32), 0x00007fff)
    assert_eq!(sign_extend(0x00007fff, 16), 0x00007fff);
    // assert2(sign_extend(0xffffe3 << 2, 26, 32), 0xffffff8c)
    assert_eq!(sign_extend(0xffffe3 << 2, 26), 0xffffff8c);
}

#[test]
fn test_copy_bits() {
    fn copy_bits(dest: u32, start: u32, end: u32, src: u32) -> u32 {
        let mask = ((1u64 << (start + 1)) - 1) & !((1u64 << end) - 1);
        let src_bits = (src >> end) & ((1 << (start - end + 1)) - 1);
        (dest & !(mask as u32)) | ((src_bits << end) & mask as u32)
    }
    
    // assert2(copy_bits(0xf0000000, 31, 27, 0), 0)
    assert_eq!(copy_bits(0xf0000000, 31, 27, 0), 0);
    // assert2(copy_bits(0xf0000000, 31, 27, 0xc0000000), 0xc0000000)
    assert_eq!(copy_bits(0xf0000000, 31, 27, 0xc0000000), 0xc0000000);
}

#[test]
fn test_copy_bit() {
    fn copy_bit(dest: u32, pos: u32, src: u32) -> u32 {
        let bit = (src >> pos) & 1;
        if bit != 0 {
            dest | (1 << pos)
        } else {
            dest & !(1 << pos)
        }
    }
    
    // assert2(copy_bit(0, 0, 1), 1)
    assert_eq!(copy_bit(0, 0, 1), 1);
    // assert2(copy_bit(1, 0, 0), 0)
    assert_eq!(copy_bit(1, 0, 0), 0);
    // assert2(copy_bit(0xffffffff, 0, 0), 0xfffffffe)
    assert_eq!(copy_bit(0xffffffff, 0, 0), 0xfffffffe);
    // assert2(copy_bit(0xffffffff, 31, 0), 0x7fffffff)
    assert_eq!(copy_bit(0xffffffff, 31, 0), 0x7fffffff);
}

#[test]
fn test_ror_from_bitops() {
    // assert2(ror(0x10000000, 1), 0x08000000)
    assert_eq!(ror(0x10000000, 1), 0x08000000);
    // assert2(ror(0x10000001, 1), 0x88000000)
    assert_eq!(ror(0x10000001, 1), 0x88000000);
    // assert2(ror(0xffffffff, 1), 0xffffffff)
    assert_eq!(ror(0xffffffff, 1), 0xffffffff);
    // assert2(ror(0x0000ffff, 16), 0xffff0000)
    assert_eq!(ror(0x0000ffff, 16), 0xffff0000);
    // assert2(ror(0x000ffff0, 16), 0xfff0000f)
    assert_eq!(ror(0x000ffff0, 16), 0xfff0000f);
}

#[test]
fn test_count_leading_zero_bits() {
    // assert2(count_leading_zero_bits(0), 32)
    assert_eq!(count_leading_zeros(0), 32);
    // assert2(count_leading_zero_bits(0x80000000), 0)
    assert_eq!(count_leading_zeros(0x80000000), 0);
    // assert2(count_leading_zero_bits(0x00008000), 16)
    assert_eq!(count_leading_zeros(0x00008000), 16);
}

// =============================================================================
// Additional Execution Module Tests
// =============================================================================

#[test]
fn test_add_with_carry_comprehensive() {
    // Test basic addition
    let (r, c, v) = add_with_carry(100, 50, 0);
    assert_eq!(r, 150);
    assert!(!c);
    assert!(!v);
    
    // Test addition with carry in
    let (r, c, v) = add_with_carry(100, 50, 1);
    assert_eq!(r, 151);
    assert!(!c);
    assert!(!v);
    
    // Test unsigned overflow (carry)
    let (r, c, v) = add_with_carry(0xFFFFFFFF, 2, 0);
    assert_eq!(r, 1);
    assert!(c);
    assert!(!v);
    
    // Test signed overflow (positive + positive = negative)
    let (r, c, v) = add_with_carry(0x7FFFFFFF, 1, 0);
    assert_eq!(r, 0x80000000);
    assert!(!c);
    assert!(v);
    
    // Test signed overflow (negative + negative = positive)
    let (r, c, v) = add_with_carry(0x80000000, 0x80000000, 0);
    assert_eq!(r, 0);
    assert!(c);
    assert!(v);
    
    // Test subtraction (x - y = x + ~y + 1)
    let (r, c, v) = add_with_carry(100, !50u32, 1);
    assert_eq!(r, 50);
    assert!(c); // No borrow means carry is set
    assert!(!v);
    
    // Test subtraction with borrow
    let (r, c, v) = add_with_carry(50, !100u32, 1);
    assert_eq!(r, 0xFFFFFFCE); // -50 in two's complement
    assert!(!c); // Borrow means carry is clear
    assert!(!v);
}

#[test]
fn test_shift_c_comprehensive() {
    // LSL tests
    let (r, c) = shift_c(0x12345678, ShiftType::LSL, 0, true);
    assert_eq!(r, 0x12345678);
    assert!(c); // Carry preserved when shift = 0
    
    let (r, c) = shift_c(0x12345678, ShiftType::LSL, 4, false);
    assert_eq!(r, 0x23456780);
    assert!(c); // Bit 28 shifted out
    
    let (r, c) = shift_c(0x00000001, ShiftType::LSL, 31, false);
    assert_eq!(r, 0x80000000);
    assert!(!c);
    
    let (r, c) = shift_c(0x00000001, ShiftType::LSL, 32, false);
    assert_eq!(r, 0);
    assert!(c); // Bit 0 shifted to carry on 32-bit shift
    
    // LSR tests
    let (r, c) = shift_c(0x87654321, ShiftType::LSR, 4, false);
    assert_eq!(r, 0x08765432);
    assert!(!c);
    
    let (r, c) = shift_c(0x80000001, ShiftType::LSR, 1, false);
    assert_eq!(r, 0x40000000);
    assert!(c); // Bit 0 shifted out
    
    let (r, c) = shift_c(0x80000000, ShiftType::LSR, 32, false);
    assert_eq!(r, 0);
    assert!(c); // Bit 31 goes to carry
    
    // ASR tests
    let (r, c) = shift_c(0x80000000, ShiftType::ASR, 4, false);
    assert_eq!(r, 0xF8000000);
    assert!(!c);
    
    let (r, c) = shift_c(0x7FFFFFFF, ShiftType::ASR, 4, false);
    assert_eq!(r, 0x07FFFFFF);
    assert!(c);
    
    let (r, c) = shift_c(0x80000000, ShiftType::ASR, 32, false);
    assert_eq!(r, 0xFFFFFFFF);
    assert!(c);
    
    let (r, c) = shift_c(0x7FFFFFFF, ShiftType::ASR, 32, false);
    assert_eq!(r, 0);
    assert!(!c);
    
    // ROR tests
    let (r, c) = shift_c(0x00000001, ShiftType::ROR, 1, false);
    assert_eq!(r, 0x80000000);
    assert!(c);
    
    let (r, c) = shift_c(0x12345678, ShiftType::ROR, 8, false);
    assert_eq!(r, 0x78123456);
    assert!(!c);
    
    // RRX tests
    let (r, c) = shift_c(0x00000001, ShiftType::RRX, 1, false);
    assert_eq!(r, 0x00000000);
    assert!(c);
    
    let (r, c) = shift_c(0x00000001, ShiftType::RRX, 1, true);
    assert_eq!(r, 0x80000000);
    assert!(c);
    
    let (r, c) = shift_c(0x80000000, ShiftType::RRX, 1, false);
    assert_eq!(r, 0x40000000);
    assert!(!c);
    
    let (r, c) = shift_c(0x80000000, ShiftType::RRX, 1, true);
    assert_eq!(r, 0xC0000000);
    assert!(!c);
}

#[test]
fn test_decode_imm_shift() {
    // LSL
    assert_eq!(decode_imm_shift(0b00, 0), (ShiftType::LSL, 0));
    assert_eq!(decode_imm_shift(0b00, 5), (ShiftType::LSL, 5));
    assert_eq!(decode_imm_shift(0b00, 31), (ShiftType::LSL, 31));
    
    // LSR
    assert_eq!(decode_imm_shift(0b01, 0), (ShiftType::LSR, 32)); // imm5=0 means 32
    assert_eq!(decode_imm_shift(0b01, 5), (ShiftType::LSR, 5));
    
    // ASR
    assert_eq!(decode_imm_shift(0b10, 0), (ShiftType::ASR, 32)); // imm5=0 means 32
    assert_eq!(decode_imm_shift(0b10, 5), (ShiftType::ASR, 5));
    
    // RRX/ROR
    assert_eq!(decode_imm_shift(0b11, 0), (ShiftType::RRX, 1)); // imm5=0 means RRX
    assert_eq!(decode_imm_shift(0b11, 5), (ShiftType::ROR, 5)); // imm5!=0 means ROR
}

#[test]
fn test_expand_imm_c_comprehensive() {
    // No rotation
    let (r, c) = expand_imm_c(0x0AB, false);
    assert_eq!(r, 0xAB);
    assert!(!c); // Carry unchanged
    
    let (r, c) = expand_imm_c(0x0AB, true);
    assert_eq!(r, 0xAB);
    assert!(c); // Carry unchanged
    
    // Rotation by 2 (rot = 1, rotation = 2)
    // 0xFF rotate_right 2 = 0xC000003F
    let (r, c) = expand_imm_c(0x1FF, false);
    assert_eq!(r, 0xC000003F);
    assert!(c); // MSB is 1
    
    // Rotation by 8 (rot = 4, rotation = 8)
    // 0xFF rotate_right 8 = 0xFF000000
    let (r, c) = expand_imm_c(0x4FF, false);
    assert_eq!(r, 0xFF000000);
    assert!(c); // MSB is 1
    
    // 0x102: rot = 1, imm = 0x02, rotation = 2
    // 0x02 rotate_right 2 = 0x80000000
    let (r, _) = expand_imm_c(0x102, false);
    assert_eq!(r, 0x80000000);
}

#[test]
fn test_condition_passed_comprehensive() {
    // Test all conditions with appropriate flag combinations
    
    // EQ (0): Z=1
    assert!(condition_passed(0b0000, false, true, false, false));
    assert!(!condition_passed(0b0000, false, false, false, false));
    
    // NE (1): Z=0
    assert!(condition_passed(0b0001, false, false, false, false));
    assert!(!condition_passed(0b0001, false, true, false, false));
    
    // CS/HS (2): C=1
    assert!(condition_passed(0b0010, false, false, true, false));
    assert!(!condition_passed(0b0010, false, false, false, false));
    
    // CC/LO (3): C=0
    assert!(condition_passed(0b0011, false, false, false, false));
    assert!(!condition_passed(0b0011, false, false, true, false));
    
    // MI (4): N=1
    assert!(condition_passed(0b0100, true, false, false, false));
    assert!(!condition_passed(0b0100, false, false, false, false));
    
    // PL (5): N=0
    assert!(condition_passed(0b0101, false, false, false, false));
    assert!(!condition_passed(0b0101, true, false, false, false));
    
    // VS (6): V=1
    assert!(condition_passed(0b0110, false, false, false, true));
    assert!(!condition_passed(0b0110, false, false, false, false));
    
    // VC (7): V=0
    assert!(condition_passed(0b0111, false, false, false, false));
    assert!(!condition_passed(0b0111, false, false, false, true));
    
    // HI (8): C=1 && Z=0
    assert!(condition_passed(0b1000, false, false, true, false));
    assert!(!condition_passed(0b1000, false, true, true, false)); // Z=1
    assert!(!condition_passed(0b1000, false, false, false, false)); // C=0
    
    // LS (9): C=0 || Z=1
    assert!(condition_passed(0b1001, false, true, false, false));
    assert!(condition_passed(0b1001, false, false, false, false));
    assert!(!condition_passed(0b1001, false, false, true, false));
    
    // GE (10): N=V
    assert!(condition_passed(0b1010, false, false, false, false));
    assert!(condition_passed(0b1010, true, false, false, true));
    assert!(!condition_passed(0b1010, true, false, false, false));
    
    // LT (11): N!=V
    assert!(condition_passed(0b1011, true, false, false, false));
    assert!(condition_passed(0b1011, false, false, false, true));
    assert!(!condition_passed(0b1011, false, false, false, false));
    
    // GT (12): Z=0 && N=V
    assert!(condition_passed(0b1100, false, false, false, false));
    assert!(!condition_passed(0b1100, false, true, false, false)); // Z=1
    assert!(!condition_passed(0b1100, true, false, false, false)); // N!=V
    
    // LE (13): Z=1 || N!=V
    assert!(condition_passed(0b1101, false, true, false, false));
    assert!(condition_passed(0b1101, true, false, false, false));
    assert!(!condition_passed(0b1101, false, false, false, false));
    
    // AL (14): always
    assert!(condition_passed(0b1110, false, false, false, false));
    assert!(condition_passed(0b1110, true, true, true, true));
}

#[test]
fn test_processor_mode() {
    assert_eq!(ProcessorMode::from_bits(0x10), Some(ProcessorMode::User));
    assert_eq!(ProcessorMode::from_bits(0x11), Some(ProcessorMode::Fiq));
    assert_eq!(ProcessorMode::from_bits(0x12), Some(ProcessorMode::Irq));
    assert_eq!(ProcessorMode::from_bits(0x13), Some(ProcessorMode::Supervisor));
    assert_eq!(ProcessorMode::from_bits(0x16), Some(ProcessorMode::Monitor));
    assert_eq!(ProcessorMode::from_bits(0x17), Some(ProcessorMode::Abort));
    assert_eq!(ProcessorMode::from_bits(0x1B), Some(ProcessorMode::Undefined));
    assert_eq!(ProcessorMode::from_bits(0x1F), Some(ProcessorMode::System));
    assert_eq!(ProcessorMode::from_bits(0x00), None);
    assert_eq!(ProcessorMode::from_bits(0x15), None);
    
    assert!(!ProcessorMode::User.is_privileged());
    assert!(ProcessorMode::Supervisor.is_privileged());
    assert!(ProcessorMode::System.is_privileged());
    
    assert!(!ProcessorMode::User.has_spsr());
    assert!(!ProcessorMode::System.has_spsr());
    assert!(ProcessorMode::Supervisor.has_spsr());
    assert!(ProcessorMode::Irq.has_spsr());
}

#[test]
fn test_psr_comprehensive() {
    // Test typical SVC mode PSR
    let psr = Psr::from_u32(0x600001D3); // NZCV clear except C, SVC mode, I+F set
    assert!(!psr.n);
    assert!(psr.z);
    assert!(psr.c);
    assert!(!psr.v);
    assert!(psr.i);
    assert!(psr.f);
    assert!(!psr.t);
    assert_eq!(psr.mode, 0x13);
    
    // Test roundtrip
    let reconstructed = psr.to_u32();
    assert_eq!(reconstructed, 0x600001D3);
    
    // Test all flags set
    let psr = Psr::from_u32(0xF80001D3);
    assert!(psr.n);
    assert!(psr.z);
    assert!(psr.c);
    assert!(psr.v);
    assert!(psr.q);
    
    // Test Thumb mode
    let psr = Psr::from_u32(0x00000030); // Thumb bit set, User mode
    assert!(psr.t);
    assert_eq!(psr.mode, 0x10);
}

#[test]
fn test_byte_operations() {
    // REV
    assert_eq!(byte_reverse(0x12345678), 0x78563412);
    assert_eq!(byte_reverse(0x00000001), 0x01000000);
    assert_eq!(byte_reverse(0xFF000000), 0x000000FF);
    
    // REV16
    assert_eq!(byte_reverse_16(0x12345678), 0x34127856);
    assert_eq!(byte_reverse_16(0xAABBCCDD), 0xBBAADDCC);
    
    // RBIT
    assert_eq!(bit_reverse(0x80000000), 0x00000001);
    assert_eq!(bit_reverse(0x00000001), 0x80000000);
    assert_eq!(bit_reverse(0xF0000000), 0x0000000F);
}
