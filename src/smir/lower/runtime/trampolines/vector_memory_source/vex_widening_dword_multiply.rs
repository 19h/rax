//! Exact VEX widening-dword multiply memory-source sequence admission.

use std::collections::{HashMap, HashSet};

use super::{X86JitVexBinaryMemorySequence, low_vex_vector_index, x86_jit_mem_address_shape_valid};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
};

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

pub(super) fn sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    let qwords = width.lanes(VecElementType::I64) as usize;
    if !virtual_counts_are(loaded, 1, qwords, virtual_definitions, virtual_uses) {
        return None;
    }

    let same_frontier = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc && op.x86_hint.is_none())
    };
    let mut virtuals = HashSet::from([loaded]);
    let mut source1 = None;
    let mut signed = None;
    let mut products = Vec::with_capacity(qwords);
    for lane in 0..qwords {
        let offset = 1 + lane * 3;
        let (a, lane_source1, lane_sign) = match &block.ops.get(index + offset)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: source_lane,
                elem: VecElementType::I32,
                sign,
            } if usize::from(*source_lane) == lane * 2 => (*dst, *vec, *sign),
            _ => return None,
        };
        if !same_frontier(offset)
            || !virtuals.insert(a)
            || !virtual_counts_are(a, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        if source1
            .replace(lane_source1)
            .is_some_and(|prior| prior != lane_source1)
            || signed
                .replace(lane_sign == SignExtend::Sign)
                .is_some_and(|prior| prior != (lane_sign == SignExtend::Sign))
        {
            return None;
        }

        let b = match &block.ops.get(index + offset + 1)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: source_lane,
                elem: VecElementType::I32,
                sign,
            } if *vec == loaded && usize::from(*source_lane) == lane * 2 && *sign == lane_sign => {
                *dst
            }
            _ => return None,
        };
        if !same_frontier(offset + 1)
            || !virtuals.insert(b)
            || !virtual_counts_are(b, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }

        let product = match (&block.ops.get(index + offset + 2)?.kind, lane_sign) {
            (
                OpKind::MulS {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                SignExtend::Sign,
            )
            | (
                OpKind::MulU {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                SignExtend::Zero,
            ) if *src1 == a && *src2 == b => *dst_lo,
            _ => return None,
        };
        if !same_frontier(offset + 2)
            || !virtuals.insert(product)
            || !virtual_counts_are(product, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        products.push(product);
    }

    let zero_offset = 1 + qwords * 3;
    let zero = match &block.ops.get(index + zero_offset)?.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        _ => return None,
    };
    if !same_frontier(zero_offset)
        || !virtuals.insert(zero)
        || !virtual_counts_are(zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let output = match &block.ops.get(index + zero_offset + 1)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes,
        } if *scalar == zero && usize::from(*lanes) == qwords => *dst,
        _ => return None,
    };
    if !same_frontier(zero_offset + 1)
        || !virtuals.insert(output)
        || !virtual_counts_are(
            output,
            qwords + 1,
            qwords + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let first_insert = zero_offset + 2;
    for (lane, product) in products.into_iter().enumerate() {
        let offset = first_insert + lane;
        if !matches!(
            &block.ops.get(index + offset)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: insert_lane,
                elem: VecElementType::I64,
            } if *dst == output
                && *vec == output
                && *scalar == product
                && usize::from(*insert_lane) == lane
        ) || !same_frontier(offset)
        {
            return None;
        }
    }

    let move_offset = first_insert + qwords;
    let destination = match &block.ops.get(index + move_offset)?.kind {
        OpKind::VMov {
            dst,
            src,
            width: move_width,
        } if *src == output && *move_width == width => low_vex_vector_index(dst, width)?,
        _ => return None,
    };
    if !same_frontier(move_offset) {
        return None;
    }
    let source1 = low_vex_vector_index(&source1?, width)?;
    let signed = signed?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_signed, encoded_width, _encoded_w) =
        instruction.vex_memory_widening_dword_multiply_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_signed,
        encoded_width,
    ) != (destination, source1, signed, width)
    {
        return None;
    }
    let (map, opcode) = if signed {
        (X86VecMap::Map0F38, 0x28)
    } else {
        (X86VecMap::Map0F, 0xF4)
    };

    Some(X86JitVexBinaryMemorySequence {
        consumed: move_offset + 1,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map,
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VPMULUDQ/VPMULDQ are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
