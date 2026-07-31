//! Fail-closed helper-backed EVEX VALIGND/Q memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexVectorAlignMemoryEncoding, X86EvexVectorAlignMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_reconstructed_vector_mask_result, exact_virtual_definition_use,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// VALIGND/Q memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexVectorAlignMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexVectorAlignMemoryEncoding,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn no_following_same_pc(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    guest_pc: GuestAddr,
) -> bool {
    !block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VALIGND/Q memory source.
///
/// Exact provenance binds the vector/element widths, immediate, architectural
/// low/high ordering, destination, mask policy, broadcast/full-vector tuple,
/// helper address, lane reconstruction, and the single architectural commit.
/// The E4NF memory read remains unconditional. Classification is O(L) time and
/// O(1) auxiliary space for L <= 16 lanes; callers build definition/use maps
/// once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_vector_align_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexVectorAlignMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_vector_align_memory_encoding()?;
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let shift = usize::from(encoding.immediate) % lanes;
    let low_uses = lanes - shift;
    let mut offset;
    let low = match encoding.replay {
        X86EvexVectorAlignMemoryReplay::Vector { .. } => {
            let loaded = match &first.kind {
                OpKind::VLoad { dst, addr, width }
                    if first.x86_hint.is_none()
                        && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !exact_virtual_definition_use(loaded, 1, low_uses, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset = 1;
            loaded
        }
        X86EvexVectorAlignMemoryReplay::Broadcast { .. } => {
            let scalar = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if first.x86_hint.is_none()
                    && *width == memory_width(encoding.elem)?
                    && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !single_definition_single_use(scalar, virtual_definitions, virtual_uses) {
                return None;
            }
            let broadcast = block.ops.get(index + 1)?;
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem,
                    lanes: actual_lanes,
                } if broadcast.x86_hint.is_none()
                    && actual_scalar == scalar
                    && elem == encoding.elem
                    && usize::from(actual_lanes) == lanes =>
                {
                    dst
                }
                _ => return None,
            };
            if broadcast.guest_pc != guest_pc
                || !exact_virtual_definition_use(
                    loaded,
                    1,
                    low_uses,
                    virtual_definitions,
                    virtual_uses,
                )
            {
                return None;
            }
            offset = 2;
            loaded
        }
    };

    let zero_op = block.ops.get(index + offset)?;
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
    offset += 1;

    let raw_op = block.ops.get(index + offset)?;
    let raw = match raw_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if raw_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && usize::from(actual_lanes) == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if raw_op.guest_pc != guest_pc || !matches!(raw, VReg::Virtual(_)) {
        return None;
    }
    offset += 1;

    for lane in 0..lanes {
        let concatenated_index = lane + shift;
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && elem == encoding.elem
                && if concatenated_index < lanes {
                    vec == low && usize::from(actual_lane) == concatenated_index
                } else {
                    vector_index(&vec, encoding.width) == Some(encoding.high)
                        && usize::from(actual_lane) == concatenated_index - lanes
                } =>
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
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if dst == raw
                    && vec == raw
                    && actual_scalar == scalar
                    && usize::from(actual_lane) == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    if let Some(mask_index) = encoding.writemask {
        let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
        exact_evex_reconstructed_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            mask,
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing
            || !exact_virtual_definition_use(
                raw,
                lanes + 1,
                lanes + 1,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.x86_hint.is_some()
            || commit.guest_pc != guest_pc
            || !matches!(
                commit.kind,
                OpKind::VMov {
                    dst,
                    src,
                    width,
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
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
    Some(X86JitEvexVectorAlignMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: match encoding.replay {
            X86EvexVectorAlignMemoryReplay::Vector { .. } => encoding.width.bytes(),
            X86EvexVectorAlignMemoryReplay::Broadcast { .. } => {
                memory_width(encoding.elem)?.bytes()
            }
        },
        encoding,
    })
}
