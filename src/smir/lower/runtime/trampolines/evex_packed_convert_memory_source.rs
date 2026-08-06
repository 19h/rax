//! Fail-closed helper-backed EVEX packed conversion memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedConvertMemoryEncoding, X86EvexPackedConvertMemoryKind,
    X86EvexPackedConvertMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryMatch, X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape,
    exact_evex_e4_memory_sequence, exact_evex_memory_apx_frontier,
    exact_evex_memory_sequence_frontier, exact_lane_address, exact_lane_predicate,
    exact_virtual_definition_use, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact EVEX packed conversion memory decomposition consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedConvertMemoryEncoding,
}

fn prefix(pp: u8) -> Option<X86SsePrefix> {
    match pp {
        0 => Some(X86SsePrefix::None),
        1 => Some(X86SsePrefix::OpSize),
        2 => Some(X86SsePrefix::Rep),
        3 => Some(X86SsePrefix::Repne),
        _ => None,
    }
}

fn exact_conversion(
    op: &crate::smir::ir::ops::SmirOp,
    source: VReg,
    encoding: X86EvexPackedConvertMemoryEncoding,
) -> bool {
    if op.x86_hint
        != Some(X86OpHint::EvexOp {
            map: encoding.map,
            pp: match prefix(encoding.pp) {
                Some(pp) => pp,
                None => return false,
            },
            opcode: encoding.opcode,
            width: encoding.operation_width,
            w: encoding.w,
        })
    {
        return false;
    }
    let expected_destination = match encoding.destination_width {
        crate::smir::ir::types::VecWidth::V128 => {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.destination)))
        }
        crate::smir::ir::types::VecWidth::V256 => {
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(encoding.destination)))
        }
        crate::smir::ir::types::VecWidth::V512 => {
            VReg::Arch(ArchReg::X86(X86Reg::Zmm(encoding.destination)))
        }
        _ => return false,
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));

    match (encoding.kind, op.kind.clone()) {
        (
            X86EvexPackedConvertMemoryKind::FpPrecision { from, to },
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from: actual_from,
                to: actual_to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
                report_fp16_denormal: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_from == from
                && actual_to == to
                && lanes == encoding.lanes
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        (
            X86EvexPackedConvertMemoryKind::IntToFp {
                int_elem,
                fp_elem,
                signed,
            },
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem: actual_int_elem,
                fp_elem: actual_fp_elem,
                signed: actual_signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_int_elem == int_elem
                && actual_fp_elem == fp_elem
                && actual_signed == signed
                && lanes == encoding.lanes
                && src_width == encoding.source_width
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        (
            X86EvexPackedConvertMemoryKind::FpToInt {
                fp_elem,
                int_elem,
                signed,
                truncate,
            },
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem: actual_fp_elem,
                int_elem: actual_int_elem,
                signed: actual_signed,
                truncate: actual_truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_fp_elem == fp_elem
                && actual_int_elem == int_elem
                && actual_signed == signed
                && actual_truncate == truncate
                && lanes == encoding.lanes
                && src_width == encoding.source_width
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        _ => false,
    }
}

/// Match the optimizer-stable zero-seeded vector materialization used by an
/// unmasked Type-E11 `VCVTPH2PS` memory source. A 64-bit tuple cannot be the
/// initial definition of the lifter's vector temporary, so the graph is
/// `Mov(0)`/`VBroadcast(F16)`/`VLoad`/`X86PackedFpConvert` at O0/O1/O2.
fn exact_unmasked_fp16_widen_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedConvertMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86EvexE4MemoryMatch> {
    if !matches!(
        encoding.kind,
        X86EvexPackedConvertMemoryKind::FpPrecision {
            from: VecElementType::F16,
            to: VecElementType::F32
        }
    ) || !matches!(
        encoding.replay,
        X86EvexPackedConvertMemoryReplay::Vector { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
        || encoding.broadcast
    {
        return None;
    }

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
    if !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::F16,
            lanes,
        } if broadcast.x86_hint.is_none() && scalar == zero && lanes == encoding.lanes => dst,
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(loaded, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let load = block.ops.get(index + 2)?;
    let address = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if *dst == loaded
                && *width == encoding.source_width
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            addr
        }
        _ => return None,
    };
    if load.guest_pc != guest_pc
        || !exact_conversion(block.ops.get(index + 3)?, loaded, encoding)
        || block.ops[index + 3].guest_pc != guest_pc
        || !no_following_same_pc(block, index, 4, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }

    Some(X86EvexE4MemoryMatch {
        consumed: 4,
        address_offset: 2,
        memory_size: encoding.source_width.bytes(),
    })
}

/// Match the Type-E11 masked source reconstruction. Unlike the older E4
/// graphs, each lane's zero seed precedes its `(k >> lane) & 1` predicate.
/// Active `PredLoad(B2)` operations remain ordered from lane zero upward.
fn exact_masked_fp16_widen_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedConvertMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86EvexE4MemoryMatch> {
    if !matches!(
        encoding.kind,
        X86EvexPackedConvertMemoryKind::FpPrecision {
            from: VecElementType::F16,
            to: VecElementType::F32
        }
    ) || !matches!(
        encoding.replay,
        X86EvexPackedConvertMemoryReplay::MaskedVector { .. }
    ) || encoding.broadcast
    {
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
    if !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::F16,
            lanes,
        } if broadcast.x86_hint.is_none() && scalar == zero && lanes == encoding.lanes => dst,
        _ => return None,
    };
    let lanes = usize::from(encoding.lanes);
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            lanes + 1,
            lanes + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
    let (base, address) = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*base, addr),
        _ => return None,
    };
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(base, 1, lanes, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let mut offset = address_offset + 1;
    for lane in 0..encoding.lanes {
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

        let condition = exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;
        let load = block.ops.get(index + offset)?;
        if load.guest_pc != guest_pc
            || !matches!(
                &load.kind,
                OpKind::PredLoad {
                    dst,
                    cond,
                    addr,
                    width: MemWidth::B2,
                    signed: SignExtend::Zero,
                } if load.x86_hint.is_none()
                    && *dst == scalar
                    && *cond == condition
                    && exact_lane_address(addr, base, i64::from(lane) * 2)
            )
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem: VecElementType::F16,
                } if insert.x86_hint.is_none()
                    && dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
            )
        {
            return None;
        }
        offset += 1;
    }

    let conversion = block.ops.get(index + offset)?;
    if conversion.guest_pc != guest_pc
        || !exact_conversion(conversion, loaded, encoding)
        || !no_following_same_pc(block, index, offset + 1, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset + 1,
        address_offset,
        memory_size: encoding.source_width.bytes(),
    })
}

/// Validate the complete O0/O1/O2 decomposition for any selected EVEX
/// packed F16/F32/F64/I32/I64 conversion memory source.
///
/// Exact provenance binds all 27 mnemonics, operation/source/destination
/// widths, signedness, truncation, masks, broadcast, APX address frontier,
/// MXCSR control, and the terminal guest-PC boundary. Matching is O(L) time
/// and O(1) auxiliary space for L <= 16 source lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_convert_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedConvertMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedConvertMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexPackedConvertMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        X86EvexE4MemoryShape {
            width: encoding.source_width,
            elem: encoding.kind.source_elem(),
            writemask: encoding.writemask,
            zeroing: encoding.zeroing,
            vector_load_hint: None,
            form,
            memory_source_uses: 1,
        },
        virtual_definitions,
        virtual_uses,
        |conversion, source| exact_conversion(conversion, source, encoding),
    )
    .or_else(|| {
        exact_unmasked_fp16_widen_sequence(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        )
    })
    .or_else(|| {
        exact_masked_fp16_widen_sequence(block, index, encoding, virtual_definitions, virtual_uses)
    })?;
    Some(X86JitEvexPackedConvertMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
