// BMI (Bit Manipulation Instruction) Tests
//
// This module contains comprehensive tests for BMI1 and BMI2 instructions.
// These are VEX-encoded instructions that provide efficient bit manipulation operations.
//
// BMI1 Instructions:
// - BLSI: Extract Lowest Set Isolated Bit (dest = src & -src)
// - BLSMSK: Get Mask Up to Lowest Set Bit (dest = src ^ (src - 1))
// - BLSR: Reset Lowest Set Bit (dest = src & (src - 1))
// - ANDN: Logical AND NOT (dest = src1 & ~src2)
// - BEXTR: Bit Field Extract (extract specified bits using start and length)
//
// Each test file contains:
// - Basic functionality tests
// - 32-bit and 64-bit variants
// - Flag behavior tests (ZF, CF, SF, OF)
// - Edge cases (all zeros, all ones, boundary conditions)
// - Memory operand tests
// - Extended register tests (R8-R15)
// - Practical use cases

#[cfg(test)]
mod blsi;

#[cfg(test)]
mod blsmsk;

#[cfg(test)]
mod blsr;

#[cfg(test)]
mod andn;

#[cfg(test)]
mod bextr;
