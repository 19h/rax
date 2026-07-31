//! Fail-closed helper-backed EVEX saturating integer-pack admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexIntegerArithmeticMemoryReplay, X86EvexIntegerPackMemoryEncoding, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_virtual_definition_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// saturating integer-pack memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerPackMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexIntegerPackMemoryEncoding,
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerPackMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize, u32)> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    match encoding.replay {
        X86EvexIntegerArithmeticMemoryReplay::Vector { .. } => {
            let load = block.ops.get(index)?;
            let loaded = match &load.kind {
                OpKind::VLoad { dst, addr, width }
                    if load.x86_hint.is_none()
                        && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses) {
                return None;
            }
            Some((loaded, 1, encoding.width.bytes()))
        }
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. } => {
            let load = block.ops.get(index)?;
            let scalar = match &load.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width: MemWidth::B4,
                    sign: SignExtend::Zero,
                } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
                _ => return None,
            };
            if !exact_virtual_definition_use(scalar, 1, 1, virtual_definitions, virtual_uses) {
                return None;
            }

            let broadcast = block.ops.get(index + 1)?;
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem: VecElementType::I32,
                    lanes,
                } if broadcast.x86_hint.is_none()
                    && actual_scalar == scalar
                    && u32::from(lanes) == encoding.width.lanes(VecElementType::I32) =>
                {
                    dst
                }
                _ => return None,
            };
            if broadcast.guest_pc != guest_pc
                || !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses)
            {
                return None;
            }
            Some((loaded, 2, MemWidth::B4.bytes()))
        }
        X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => None,
    }
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// signed/unsigned saturating-pack memory source.
///
/// Exact provenance binds the map, opcode, W/WIG interpretation, vector and
/// element widths, architectural operands, signedness, mask policy, tuple
/// kind, address, pack lane grouping, and final commit. Runtime is O(L) with
/// O(1) auxiliary space for L <= 64 output lanes; callers build definition/use
/// maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_integer_pack_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerPackMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_integer_pack_memory_encoding()?;
    let (loaded, mut offset, memory_size) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;

    let pack = block.ops.get(index + offset)?;
    let raw = match pack.kind {
        OpKind::VPackSat {
            dst,
            src1,
            src2,
            src_elem,
            to_unsigned,
            src_lanes,
            block_lanes,
        } if src1 == loaded
            && vector_index(&src2, encoding.width) == Some(encoding.source1)
            && src_elem == encoding.src_elem
            && to_unsigned == encoding.to_unsigned
            && u32::from(src_lanes) == encoding.width.lanes(encoding.src_elem)
            && u32::from(block_lanes) == 16 / encoding.src_elem.bytes()
            && pack.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: encoding.map,
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
    if pack.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.dst_elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else if vector_index(&raw, encoding.width) != Some(encoding.destination) {
        return None;
    }

    if block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }

    Some(X86JitEvexIntegerPackMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size,
        encoding,
    })
}
