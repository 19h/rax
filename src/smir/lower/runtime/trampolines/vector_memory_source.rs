//! Fail-closed helper-backed VEX packed-binary memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous `VLoad` plus VEX packed-binary sequence consumed by the
/// helper-backed lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexBinaryMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) width: VecWidth,
    pub(crate) prefix: X86SsePrefix,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) needs_avx2: bool,
}

fn low_vex_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

/// Validate one full-width, unmasked VEX.128/VEX.256 packed logic or integer
/// add/subtract memory source. The lifter represents these instructions as one
/// virtual `VLoad` immediately consumed by the corresponding binary operation.
/// Exact single-definition/single-use checks prevent the fused lowerer from
/// hiding any independently observable virtual value.
///
/// The classifier is O(1); callers build the definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_binary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let (destination, source1, source2, consumer_width, integer_arithmetic) = match &consumer.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, *width, false),
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } => (
            dst,
            src1,
            src2,
            super::x86_vector_width_from_lanes(*elem, *lanes)?,
            true,
        ),
        _ => return None,
    };
    if *source2 != temporary || consumer_width != width {
        return None;
    }
    let destination = low_vex_vector_index(destination, width)?;
    let source1 = low_vex_vector_index(source1, width)?;
    let Some(X86OpHint::VexOp {
        map,
        pp: prefix,
        opcode,
        width: hint_width,
        w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if hint_width != width {
        return None;
    }
    let needs_avx2 = if integer_arithmetic {
        if !super::x86_vector_integer_arithmetic_map_valid(&consumer.kind, map)
            || !super::x86_vector_integer_arithmetic_encoding_valid(
                &consumer.kind,
                prefix,
                opcode,
                false,
                w,
            )
        {
            return None;
        }
        let (needs_avx, needs_avx2, needs_avx512vl) =
            super::x86_vector_integer_arithmetic_feature_requirements(consumer);
        if !needs_avx || needs_avx512vl {
            return None;
        }
        needs_avx2
    } else {
        if map != X86VecMap::Map0F
            || !super::x86_vector_logic_encoding_valid(&consumer.kind, prefix, opcode, false, w)
        {
            return None;
        }
        let (needs_avx, needs_avx2, needs_avx512dq, needs_avx512vl) =
            super::x86_vector_logic_feature_requirements(consumer);
        if !needs_avx || needs_avx512dq || needs_avx512vl {
            return None;
        }
        needs_avx2
    };

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        destination,
        source1,
        width,
        prefix,
        opcode,
        w,
        needs_avx2,
    })
}
