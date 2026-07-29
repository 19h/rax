//! tests::evex tests

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod arithmetic;
#[cfg(test)]
mod fp;
#[cfg(test)]
mod fp16_convert;
#[cfg(test)]
mod fp_unpack;
#[cfg(test)]
mod integer;
#[cfg(test)]
mod legacy;
#[cfg(test)]
mod logic_broadcast;
#[cfg(test)]
mod mask;
#[cfg(test)]
mod ops;
#[cfg(test)]
mod packed_compare;
#[cfg(test)]
mod permute;
#[cfg(test)]
mod saturating_convert;
#[cfg(test)]
mod scalar_integer_convert;
#[cfg(test)]
mod sqrt;
use crate::smir::lift::x86_64::*;
