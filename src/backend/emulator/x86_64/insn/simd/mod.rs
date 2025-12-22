//! SSE/AVX SIMD instruction implementations.
//!
//! This module contains all SIMD-related instructions organized into submodules:
//! - `mov`: Data movement (MOVD, MOVQ, MOVDQA, MOVDQU)
//! - `convert`: Type conversion (CVT* instructions)
//! - `arith`: Arithmetic (ADD, SUB, MUL, DIV, SQRT)
//! - `compare`: Comparisons (CMPPS, CMPPD, CMPSS, CMPSD)
//! - `shuffle`: Shuffle and unpack (PSHUFD, UNPCKLPS, UNPCKHPS)
//! - `minmax`: Min/max operations (MINPS, MAXPS, MINPD, MAXPD)

mod arith;
mod compare;
mod convert;
mod minmax;
mod mov;
mod shuffle;

// Re-export all instruction functions
pub use arith::*;
pub use compare::*;
pub use convert::*;
pub use minmax::*;
pub use mov::*;
pub use shuffle::*;
