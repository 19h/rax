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
// BMI2 Instructions:
// - MULX: Unsigned Multiply Without Affecting Flags (dest1:dest2 = rdx * src)
// - SARX: Arithmetic Right Shift Without Affecting Flags
// - SHLX: Logical Left Shift Without Affecting Flags
// - SHRX: Logical Right Shift Without Affecting Flags
// - RORX: Rotate Right Logical Without Affecting Flags
// - BZHI: Zero High Bits Starting with Specified Bit Position
//
// Additional Instructions:
// - POPCNT: Count Number of Bits Set to 1
//
// Each test file contains:
// - Basic functionality tests
// - 16/32/64-bit variants (where applicable)
// - Flag behavior tests (ZF, CF, SF, OF) or verification flags are NOT modified
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

#[cfg(test)]
mod mulx;

#[cfg(test)]
mod popcnt;

#[cfg(test)]
mod sarx_shlx_shrx;

#[cfg(test)]
mod sarx_shlx_shrx_extended;

#[cfg(test)]
mod rorx;

#[cfg(test)]
mod bzhi_extended;
