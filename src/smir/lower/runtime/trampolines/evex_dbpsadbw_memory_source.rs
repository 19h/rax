//! Fail-closed helper-backed EVEX `VDBPSADBW` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86EvexDbpsadbwMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// `VDBPSADBW` memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexDbpsadbwMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexDbpsadbwMemoryEncoding,
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX VDBPSADBW width"),
    }))
}

#[allow(clippy::too_many_arguments)]
fn exact_zero_vector_seed(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    elem: VecElementType,
    lanes: u8,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let seed_op = block.ops.get(index + *offset)?;
    let seed = match seed_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: actual_elem,
            lanes: actual_lanes,
        } if seed_op.x86_hint.is_none()
            && scalar == zero
            && actual_elem == elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if seed_op.guest_pc != guest_pc
        || !single_definition_single_use(seed, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;
    Some(seed)
}

#[allow(clippy::too_many_arguments)]
fn exact_dbpsadbw_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    loaded: VReg,
    encoding: X86EvexDbpsadbwMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let dwords = encoding.width.lanes(VecElementType::I32) as u8;
    let words = encoding.width.lanes(VecElementType::I16) as u8;
    if !exact_virtual_definition_use(
        loaded,
        1,
        usize::from(dwords),
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }

    let mut shuffled = exact_zero_vector_seed(
        block,
        index,
        offset,
        guest_pc,
        VecElementType::I32,
        dwords,
        virtual_definitions,
        virtual_uses,
    )?;
    for lane in 0..dwords {
        let block_base = lane & !3;
        let selector = (encoding.immediate >> (2 * (lane & 3))) & 3;
        let extract = block.ops.get(index + *offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem: VecElementType::I32,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vec == loaded
                && actual_lane == block_base + selector =>
            {
                dst
            }
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert = block.ops.get(index + *offset)?;
        let next = match insert.kind {
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I32,
            } if insert.x86_hint.is_none()
                && vec == shuffled
                && actual_scalar == scalar
                && actual_lane == lane
                && matches!(dst, VReg::Virtual(_)) =>
            {
                dst
            }
            _ => return None,
        };
        if insert.guest_pc != guest_pc
            || !single_definition_single_use(shuffled, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;
        shuffled = next;
    }
    if !exact_virtual_definition_use(shuffled, 1, 4, virtual_definitions, virtual_uses) {
        return None;
    }

    let source1 = encoded_vector(encoding.source1, encoding.width);
    let mut partials = Vec::with_capacity(4);
    for immediate in [0u8, 9, 54, 63] {
        let sad = block.ops.get(index + *offset)?;
        let partial = match sad.kind {
            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask: None,
                width,
                imm,
                zeroing: false,
            } if sad.x86_hint.is_none()
                && src1 == shuffled
                && src2 == source1
                && width == encoding.width
                && imm == immediate
                && matches!(dst, VReg::Virtual(_)) =>
            {
                dst
            }
            _ => return None,
        };
        if sad.guest_pc != guest_pc {
            return None;
        }
        *offset += 1;
        partials.push(partial);
    }
    let partial_uses = (encoding.width.bytes() / 8) as usize;
    if partials.iter().any(|partial| {
        !exact_virtual_definition_use(*partial, 1, partial_uses, virtual_definitions, virtual_uses)
    }) {
        return None;
    }

    let mut result = exact_zero_vector_seed(
        block,
        index,
        offset,
        guest_pc,
        VecElementType::I16,
        words,
        virtual_definitions,
        virtual_uses,
    )?;
    for lane in 0..words {
        let extract = block.ops.get(index + *offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vec == partials[usize::from((lane & 7) / 2)]
                && actual_lane == lane =>
            {
                dst
            }
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert = block.ops.get(index + *offset)?;
        let next = match insert.kind {
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I16,
            } if insert.x86_hint.is_none()
                && vec == result
                && actual_scalar == scalar
                && actual_lane == lane
                && matches!(dst, VReg::Virtual(_)) =>
            {
                dst
            }
            _ => return None,
        };
        if insert.guest_pc != guest_pc
            || !single_definition_single_use(result, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;
        result = next;
    }
    Some(result)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// `VDBPSADBW` Full Mem source.
///
/// Exact provenance binds W0, vector width, imm8, operands, word writemask,
/// one unconditional E4NF.nb tuple read, every in-lane dword selector, all
/// four projected SAD computations, the destination reconstruction, APX
/// address guard, and guest-PC frontier. Runtime is O(L) and auxiliary space
/// is O(1), where L <= 32 word lanes; callers build definition/use maps once
/// in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_dbpsadbw_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexDbpsadbwMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_dbpsadbw_memory_encoding()?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && *width == encoding.width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let mut offset = 1usize;
    let raw = exact_dbpsadbw_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        loaded,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            VecElementType::I16,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                commit.kind,
                OpKind::VMov { dst, src, width }
                    if vector_index(&dst, encoding.width) == Some(encoding.destination)
                        && src == raw
                        && width == encoding.width
            )
        {
            return None;
        }
        offset += 1;
    }
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }

    Some(X86JitEvexDbpsadbwMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
