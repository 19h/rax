//! tests::simd tests

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod convert;
#[cfg(test)]
mod misc;
#[cfg(test)]
mod packed;
#[cfg(test)]
mod sse;
use crate::smir::interpret::*;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::flags::{FlagSet, FlagUpdate, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::types::ShiftOp;
