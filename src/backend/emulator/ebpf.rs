//! Compatibility path for the eBPF / Solana SBPF software interpreter.
//!
//! New code should import eBPF and Solana SBPF ISA types from [`crate::isa::ebpf`].

pub use crate::isa::ebpf::*;
