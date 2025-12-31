//! x86_64 CPU emulator implementation.

mod aes;
mod cpu;
mod decoder;
mod dispatch;
pub mod flags;
mod mmu;
mod sha;
mod simd_native;
mod threaded;
pub mod timing;

mod insn;

pub use cpu::{get_total_instruction_count, X86_64Vcpu, RIP_HISTORY, RIP_IDX, CURRENT_RIP};
pub use mmu::{AccessType, Mmu};
