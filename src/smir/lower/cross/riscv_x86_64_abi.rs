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

/// Operation codes accepted by the RISC-V scalar integer-crypto helper.
///
/// The values are an explicit cross-module ABI rather than relying on the
/// compiler-defined discriminants of [`crate::isa::riscv::Op`].
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiscVIntCryptoOpCode {
    Clmul = 0,
    Clmulh = 1,
    Clmulr = 2,
    Xperm4 = 3,
    Xperm8 = 4,
    Sha512Sig0l = 5,
    Sha512Sig0h = 6,
    Sha512Sig1l = 7,
    Sha512Sig1h = 8,
    Sha512Sum0r = 9,
    Sha512Sum1r = 10,
    Sm4ed = 11,
    Sm4ks = 12,
    Aes32esi = 13,
    Aes32esmi = 14,
    Aes32dsi = 15,
    Aes32dsmi = 16,
    Aes64es = 17,
    Aes64esm = 18,
    Aes64ds = 19,
    Aes64dsm = 20,
    Aes64im = 21,
    Aes64ks1i = 22,
    Aes64ks2 = 23,
}
