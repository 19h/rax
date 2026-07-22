//! jit_gate_tests::gate tests

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod crc32;
#[cfg(test)]
mod memory;
#[cfg(test)]
mod misc;
#[cfg(test)]
mod native;
#[cfg(test)]
mod scalar;
#[cfg(test)]
mod state;
#[cfg(test)]
mod tsx;
use crate::smir::lower::runtime::*;
