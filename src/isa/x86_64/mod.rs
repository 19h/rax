//! x86_64 CPU emulator implementation.

pub mod bios;
mod cpu;
mod decode;
pub(crate) mod execute;
pub mod flags;
pub(crate) mod memory;
mod simd_native;
mod threaded;

pub use cpu::{CURRENT_RIP, RIP_HISTORY, RIP_IDX, X86_64Vcpu, get_total_instruction_count};
pub use memory::{AccessType, Mmu};

// Compatibility path for callers that previously reached VM-wide timing
// through the x86-64 ISA namespace. Internal code uses `crate::vm::timing`.
#[doc(hidden)]
pub use crate::vm::timing;

// Compatibility alias for the former file/module name.
pub(crate) use memory as mmu;
