//! JIT runtime trampolines and guest-callout helper routines

// ---- split submodules ----
mod aarch64;
pub use aarch64::*;
mod clobber;
pub use clobber::*;
mod crc32;
pub use crc32::*;
mod jit;
pub use jit::*;
mod maskmov;
pub use maskmov::*;
mod misc;
pub use misc::*;
mod mmx;
pub use mmx::*;
mod mmx_memory;
pub use mmx_memory::*;
mod vector;
use crate::smir::lower::runtime::*;
use crate::smir::lower::{
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GPR_COUNT, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MM_OFFSET, X86_GUEST_MMX_ACTIVE_OFFSET,
    X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET, X86_GUEST_PAIR_STORE_FN_OFFSET,
    X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET, X86_GUEST_TSC_AUX_OFFSET,
    X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET, X86_GUEST_VECTOR_ACTIVE_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};
pub use vector::*;
