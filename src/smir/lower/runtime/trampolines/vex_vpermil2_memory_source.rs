//! Fail-closed helper-backed AMD VEX VPERMIL2 memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, ShiftOp, SrcOperand, VReg, VecCmpCond, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexVpermil2MemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact canonical decomposition consumed for one VPERMIL2 memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexVpermil2MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexVpermil2MemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VPERMIL2 has 128-/256-bit vector widths"),
    }))
}

fn local_virtual_definitions_are_closed(
    ops: &[SmirOp],
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
        .into_iter()
        .all(|(register, definitions)| {
            virtual_definitions.get(&register) == Some(&definitions)
                && virtual_uses.get(&register).copied().unwrap_or(0)
                    == local_uses.get(&register).copied().unwrap_or(0)
        })
}

struct Vpermil2GraphCursor<'a> {
    block: &'a crate::smir::ir::SmirBlock,
    next_index: usize,
    guest_pc: GuestAddr,
    virtuals: HashSet<VReg>,
}

impl<'a> Vpermil2GraphCursor<'a> {
    fn new(
        block: &'a crate::smir::ir::SmirBlock,
        next_index: usize,
        guest_pc: GuestAddr,
        loaded: VReg,
    ) -> Option<Self> {
        let mut virtuals = HashSet::new();
        matches!(loaded, VReg::Virtual(_))
            .then(|| virtuals.insert(loaded))
            .filter(|inserted| *inserted)?;
        Some(Self {
            block,
            next_index,
            guest_pc,
            virtuals,
        })
    }

    fn next(&mut self) -> Option<&'a SmirOp> {
        let op = self.block.ops.get(self.next_index)?;
        if op.guest_pc != self.guest_pc || op.x86_hint.is_some() {
            return None;
        }
        self.next_index += 1;
        Some(op)
    }

    fn fresh(&mut self, register: VReg) -> Option<VReg> {
        matches!(register, VReg::Virtual(_))
            .then_some(register)
            .filter(|candidate| self.virtuals.insert(*candidate))
    }

    fn mov_imm(&mut self, value: i64) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(actual),
                width: OpWidth::W64,
            } if *actual == value => *dst,
            _ => return None,
        };
        self.fresh(destination)
    }

    fn broadcast(&mut self, scalar: VReg, elem: VecElementType, lanes: u8) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem: actual_elem,
                lanes: actual_lanes,
            } if *actual_scalar == scalar && *actual_elem == elem && *actual_lanes == lanes => *dst,
            _ => return None,
        };
        self.fresh(destination)
    }

    fn splat(&mut self, value: i64, elem: VecElementType, lanes: u8) -> Option<VReg> {
        let scalar = self.mov_imm(value)?;
        self.broadcast(scalar, elem, lanes)
    }

    fn and(&mut self, src1: VReg, src2: VReg, width: VecWidth) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VAnd {
                dst,
                src1: actual_src1,
                src2: actual_src2,
                width: actual_width,
            } if *actual_src1 == src1 && *actual_src2 == src2 && *actual_width == width => *dst,
            _ => return None,
        };
        self.fresh(destination)
    }

    fn and_not(&mut self, src1: VReg, src2: VReg, width: VecWidth) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VAndNot {
                dst,
                src1: actual_src1,
                src2: actual_src2,
                width: actual_width,
            } if *actual_src1 == src1 && *actual_src2 == src2 && *actual_width == width => *dst,
            _ => return None,
        };
        self.fresh(destination)
    }

    fn or(&mut self, src1: VReg, src2: VReg, width: VecWidth) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VOr {
                dst,
                src1: actual_src1,
                src2: actual_src2,
                width: actual_width,
            } if *actual_src1 == src1 && *actual_src2 == src2 && *actual_width == width => *dst,
            _ => return None,
        };
        self.fresh(destination)
    }

    fn shift(
        &mut self,
        source: VReg,
        direction: ShiftOp,
        elem: VecElementType,
        lanes: u8,
    ) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VShift {
                dst,
                src,
                amount: SrcOperand::Imm(1),
                shift,
                elem: actual_elem,
                lanes: actual_lanes,
            } if *src == source
                && *shift == direction
                && *actual_elem == elem
                && *actual_lanes == lanes =>
            {
                *dst
            }
            _ => return None,
        };
        self.fresh(destination)
    }

    fn insert_lane(
        &mut self,
        vector: VReg,
        scalar: VReg,
        lane: u8,
        elem: VecElementType,
    ) -> Option<()> {
        matches!(
            &self.next()?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: actual_elem,
            } if *dst == vector
                && *vec == vector
                && *actual_scalar == scalar
                && *actual_lane == lane
                && *actual_elem == elem
        )
        .then_some(())
    }

    fn permute(
        &mut self,
        src1: VReg,
        src2: VReg,
        indices: VReg,
        elem: VecElementType,
        width: VecWidth,
    ) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VPermute {
                dst,
                src1: actual_src1,
                src2: Some(actual_src2),
                indices: actual_indices,
                elem: actual_elem,
                width: actual_width,
                overwrite_table: false,
            } if *actual_src1 == src1
                && *actual_src2 == src2
                && *actual_indices == indices
                && *actual_elem == elem
                && *actual_width == width =>
            {
                *dst
            }
            _ => return None,
        };
        self.fresh(destination)
    }

    fn compare_not_equal(
        &mut self,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
    ) -> Option<VReg> {
        let destination = match &self.next()?.kind {
            OpKind::VCmp {
                dst,
                src1: actual_src1,
                src2: actual_src2,
                cond: VecCmpCond::Ne,
                elem: actual_elem,
                lanes: actual_lanes,
            } if *actual_src1 == src1
                && *actual_src2 == src2
                && *actual_elem == elem
                && *actual_lanes == lanes =>
            {
                *dst
            }
            _ => return None,
        };
        self.fresh(destination)
    }

    fn destination_move(&mut self, destination: VReg, source: VReg, width: VecWidth) -> Option<()> {
        matches!(
            &self.next()?.kind,
            OpKind::VMov {
                dst,
                src,
                width: actual_width,
            } if *dst == destination && *src == source && *actual_width == width
        )
        .then_some(())
    }
}

/// Validate the complete 6- through 29-op semantic graph following one
/// VPERMIL2 vector load.
///
/// Instruction provenance binds opcode, W/L, all architectural registers,
/// imm8, memory width, and stack-segment selection. The two preceding
/// operations must be the exact dynamic XOP and conditional-alignment guards.
/// Every generated selector/index/mask edge is validated, every generated
/// virtual is distinct except for the lifter's deliberate in-place lane
/// inserts, and no locally defined virtual may escape or be redefined.
///
/// The architectural maximum of eight lanes bounds classification to O(1)
/// time and O(1) auxiliary space. Callers construct definition/use maps once
/// in O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_vpermil2_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexVpermil2MemorySequence> {
    if !allow_mem {
        return None;
    }
    let feature_index = index.checked_sub(2)?;
    let feature = block.ops.get(feature_index)?;
    let alignment = block.ops.get(index - 1)?;
    let load = block.ops.get(index)?;
    if feature.guest_pc != load.guest_pc
        || alignment.guest_pc != load.guest_pc
        || feature.x86_hint.is_some()
        || alignment.x86_hint.is_some()
        || !matches!(feature.kind, OpKind::X86RequireXop)
        || (feature_index != 0 && block.ops[feature_index - 1].guest_pc == load.guest_pc)
    {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_vpermil2_memory_encoding()?;
    let (loaded, address) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if *width == encoding.width
                && load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, addr)
        }
        _ => return None,
    };
    if !matches!(
        &alignment.kind,
        OpKind::X86CheckAlignmentAc {
            addr,
            access_size,
            alignment: 16,
            stack_segment,
            natural_alignment: false,
        } if addr == address
            && u32::from(*access_size) == encoding.memory_size
            && *stack_segment == encoding.stack_segment
    ) {
        return None;
    }

    let width = encoding.width;
    let elem = encoding.elem;
    let lanes = width.lanes(elem) as u8;
    let block_lanes = (16 / elem.bytes()) as u8;
    let selector = if encoding.w {
        loaded
    } else {
        vector(encoding.is4, width)
    };
    let source2 = if encoding.w {
        vector(encoding.is4, width)
    } else {
        loaded
    };

    let mut cursor = Vpermil2GraphCursor::new(block, index + 1, load.guest_pc, loaded)?;
    let shifted = if elem == VecElementType::I64 {
        cursor.shift(selector, ShiftOp::Lsr, elem, lanes)?
    } else {
        selector
    };
    let selector_mask = cursor.splat(i64::from(2 * block_lanes - 1), elem, lanes)?;
    let selected = cursor.and(shifted, selector_mask, width)?;
    let indices = if width == VecWidth::V256 {
        let within_mask = cursor.splat(i64::from(block_lanes - 1), elem, lanes)?;
        let within = cursor.and(selected, within_mask, width)?;
        let source_mask = cursor.splat(i64::from(block_lanes), elem, lanes)?;
        let source = cursor.and(selected, source_mask, width)?;
        let source = cursor.shift(source, ShiftOp::Lsl, elem, lanes)?;
        let block_offsets = cursor.splat(0, elem, lanes)?;
        let high_block_offset = cursor.mov_imm(i64::from(block_lanes))?;
        for lane in block_lanes..lanes {
            cursor.insert_lane(block_offsets, high_block_offset, lane, elem)?;
        }
        let normalized = cursor.or(within, source, width)?;
        cursor.or(normalized, block_offsets, width)?
    } else {
        selected
    };

    let permuted = cursor.permute(
        vector(encoding.source1, width),
        source2,
        indices,
        elem,
        width,
    )?;
    let result = if encoding.immediate & 0b10 == 0 {
        permuted
    } else {
        let m_bit = cursor.splat(8, elem, lanes)?;
        let selected_m = cursor.and(selector, m_bit, width)?;
        let zero = cursor.splat(0, elem, lanes)?;
        let m_mask = cursor.compare_not_equal(selected_m, zero, elem, lanes)?;
        if encoding.immediate & 0b11 == 0b10 {
            cursor.and_not(m_mask, permuted, width)?
        } else {
            cursor.and(m_mask, permuted, width)?
        }
    };
    cursor.destination_move(vector(encoding.destination, width), result, width)?;

    let consumed = cursor.next_index.checked_sub(index)?;
    let sequence = block.ops.get(index..index.checked_add(consumed)?)?;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == load.guest_pc)
        || !local_virtual_definitions_are_closed(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexVpermil2MemorySequence { consumed, encoding })
}
