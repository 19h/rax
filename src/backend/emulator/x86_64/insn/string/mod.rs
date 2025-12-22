//! String instructions: MOVS, STOS, LODS, SCAS, CMPS with REP prefix support.

mod cmps;
mod lods;
mod movs;
mod scas;
mod stos;

// Re-export all instruction functions
pub use cmps::*;
pub use lods::*;
pub use movs::*;
pub use scas::*;
pub use stos::*;
