//! Fail-closed helper-backed VEX scalar-insert memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexScalarInsertMemoryFields, X86VexScalarInsertMemoryKind,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous scalar-insert decomposition consumed by the helper-backed
/// x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexScalarInsertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86VexScalarInsertMemoryFields,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn unique_virtual(reg: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(reg, VReg::Virtual(_))
        .then_some(reg)
        .filter(|candidate| seen.insert(*candidate))
}

/// Require every locally defined or consumed virtual to have exactly the same
/// definition/use multiplicity globally. Iterating locally defined registers
/// explicitly is necessary for VINSERTPS: an inserted memory value may have
/// zero canonical uses when the immediate also zeroes its destination lane,
/// but any use outside the sequence must still fail closed.
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
    let local_registers: HashSet<_> = local_definitions
        .keys()
        .chain(local_uses.keys())
        .copied()
        .collect();
    local_registers.into_iter().all(|reg| {
        virtual_definitions.get(&reg).copied().unwrap_or(0)
            == local_definitions.get(&reg).copied().unwrap_or(0)
            && virtual_uses.get(&reg).copied().unwrap_or(0)
                == local_uses.get(&reg).copied().unwrap_or(0)
    })
}

fn match_zero_vector(
    ops: &[crate::smir::ir::ops::SmirOp],
    cursor: &mut usize,
    seen: &mut HashSet<VReg>,
    element: VecElementType,
    lanes: u8,
) -> Option<VReg> {
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = ops.get(*cursor)?.kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, seen)?;
    *cursor += 1;

    let OpKind::VBroadcast {
        dst: output,
        scalar,
        elem,
        lanes: broadcast_lanes,
    } = ops.get(*cursor)?.kind
    else {
        return None;
    };
    let output = unique_virtual(output, seen)?;
    if scalar != zero || elem != element || broadcast_lanes != lanes {
        return None;
    }
    *cursor += 1;
    Some(output)
}

fn match_pinsr_graph(
    ops: &[crate::smir::ir::ops::SmirOp],
    cursor: &mut usize,
    loaded: VReg,
    encoding: X86VexScalarInsertMemoryFields,
    seen: &mut HashSet<VReg>,
) -> Option<VReg> {
    let element = encoding.kind.element();
    let lanes = VecWidth::V128.lanes(element) as u8;
    let inserted_lane = encoding.kind.destination_lane(encoding.immediate);
    let merge = xmm(encoding.source1);
    let mut values = Vec::with_capacity(usize::from(lanes));

    for lane in 0..lanes {
        if lane == inserted_lane {
            values.push(loaded);
            continue;
        }
        let OpKind::VExtractLane {
            dst,
            vec,
            lane: extracted_lane,
            elem,
            sign: SignExtend::Zero,
        } = ops.get(*cursor)?.kind
        else {
            return None;
        };
        let dst = unique_virtual(dst, seen)?;
        if vec != merge || extracted_lane != lane || elem != element {
            return None;
        }
        values.push(dst);
        *cursor += 1;
    }

    let output = match_zero_vector(ops, cursor, seen, element, lanes)?;
    for (lane, scalar) in values.into_iter().enumerate() {
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar: inserted_scalar,
            lane: inserted_lane,
            elem,
        } = ops.get(*cursor)?.kind
        else {
            return None;
        };
        if dst != output
            || vec != output
            || inserted_scalar != scalar
            || usize::from(inserted_lane) != lane
            || elem != element
        {
            return None;
        }
        *cursor += 1;
    }
    Some(output)
}

fn match_insertps_graph(
    ops: &[crate::smir::ir::ops::SmirOp],
    cursor: &mut usize,
    loaded: VReg,
    encoding: X86VexScalarInsertMemoryFields,
    seen: &mut HashSet<VReg>,
) -> Option<VReg> {
    let destination_lane = encoding.kind.destination_lane(encoding.immediate);
    let zero_mask = encoding.immediate & 0x0F;
    let masked_zero = if let Some(OpKind::Mov {
        dst,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    }) = ops.get(*cursor).map(|op| &op.kind)
    {
        let zero = unique_virtual(*dst, seen)?;
        *cursor += 1;
        Some(zero)
    } else if zero_mask == 0 {
        // O1/O2 legitimately remove append_insertps's unused mask-zero
        // constant when no lane is zeroed. O0 retains it.
        None
    } else {
        return None;
    };
    let merge = xmm(encoding.source1);
    let mut values = Vec::with_capacity(4);
    for lane in 0..4u8 {
        if (zero_mask >> lane) & 1 != 0 {
            values.push(masked_zero?);
        } else if lane == destination_lane {
            values.push(loaded);
        } else {
            let OpKind::VExtractLane {
                dst,
                vec,
                lane: extracted_lane,
                elem: VecElementType::I32,
                sign: SignExtend::Zero,
            } = ops.get(*cursor)?.kind
            else {
                return None;
            };
            let dst = unique_virtual(dst, seen)?;
            if vec != merge || extracted_lane != lane {
                return None;
            }
            values.push(dst);
            *cursor += 1;
        }
    }

    let output = match_zero_vector(ops, cursor, seen, VecElementType::I32, 4)?;
    for (lane, scalar) in values.into_iter().enumerate() {
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar: inserted_scalar,
            lane: inserted_lane,
            elem: VecElementType::I32,
        } = ops.get(*cursor)?.kind
        else {
            return None;
        };
        if dst != output
            || vec != output
            || inserted_scalar != scalar
            || usize::from(inserted_lane) != lane
        {
            return None;
        }
        *cursor += 1;
    }
    Some(output)
}

/// Validate the complete 7- through 35-op canonical decomposition for a VEX
/// scalar-insert memory source.
///
/// Source-byte provenance binds the destination, merge source, operation kind,
/// immediate, W/WIG value, exact scalar memory width, and every extraction and
/// insertion edge. The memory load is retained even when VINSERTPS zeroing
/// discards its value, preserving the architectural fault. No virtual defined
/// by the sequence may escape it.
///
/// The architectural maximum of 16 byte lanes bounds classification to O(1)
/// time and O(1) auxiliary space. Callers build global definition/use maps once
/// in O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_scalar_insert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexScalarInsertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, memory_width) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && matches!(dst, VReg::Virtual(_))
            && matches!(
                width,
                MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8
            )
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_memory_scalar_insert_fields()?;
    if encoding.kind.memory_width() != memory_width {
        return None;
    }

    let mut seen = HashSet::new();
    unique_virtual(loaded, &mut seen)?;
    let mut cursor = index + 1;
    let output = match encoding.kind {
        X86VexScalarInsertMemoryKind::Vinsertps => {
            match_insertps_graph(&block.ops, &mut cursor, loaded, encoding, &mut seen)?
        }
        X86VexScalarInsertMemoryKind::Vpinsrb
        | X86VexScalarInsertMemoryKind::Vpinsrw
        | X86VexScalarInsertMemoryKind::Vpinsrd
        | X86VexScalarInsertMemoryKind::Vpinsrq => {
            match_pinsr_graph(&block.ops, &mut cursor, loaded, encoding, &mut seen)?
        }
    };

    let OpKind::VMov {
        dst,
        src,
        width: VecWidth::V128,
    } = block.ops.get(cursor)?.kind
    else {
        return None;
    };
    if dst != xmm(encoding.destination) || src != output {
        return None;
    }
    cursor += 1;

    let sequence = block.ops.get(index..cursor)?;
    if sequence
        .iter()
        .skip(1)
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(cursor)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexScalarInsertMemorySequence {
        consumed: cursor - index,
        memory_size: memory_width.bytes(),
        encoding,
    })
}
