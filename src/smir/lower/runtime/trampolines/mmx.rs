//! Fail-closed native MMX region admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign, X86X87ControlKind};
use crate::smir::ir::types::{
    ArchReg, BlockId, VLaneOp, VReg, VecElementType, VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction};

/// Exact host encoding selected for a helper-backed MMX m64 source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MmxM64SourceEncoding {
    pub(crate) map: crate::smir::ir::ops::X86VecMap,
    pub(crate) opcode: u8,
    pub(crate) dst_index: u8,
    pub(crate) immediate: Option<u8>,
    pub(crate) requires_ssse3: bool,
}

/// Exact contiguous lifted sequence consumed by helper-backed lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MmxM64SourceSequence {
    pub(crate) consumed: usize,
    pub(crate) marker_offset: usize,
    pub(crate) encoding: X86MmxM64SourceEncoding,
}

fn mm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Mm(index @ 0..=7))) => Some(index),
        _ => None,
    }
}

/// Replace only the architecturally encoded m64 source with the destination MM
/// register in a clone, then reuse the register-register validator as the
/// semantic and encoding oracle. The clone is never lowered or executed.
fn x86_mmx_m64_source_encoding(op: &SmirOp, temporary: VReg) -> Option<X86MmxM64SourceEncoding> {
    use crate::smir::ir::ops::{X86SsePrefix, X86VecMap};

    let mut canonical = op.clone();
    let destination = match &mut canonical.kind {
        OpKind::X86PackedShuffleImm { dst, src, .. } if *src == temporary => {
            *src = *dst;
            *dst
        }
        OpKind::X86PackedAlignRight { dst, high, low, .. }
            if *high == *dst && *low == temporary =>
        {
            *low = *dst;
            *dst
        }
        OpKind::VByteShuffle {
            dst, src, control, ..
        } if *src == *dst && *control == temporary => {
            *control = *dst;
            *dst
        }
        OpKind::VUnary { dst, src, .. } if *src == temporary => {
            *src = *dst;
            *dst
        }
        OpKind::VAnd {
            dst, src1, src2, ..
        }
        | OpKind::VAndNot {
            dst, src1, src2, ..
        }
        | OpKind::VOr {
            dst, src1, src2, ..
        }
        | OpKind::VXor {
            dst, src1, src2, ..
        }
        | OpKind::VAdd {
            dst, src1, src2, ..
        }
        | OpKind::VSub {
            dst, src1, src2, ..
        }
        | OpKind::VAddSubSat {
            dst, src1, src2, ..
        }
        | OpKind::VCmp {
            dst, src1, src2, ..
        }
        | OpKind::VInterleave {
            dst, src1, src2, ..
        }
        | OpKind::VLane {
            dst, src1, src2, ..
        }
        | OpKind::VDotProduct {
            dst, src1, src2, ..
        }
        | OpKind::VSadBytes {
            dst, src1, src2, ..
        }
        | OpKind::VMul {
            dst, src1, src2, ..
        }
        | OpKind::VMulShiftSat {
            dst, src1, src2, ..
        }
        | OpKind::VHorizontalBin {
            dst, src1, src2, ..
        } if *src1 == *dst && *src2 == temporary => {
            *src2 = *dst;
            *dst
        }
        OpKind::VPackSat {
            dst, src1, src2, ..
        } if *src2 == *dst && *src1 == temporary => {
            *src1 = *dst;
            *dst
        }
        OpKind::X86PackedShift {
            dst, src, count, ..
        } if *src == *dst && *count == temporary => {
            *count = *dst;
            *dst
        }
        _ => return None,
    };
    let dst_index = mm_index(destination)?;
    if !super::is_x86_native_mmx_op(&canonical) {
        return None;
    }
    let opcode = match canonical.x86_hint {
        Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        }) => opcode,
        _ => return None,
    };
    let requires_ssse3 = x86_native_mmx_op_requires_ssse3(&canonical);
    let (map, immediate) = match canonical.kind {
        OpKind::X86PackedShuffleImm { imm, .. } => (X86VecMap::Map0F, Some(imm)),
        OpKind::X86PackedAlignRight { amount, .. } => (X86VecMap::Map0F3A, Some(amount)),
        _ if requires_ssse3 => (X86VecMap::Map0F38, None),
        _ => (X86VecMap::Map0F, None),
    };
    Some(X86MmxM64SourceEncoding {
        map,
        opcode,
        dst_index,
        immediate,
        requires_ssse3,
    })
}

/// Validate one exact `VLoad(V64 virtual)` plus MMX operation and architectural
/// `EnterMmx` marker. Both marker orders emitted by current lifters are legal;
/// the helper load must remain first so a fault cannot change MMX state.
pub(crate) fn x86_jit_mmx_m64_source_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86MmxM64SourceSequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr) = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: VecWidth::V64,
        } => (*temporary, addr),
        _ => return None,
    };
    if !matches!(
        load.x86_hint,
        None | Some(X86OpHint::VecAlign(
            X86VecAlign::Unaligned | X86VecAlign::Aligned
        ))
    ) || !super::x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let second = block.ops.get(index + 1)?;
    let third = block.ops.get(index + 2)?;
    if second.guest_pc != load.guest_pc || third.guest_pc != load.guest_pc {
        return None;
    }
    let is_marker = |op: &SmirOp| {
        matches!(
            op.kind,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            }
        ) && op.x86_hint.is_none()
    };
    let (consumed, marker_offset, encoding) = if is_marker(second) {
        (3, 1, x86_mmx_m64_source_encoding(third, temporary)?)
    } else if is_marker(third) {
        (3, 2, x86_mmx_m64_source_encoding(second, temporary)?)
    } else {
        use crate::smir::ir::types::{SignExtend, VecElementType};

        let fourth = block.ops.get(index + 3)?;
        let count = match &second.kind {
            OpKind::VExtractLane {
                dst: count @ VReg::Virtual(_),
                vec,
                lane: 0,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
            } if *vec == temporary && second.x86_hint.is_none() => *count,
            _ => return None,
        };
        if third.guest_pc != load.guest_pc
            || fourth.guest_pc != load.guest_pc
            || !is_marker(fourth)
            || virtual_definitions.get(&count) != Some(&1)
            || virtual_uses.get(&count) != Some(&1)
        {
            return None;
        }
        (4, 3, x86_mmx_m64_source_encoding(third, count)?)
    };
    Some(X86MmxM64SourceSequence {
        consumed,
        marker_offset,
        encoding,
    })
}

pub(crate) fn x86_jit_mmx_m64_source_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_mmx_m64_source_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|sequence| sequence.consumed)
}

/// Whether an executable (non-exit) block enters architectural MMX state.
pub fn uses_x86_native_mmx_excluding(
    func: &SmirFunction,
    excluded: &HashMap<BlockId, u64>,
) -> bool {
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .any(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                }
            )
        })
}

pub(crate) fn x86_native_mmx_op_requires_ssse3(op: &SmirOp) -> bool {
    matches!(
        op.kind,
        OpKind::VUnary {
            op: VecUnaryOp::Abs,
            ..
        } | OpKind::X86PackedAlignRight {
            width: VecWidth::V64,
            ..
        } | OpKind::VByteShuffle {
            lanes: 8,
            block_lanes: 8,
            ..
        } | OpKind::VLane {
            op: VLaneOp::Sign,
            ..
        } | OpKind::VHorizontalBin { .. }
            | OpKind::VDotProduct {
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                src1_unsigned: true,
                saturate: true,
                ..
            }
            | OpKind::VMulShiftSat {
                src_elem: VecElementType::I16,
                signed1: true,
                signed2: true,
                round: true,
                out_shift: 15,
                ..
            }
    ) && super::is_x86_native_mmx_op(op)
}

/// Verify host extensions required by admitted native MMX opcodes without
/// coupling MMX-only regions to the AVX-512 vector-state trampoline gate.
pub fn x86_native_mmx_features_supported_excluding(
    func: &SmirFunction,
    excluded: &HashMap<BlockId, u64>,
) -> bool {
    let mut needs_ssse3 = false;
    for block in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
    {
        if block.ops.iter().any(x86_native_mmx_op_requires_ssse3) {
            needs_ssse3 = true;
            break;
        }
        let mut virtual_definitions = HashMap::new();
        let mut virtual_uses = HashMap::new();
        for op in &block.ops {
            for reg in op.kind.dests() {
                if matches!(reg, VReg::Virtual(_)) {
                    *virtual_definitions.entry(reg).or_insert(0usize) += 1;
                }
            }
            for reg in op.kind.source_vregs() {
                if matches!(reg, VReg::Virtual(_)) {
                    *virtual_uses.entry(reg).or_insert(0usize) += 1;
                }
            }
        }
        let mut index = 0;
        while index < block.ops.len() {
            if let Some(sequence) = x86_jit_mmx_m64_source_sequence(
                block,
                index,
                true,
                &virtual_definitions,
                &virtual_uses,
            ) {
                if sequence.encoding.requires_ssse3 {
                    needs_ssse3 = true;
                    break;
                }
                index += sequence.consumed;
            } else {
                index += 1;
            }
        }
        if needs_ssse3 {
            break;
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        !needs_ssse3 || std::is_x86_feature_detected!("ssse3")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        !needs_ssse3
    }
}

/// Verify the exact architectural-state marker paired with every admitted MMX
/// operation, including helper-backed m64 source sequences.
pub fn x86_native_mmx_pairs_valid_excluding(
    func: &SmirFunction,
    excluded: &HashMap<BlockId, u64>,
) -> bool {
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .all(|block| {
            let mut virtual_definitions = HashMap::new();
            let mut virtual_uses = HashMap::new();
            for op in &block.ops {
                for reg in op.kind.dests() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *virtual_definitions.entry(reg).or_insert(0usize) += 1;
                    }
                }
                for reg in op.kind.source_vregs() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *virtual_uses.entry(reg).or_insert(0usize) += 1;
                    }
                }
            }
            let is_enter = |op: &SmirOp| {
                matches!(
                    op.kind,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    }
                ) && op.x86_hint.is_none()
            };
            if block.ops.iter().any(|op| {
                matches!(
                    op.kind,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    }
                ) && !is_enter(op)
            }) {
                return false;
            }
            let mut index = 0;
            while index < block.ops.len() {
                if let Some(consumed) = x86_jit_mmx_m64_source_sequence_len(
                    block,
                    index,
                    true,
                    &virtual_definitions,
                    &virtual_uses,
                ) {
                    index += consumed;
                    continue;
                }
                let first = &block.ops[index];
                let first_is_mmx =
                    super::is_x86_native_mmx_op(first) || super::x86_jit_mmx_mem_shape_valid(first);
                if is_enter(first) || first_is_mmx {
                    let Some(second) = block.ops.get(index + 1) else {
                        return false;
                    };
                    let second_is_mmx = super::is_x86_native_mmx_op(second)
                        || super::x86_jit_mmx_mem_shape_valid(second);
                    let paired = first.guest_pc == second.guest_pc
                        && ((is_enter(first) && second_is_mmx)
                            || (first_is_mmx && is_enter(second)));
                    if !paired {
                        return false;
                    }
                    index += 2;
                } else {
                    index += 1;
                }
            }
            true
        })
}
