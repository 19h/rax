//! SSE/AVX/AVX-512 SIMD instruction implementations.
//!
//! This module contains all SIMD-related instructions organized into submodules:
//! - `mov`: Data movement (MOVD, MOVQ, MOVDQA, MOVDQU)
//! - `sse`: Packed SSE operations (MOVUPS, MOVAPS, ANDPS, ORPS, XORPS)
//! - `convert`: Type conversion (CVT* instructions)
//! - `arith`: Arithmetic (ADD, SUB, MUL, DIV, SQRT)
//! - `compare`: Comparisons (CMPPS, CMPPD, CMPSS, CMPSD)
//! - `shuffle`: Shuffle and unpack (PSHUFD, UNPCKLPS, UNPCKHPS)
//! - `minmax`: Min/max operations (MINPS, MAXPS, MINPD, MAXPD)
//! - `avx512`: AVX-512 instructions (EVEX-encoded)

mod arith;
mod avx;
mod avx512;
mod avx512_align;
mod avx512_blend;
mod avx512_bw;
mod avx512_chunk_extract;
mod avx512_chunk_insert;
mod avx512_chunk_shuffle;
mod avx512_compare;
mod avx512_fp_class;
mod avx512_gpr_broadcast;
mod avx512_mask_convert;
mod avx512_pair_intersect;
mod compare;
mod convert;
mod gfni;
mod minmax;
mod mov;
pub(crate) mod pcmpxstrx;
mod shuffle;
mod sse;
mod sse4;
mod sse4a;
mod ssse3;

// Re-export all instruction functions
pub use arith::*;
pub use avx::*;
pub use avx512::*;
pub use avx512_align::*;
pub use avx512_blend::*;
pub use avx512_bw::*;
pub use avx512_chunk_extract::*;
pub use avx512_chunk_insert::*;
pub use avx512_chunk_shuffle::*;
pub use avx512_compare::*;
pub use avx512_fp_class::*;
pub use avx512_gpr_broadcast::*;
pub use avx512_mask_convert::*;
pub use avx512_pair_intersect::*;
pub use compare::*;
pub use convert::*;
pub use gfni::*;
pub use minmax::*;
pub use mov::*;
pub use shuffle::*;
pub use sse::*;
pub use sse4::*;
pub use sse4a::*;
pub use ssse3::*;
