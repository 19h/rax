//! Formal verification of the rax eBPF implementation.
//!
//! [`spec`] is an independent reference semantics written from RFC 9669, and
//! [`registry`] is the RFC's Appendix A instruction registry, generated from
//! the checked-in RFC text. The Kani proofs in `proofs` compare the engine's
//! decoder and every execution plane with them, for each opcode byte and
//! byte order, over every value of every instruction field and every
//! register and memory state. The proofs in `sbpf` compare the engine's
//! Solana SBPF verifier and CPU with the pinned Anza `solana-sbpf` verifier
//! and interpreter, for each version and opcode byte. Run them with
//! `tools/ebpf-formal/run.py`.

// Each `#[kani::stub]` is one nested attribute-macro expansion; the
// direct-plane harnesses carry seventeen.
#![recursion_limit = "256"]

pub mod registry;
pub mod spec;

#[cfg(kani)]
mod proofs;
#[cfg(kani)]
mod sbpf;
