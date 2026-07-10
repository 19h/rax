//! Compatibility path for the Hexagon software interpreter.
//!
//! New code should import Hexagon ISA types from [`crate::isa::hexagon`].

pub(crate) use crate::isa::hexagon::opcode;
pub use crate::isa::hexagon::*;
