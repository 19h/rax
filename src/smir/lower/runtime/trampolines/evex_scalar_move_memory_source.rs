//! Fail-closed helper-backed EVEX scalar move memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexScalarMoveMemoryEncoding, X86EvexScalarMoveMemoryKind, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_virtual_definition_use, no_following_same_pc,
};
use super::evex_scalar_memory_source_common::exact_evex_scalar_mask_condition;
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX scalar move memory decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexScalarMoveMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn op_width(elem: VecElementType) -> Option<OpWidth> {
    match elem {
        VecElementType::F16 => Some(OpWidth::W16),
        VecElementType::F32 => Some(OpWidth::W32),
        VecElementType::F64 => Some(OpWidth::W64),
        _ => None,
    }
}

fn exact_terminal_hint(
    op: &crate::smir::ir::ops::SmirOp,
    encoding: X86EvexScalarMoveMemoryEncoding,
) -> bool {
    if encoding.elem == VecElementType::F16 {
        return op.x86_hint.is_none();
    }
    let pp = match encoding.pp {
        2 => X86SsePrefix::Rep,
        3 => X86SsePrefix::Repne,
        _ => return false,
    };
    let width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return false,
    };
    op.x86_hint
        == Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp,
            opcode: encoding.opcode,
            width,
            w: encoding.w,
        })
}

#[allow(clippy::too_many_arguments)]
fn exact_unmasked_load(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(usize, usize)> {
    if encoding.writemask.is_some() || encoding.zeroing {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let broadcast = block.ops.get(index + 1)?;
    if !matches!(
        broadcast.kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: 1,
        } if dst == xmm(encoding.vector) && scalar == loaded && elem == encoding.elem
    ) || !exact_terminal_hint(broadcast, encoding)
    {
        return None;
    }
    Some((2, 0))
}

#[allow(clippy::too_many_arguments)]
fn exact_masked_load(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(usize, usize)> {
    let mask = encoding.writemask?;
    let guest_pc = block.ops.get(index)?.guest_pc;
    let condition = exact_evex_scalar_mask_condition(
        block,
        index,
        guest_pc,
        mask,
        2,
        virtual_definitions,
        virtual_uses,
    )?;
    let width = op_width(encoding.elem)?;

    let initialize = block.ops.get(index + 1)?;
    let loaded = match initialize.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: actual_width,
        } if initialize.x86_hint.is_none() && actual_width == width => dst,
        _ => return None,
    };
    let load = block.ops.get(index + 2)?;
    if !matches!(
        &load.kind,
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width: actual_width,
            signed: SignExtend::Zero,
        } if *dst == loaded
            && *cond == condition
            && *actual_width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr)
    ) || load.x86_hint.is_some()
        || !exact_virtual_definition_use(loaded, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let inactive_op = block.ops.get(index + 3)?;
    let inactive = if encoding.zeroing {
        match inactive_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: actual_width,
            } if inactive_op.x86_hint.is_none() && actual_width == width => dst,
            _ => return None,
        }
    } else {
        match inactive_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem,
                sign: SignExtend::Zero,
            } if inactive_op.x86_hint.is_none()
                && vec == xmm(encoding.vector)
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        }
    };
    if !exact_virtual_definition_use(inactive, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let select = block.ops.get(index + 4)?;
    let selected = match select.kind {
        OpKind::Select {
            dst,
            cond,
            src_true,
            src_false,
            width: actual_width,
        } if select.x86_hint.is_none()
            && cond == condition
            && src_true == loaded
            && src_false == inactive
            && actual_width == width =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(selected, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let broadcast = block.ops.get(index + 5)?;
    if !matches!(
        broadcast.kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: 1,
        } if dst == xmm(encoding.vector) && scalar == selected && elem == encoding.elem
    ) || !exact_terminal_hint(broadcast, encoding)
    {
        return None;
    }
    Some((6, 2))
}

#[allow(clippy::too_many_arguments)]
fn exact_store(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(usize, usize)> {
    if encoding.zeroing {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let (condition, extract_offset) = if let Some(mask) = encoding.writemask {
        (
            Some(exact_evex_scalar_mask_condition(
                block,
                index,
                guest_pc,
                mask,
                1,
                virtual_definitions,
                virtual_uses,
            )?),
            1,
        )
    } else {
        (None, 0)
    };
    let extract = block.ops.get(index + extract_offset)?;
    let scalar = match extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem,
            sign: SignExtend::Zero,
        } if extract.x86_hint.is_none() && vec == xmm(encoding.vector) && elem == encoding.elem => {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(scalar, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let memory_offset = extract_offset + 1;
    let memory = block.ops.get(index + memory_offset)?;
    let exact = match condition {
        None => matches!(
            &memory.kind,
            OpKind::Store { src, addr, width }
                if *src == scalar
                    && *width == encoding.memory_width
                    && x86_jit_mem_address_shape_valid(addr)
        ),
        Some(condition) => matches!(
            &memory.kind,
            OpKind::PredStore {
                src: SrcOperand::Reg(src),
                cond,
                addr,
                width,
            } if *src == scalar
                && *cond == condition
                && *width == encoding.memory_width
                && x86_jit_mem_address_shape_valid(addr)
        ),
    };
    if !exact || !exact_terminal_hint(memory, encoding) {
        return None;
    }
    Some((memory_offset + 1, memory_offset))
}

/// Validate the complete O0/O1/O2 decomposition for one EVEX scalar move
/// memory form.
///
/// Exact byte provenance binds precision, direction, vector register,
/// writemask, merge/zero behavior, LLIG image, and the complete source
/// instruction. The canonical graph binds the precise helper address and all
/// virtual definition/use counts, including access suppression when K[0] is
/// clear. Classification is O(1) time and auxiliary space; callers build
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_scalar_move_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_scalar_move_memory_encoding()?;
    let (consumed, address_offset) = match encoding.kind {
        X86EvexScalarMoveMemoryKind::Load => {
            exact_unmasked_load(block, index, encoding, virtual_definitions, virtual_uses).or_else(
                || exact_masked_load(block, index, encoding, virtual_definitions, virtual_uses),
            )?
        }
        X86EvexScalarMoveMemoryKind::Store => {
            exact_store(block, index, encoding, virtual_definitions, virtual_uses)?
        }
    };
    let end = index.checked_add(consumed)?;
    if end > block.ops.len()
        || block.ops[index..end]
            .iter()
            .any(|op| op.guest_pc != guest_pc)
        || !no_following_same_pc(block, index, consumed, guest_pc)
    {
        return None;
    }
    let address = match &block.ops.get(index + address_offset)?.kind {
        OpKind::Load { addr, .. }
        | OpKind::PredLoad { addr, .. }
        | OpKind::Store { addr, .. }
        | OpKind::PredStore { addr, .. } => addr,
        _ => return None,
    };
    exact_evex_memory_apx_frontier(block, index, guest_pc, address).then_some(
        X86JitEvexScalarMoveMemorySequence {
            consumed,
            address_offset,
            encoding,
        },
    )
}
