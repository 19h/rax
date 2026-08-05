//! Narrow x86 AVX-512 opmask-state admission.

use super::*;
use crate::smir::lower::runtime::*;

/// Whether every admitted native vector operation in executable blocks is a
/// VEXP2/VRCP14/VRSQRT14/VRCP28/VRSQRT28 operation whose opmask width is at
/// most 16 bits. Such a region can marshal K0-K7 with AVX512F KMOVW: each
/// instruction observes only the low 8/16 bits, and the trampoline leaves
/// every upper architectural bit intact in `GuestRegs`. Any additional vector
/// operation fails closed to full KMOVQ.
pub fn x86_native_vector_uses_k16_opmasks_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    let replay = x86_native_replay_feature_requirements(func, excluded);
    // Any replay span requiring the full KMOVQ bridge may observe K[63:16].
    // The exact scalar approximation spans are instead represented by the
    // explicit K16 capability bit because their decomposed semantic op has a
    // virtual memory source and is deliberately not directly admissible.
    if replay.needs_avx512bw {
        return false;
    }
    let mut saw_narrow_opmask_operation = replay.has_k16_opmask_span;
    for op in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .filter(|op| x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind))
    {
        if matches!(
            op.kind,
            crate::smir::ir::ops::OpKind::X86Exp2 { .. }
                | crate::smir::ir::ops::OpKind::X86Recip14 { .. }
                | crate::smir::ir::ops::OpKind::X86Rsqrt14 { .. }
                | crate::smir::ir::ops::OpKind::X86Recip28 { .. }
                | crate::smir::ir::ops::OpKind::X86Rsqrt28 { .. }
        ) {
            saw_narrow_opmask_operation = true;
        } else {
            return false;
        }
    }
    saw_narrow_opmask_operation
}
