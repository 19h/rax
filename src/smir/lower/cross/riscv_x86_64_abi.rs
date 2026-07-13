//! Shared helper ABI constants for RISC-V SMIR lowered to x86-64.
//!
//! This module is deliberately independent of the `smir-jit` executor.  The
//! pure cross-lowerer is available in no-default-feature builds, while the
//! runtime that supplies these helpers is optional.

/// Operation codes accepted by the RISC-V atomic read-modify-write helper.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiscVAtomicOpCode {
    Add = 0,
    Sub = 1,
    Neg = 2,
    And = 3,
    Or = 4,
    Xor = 5,
    Nand = 6,
    Max = 7,
    Min = 8,
    Umax = 9,
    Umin = 10,
    Swap = 11,
}

/// Ordering codes accepted by the RISC-V atomic helper ABI.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiscVMemoryOrderCode {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcqRel = 3,
    SeqCst = 4,
}
