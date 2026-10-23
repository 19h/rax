mod cpu;
pub mod decode;
pub(crate) mod memory_access;
pub(crate) mod opcode;
mod semantics;

pub use cpu::HexagonVcpu;
