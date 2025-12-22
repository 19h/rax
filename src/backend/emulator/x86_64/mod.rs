//! x86_64 CPU emulator implementation.

mod cpu;
mod decoder;
mod dispatch;
pub mod flags;
mod mmu;

mod insn;

pub use cpu::X86_64Vcpu;
pub use mmu::{AccessType, Mmu};
