//! Fail-closed helper-backed packed AVX-512-FP16 conversion admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFp16ConvertMemoryEncoding, X86EvexPackedFp16ConvertMemoryKind,
    X86EvexPackedFp16ConvertMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_lane_address, exact_lane_predicate,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact packed AVX-512-FP16 conversion decomposition consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFp16ConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFp16ConvertMemoryEncoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactSource {
    consumed: usize,
    address_offset: usize,
    memory_size: u32,
    source: VReg,
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

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::F16 | VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::F32 | VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::F64 | VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn architectural_destination(encoding: X86EvexPackedFp16ConvertMemoryEncoding) -> Option<VReg> {
    Some(VReg::Arch(ArchReg::X86(match encoding.destination_width {
        VecWidth::V64 | VecWidth::V128 => X86Reg::Xmm(encoding.destination),
        VecWidth::V256 => X86Reg::Ymm(encoding.destination),
        VecWidth::V512 => X86Reg::Zmm(encoding.destination),
    })))
}

fn exact_conversion(
    op: &crate::smir::ir::ops::SmirOp,
    source: VReg,
    encoding: X86EvexPackedFp16ConvertMemoryEncoding,
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
    let Some(expected_destination) = architectural_destination(encoding) else {
        return false;
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));

    match (encoding.kind, op.kind.clone()) {
        (
            X86EvexPackedFp16ConvertMemoryKind::FpPrecision { from, to },
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
                report_fp16_denormal,
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
                && report_fp16_denormal
                    == (from == VecElementType::F16
                        && (to == VecElementType::F64 || encoding.broadcast))
        }
        (
            X86EvexPackedFp16ConvertMemoryKind::IntToFp16 { int_elem, signed },
            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem: actual_int_elem,
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
                && actual_signed == signed
                && lanes == encoding.lanes
                && src_width == encoding.source_width
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        (
            X86EvexPackedFp16ConvertMemoryKind::Fp16ToInt {
                int_elem,
                signed,
                truncate,
            },
            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
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

#[allow(clippy::too_many_arguments)]
fn exact_zero_vector(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    elem: VecElementType,
    lanes: u8,
    loaded_definitions: usize,
    loaded_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let zero_op = block.ops.get(index)?;
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
    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: actual_elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == zero
            && actual_elem == elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            loaded_definitions,
            loaded_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    Some(loaded)
}

#[allow(clippy::too_many_arguments)]
fn exact_direct_vector(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ConvertMemoryEncoding,
    zero_prelude: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<ExactSource> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let memory_size = u32::from(encoding.lanes) * encoding.kind.source_elem().bytes();
    if memory_size != encoding.source_width.bytes() {
        return None;
    }
    let mut offset = 0usize;
    let preloaded = if zero_prelude {
        let loaded = exact_zero_vector(
            block,
            index,
            guest_pc,
            encoding.kind.source_elem(),
            encoding.lanes,
            2,
            1,
            virtual_definitions,
            virtual_uses,
        )?;
        offset = 2;
        Some(loaded)
    } else {
        None
    };
    let load = block.ops.get(index + offset)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && *width == encoding.source_width
                && x86_jit_mem_address_shape_valid(addr)
                && preloaded.is_none_or(|preloaded| preloaded == *dst) =>
        {
            *dst
        }
        _ => return None,
    };
    if load.guest_pc != guest_pc
        || (!zero_prelude
            && !single_definition_single_use(loaded, virtual_definitions, virtual_uses))
    {
        return None;
    }
    Some(ExactSource {
        consumed: offset + 1,
        address_offset: offset,
        memory_size,
        source: loaded,
    })
}

fn exact_zero_scalar(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let seed = block.ops.get(index)?;
    let scalar = match seed.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if seed.x86_hint.is_none() => dst,
        _ => return None,
    };
    (seed.guest_pc == guest_pc
        && exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses))
    .then_some(scalar)
}

#[allow(clippy::too_many_arguments)]
fn exact_reconstructed_vector(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ConvertMemoryEncoding,
    broadcast: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<ExactSource> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let lanes = encoding.lanes;
    let elem = encoding.kind.source_elem();
    // The dedicated FP16-to-integer lifter initializes the complete minimum
    // V64 container (four F16 lanes) even when a quarter tuple supplies only
    // two architectural lanes for an I64 destination. Insert/load counts and
    // memory extent remain bound to the explicit architectural lane count.
    let zero_lanes = if matches!(
        encoding.kind,
        X86EvexPackedFp16ConvertMemoryKind::Fp16ToInt { .. }
    ) {
        encoding.source_width.lanes(elem) as u8
    } else {
        lanes
    };
    let loaded = exact_zero_vector(
        block,
        index,
        guest_pc,
        elem,
        zero_lanes,
        usize::from(lanes) + 1,
        usize::from(lanes) + 1,
        virtual_definitions,
        virtual_uses,
    )?;
    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
    let base = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *base,
        _ => return None,
    };
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            base,
            1,
            usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let expected_width = memory_width(elem)?;
    let mut offset = address_offset + 1;
    for lane in 0..lanes {
        let (scalar, condition) = if let Some(mask) = mask {
            if matches!(
                block.ops.get(index + offset)?.kind,
                OpKind::Mov {
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                    ..
                }
            ) {
                let scalar = exact_zero_scalar(
                    block,
                    index + offset,
                    guest_pc,
                    virtual_definitions,
                    virtual_uses,
                )?;
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
                (scalar, Some(condition))
            } else {
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
                let scalar = exact_zero_scalar(
                    block,
                    index + offset,
                    guest_pc,
                    virtual_definitions,
                    virtual_uses,
                )?;
                offset += 1;
                (scalar, Some(condition))
            }
        } else {
            let scalar = exact_zero_scalar(
                block,
                index + offset,
                guest_pc,
                virtual_definitions,
                virtual_uses,
            )?;
            offset += 1;
            (scalar, None)
        };

        let load = block.ops.get(index + offset)?;
        let lane_offset = if broadcast {
            0
        } else {
            i64::from(lane) * i64::from(elem.bytes())
        };
        let exact_load = match (&load.kind, condition) {
            (
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                },
                None,
            ) => {
                *dst == scalar
                    && *width == expected_width
                    && load.x86_hint.is_none()
                    && exact_lane_address(addr, base, lane_offset)
            }
            (
                OpKind::PredLoad {
                    dst,
                    cond,
                    addr,
                    width,
                    signed: SignExtend::Zero,
                },
                Some(expected_condition),
            ) => {
                *dst == scalar
                    && *cond == expected_condition
                    && *width == expected_width
                    && load.x86_hint.is_none()
                    && exact_lane_address(addr, base, lane_offset)
            }
            _ => false,
        };
        if !exact_load || load.guest_pc != guest_pc {
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
                    elem: actual_elem,
                } if insert.x86_hint.is_none()
                    && dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && actual_elem == elem
            )
        {
            return None;
        }
        offset += 1;
    }

    Some(ExactSource {
        consumed: offset,
        address_offset,
        memory_size: if broadcast {
            elem.bytes()
        } else {
            u32::from(lanes) * elem.bytes()
        },
        source: loaded,
    })
}

#[allow(clippy::too_many_arguments)]
fn exact_mask_value_predicate(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    mask: VReg,
    applicable_bits: u64,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let and = block.ops.get(index)?;
    let predicate = match and.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(actual_bits),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if and.x86_hint.is_none() && src1 == mask && actual_bits == applicable_bits as i64 => dst,
        _ => return None,
    };
    (and.guest_pc == guest_pc
        && single_definition_single_use(predicate, virtual_definitions, virtual_uses))
    .then_some(predicate)
}

#[allow(clippy::too_many_arguments)]
fn exact_aggregate_broadcast(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ConvertMemoryEncoding,
    zero_prelude: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<ExactSource> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let elem = encoding.kind.source_elem();
    let lanes = encoding.lanes;
    let mut offset = 0usize;
    let preloaded = if zero_prelude {
        let loaded = exact_zero_vector(
            block,
            index,
            guest_pc,
            elem,
            lanes,
            2,
            1,
            virtual_definitions,
            virtual_uses,
        )?;
        offset = 2;
        Some(loaded)
    } else {
        None
    };

    let mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let (scalar, condition, scalar_seeded) = if let Some(mask) = mask {
        let applicable_bits = (1u64 << lanes) - 1;
        if matches!(
            block.ops.get(index + offset)?.kind,
            OpKind::Mov {
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
                ..
            }
        ) {
            let scalar = exact_zero_scalar(
                block,
                index + offset,
                guest_pc,
                virtual_definitions,
                virtual_uses,
            )?;
            offset += 1;
            let condition = exact_mask_value_predicate(
                block,
                index + offset,
                guest_pc,
                mask,
                applicable_bits,
                virtual_definitions,
                virtual_uses,
            )?;
            offset += 1;
            (scalar, Some(condition), true)
        } else {
            let condition = exact_mask_value_predicate(
                block,
                index + offset,
                guest_pc,
                mask,
                applicable_bits,
                virtual_definitions,
                virtual_uses,
            )?;
            offset += 1;
            let scalar = exact_zero_scalar(
                block,
                index + offset,
                guest_pc,
                virtual_definitions,
                virtual_uses,
            )?;
            offset += 1;
            (scalar, Some(condition), true)
        }
    } else if matches!(
        block.ops.get(index + offset)?.kind,
        OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
            ..
        }
    ) {
        let scalar = exact_zero_scalar(
            block,
            index + offset,
            guest_pc,
            virtual_definitions,
            virtual_uses,
        )?;
        offset += 1;
        (scalar, None, true)
    } else {
        let load = block.ops.get(index + offset)?;
        let scalar = match load.kind {
            OpKind::Load { dst, .. } => dst,
            _ => return None,
        };
        (scalar, None, false)
    };

    let address_offset = offset;
    let expected_width = memory_width(elem)?;
    let load = block.ops.get(index + offset)?;
    let exact_load = match (&load.kind, condition) {
        (
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            },
            None,
        ) => {
            *dst == scalar
                && *width == expected_width
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
        }
        (
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            },
            Some(expected_condition),
        ) => {
            *dst == scalar
                && *cond == expected_condition
                && *width == expected_width
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    };
    if !exact_load || load.guest_pc != guest_pc {
        return None;
    }
    if !scalar_seeded && !single_definition_single_use(scalar, virtual_definitions, virtual_uses) {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem: actual_elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && actual_elem == elem
            && actual_lanes == lanes
            && preloaded.is_none_or(|preloaded| preloaded == dst) =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || (!zero_prelude
            && !single_definition_single_use(loaded, virtual_definitions, virtual_uses))
    {
        return None;
    }
    offset += 1;
    Some(ExactSource {
        consumed: offset,
        address_offset,
        memory_size: elem.bytes(),
        source: loaded,
    })
}

/// Validate the complete O0/O1/O2 decomposition for all 22 packed
/// AVX-512-FP16 conversion memory-source mnemonics.
///
/// Exact provenance binds map/opcode/prefix, operation/source/destination
/// widths, the explicit architectural lane count, signedness, truncation,
/// writemask, broadcast, APX address frontier, MXCSR controls, and terminal
/// guest-PC boundary. Matching is O(L) time and O(1) auxiliary space for at
/// most 32 lanes; callers build definition/use maps once in O(N) time and
/// O(V) space.
pub(crate) fn x86_jit_evex_packed_fp16_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_packed_fp16_convert_memory_encoding()?;
    let memory_size = u32::from(encoding.lanes) * encoding.kind.source_elem().bytes();
    let exact = match encoding.replay {
        X86EvexPackedFp16ConvertMemoryReplay::Vector { .. } => match encoding.kind {
            X86EvexPackedFp16ConvertMemoryKind::FpPrecision { .. } if memory_size < 8 => {
                exact_reconstructed_vector(
                    block,
                    index,
                    encoding,
                    false,
                    virtual_definitions,
                    virtual_uses,
                )
            }
            X86EvexPackedFp16ConvertMemoryKind::FpPrecision { .. } => exact_direct_vector(
                block,
                index,
                encoding,
                true,
                virtual_definitions,
                virtual_uses,
            ),
            X86EvexPackedFp16ConvertMemoryKind::IntToFp16 { .. } => exact_direct_vector(
                block,
                index,
                encoding,
                false,
                virtual_definitions,
                virtual_uses,
            ),
            X86EvexPackedFp16ConvertMemoryKind::Fp16ToInt { .. } => exact_reconstructed_vector(
                block,
                index,
                encoding,
                false,
                virtual_definitions,
                virtual_uses,
            ),
        },
        X86EvexPackedFp16ConvertMemoryReplay::MaskedVector { .. } => exact_reconstructed_vector(
            block,
            index,
            encoding,
            false,
            virtual_definitions,
            virtual_uses,
        ),
        X86EvexPackedFp16ConvertMemoryReplay::Broadcast { .. } => {
            if matches!(
                encoding.kind,
                X86EvexPackedFp16ConvertMemoryKind::IntToFp16 { .. }
            ) && encoding.writemask.is_some()
            {
                exact_reconstructed_vector(
                    block,
                    index,
                    encoding,
                    true,
                    virtual_definitions,
                    virtual_uses,
                )
            } else {
                exact_aggregate_broadcast(
                    block,
                    index,
                    encoding,
                    matches!(
                        encoding.kind,
                        X86EvexPackedFp16ConvertMemoryKind::FpPrecision { .. }
                    ),
                    virtual_definitions,
                    virtual_uses,
                )
            }
        }
    }?;
    let conversion = block.ops.get(index + exact.consumed)?;
    if conversion.guest_pc != guest_pc
        || !exact_conversion(conversion, exact.source, encoding)
        || !no_following_same_pc(block, index, exact.consumed + 1, guest_pc)
    {
        return None;
    }
    let consumed = exact.consumed + 1;
    let address = exact_evex_memory_sequence_address(block, index, exact.address_offset)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    Some(X86JitEvexPackedFp16ConvertMemorySequence {
        consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
