//! Fail-closed helper-backed VEX GFNI memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, ShiftOp, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexGfniMemoryEncoding, X86VexGfniMemoryKind};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX GFNI memory-source decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexGfniMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86VexGfniMemoryEncoding,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GfniOpProfile {
    mov: usize,
    broadcast: usize,
    and: usize,
    or: usize,
    xor: usize,
    sub: usize,
    shift: usize,
    byte_shuffle: usize,
    load: usize,
    vector_move: usize,
}

impl GfniOpProfile {
    fn total(self) -> usize {
        self.mov
            + self.broadcast
            + self.and
            + self.or
            + self.xor
            + self.sub
            + self.shift
            + self.byte_shuffle
            + self.load
            + self.vector_move
    }
}

const GFNI_MULTIPLY_O0: GfniOpProfile = GfniOpProfile {
    mov: 3,
    broadcast: 3,
    and: 24,
    or: 0,
    xor: 16,
    sub: 16,
    shift: 24,
    byte_shuffle: 0,
    load: 1,
    vector_move: 1,
};

const GFNI_MULTIPLY_OPTIMIZED: GfniOpProfile = GfniOpProfile {
    mov: 3,
    broadcast: 3,
    and: 23,
    or: 0,
    xor: 15,
    sub: 15,
    shift: 21,
    byte_shuffle: 0,
    load: 1,
    vector_move: 1,
};

const GFNI_AFFINE: GfniOpProfile = GfniOpProfile {
    mov: 11,
    broadcast: 11,
    and: 16,
    or: 8,
    xor: 25,
    sub: 0,
    shift: 31,
    byte_shuffle: 8,
    load: 1,
    vector_move: 1,
};

const GFNI_AFFINE_INVERSE_O0: GfniOpProfile = GfniOpProfile {
    mov: 50,
    broadcast: 50,
    and: 328,
    or: 8,
    xor: 233,
    sub: 208,
    shift: 343,
    byte_shuffle: 8,
    load: 1,
    vector_move: 1,
};

const GFNI_AFFINE_INVERSE_OPTIMIZED: GfniOpProfile = GfniOpProfile {
    mov: 50,
    broadcast: 50,
    and: 315,
    or: 8,
    xor: 220,
    sub: 195,
    shift: 304,
    byte_shuffle: 8,
    load: 1,
    vector_move: 1,
};

fn expected_profiles(kind: X86VexGfniMemoryKind) -> &'static [GfniOpProfile] {
    match kind {
        X86VexGfniMemoryKind::Multiply => &[GFNI_MULTIPLY_O0, GFNI_MULTIPLY_OPTIMIZED],
        X86VexGfniMemoryKind::Affine => &[GFNI_AFFINE],
        X86VexGfniMemoryKind::AffineInverse => {
            &[GFNI_AFFINE_INVERSE_O0, GFNI_AFFINE_INVERSE_OPTIMIZED]
        }
    }
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated GFNI width"),
    }))
}

pub(super) fn local_gfni_virtual_counts_match(
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
    local_definitions.iter().all(|(reg, count)| {
        virtual_definitions.get(reg) == Some(count)
            && virtual_uses.get(reg).copied().unwrap_or(0)
                == local_uses.get(reg).copied().unwrap_or(0)
    }) && local_uses.iter().all(|(reg, count)| {
        virtual_uses.get(reg) == Some(count)
            && virtual_definitions.get(reg).copied().unwrap_or(0)
                == local_definitions.get(reg).copied().unwrap_or(0)
    }) && local_uses
        .keys()
        .all(|reg| local_definitions.contains_key(reg))
}

/// Match only the arithmetic expansion shared by VEX and EVEX GFNI replay.
///
/// The source memory operation and final architectural commit are deliberately
/// excluded. Returning the exact raw virtual result lets each encoding family
/// validate its own source and merge/zero tail while sharing the closed
/// O0/O1/O2 affine profiles.
pub(super) fn x86_jit_gfni_expansion_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    kind: X86VexGfniMemoryKind,
    width: VecWidth,
    source1: u8,
    loaded: VReg,
    immediate: Option<u8>,
) -> Option<(VReg, usize)> {
    let lanes = width.bytes() as u8;
    if immediate.is_some() == (kind == X86VexGfniMemoryKind::Multiply) {
        return None;
    }
    for expected in expected_profiles(kind) {
        let core_len = expected.total().checked_sub(2)?;
        let Some(sequence) = block.ops.get(index..index.checked_add(core_len)?) else {
            continue;
        };
        let Some(raw) = sequence.last().and_then(|op| match op.kind {
            OpKind::VXor {
                dst,
                width: op_width,
                ..
            } if matches!(dst, VReg::Virtual(_)) && op_width == width => Some(dst),
            _ => None,
        }) else {
            continue;
        };

        let mut profile = GfniOpProfile::default();
        let mut mov_immediates = Vec::new();
        let mut left_shifts = [0usize; 8];
        let mut right_shifts = [0usize; 8];
        let mut valid = true;
        for op in sequence {
            if op.guest_pc != guest_pc || op.x86_hint.is_some() {
                valid = false;
                break;
            }
            match &op.kind {
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(value),
                    width: OpWidth::W64,
                } if matches!(dst, VReg::Virtual(_)) && (0..=u8::MAX as i64).contains(value) => {
                    profile.mov += 1;
                    mov_immediates.push(*value);
                }
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: VecElementType::I8,
                    lanes: op_lanes,
                } if matches!(dst, VReg::Virtual(_))
                    && matches!(scalar, VReg::Virtual(_))
                    && *op_lanes == lanes =>
                {
                    profile.broadcast += 1;
                }
                OpKind::VAnd {
                    dst,
                    width: op_width,
                    ..
                } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.and += 1,
                OpKind::VOr {
                    dst,
                    width: op_width,
                    ..
                } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.or += 1,
                OpKind::VXor {
                    dst,
                    width: op_width,
                    ..
                } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.xor += 1,
                OpKind::VSub {
                    dst,
                    elem: VecElementType::I8,
                    lanes: op_lanes,
                    ..
                } if matches!(dst, VReg::Virtual(_)) && *op_lanes == lanes => profile.sub += 1,
                OpKind::VShift {
                    dst,
                    amount: SrcOperand::Imm(amount),
                    shift,
                    elem: VecElementType::I8,
                    lanes: op_lanes,
                    ..
                } if matches!(dst, VReg::Virtual(_))
                    && *op_lanes == lanes
                    && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr)
                    && (1..=7).contains(amount) =>
                {
                    profile.shift += 1;
                    let counts = if *shift == ShiftOp::Lsl {
                        &mut left_shifts
                    } else {
                        &mut right_shifts
                    };
                    counts[*amount as usize] += 1;
                }
                OpKind::VByteShuffle {
                    dst,
                    src,
                    lanes: op_lanes,
                    block_lanes: 8,
                    ..
                } if matches!(dst, VReg::Virtual(_)) && *src == loaded && *op_lanes == lanes => {
                    profile.byte_shuffle += 1;
                }
                _ => {
                    valid = false;
                    break;
                }
            }

            if op.kind.reads_memory()
                || op.kind.writes_memory()
                || op
                    .kind
                    .dests()
                    .iter()
                    .any(|reg| !matches!(reg, VReg::Virtual(_)))
                || op
                    .kind
                    .source_vregs()
                    .iter()
                    .any(|reg| matches!(reg, VReg::Arch(_)) && *reg != vector_reg(source1, width))
            {
                valid = false;
                break;
            }
        }

        // Reconstitute the source-load and final-commit counters used by the
        // established VEX profiles without weakening the core comparison.
        profile.load = 1;
        profile.vector_move = 1;

        let multiply_count = match kind {
            X86VexGfniMemoryKind::Multiply => 1,
            X86VexGfniMemoryKind::Affine => 0,
            X86VexGfniMemoryKind::AffineInverse => 13,
        };
        let multiply_iterations = if multiply_count == 0 {
            0
        } else if expected.shift
            == 24 * multiply_count
                + usize::from(kind != X86VexGfniMemoryKind::Multiply) * GFNI_AFFINE.shift
        {
            8
        } else {
            7
        };
        let mut expected_immediates = Vec::new();
        for _ in 0..multiply_count {
            expected_immediates.extend([0, 1, 0x1B]);
        }
        if let Some(immediate) = immediate {
            expected_immediates.extend([0, 1, 0, 1, 2, 3, 4, 5, 6, 7]);
            expected_immediates.push(i64::from(immediate));
            for amount in [4usize, 2, 1] {
                right_shifts[amount] = right_shifts[amount]
                    .checked_sub(8)
                    .filter(|_| kind != X86VexGfniMemoryKind::Multiply)?;
            }
            for amount in 1usize..=7 {
                left_shifts[amount] = left_shifts[amount]
                    .checked_sub(1)
                    .filter(|_| kind != X86VexGfniMemoryKind::Multiply)?;
            }
        }
        let expected_multiply_shifts = multiply_count * multiply_iterations;
        if multiply_count != 0 {
            right_shifts[7] = right_shifts[7].checked_sub(expected_multiply_shifts)?;
            right_shifts[1] = right_shifts[1].checked_sub(expected_multiply_shifts)?;
            left_shifts[1] = left_shifts[1].checked_sub(expected_multiply_shifts)?;
        }
        mov_immediates.sort_unstable();
        expected_immediates.sort_unstable();
        if valid
            && profile == *expected
            && mov_immediates == expected_immediates
            && left_shifts == [0; 8]
            && right_shifts == [0; 8]
            && sequence
                .iter()
                .flat_map(|op| op.kind.source_vregs())
                .filter(|reg| *reg == loaded)
                .count()
                == 8
        {
            return Some((raw, core_len));
        }
    }
    None
}

/// Validate the complete O0/O1/O2 decomposition emitted for one VEX
/// VGF2P8MULB, VGF2P8AFFINEQB, or VGF2P8AFFINEINVQB memory source.
///
/// Exact instruction provenance binds the operation, width, architectural
/// operands, immediate, and native register-source rewrite. Admission also
/// requires the closed operation profile emitted by the lifter at a supported
/// optimization level, one leading state-backed load, one final architectural
/// write, no other memory or side effects, and no virtual definition or use
/// outside the sequence.
///
/// Classification is O(K) time and O(V) auxiliary space for K operations and V
/// virtual registers in the one-instruction sequence. The current
/// instruction-defined maximum is 1,230 operations. Callers build the global
/// definition/use maps once in O(N) time and O(U) space for N block operations
/// and U block-local virtual registers.
pub(crate) fn x86_jit_vex_gfni_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexGfniMemorySequence> {
    if !allow_mem {
        return None;
    }
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
    if index
        .checked_sub(1)
        .and_then(|previous| block.ops.get(previous))
        .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .vex_gfni_memory_encoding()?;
    if encoding.width != width {
        return None;
    }

    let consumed = block.ops[index..]
        .iter()
        .take_while(|op| op.guest_pc == load.guest_pc)
        .count();
    let sequence = block.ops.get(index..index.checked_add(consumed)?)?;
    if consumed == 0
        || sequence.iter().any(|op| op.x86_hint.is_some())
        || block.ops[index + consumed..]
            .iter()
            .any(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let lanes = width.bytes() as u8;
    let mut profile = GfniOpProfile::default();
    for (offset, op) in sequence.iter().enumerate() {
        let final_op = offset + 1 == sequence.len();
        match &op.kind {
            OpKind::VLoad { .. } if offset == 0 => profile.load += 1,
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(_),
                width: OpWidth::W64,
            } if matches!(dst, VReg::Virtual(_)) => profile.mov += 1,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::I8,
                lanes: op_lanes,
            } if matches!(dst, VReg::Virtual(_))
                && matches!(scalar, VReg::Virtual(_))
                && *op_lanes == lanes =>
            {
                profile.broadcast += 1;
            }
            OpKind::VAnd {
                dst,
                width: op_width,
                ..
            } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.and += 1,
            OpKind::VOr {
                dst,
                width: op_width,
                ..
            } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.or += 1,
            OpKind::VXor {
                dst,
                width: op_width,
                ..
            } if matches!(dst, VReg::Virtual(_)) && *op_width == width => profile.xor += 1,
            OpKind::VSub {
                dst,
                elem: VecElementType::I8,
                lanes: op_lanes,
                ..
            } if matches!(dst, VReg::Virtual(_)) && *op_lanes == lanes => profile.sub += 1,
            OpKind::VShift {
                dst,
                amount: SrcOperand::Imm(amount),
                shift: ShiftOp::Lsl | ShiftOp::Lsr,
                elem: VecElementType::I8,
                lanes: op_lanes,
                ..
            } if matches!(dst, VReg::Virtual(_))
                && *op_lanes == lanes
                && (1..=7).contains(amount) =>
            {
                profile.shift += 1;
            }
            OpKind::VByteShuffle {
                dst,
                lanes: op_lanes,
                block_lanes: 8,
                ..
            } if matches!(dst, VReg::Virtual(_)) && *op_lanes == lanes => {
                profile.byte_shuffle += 1;
            }
            OpKind::VMov {
                dst,
                src,
                width: op_width,
            } if final_op
                && *dst == vector_reg(encoding.destination, width)
                && matches!(src, VReg::Virtual(_))
                && *op_width == width =>
            {
                profile.vector_move += 1;
            }
            _ => return None,
        }

        if offset != 0
            && (op.kind.reads_memory()
                || op.kind.writes_memory()
                || op
                    .kind
                    .dests()
                    .iter()
                    .any(|reg| !matches!(reg, VReg::Virtual(_)) && !final_op)
                || op.kind.source_vregs().iter().any(|reg| {
                    matches!(reg, VReg::Arch(_)) && *reg != vector_reg(encoding.source1, width)
                }))
        {
            return None;
        }
    }

    if !expected_profiles(encoding.kind)
        .iter()
        .any(|expected| *expected == profile && expected.total() == consumed)
        || !sequence
            .iter()
            .skip(1)
            .flat_map(|op| op.kind.source_vregs())
            .any(|reg| reg == loaded)
        || !local_gfni_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexGfniMemorySequence {
        consumed,
        memory_size: width.bytes(),
        encoding,
    })
}
