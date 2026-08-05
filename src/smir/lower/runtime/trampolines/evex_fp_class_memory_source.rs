//! Fail-closed helper-backed EVEX `VFPCLASS*` memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecCmpCond, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexFpClassMemoryEncoding, X86EvexFpClassMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence_tail,
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, exact_lane_address,
    exact_lane_predicate, exact_virtual_definition_use, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed `VFPCLASS*`
/// memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFpClassMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexFpClassMemoryEncoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphProfile {
    /// Unoptimized lifting retains every class predicate.
    Full,
    /// O1/O2 dead-code elimination retains only selected class predicates.
    Live,
}

impl GraphProfile {
    fn retain(self, selected: bool) -> bool {
        self == Self::Full || selected
    }
}

fn exact_op(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: usize,
    guest_pc: GuestAddr,
) -> Option<&SmirOp> {
    let op = block.ops.get(index + offset)?;
    (op.guest_pc == guest_pc && op.x86_hint.is_none()).then_some(op)
}

fn fresh_virtual(register: VReg, external: VReg, fresh: &mut HashSet<VReg>) -> bool {
    register != external && matches!(register, VReg::Virtual(_)) && fresh.insert(register)
}

#[allow(clippy::too_many_arguments)]
fn exact_splat(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    value: i64,
    elem: VecElementType,
    lanes: u8,
    external: VReg,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let scalar_op = exact_op(block, index, *offset, guest_pc)?;
    let scalar = match scalar_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(actual),
            width: OpWidth::W64,
        } if actual == value => dst,
        _ => return None,
    };
    if !fresh_virtual(scalar, external, fresh) {
        return None;
    }
    *offset += 1;

    let broadcast = exact_op(block, index, *offset, guest_pc)?;
    let vector = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem: actual_elem,
            lanes: actual_lanes,
        } if actual_scalar == scalar && actual_elem == elem && actual_lanes == lanes => dst,
        _ => return None,
    };
    if !fresh_virtual(vector, external, fresh) {
        return None;
    }
    *offset += 1;
    Some(vector)
}

#[allow(clippy::too_many_arguments)]
fn exact_vector_binary(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source1: VReg,
    source2: VReg,
    width: VecWidth,
    expected: fn(VReg, VReg, VReg, VecWidth) -> OpKind,
    external: VReg,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let op = exact_op(block, index, *offset, guest_pc)?;
    let destination = match op.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width: actual_width,
        } if matches!(expected(dst, source1, source2, width), OpKind::VAnd { .. })
            && src1 == source1
            && src2 == source2
            && actual_width == width =>
        {
            dst
        }
        OpKind::VAndNot {
            dst,
            src1,
            src2,
            width: actual_width,
        } if matches!(
            expected(dst, source1, source2, width),
            OpKind::VAndNot { .. }
        ) && src1 == source1
            && src2 == source2
            && actual_width == width =>
        {
            dst
        }
        OpKind::VOr {
            dst,
            src1,
            src2,
            width: actual_width,
        } if matches!(expected(dst, source1, source2, width), OpKind::VOr { .. })
            && src1 == source1
            && src2 == source2
            && actual_width == width =>
        {
            dst
        }
        _ => return None,
    };
    if !fresh_virtual(destination, external, fresh) {
        return None;
    }
    *offset += 1;
    Some(destination)
}

fn vand(dst: VReg, src1: VReg, src2: VReg, width: VecWidth) -> OpKind {
    OpKind::VAnd {
        dst,
        src1,
        src2,
        width,
    }
}

fn vand_not(dst: VReg, src1: VReg, src2: VReg, width: VecWidth) -> OpKind {
    OpKind::VAndNot {
        dst,
        src1,
        src2,
        width,
    }
}

fn vor(dst: VReg, src1: VReg, src2: VReg, width: VecWidth) -> OpKind {
    OpKind::VOr {
        dst,
        src1,
        src2,
        width,
    }
}

#[allow(clippy::too_many_arguments)]
fn exact_compare(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source1: VReg,
    source2: VReg,
    condition: VecCmpCond,
    elem: VecElementType,
    lanes: u8,
    external: VReg,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let op = exact_op(block, index, *offset, guest_pc)?;
    let destination = match op.kind {
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem: actual_elem,
            lanes: actual_lanes,
        } if src1 == source1
            && src2 == source2
            && cond == condition
            && actual_elem == elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if !fresh_virtual(destination, external, fresh) {
        return None;
    }
    *offset += 1;
    Some(destination)
}

#[allow(clippy::too_many_arguments)]
fn exact_daz_zero_compare(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source: VReg,
    zero: VReg,
    encoding: X86EvexFpClassMemoryEncoding,
    lanes: u8,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let op = exact_op(block, index, *offset, guest_pc)?;
    let destination = match op.kind {
        OpKind::X86VectorFpCompare {
            dst,
            src1,
            src2,
            mask: None,
            elem,
            width,
            lanes: actual_lanes,
            predicate: 0,
            scalar: false,
            mask_destination: false,
            zero_upper: false,
            suppress_exceptions: true,
        } if src1 == source
            && src2 == zero
            && elem == encoding.elem
            && width == encoding.width
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if !fresh_virtual(destination, source, fresh) {
        return None;
    }
    *offset += 1;
    Some(destination)
}

#[allow(clippy::too_many_arguments)]
fn exact_movemask(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    classified: VReg,
    elem: VecElementType,
    lanes: u8,
    external: VReg,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let zero = exact_op(block, index, *offset, guest_pc)?;
    let accumulated = match zero.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => dst,
        _ => return None,
    };
    if !fresh_virtual(accumulated, external, fresh) {
        return None;
    }
    *offset += 1;

    for lane in 0..lanes {
        let extract = exact_op(block, index, *offset, guest_pc)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem: actual_elem,
                sign: SignExtend::Zero,
            } if vec == classified && actual_lane == lane && actual_elem == elem => dst,
            _ => return None,
        };
        if !fresh_virtual(scalar, external, fresh) {
            return None;
        }
        *offset += 1;

        let shift = exact_op(block, index, *offset, guest_pc)?;
        let sign = match shift.kind {
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if src == scalar && amount == i64::from(elem.bytes() * 8 - 1) => dst,
            _ => return None,
        };
        if !fresh_virtual(sign, external, fresh) {
            return None;
        }
        *offset += 1;

        let positioned = if lane == 0 {
            sign
        } else {
            let shift = exact_op(block, index, *offset, guest_pc)?;
            let positioned = match shift.kind {
                OpKind::Shl {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if src == sign && amount == i64::from(lane) => dst,
                _ => return None,
            };
            if !fresh_virtual(positioned, external, fresh) {
                return None;
            }
            *offset += 1;
            positioned
        };

        let combine = exact_op(block, index, *offset, guest_pc)?;
        if !matches!(
            combine.kind,
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(actual_positioned),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == accumulated && src1 == accumulated && actual_positioned == positioned
        ) {
            return None;
        }
        *offset += 1;
    }

    let Some(candidate) = block.ops.get(index + *offset) else {
        return Some(accumulated);
    };
    match candidate.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } if candidate.guest_pc == guest_pc
            && candidate.x86_hint.is_none()
            && matches!(dst, VReg::Virtual(_))
            && src == accumulated =>
        {
            if !fresh_virtual(dst, external, fresh) {
                return None;
            }
            *offset += 1;
            Some(dst)
        }
        _ => Some(accumulated),
    }
}

fn exact_commit(op: &SmirOp, raw_mask: VReg, encoding: X86EvexFpClassMemoryEncoding) -> bool {
    let destination = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.destination)));
    if op.x86_hint.is_some() {
        return false;
    }
    match encoding.writemask {
        Some(mask) => matches!(
            op.kind,
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Reg(actual_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == destination
                && src1 == raw_mask
                && actual_mask == VReg::Arch(ArchReg::X86(X86Reg::K(mask)))
        ),
        None => matches!(
            op.kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            } if dst == destination && src == raw_mask
        ),
    }
}

fn exact_tail_virtual_closure(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    external: VReg,
    fresh: &HashSet<VReg>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let Some(end) = index.checked_add(consumed) else {
        return false;
    };
    let Some(ops) = block.ops.get(index..end) else {
        return false;
    };
    let mut local_definitions = HashMap::<VReg, usize>::new();
    let mut local_uses = HashMap::<VReg, usize>::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_default() += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_default() += 1;
            }
        }
    }
    fresh.iter().all(|register| {
        local_definitions.contains_key(register)
            && virtual_definitions.get(register) == local_definitions.get(register)
            && virtual_uses.get(register).copied().unwrap_or(0)
                == local_uses.get(register).copied().unwrap_or(0)
    }) && local_definitions
        .keys()
        .all(|register| fresh.contains(register))
        && local_uses
            .keys()
            .all(|register| *register == external || fresh.contains(register))
}

fn exact_fp_class_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    source: VReg,
    encoding: X86EvexFpClassMemoryEncoding,
    profile: GraphProfile,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let lanes = if encoding.scalar {
        1
    } else {
        encoding.width.lanes(encoding.elem) as u8
    };
    let (bit_elem, exponent_mask, mantissa_mask, sign_mask, quiet_mask): (
        VecElementType,
        u64,
        u64,
        u64,
        u64,
    ) = match encoding.elem {
        VecElementType::F16 => (VecElementType::I16, 0x7C00, 0x03FF, 0x8000, 0x0200),
        VecElementType::F32 => (
            VecElementType::I32,
            0x7F80_0000,
            0x007F_FFFF,
            0x8000_0000,
            0x0040_0000,
        ),
        VecElementType::F64 => (
            VecElementType::I64,
            0x7FF0_0000_0000_0000,
            0x000F_FFFF_FFFF_FFFF,
            0x8000_0000_0000_0000,
            0x0008_0000_0000_0000,
        ),
        _ => return None,
    };
    let selected = |bit: u8| encoding.immediate & (1 << bit) != 0;
    let qnan_live = selected(0);
    let positive_zero_live = selected(1);
    let negative_zero_live = selected(2);
    let positive_infinity_live = selected(3);
    let negative_infinity_live = selected(4);
    let denormal_live = selected(5);
    let negative_finite_live = selected(6);
    let snan_live = selected(7);
    let zero_number_live =
        positive_zero_live || negative_zero_live || denormal_live || negative_finite_live;
    let nan_live = qnan_live || snan_live;
    let infinity_live = positive_infinity_live || negative_infinity_live;
    let negative_live = positive_zero_live
        || negative_zero_live
        || positive_infinity_live
        || negative_infinity_live
        || negative_finite_live;
    let exponent_ones_live = nan_live || infinity_live || negative_finite_live;
    let exponent_zeros_live =
        denormal_live || (encoding.elem == VecElementType::F16 && zero_number_live);
    let mantissa_zeros_live =
        nan_live || infinity_live || (encoding.elem == VecElementType::F16 && zero_number_live);
    let exponent_live = exponent_ones_live || exponent_zeros_live;

    let full_lanes = encoding.width.lanes(bit_elem) as u8;
    let mut offset = 0usize;
    let mut fresh = HashSet::new();
    macro_rules! splat {
        ($value:expr) => {
            exact_splat(
                block,
                index,
                &mut offset,
                guest_pc,
                $value,
                bit_elem,
                full_lanes,
                source,
                &mut fresh,
            )?
        };
    }
    macro_rules! binary {
        ($source1:expr, $source2:expr, $expected:expr) => {
            exact_vector_binary(
                block,
                index,
                &mut offset,
                guest_pc,
                $source1,
                $source2,
                encoding.width,
                $expected,
                source,
                &mut fresh,
            )?
        };
    }
    macro_rules! compare {
        ($source1:expr, $source2:expr, $condition:expr) => {
            exact_compare(
                block,
                index,
                &mut offset,
                guest_pc,
                $source1,
                $source2,
                $condition,
                bit_elem,
                lanes,
                source,
                &mut fresh,
            )?
        };
    }

    let zero = splat!(0);
    let exponent_constant = if profile.retain(exponent_live) {
        Some(splat!(exponent_mask as i64))
    } else {
        None
    };
    let mantissa_constant = if profile.retain(mantissa_zeros_live) {
        Some(splat!(mantissa_mask as i64))
    } else {
        None
    };
    let sign_constant = if profile.retain(negative_live) {
        Some(splat!(sign_mask as i64))
    } else {
        None
    };
    let quiet_constant = if profile.retain(nan_live) {
        Some(splat!(quiet_mask as i64))
    } else {
        None
    };

    let exponent = if let Some(constant) = exponent_constant {
        Some(binary!(source, constant, vand))
    } else {
        None
    };
    let mantissa = if let Some(constant) = mantissa_constant {
        Some(binary!(source, constant, vand))
    } else {
        None
    };
    let sign_bits = if let Some(constant) = sign_constant {
        Some(binary!(source, constant, vand))
    } else {
        None
    };
    let quiet_bits = if let Some(constant) = quiet_constant {
        Some(binary!(source, constant, vand))
    } else {
        None
    };

    let exponent_all_ones = if profile.retain(exponent_ones_live) {
        Some(compare!(exponent?, exponent_constant?, VecCmpCond::Eq))
    } else {
        None
    };
    let exponent_all_zeros = if profile.retain(exponent_zeros_live) {
        Some(compare!(exponent?, zero, VecCmpCond::Eq))
    } else {
        None
    };
    let mantissa_all_zeros = if profile.retain(mantissa_zeros_live) {
        Some(compare!(mantissa?, zero, VecCmpCond::Eq))
    } else {
        None
    };
    let negative = if profile.retain(negative_live) {
        Some(compare!(sign_bits?, zero, VecCmpCond::Ne))
    } else {
        None
    };
    let quiet = if profile.retain(nan_live) {
        Some(compare!(quiet_bits?, zero, VecCmpCond::Ne))
    } else {
        None
    };

    let zero_number = if encoding.elem == VecElementType::F16 {
        if profile.retain(zero_number_live) {
            Some(binary!(exponent_all_zeros?, mantissa_all_zeros?, vand))
        } else {
            None
        }
    } else {
        Some(exact_daz_zero_compare(
            block,
            index,
            &mut offset,
            guest_pc,
            source,
            zero,
            encoding,
            lanes,
            &mut fresh,
        )?)
    };

    let nan = if profile.retain(nan_live) {
        Some(binary!(mantissa_all_zeros?, exponent_all_ones?, vand_not))
    } else {
        None
    };
    let qnan = if profile.retain(qnan_live) {
        Some(binary!(nan?, quiet?, vand))
    } else {
        None
    };
    let snan = if profile.retain(snan_live) {
        Some(binary!(quiet?, nan?, vand_not))
    } else {
        None
    };
    let positive_zero = if profile.retain(positive_zero_live) {
        Some(binary!(negative?, zero_number?, vand_not))
    } else {
        None
    };
    let negative_zero = if profile.retain(negative_zero_live) {
        Some(binary!(negative?, zero_number?, vand))
    } else {
        None
    };
    let infinity = if profile.retain(infinity_live) {
        Some(binary!(exponent_all_ones?, mantissa_all_zeros?, vand))
    } else {
        None
    };
    let positive_infinity = if profile.retain(positive_infinity_live) {
        Some(binary!(negative?, infinity?, vand_not))
    } else {
        None
    };
    let negative_infinity = if profile.retain(negative_infinity_live) {
        Some(binary!(negative?, infinity?, vand))
    } else {
        None
    };
    let denormal = if profile.retain(denormal_live) {
        Some(binary!(zero_number?, exponent_all_zeros?, vand_not))
    } else {
        None
    };
    let negative_non_infinite = if profile.retain(negative_finite_live) {
        Some(binary!(exponent_all_ones?, negative?, vand_not))
    } else {
        None
    };
    let negative_finite = if profile.retain(negative_finite_live) {
        Some(binary!(zero_number?, negative_non_infinite?, vand_not))
    } else {
        None
    };

    let classes = [
        qnan,
        positive_zero,
        negative_zero,
        positive_infinity,
        negative_infinity,
        denormal,
        negative_finite,
        snan,
    ];
    let mut classified = zero;
    for (bit, class) in classes.into_iter().enumerate() {
        if encoding.immediate & (1 << bit) != 0 {
            classified = binary!(classified, class?, vor);
        }
    }
    let raw_mask = exact_movemask(
        block,
        index,
        &mut offset,
        guest_pc,
        classified,
        encoding.elem,
        lanes,
        source,
        &mut fresh,
    )?;
    let commit = exact_op(block, index, offset, guest_pc)?;
    if !exact_commit(commit, raw_mask, encoding) {
        return None;
    }
    offset += 1;

    exact_tail_virtual_closure(
        block,
        index,
        offset,
        source,
        &fresh,
        virtual_definitions,
        virtual_uses,
    )
    .then_some(offset)
}

fn memory_source_uses(encoding: X86EvexFpClassMemoryEncoding, profile: GraphProfile) -> usize {
    if profile == GraphProfile::Full {
        return 4 + usize::from(encoding.elem != VecElementType::F16);
    }
    let selected = |bit: u8| encoding.immediate & (1 << bit) != 0;
    let zero_number = selected(1) || selected(2) || selected(5) || selected(6);
    let nan = selected(0) || selected(7);
    let infinity = selected(3) || selected(4);
    let negative = selected(1) || selected(2) || selected(3) || selected(4) || selected(6);
    let exponent_ones = nan || infinity || selected(6);
    let exponent_zeros = selected(5) || (encoding.elem == VecElementType::F16 && zero_number);
    usize::from(exponent_ones || exponent_zeros)
        + usize::from(nan || infinity || (encoding.elem == VecElementType::F16 && zero_number))
        + usize::from(negative)
        + usize::from(nan)
        + usize::from(encoding.elem != VecElementType::F16)
}

#[allow(clippy::too_many_arguments)]
fn exact_fault_only_masked_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexFpClassMemoryEncoding,
    broadcast: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpClassMemorySequence> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let lea = block.ops.get(index)?;
    let base = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.guest_pc == guest_pc
            && lea.x86_hint.is_none()
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *base
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(
        base,
        1,
        usize::from(lanes),
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }

    let mut offset = 1usize;
    let mut external = None;
    for lane in 0..lanes {
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
        let loaded = match &load.kind {
            OpKind::PredLoad {
                dst: loaded @ VReg::Virtual(_),
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            } if load.guest_pc == guest_pc
                && load.x86_hint.is_none()
                && *cond == condition
                && *width == encoding.memory_width
                && exact_lane_address(
                    addr,
                    base,
                    if broadcast {
                        0
                    } else {
                        i64::from(lane) * i64::from(encoding.elem.bytes())
                    },
                ) =>
            {
                *loaded
            }
            _ => return None,
        };
        if !exact_virtual_definition_use(loaded, 1, 0, virtual_definitions, virtual_uses) {
            return None;
        }
        external.get_or_insert(loaded);
        offset += 1;
    }

    let tail = exact_fp_class_tail(
        block,
        index + offset,
        external?,
        encoding,
        GraphProfile::Live,
        virtual_definitions,
        virtual_uses,
    )?;
    offset += tail;
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(
            block,
            index,
            guest_pc,
            match &lea.kind {
                OpKind::Lea { addr, .. } => addr,
                _ => unreachable!(),
            },
        )
    {
        return None;
    }
    Some(X86JitEvexFpClassMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: if broadcast {
            encoding.memory_width.bytes()
        } else {
            encoding.width.bytes()
        },
        encoding,
    })
}

#[allow(clippy::too_many_arguments)]
fn exact_fault_only_scalar_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexFpClassMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpClassMemorySequence> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let mut offset = 0usize;
    let condition = if let Some(mask) = encoding.writemask {
        Some(exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            0,
            virtual_definitions,
            virtual_uses,
        )?)
    } else {
        None
    };
    let address_offset = offset;
    let load = block.ops.get(index + offset)?;
    let (loaded, address) = match (&load.kind, condition) {
        (
            OpKind::Load {
                dst: loaded @ VReg::Virtual(_),
                addr,
                width,
                sign: SignExtend::Zero,
            },
            None,
        ) if *width == encoding.memory_width => (*loaded, addr),
        (
            OpKind::PredLoad {
                dst: loaded @ VReg::Virtual(_),
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            },
            Some(expected_condition),
        ) if *cond == expected_condition && *width == encoding.memory_width => (*loaded, addr),
        _ => return None,
    };
    if load.guest_pc != guest_pc
        || load.x86_hint.is_some()
        || !x86_jit_mem_address_shape_valid(address)
        || !exact_virtual_definition_use(loaded, 1, 0, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    offset += exact_fp_class_tail(
        block,
        index + offset,
        loaded,
        encoding,
        GraphProfile::Live,
        virtual_definitions,
        virtual_uses,
    )?;
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexFpClassMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: encoding.memory_width.bytes(),
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed or
/// scalar EVEX `VFPCLASS*` memory source.
///
/// Exact provenance binds precision, vector width/LLIG image, K destination
/// and writemask, `imm8`, full/broadcast/scalar tuple, helper address, E4/E6/
/// E10 fault suppression, DAZ-aware binary32/binary64 zero classification,
/// raw binary16 classification, every selected class predicate, movemask
/// reduction, APX address frontier, virtual-value closure, and sole K commit.
/// Classification is O(L) time and O(V) space, where L <= 32 lanes and V is
/// the matched virtual-value count; callers build definition/use maps once in
/// O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_fp_class_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpClassMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_fp_class_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexFpClassMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexFpClassMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexFpClassMemoryReplay::MaskedVector { .. } => X86EvexE4MemoryReplayForm::MaskedVector,
        X86EvexFpClassMemoryReplay::Scalar { .. } => X86EvexE4MemoryReplayForm::Scalar,
    };
    for profile in [GraphProfile::Full, GraphProfile::Live] {
        if profile == GraphProfile::Live && memory_source_uses(encoding, profile) == 0 {
            let fault_only = match encoding.replay {
                X86EvexFpClassMemoryReplay::Broadcast { .. } if encoding.writemask.is_some() => {
                    exact_fault_only_masked_source(
                        block,
                        index,
                        encoding,
                        true,
                        virtual_definitions,
                        virtual_uses,
                    )
                }
                X86EvexFpClassMemoryReplay::MaskedVector { .. } => exact_fault_only_masked_source(
                    block,
                    index,
                    encoding,
                    false,
                    virtual_definitions,
                    virtual_uses,
                ),
                X86EvexFpClassMemoryReplay::Broadcast { .. }
                | X86EvexFpClassMemoryReplay::Scalar { .. } => exact_fault_only_scalar_source(
                    block,
                    index,
                    encoding,
                    virtual_definitions,
                    virtual_uses,
                ),
                X86EvexFpClassMemoryReplay::Vector { .. } => None,
            };
            if fault_only.is_some() {
                return fault_only;
            }
        }
        let shape = X86EvexE4MemoryShape {
            width: encoding.width,
            elem: encoding.elem,
            writemask: encoding.writemask,
            zeroing: false,
            vector_load_hint: None,
            form,
            memory_source_uses: memory_source_uses(encoding, profile),
        };
        if let Some(exact) = exact_evex_e4_memory_sequence_tail(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            |block, tail_index, memory_source| {
                exact_fp_class_tail(
                    block,
                    tail_index,
                    memory_source,
                    encoding,
                    profile,
                    virtual_definitions,
                    virtual_uses,
                )
            },
        ) {
            return Some(X86JitEvexFpClassMemorySequence {
                consumed: exact.consumed,
                address_offset: exact.address_offset,
                memory_size: exact.memory_size,
                encoding,
            });
        }
    }
    None
}
