//! tests::misc tests

use super::*;
use crate::smir::optimize::*;

// ---- even-chunked tests ----
#[cfg(test)]
mod fp_binary;
#[cfg(test)]
mod part1;
#[cfg(test)]
mod part2;
#[cfg(test)]
mod part3;
#[cfg(test)]
mod saturating_convert;
#[cfg(test)]
mod scalar_fp_convert;
#[cfg(test)]
mod scalar_fp_to_int;
#[cfg(test)]
mod scalar_int_to_fp;
#[cfg(test)]
mod x87_control;
#[cfg(test)]
mod x87_transcendental;
