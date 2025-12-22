//! Instruction dispatch modules for the x86_64 CPU emulator.
//!
//! This module contains the opcode dispatch logic, split by encoding:
//! - `legacy`: Single-byte opcode dispatch
//! - `twobyte`: Two-byte (0x0F-prefixed) opcode dispatch
//! - `vex`: VEX-encoded (AVX) instruction dispatch

mod legacy;
mod twobyte;
mod vex;
