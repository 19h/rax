//! I/O instructions: IN, OUT, INSB, INSW, OUTSB, OUTSW.

mod permission;
mod port;
mod string;

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(in crate::isa::x86_64) use permission::IoPermissionState;

// Re-export all instruction functions
pub use port::*;
pub use string::*;
