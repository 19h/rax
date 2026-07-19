//! tests::evex tests

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod fp;
#[cfg(test)]
mod integer;
#[cfg(test)]
mod legacy;
#[cfg(test)]
mod mask;
#[cfg(test)]
mod ops;
#[cfg(test)]
mod permute;
use crate::smir::lift::x86_64::*;
