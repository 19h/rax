//! ARM architecture definitions and ISA support.
//!
//! This module provides comprehensive ARM architecture definitions including:
//! - Hierarchical ISA selection (Profile -> Version -> Extensions)
//! - Execution state management (ARM, Thumb, AArch32, AArch64)
//! - System register encoding/decoding for CP15 and AArch64
//! - Feature flags for optional extensions
//! - Instruction decoding for all ARM execution states
//!
//! # Architecture Hierarchy
//!
//! ARM processors are organized by profile:
//! - **A-profile**: Application processors (Cortex-A, mobile/server)
//! - **R-profile**: Real-time processors (Cortex-R, automotive/industrial)
//! - **M-profile**: Microcontroller processors (Cortex-M, embedded)
//!
//! Each profile has multiple architecture versions (v6, v7, v8, v9) with
//! different mandatory and optional features.
//!
//! # Instruction Decoder
//!
//! The decoder module provides comprehensive instruction decoding for:
//! - AArch64 (A64): 64-bit ARM instructions
//! - AArch32 (A32): 32-bit ARM instructions
//! - Thumb (T16): 16-bit compact instructions
//! - Thumb-2 (T32): Mixed 16/32-bit instructions
//!
//! ```ignore
//! use rax::arm::decoder::{Decoder, DecodedInsn};
//!
//! let decoder = Decoder::new_aarch64();
//! let insn = decoder.decode(&[0x20, 0x00, 0x80, 0xd2]).unwrap(); // mov x0, #1
//! println!("{}: {:?}", insn.mnemonic, insn.operands);
//! ```

pub mod cp15;
pub mod decoder;
pub mod execution;
pub mod features;
pub mod instructions;
pub mod isa;
pub mod state;
pub mod sysreg;
pub mod vfp;

pub use features::*;
pub use isa::*;
pub use state::*;
pub use sysreg::{Aarch64SysReg, Cp15Encoding, Aarch64SysRegEncoding};
pub use decoder::{Decoder, DecodedInsn, DecodeError, Mnemonic, Condition};
pub use execution::{
    Armv7Cpu, Psr, ProcessorMode, ArmMemory, MemoryError, FlatMemory,
};
pub use instructions::{Executor, ExecResult, ExceptionType};
