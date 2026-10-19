//! RFC 9669 execution adapters for external eBPF conformance oracles.
//!
//! The flat layout, engine adapters, and Linux-kernel campaign generator are
//! shared with the engine's `ebpf_diff` corpus replay
//! (`tests/support/ebpf_rfc_oracle.rs`); this package adds the external
//! oracle drivers and comparisons.

#[path = "../../../tests/support/ebpf_rfc_oracle.rs"]
mod shared;

pub use shared::*;

pub mod kernel;
