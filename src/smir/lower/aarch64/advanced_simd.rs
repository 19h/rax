//! Exact AdvSIMD fusions. Every eliminated temporary has one definition and
//! one use within the composition. No virtual value is assigned a guest register.

use std::collections::HashMap;

use crate::smir::ir::SmirFunction;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    SignExtend, VLaneOp, VReg, VecCmpCond, VecElementType, VecPermuteKind,
};
use crate::smir::lower::LowerError;
use crate::smir::optimize::terminator_reg_uses;

use super::Aarch64Lowerer;

pub(crate) struct NeonFusion {
    instruction: u32,
    pub(crate) consumed: usize,
    temporaries: Vec<VReg>,
}

impl NeonFusion {
    fn two_ops(instruction: u32, temporary: VReg) -> Self {
        Self {
            instruction,
            consumed: 2,
            temporaries: vec![temporary],
        }
    }
}

fn neon_vector_register(reg: VReg) -> Option<u32> {
    Aarch64Lowerer::fp_reg(reg).ok().map(u32::from)
}

fn neon_integer_shape(elem: VecElementType, lanes: u8) -> Option<(u32, u32)> {
    let (q, size) = Aarch64Lowerer::simd_integer_shape(elem, lanes).ok()?;
    if size == 3 && q == 0 {
        return None;
    }
    Some((q, size))
}

fn neon_copy_fusion(first: &OpKind, second: &OpKind) -> Option<(u32, VReg)> {
    let OpKind::VExtractLane {
        dst: temporary,
        vec: source,
        lane: source_lane,
        elem,
        sign: SignExtend::Zero,
    } = first
    else {
        return None;
    };
    if !matches!(temporary, VReg::Virtual(_)) {
        return None;
    }

    match second {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes,
        } => {
            if scalar != temporary || broadcast_elem != elem {
                return None;
            }
            let (q, size) = neon_integer_shape(*elem, *lanes)?;
            if u32::from(*source_lane) >= (16 >> size) {
                return None;
            }

            let rd = neon_vector_register(*dst)?;
            let rn = neon_vector_register(*source)?;
            let imm5 = (u32::from(*source_lane) << (size + 1)) | (1 << size);
            let instruction = 0x0e00_0400 | (q << 30) | (imm5 << 16) | (rn << 5) | rd;
            Some((instruction, *temporary))
        }
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: insert_elem,
        } => {
            if scalar != temporary || insert_elem != elem || dst != vec {
                return None;
            }
            let lanes = (16 / elem.bytes()) as u8;
            let (_, size) = neon_integer_shape(*elem, lanes)?;
            if u32::from(*source_lane) >= (16 >> size) || u32::from(*lane) >= (16 >> size) {
                return None;
            }

            let rd = neon_vector_register(*dst)?;
            let rn = neon_vector_register(*source)?;
            let imm5 = (u32::from(*lane) << (size + 1)) | (1 << size);
            let imm4 = u32::from(*source_lane) << size;
            let instruction = 0x6e00_0400 | (imm5 << 16) | (imm4 << 11) | (rn << 5) | rd;
            Some((instruction, *temporary))
        }
        _ => None,
    }
}

fn neon_arithmetic_fusion(first: &OpKind, second: &OpKind) -> Option<(u32, VReg)> {
    let OpKind::VLane {
        dst: temporary,
        src1,
        src2,
        elem,
        lanes,
        op: producer,
        signed,
        set_ovf: false,
    } = first
    else {
        return None;
    };
    if !matches!(temporary, VReg::Virtual(_)) {
        return None;
    }

    let OpKind::VLane {
        dst,
        src1: accumulator,
        src2: intermediate,
        elem: result_elem,
        lanes: result_lanes,
        op: consumer,
        set_ovf: false,
        ..
    } = second
    else {
        return None;
    };
    if intermediate != temporary
        || dst != accumulator
        || result_elem != elem
        || result_lanes != lanes
    {
        return None;
    }

    let (q, size) = neon_integer_shape(*elem, *lanes)?;
    if size == 3 {
        return None;
    }
    let (u, opcode) = match (producer, consumer) {
        (VLaneOp::AbsDiff, VLaneOp::Add) => (u32::from(!signed), 0b01111),
        (VLaneOp::Mul, VLaneOp::Add) => (0, 0b10010),
        (VLaneOp::Mul, VLaneOp::Sub) => (1, 0b10010),
        _ => return None,
    };

    let rd = neon_vector_register(*dst)?;
    let rn = neon_vector_register(*src1)?;
    let rm = neon_vector_register(*src2)?;
    let instruction = 0x0e20_0400
        | (q << 30)
        | (u << 29)
        | (size << 22)
        | (rm << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd;
    Some((instruction, *temporary))
}

fn neon_bit_test_fusion(first: &OpKind, second: &OpKind) -> Option<(u32, VReg)> {
    let OpKind::VLane {
        dst: temporary,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::And,
        set_ovf: false,
        ..
    } = first
    else {
        return None;
    };
    if !matches!(temporary, VReg::Virtual(_)) {
        return None;
    }

    let OpKind::VCmp {
        dst,
        src1: tested,
        src2: VReg::Imm(0),
        cond: VecCmpCond::Ne,
        elem: result_elem,
        lanes: result_lanes,
    } = second
    else {
        return None;
    };
    if tested != temporary || result_elem != elem || result_lanes != lanes {
        return None;
    }

    let shape = Aarch64Lowerer::simd_scalar_or_vector_integer_shape(*elem, *lanes).ok()?;
    let rd = neon_vector_register(*dst)?;
    let rn = neon_vector_register(*src1)?;
    let rm = neon_vector_register(*src2)?;
    let instruction = 0x0e20_8c00 | shape.encoding_bits() | (rm << 16) | (rn << 5) | rd;
    Some((instruction, *temporary))
}

fn neon_pairwise_fusion(ops: &[SmirOp]) -> Option<NeonFusion> {
    let [first, second, third, ..] = ops else {
        return None;
    };
    let OpKind::VPermute2 {
        dst: even,
        src1,
        src2,
        elem,
        lanes,
        kind: VecPermuteKind::Uzp1,
    } = &first.kind
    else {
        return None;
    };
    let OpKind::VPermute2 {
        dst: odd,
        src1: odd_src1,
        src2: odd_src2,
        elem: odd_elem,
        lanes: odd_lanes,
        kind: VecPermuteKind::Uzp2,
    } = &second.kind
    else {
        return None;
    };
    let OpKind::VLane {
        dst,
        src1: combined_even,
        src2: combined_odd,
        elem: result_elem,
        lanes: result_lanes,
        op,
        signed,
        set_ovf: false,
    } = &third.kind
    else {
        return None;
    };
    let operands_match = src1 == odd_src1
        && src2 == odd_src2
        && elem == odd_elem
        && elem == result_elem
        && lanes == odd_lanes
        && lanes == result_lanes
        && even == combined_even
        && odd == combined_odd;
    if !operands_match
        || even == odd
        || !matches!(even, VReg::Virtual(_))
        || !matches!(odd, VReg::Virtual(_))
    {
        return None;
    }
    let (q, size) = neon_integer_shape(*elem, *lanes)?;
    let (u, opcode) = match op {
        VLaneOp::Add => (0, 0b10111),
        VLaneOp::Max if size != 3 => (u32::from(!signed), 0b10100),
        VLaneOp::Min if size != 3 => (u32::from(!signed), 0b10101),
        _ => return None,
    };
    let rd = neon_vector_register(*dst)?;
    let rn = neon_vector_register(*src1)?;
    let rm = neon_vector_register(*src2)?;
    let instruction = 0x0e20_0400
        | (q << 30)
        | (u << 29)
        | (size << 22)
        | (rm << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd;
    Some(NeonFusion {
        instruction,
        consumed: 3,
        temporaries: vec![*even, *odd],
    })
}

fn neon_right_shift_fusion(ops: &[SmirOp]) -> Option<NeonFusion> {
    let [first, second, ..] = ops else {
        return None;
    };
    let OpKind::VBroadcast {
        dst: counts,
        scalar: VReg::Imm(negative_amount),
        elem,
        lanes,
    } = &first.kind
    else {
        return None;
    };
    let OpKind::VLane {
        dst: shifted,
        src1: source,
        src2: shift_counts,
        elem: shift_elem,
        lanes: shift_lanes,
        op: shift_kind,
        signed,
        set_ovf: false,
    } = &second.kind
    else {
        return None;
    };
    let rounded = match shift_kind {
        VLaneOp::ShiftSigned8 => false,
        VLaneOp::ShiftSigned8Rnd => true,
        _ => return None,
    };
    if counts != shift_counts
        || elem != shift_elem
        || lanes != shift_lanes
        || !matches!(counts, VReg::Virtual(_))
    {
        return None;
    }
    let shape = Aarch64Lowerer::simd_scalar_or_vector_integer_shape(*elem, *lanes).ok()?;
    let bits = 8i64 << shape.size;
    if !(-bits..=-1).contains(negative_amount) {
        return None;
    }
    let mut temporaries = vec![*counts];
    let (dst, consumed, opcode) = if matches!(shifted, VReg::Virtual(_)) {
        let OpKind::VLane {
            dst,
            src1: accumulator,
            src2: intermediate,
            elem: result_elem,
            lanes: result_lanes,
            op: VLaneOp::Add,
            set_ovf: false,
            ..
        } = &ops.get(2)?.kind
        else {
            return None;
        };
        if dst != accumulator
            || intermediate != shifted
            || counts == shifted
            || elem != result_elem
            || lanes != result_lanes
        {
            return None;
        }
        temporaries.push(*shifted);
        (*dst, 3, if rounded { 0b00110 } else { 0b00010 })
    } else {
        (*shifted, 2, if rounded { 0b00100 } else { 0b00000 })
    };
    let rd = neon_vector_register(dst)?;
    let rn = neon_vector_register(*source)?;
    let u = u32::from(!signed);
    let encoded_amount = (2 * bits + negative_amount) as u32;
    let register_form = shape.encoding_bits() & !(3 << 22);
    let instruction = 0x0f00_0400
        | register_form
        | (u << 29)
        | (encoded_amount << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd;
    Some(NeonFusion {
        instruction,
        consumed,
        temporaries,
    })
}

/// Recognize an exact composition. The caller must prove all its temporaries
/// are private before admitting or lowering the containing function.
pub(crate) fn aarch64_neon_fusion(ops: &[SmirOp]) -> Option<NeonFusion> {
    let [first, second, ..] = ops else {
        return None;
    };
    let fusion = neon_pairwise_fusion(ops)
        .or_else(|| neon_right_shift_fusion(ops))
        .or_else(|| {
            neon_copy_fusion(&first.kind, &second.kind)
                .or_else(|| neon_arithmetic_fusion(&first.kind, &second.kind))
                .or_else(|| neon_bit_test_fusion(&first.kind, &second.kind))
                .map(|(instruction, temporary)| NeonFusion::two_ops(instruction, temporary))
        })?;
    if ops[..fusion.consumed]
        .iter()
        .any(|op| op.guest_pc != first.guest_pc || op.x86_hint.is_some())
    {
        return None;
    }
    Some(fusion)
}

/// Validate fusion temporaries against every block and terminator, including
/// excluded interpreter frontiers. Expected time is O(N); auxiliary space is O(V),
/// where N is the function size (blocks, operations, and operands) and V is the
/// number of distinct virtual registers.
pub(crate) fn aarch64_neon_fusions_are_private(function: &SmirFunction) -> bool {
    let mut reference_counts = HashMap::<VReg, usize>::new();
    for block in &function.blocks {
        for phi in &block.phis {
            let registers =
                std::iter::once(phi.dst).chain(phi.sources.iter().map(|(_, source)| *source));
            for reg in registers {
                if matches!(reg, VReg::Virtual(_)) {
                    *reference_counts.entry(reg).or_default() += 1;
                }
            }
        }
        for op in &block.ops {
            let registers = op.kind.dests().into_iter().chain(op.kind.source_vregs());
            for reg in registers {
                if matches!(reg, VReg::Virtual(_)) {
                    *reference_counts.entry(reg).or_default() += 1;
                }
            }
        }
        for reg in terminator_reg_uses(&block.terminator) {
            if matches!(reg, VReg::Virtual(_)) {
                *reference_counts.entry(reg).or_default() += 1;
            }
        }
    }

    for block in &function.blocks {
        for index in 0..block.ops.len() {
            if let Some(fusion) = aarch64_neon_fusion(&block.ops[index..]) {
                if fusion
                    .temporaries
                    .iter()
                    .any(|temporary| reference_counts.get(temporary) != Some(&2))
                {
                    return false;
                }
            }
        }
    }
    true
}

impl Aarch64Lowerer {
    pub(crate) fn try_lower_neon_fusion(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let Some(fusion) = aarch64_neon_fusion(ops) else {
            return Ok(None);
        };
        // lower_function validated privacy across every block and frontier.
        self.emit(fusion.instruction);
        Ok(Some(fusion.consumed))
    }
}
