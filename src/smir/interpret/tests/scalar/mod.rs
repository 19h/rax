//! tests::scalar tests

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod addr32;
#[cfg(test)]
mod fp;
#[cfg(test)]
mod hex;
#[cfg(test)]
mod memory;
#[cfg(test)]
mod misc;
#[cfg(test)]
mod shift_group6;
#[cfg(test)]
mod tsx;
use crate::smir::interpret::*;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::flags::{FlagSet, FlagUpdate, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::types::ShiftOp;
