//! Fail-closed helper-backed VEX extraction to memory.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexChunkExtractMemoryEncoding, X86VexScalarExtractMemoryEncoding,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact canonical decomposition consumed for one VEX scalar extraction to
/// memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexScalarExtractMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexScalarExtractMemoryEncoding,
}

/// Exact canonical decomposition consumed for one VEX 128-bit chunk
/// extraction to memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexChunkExtractMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexChunkExtractMemoryEncoding,
}

/// Either defined VEX memory-destination extraction graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86JitVexExtractMemorySequence {
    Scalar(X86JitVexScalarExtractMemorySequence),
    Chunk(X86JitVexChunkExtractMemorySequence),
}

impl X86JitVexExtractMemorySequence {
    pub(crate) const fn consumed(self) -> usize {
        match self {
            Self::Scalar(sequence) => sequence.consumed,
            Self::Chunk(sequence) => sequence.consumed,
        }
    }

    pub(crate) const fn needs_avx2(self) -> bool {
        match self {
            Self::Scalar(_) => false,
            Self::Chunk(sequence) => sequence.encoding.needs_avx2,
        }
    }
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn ymm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)))
}

fn local_virtual_counts_match(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_insert(0usize) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(register, count)| virtual_definitions.get(register) == Some(count))
        && local_uses
            .iter()
            .all(|(register, count)| virtual_uses.get(register) == Some(count))
}

fn exact_frontier<'a>(
    block: &'a crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
) -> Option<&'a [crate::smir::ir::ops::SmirOp]> {
    let first = block.ops.get(index)?;
    if (index != 0 && block.ops[index - 1].guest_pc == first.guest_pc)
        || block
            .ops
            .get(index + consumed)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }
    let sequence = block.ops.get(index..index + consumed)?;
    sequence
        .iter()
        .all(|op| op.guest_pc == first.guest_pc)
        .then_some(sequence)
}

/// Validate the exact two-op graph for VEX `VPEXTRB/W/D/Q` or
/// `VEXTRACTPS` with a memory destination.
///
/// Complete instruction provenance binds source register, map, mandatory
/// prefix, W/L/vvvv fields, immediate-selected lane, and 1-/2-/4-/8-byte
/// access width. The extracted virtual must be unique to the sequence.
/// Classification is O(1); callers build global definition/use maps once in
/// O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_scalar_extract_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexScalarExtractMemorySequence> {
    if !allow_mem {
        return None;
    }
    let sequence = exact_frontier(block, index, 2)?;
    let instruction = instruction_bytes.get(&(block.id, sequence[0].guest_pc))?;
    let encoding = instruction.vex_scalar_extract_memory_encoding()?;

    let extracted = match sequence[0].kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign: SignExtend::Zero,
        } if vec == xmm(encoding.source)
            && lane == encoding.lane
            && elem == encoding.elem
            && sequence[0].x86_hint.is_none() =>
        {
            dst
        }
        _ => return None,
    };
    if !matches!(
        &sequence[1].kind,
        OpKind::Store { src, addr, width }
            if *src == extracted
                && *width == encoding.memory_width
                && x86_jit_mem_address_shape_valid(addr)
    ) || sequence[1].x86_hint.is_some()
        || !matches!(extracted, VReg::Virtual(_))
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexScalarExtractMemorySequence {
        consumed: 2,
        encoding,
    })
}

/// Validate the exact seven-op graph for VEX `VEXTRACTF128` or
/// `VEXTRACTI128` with a memory destination.
///
/// Complete instruction provenance binds source register, AVX/AVX2 family,
/// reserved prefix fields, immediate-selected 128-bit chunk, and unaligned
/// 16-byte store. Every virtual is distinct and all of its global
/// definitions/uses are contained in the sequence. Classification is O(1);
/// callers build global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_chunk_extract_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexChunkExtractMemorySequence> {
    if !allow_mem {
        return None;
    }
    let sequence = exact_frontier(block, index, 7)?;
    let instruction = instruction_bytes.get(&(block.id, sequence[0].guest_pc))?;
    let encoding = instruction.vex_chunk_extract_memory_encoding()?;
    let mut virtuals = HashSet::new();

    let zero = match sequence[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if sequence[0].x86_hint.is_none()
            && matches!(dst, VReg::Virtual(_))
            && virtuals.insert(dst) =>
        {
            dst
        }
        _ => return None,
    };
    let raw = match sequence[1].kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes: 2,
        } if scalar == zero
            && sequence[1].x86_hint.is_none()
            && matches!(dst, VReg::Virtual(_))
            && virtuals.insert(dst) =>
        {
            dst
        }
        _ => return None,
    };

    for lane in 0..2u8 {
        let extract_index = 2 + usize::from(lane) * 2;
        let scalar = match sequence[extract_index].kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extracted_lane,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
            } if vec == ymm(encoding.source)
                && extracted_lane == encoding.first_lane + lane
                && sequence[extract_index].x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && virtuals.insert(dst) =>
            {
                dst
            }
            _ => return None,
        };
        if !matches!(
            sequence[extract_index + 1].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem: VecElementType::I64,
            } if dst == raw && vec == raw && inserted == scalar && inserted_lane == lane
        ) || sequence[extract_index + 1].x86_hint.is_some()
        {
            return None;
        }
    }

    if !matches!(
        &sequence[6].kind,
        OpKind::VStore {
            src,
            addr,
            width: VecWidth::V128,
        } if *src == raw && x86_jit_mem_address_shape_valid(addr)
    ) || !matches!(
        sequence[6].x86_hint,
        Some(X86OpHint::VecAlign(
            X86VecAlign::Unaligned | X86VecAlign::Aligned
        ))
    ) || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexChunkExtractMemorySequence {
        consumed: 7,
        encoding,
    })
}

/// Classify either exact VEX extraction-to-memory graph.
pub(crate) fn x86_jit_vex_extract_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexExtractMemorySequence> {
    x86_jit_vex_scalar_extract_memory_sequence(
        block,
        index,
        allow_mem,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )
    .map(X86JitVexExtractMemorySequence::Scalar)
    .or_else(|| {
        x86_jit_vex_chunk_extract_memory_sequence(
            block,
            index,
            allow_mem,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        )
        .map(X86JitVexExtractMemorySequence::Chunk)
    })
}
