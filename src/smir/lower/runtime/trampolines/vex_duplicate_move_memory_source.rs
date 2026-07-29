//! Fail-closed helper-backed VEX duplicate-move memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous `VMOVSLDUP`/`VMOVSHDUP`/`VMOVDDUP` memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexDuplicateMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) high: bool,
    pub(crate) w: bool,
}

fn unique_virtual(reg: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(reg, VReg::Virtual(_))
        .then_some(reg)
        .filter(|candidate| seen.insert(*candidate))
}

fn local_virtual_counts_match(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(reg, count)| virtual_definitions.get(reg) == Some(count))
        && local_uses
            .iter()
            .all(|(reg, count)| virtual_uses.get(reg) == Some(count))
}

/// Validate one complete decomposition emitted for a VEX
/// `VMOVSLDUP`, `VMOVSHDUP`, or `VMOVDDUP` memory source.
///
/// Full-vector sources use `VLoad`; the architectural VEX.128 `VMOVDDUP`
/// `m64` source uses an exact `Load`/`VBroadcast` prefix. Source-byte
/// provenance binds destination, width, duplicate direction, WIG encoding,
/// reserved VEX.vvvv, and memory width to the generated index graph. Every
/// virtual defined inside the sequence must have all definitions and uses
/// inside it, and no same-PC tail may remain unconsumed.
///
/// The architectural maximum of eight lanes bounds classification to O(1)
/// time and O(1) auxiliary space. Callers build global definition/use maps
/// once in O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_duplicate_move_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexDuplicateMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (destination, width, elem, high, memory_size, w) =
        instruction.vex_memory_duplicate_move_fields()?;

    let mut seen = HashSet::new();
    let (source, source_ops) = if memory_size == 8 {
        let loaded_scalar = match &load.kind {
            OpKind::Load {
                dst,
                addr,
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && elem == VecElementType::F64
                && width == VecWidth::V128
                && x86_jit_mem_address_shape_valid(addr) =>
            {
                unique_virtual(*dst, &mut seen)?
            }
            _ => return None,
        };
        let broadcast = block.ops.get(index + 1)?;
        let source = match broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::F64,
                lanes: 2,
            } if broadcast.guest_pc == load.guest_pc
                && broadcast.x86_hint.is_none()
                && scalar == loaded_scalar =>
            {
                unique_virtual(dst, &mut seen)?
            }
            _ => return None,
        };
        (source, 2usize)
    } else {
        let source = match &load.kind {
            OpKind::VLoad {
                dst,
                addr,
                width: loaded_width,
            } if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && *loaded_width == width
                && memory_size == width.bytes()
                && x86_jit_mem_address_shape_valid(addr) =>
            {
                unique_virtual(*dst, &mut seen)?
            }
            _ => return None,
        };
        (source, 1usize)
    };

    let lanes = width.lanes(elem) as u8;
    let consumed = source_ops + 3 + usize::from(lanes) * 2;
    let sequence = block.ops.get(index..index.checked_add(consumed)?)?;
    if sequence
        .iter()
        .skip(source_ops)
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + consumed)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let destination_reg = VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(destination),
        VecWidth::V256 => X86Reg::Ymm(destination),
        _ => unreachable!("validated VEX duplicate-move width"),
    }));
    let zero_index = index + source_ops;
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = block.ops[zero_index].kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, &mut seen)?;
    let OpKind::VBroadcast {
        dst: indices,
        scalar,
        elem: broadcast_elem,
        lanes: broadcast_lanes,
    } = block.ops[zero_index + 1].kind
    else {
        return None;
    };
    let indices = unique_virtual(indices, &mut seen)?;
    if scalar != zero || broadcast_elem != elem || broadcast_lanes != lanes {
        return None;
    }

    for lane in 0..lanes {
        let selector = lane / 2 * 2 + u8::from(high);
        let mov_offset = zero_index + 2 + usize::from(lane) * 2;
        let OpKind::Mov {
            dst: selector_reg,
            src: SrcOperand::Imm(encoded_selector),
            width: OpWidth::W64,
        } = block.ops[mov_offset].kind
        else {
            return None;
        };
        let selector_reg = unique_virtual(selector_reg, &mut seen)?;
        if encoded_selector != i64::from(selector) {
            return None;
        }
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: inserted_lane,
            elem: inserted_elem,
        } = block.ops[mov_offset + 1].kind
        else {
            return None;
        };
        if dst != indices
            || vec != indices
            || scalar != selector_reg
            || inserted_lane != lane
            || inserted_elem != elem
        {
            return None;
        }
    }

    let OpKind::VShuffle {
        dst,
        src1,
        src2: None,
        indices: shuffled_indices,
        elem: shuffled_elem,
        lanes: shuffled_lanes,
    } = block.ops[index + consumed - 1].kind
    else {
        return None;
    };
    if dst != destination_reg
        || src1 != source
        || shuffled_indices != indices
        || shuffled_elem != elem
        || shuffled_lanes != lanes
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexDuplicateMoveMemorySequence {
        consumed,
        memory_size,
        destination,
        width,
        elem,
        high,
        w,
    })
}
