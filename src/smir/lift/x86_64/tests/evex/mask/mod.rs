//! evex::mask tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

// ---- even-chunked tests ----
#[cfg(test)]
mod conversions;
#[cfg(test)]
mod part1;
#[cfg(test)]
mod part2;
