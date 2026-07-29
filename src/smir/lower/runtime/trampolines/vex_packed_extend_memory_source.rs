//! Fail-closed helper-backed VEX packed sign/zero-extension memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand,
    VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VPMOVSX*`/`VPMOVZX*` memory-source decomposition
/// consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPackedExtendMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source_element: VecElementType,
    pub(crate) destination_element: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) signed: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
}

fn virtual_counts_are(
    register: VReg,
    definitions: usize,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&definitions)
        && virtual_uses.get(&register) == Some(&uses)
}

fn low_vex_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

fn memory_width(element: VecElementType) -> MemWidth {
    match element {
        VecElementType::I8 => MemWidth::B1,
        VecElementType::I16 => MemWidth::B2,
        VecElementType::I32 => MemWidth::B4,
        _ => unreachable!("validated packed-extension source element"),
    }
}

fn source_vector_width(memory_size: u32) -> VecWidth {
    if memory_size <= 8 {
        VecWidth::V64
    } else {
        VecWidth::V128
    }
}

/// Validate the complete per-lane SMIR decomposition of one AVX/AVX2 VEX
/// packed sign/zero-extension memory source. Exact source-byte provenance
/// binds the destination, element conversion, vector length, opcode, and
/// ignored W bit. Every virtual definition/use count, lane address, element
/// width, sign mode, and guest-PC boundary is checked before the graph may be
/// replaced by one precise helper load and a register-source host instruction.
///
/// Classification is O(L) time and O(L) auxiliary space for L destination
/// lanes; architectural VEX bounds L to at most 16. Callers build global
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_packed_extend_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPackedExtendMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if first.x86_hint.is_some() || (index != 0 && block.ops[index - 1].guest_pc == first.guest_pc) {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let (encoded_destination, source_element, destination_element, width, signed, opcode, w) =
        instruction.vex_memory_packed_extend_fields()?;
    let lanes = width.lanes(destination_element) as usize;
    let memory_size = (lanes as u32) * source_element.bytes();
    let source_lanes = source_vector_width(memory_size).lanes(source_element) as usize;
    let expected_sign = if signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let same_frontier = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == first.guest_pc && op.x86_hint.is_none())
    };
    let mut virtuals = HashSet::new();

    let source_zero = match &first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        _ => return None,
    };
    if !virtuals.insert(source_zero)
        || !virtual_counts_are(source_zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let source = match &block.ops.get(index + 1)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: broadcast_lanes,
        } if *scalar == source_zero
            && *elem == source_element
            && usize::from(*broadcast_lanes) == source_lanes =>
        {
            *dst
        }
        _ => return None,
    };
    if !same_frontier(1)
        || !virtuals.insert(source)
        || !virtual_counts_are(
            source,
            lanes + 1,
            lanes * 2,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let address_base = match &block.ops.get(index + 2)?.kind {
        OpKind::Lea { dst, addr } if x86_jit_mem_address_shape_valid(addr) => *dst,
        _ => return None,
    };
    if !same_frontier(2)
        || !virtuals.insert(address_base)
        || !virtual_counts_are(address_base, 1, lanes, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let mut extracted = Vec::with_capacity(lanes);
    let lane_width = memory_width(source_element);
    for lane in 0..lanes {
        let lane_offset = 3 + lane * 3;
        let scalar = match &block.ops.get(index + lane_offset)?.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => *dst,
            _ => return None,
        };
        if !same_frontier(lane_offset)
            || !virtuals.insert(scalar)
            || !virtual_counts_are(scalar, 2, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }

        if !matches!(
            &block.ops.get(index + lane_offset + 1)?.kind,
            OpKind::Load {
                dst,
                addr: Address::BaseOffset {
                    base,
                    offset,
                    disp_size: DispSize::Auto,
                },
                width,
                sign: SignExtend::Zero,
            } if *dst == scalar
                && *base == address_base
                && *offset == (lane as i64) * i64::from(source_element.bytes())
                && *width == lane_width
        ) || !same_frontier(lane_offset + 1)
        {
            return None;
        }

        if !matches!(
            &block.ops.get(index + lane_offset + 2)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem,
            } if *dst == source
                && *vec == source
                && *inserted == scalar
                && usize::from(*inserted_lane) == lane
                && *elem == source_element
        ) || !same_frontier(lane_offset + 2)
        {
            return None;
        }
    }

    let extract_start = 3 + lanes * 3;
    for lane in 0..lanes {
        let offset = extract_start + lane;
        let scalar = match &block.ops.get(index + offset)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extracted_lane,
                elem,
                sign,
            } if *vec == source
                && usize::from(*extracted_lane) == lane
                && *elem == source_element
                && *sign == expected_sign =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_frontier(offset)
            || !virtuals.insert(scalar)
            || !virtual_counts_are(scalar, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        extracted.push(scalar);
    }

    let result_zero_offset = extract_start + lanes;
    let result_zero = match &block.ops.get(index + result_zero_offset)?.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        _ => return None,
    };
    if !same_frontier(result_zero_offset)
        || !virtuals.insert(result_zero)
        || !virtual_counts_are(result_zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let destination = match &block.ops.get(index + result_zero_offset + 1)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: broadcast_lanes,
        } if *scalar == result_zero
            && *elem == destination_element
            && usize::from(*broadcast_lanes) == lanes =>
        {
            low_vex_vector_index(dst, width)?
        }
        _ => return None,
    };
    if !same_frontier(result_zero_offset + 1) || destination != encoded_destination {
        return None;
    }

    let insert_start = result_zero_offset + 2;
    let destination_reg = match width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(destination))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(destination))),
        _ => unreachable!("validated VEX packed-extension width"),
    };
    for (lane, scalar) in extracted.into_iter().enumerate() {
        let offset = insert_start + lane;
        if !matches!(
            &block.ops.get(index + offset)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem,
            } if *dst == destination_reg
                && *vec == destination_reg
                && *inserted == scalar
                && usize::from(*inserted_lane) == lane
                && *elem == destination_element
        ) || !same_frontier(offset)
        {
            return None;
        }
    }

    let consumed = insert_start + lanes;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }
    Some(X86JitVexPackedExtendMemorySequence {
        consumed,
        memory_size,
        destination,
        source_element,
        destination_element,
        width,
        signed,
        opcode,
        w,
    })
}
