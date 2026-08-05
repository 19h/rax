//! Fail-closed helper-backed EVEX packed integer absolute-value admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecUnaryOp,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexIntegerArithmeticMemoryReplay, X86EvexPackedAbsMemoryEncoding, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryMatch, X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape,
    exact_evex_e4_memory_sequence_tail, exact_evex_memory_apx_frontier,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result, exact_lane_predicate,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// `VPABSB`/`VPABSW`/`VPABSD`/`VPABSQ` memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedAbsMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedAbsMemoryEncoding,
}

#[allow(clippy::too_many_arguments)]
fn exact_abs_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    memory_source: VReg,
    encoding: X86EvexPackedAbsMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let operation = block.ops.get(index)?;
    let raw = match operation.kind {
        OpKind::VUnary {
            dst,
            src,
            elem,
            lanes,
            op: VecUnaryOp::Abs,
        } if src == memory_source
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8
            && operation.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: encoding.opcode,
                    width: encoding.width,
                    w: encoding.w,
                }) =>
        {
            dst
        }
        _ => return None,
    };
    if operation.guest_pc != guest_pc {
        return None;
    }

    let mut offset = 1usize;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else if encoding.zeroing || vector_index(&raw, encoding.width) != Some(encoding.destination) {
        return None;
    }
    Some(offset)
}

fn memory_width(encoding: X86EvexPackedAbsMemoryEncoding) -> MemWidth {
    match encoding.elem {
        crate::smir::ir::types::VecElementType::I8 => MemWidth::B1,
        crate::smir::ir::types::VecElementType::I16 => MemWidth::B2,
        crate::smir::ir::types::VecElementType::I32 => MemWidth::B4,
        crate::smir::ir::types::VecElementType::I64 => MemWidth::B8,
        _ => unreachable!("validated packed-abs element"),
    }
}

/// Match the lifter's original lane-wise OR reduction used by masked VPABS
/// broadcasts. Other E4 lifters use the compact nonzero-mask normalization
/// accepted by the common E4 matcher; retaining this exact VPABS form makes
/// native admission optimizer-level invariant without weakening either grammar.
#[allow(clippy::too_many_arguments)]
fn exact_lane_or_masked_broadcast(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedAbsMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86EvexE4MemoryMatch> {
    if !matches!(
        encoding.replay,
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. }
    ) {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let zero = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !single_definition_single_use(zero, virtual_definitions, virtual_uses) {
        return None;
    }

    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let mut active = zero;
    let mut offset = 1usize;
    for lane in 0..lanes {
        let bit = exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;
        let or = block.ops.get(index + offset)?;
        let combined = match or.kind {
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if or.x86_hint.is_none() && src1 == active && src2 == bit => dst,
            _ => return None,
        };
        if or.guest_pc != guest_pc
            || !single_definition_single_use(combined, virtual_definitions, virtual_uses)
        {
            return None;
        }
        active = combined;
        offset += 1;
    }

    let seed = block.ops.get(index + offset)?;
    let scalar = match seed.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if seed.x86_hint.is_none() => dst,
        _ => return None,
    };
    if seed.guest_pc != guest_pc
        || !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let address_offset = offset;
    let expected_width = memory_width(encoding);
    let load = block.ops.get(index + offset)?;
    let address = match &load.kind {
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *dst == scalar
            && *cond == active
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            addr
        }
        _ => return None,
    };
    if load.guest_pc != guest_pc || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    offset += exact_abs_tail(
        block,
        index + offset,
        guest_pc,
        loaded,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX packed
/// integer absolute-value memory source.
///
/// Exact byte provenance binds map 0F38, opcode, W/WIG, reserved vvvv/V',
/// vector and element widths, destination, mask policy, tuple form, address,
/// every active-lane Type E4 access, the exact `VUnary(Abs)` consumer, every
/// merge/zero lane, the APX address guard, and the guest-PC frontier. Runtime
/// is O(L) with O(1) auxiliary space for L <= 64 lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_abs_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedAbsMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_packed_abs_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexIntegerArithmeticMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. } => {
            X86EvexE4MemoryReplayForm::Broadcast
        }
        X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
        memory_source_uses: 1,
    };
    let exact = exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, memory_source| {
            exact_abs_tail(
                block,
                tail_index,
                guest_pc,
                memory_source,
                encoding,
                virtual_definitions,
                virtual_uses,
            )
        },
    )
    .or_else(|| {
        exact_lane_or_masked_broadcast(block, index, encoding, virtual_definitions, virtual_uses)
    })?;

    Some(X86JitEvexPackedAbsMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
