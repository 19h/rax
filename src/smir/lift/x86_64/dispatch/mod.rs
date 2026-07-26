//! Top-level opcode map dispatch (legacy, 0F, 0F38, 0F3A, VEX/EVEX)

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

// ---- module tree (auto-split) ----
mod legacy;
pub(crate) use legacy::*;
mod misc;
pub(crate) use misc::*;
mod opcode_maps;
pub(crate) use opcode_maps::*;
mod vector;
pub(crate) use vector::*;
mod vector_fp_flag_compare;
pub(crate) use vector_fp_flag_compare::*;
mod vector_map0f38;
pub(crate) use vector_map0f38::*;
mod vector_map0f3a;
pub(crate) use vector_map0f3a::*;
