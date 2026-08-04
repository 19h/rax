//! Fail-closed helper-backed EVEX scalar floating-point memory admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, Avx10FP16Op, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, X86FpBinaryOp, X86Reg,
};
use crate::smir::ir::{X86EvexScalarFpArithmeticMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_virtual_definition_use, single_definition_single_use,
};
use super::evex_scalar_memory_source_common::{exact_evex_scalar_result_tail, xmm_index};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX scalar arithmetic/square-root memory decomposition
/// consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarFpArithmeticMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) encoding: X86EvexScalarFpArithmeticMemoryEncoding,
}

fn binary_operation(opcode: u8) -> Option<X86FpBinaryOp> {
    Some(match opcode {
        0x58 => X86FpBinaryOp::Add,
        0x59 => X86FpBinaryOp::Mul,
        0x5C => X86FpBinaryOp::Sub,
        0x5D => X86FpBinaryOp::Min,
        0x5E => X86FpBinaryOp::Div,
        0x5F => X86FpBinaryOp::Max,
        _ => return None,
    })
}

fn fp16_operation(opcode: u8) -> Option<Avx10FP16Op> {
    Some(match opcode {
        0x51 => Avx10FP16Op::Sqrt,
        0x58 => Avx10FP16Op::Add,
        0x59 => Avx10FP16Op::Mul,
        0x5C => Avx10FP16Op::Sub,
        0x5D => Avx10FP16Op::Min,
        0x5E => Avx10FP16Op::Div,
        0x5F => Avx10FP16Op::Max,
        _ => return None,
    })
}

fn element_width(elem: VecElementType) -> Option<OpWidth> {
    match elem {
        VecElementType::F16 => Some(OpWidth::W16),
        VecElementType::F32 => Some(OpWidth::W32),
        VecElementType::F64 => Some(OpWidth::W64),
        _ => None,
    }
}

fn expected_prefix(elem: VecElementType) -> Option<X86SsePrefix> {
    match elem {
        VecElementType::F32 => Some(X86SsePrefix::Rep),
        VecElementType::F64 => Some(X86SsePrefix::Repne),
        _ => None,
    }
}

fn exact_mask_condition(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    mask: u8,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let op = block.ops.get(index)?;
    let condition = match op.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if op.x86_hint.is_none() && src1 == VReg::Arch(ArchReg::X86(X86Reg::K(mask))) => dst,
        _ => return None,
    };
    (op.guest_pc == guest_pc
        && exact_virtual_definition_use(condition, 1, uses, virtual_definitions, virtual_uses))
    .then_some(condition)
}

fn exact_load(op: &crate::smir::ir::ops::SmirOp, expected_width: MemWidth) -> Option<VReg> {
    match &op.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if op.x86_hint.is_none()
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            Some(*dst)
        }
        _ => None,
    }
}

fn exact_predicated_load(
    op: &crate::smir::ir::ops::SmirOp,
    scalar: VReg,
    condition: VReg,
    expected_width: MemWidth,
) -> bool {
    matches!(
        &op.kind,
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } if op.x86_hint.is_none()
            && *dst == scalar
            && *cond == condition
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr)
    )
}

fn exact_scalar_broadcast(
    op: &crate::smir::ir::ops::SmirOp,
    scalar: VReg,
    elem: VecElementType,
) -> Option<VReg> {
    match op.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem: actual_elem,
            lanes: 1,
        } if op.x86_hint.is_none() && actual_scalar == scalar && actual_elem == elem => Some(dst),
        _ => None,
    }
}

fn exact_fp32_fp64_semantic(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    condition: Option<VReg>,
    encoding: X86EvexScalarFpArithmeticMemoryEncoding,
) -> Option<VReg> {
    let prefix = expected_prefix(encoding.elem)?;
    let hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: prefix,
        opcode: encoding.opcode,
        width: VecWidth::V128,
        w: encoding.elem == VecElementType::F64,
    });
    if encoding.opcode == 0x51 {
        return match op.kind {
            OpKind::X86Sqrt {
                dst,
                src,
                elem,
                lanes: 1,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            } if src == memory_source && elem == encoding.elem && op.x86_hint == hint => Some(dst),
            _ => None,
        };
    }
    match op.kind {
        OpKind::X86FpBinary {
            dst,
            src1,
            src2,
            mask,
            elem,
            lanes: 1,
            op: actual_operation,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        } if xmm_index(&src1) == Some(encoding.source1)
            && src2 == memory_source
            && mask == condition
            && elem == encoding.elem
            && actual_operation == binary_operation(encoding.opcode)?
            && op.x86_hint == hint =>
        {
            Some(dst)
        }
        _ => None,
    }
}

fn exact_fp16_semantic(
    op: &crate::smir::ir::ops::SmirOp,
    source1: VReg,
    source2: VReg,
    encoding: X86EvexScalarFpArithmeticMemoryEncoding,
) -> Option<VReg> {
    match op.kind {
        OpKind::VFP16Arith {
            dst,
            src1,
            src2,
            mask: None,
            op: actual_operation,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 1,
            zeroing: false,
        } if op.x86_hint.is_none()
            && src1 == source1
            && src2 == source2
            && actual_operation == fp16_operation(encoding.opcode)? =>
        {
            Some(dst)
        }
        _ => None,
    }
}

fn exact_result_extract(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    raw: VReg,
    elem: VecElementType,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    if !single_definition_single_use(raw, virtual_definitions, virtual_uses) {
        return None;
    }
    let op = block.ops.get(index)?;
    let scalar = match op.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: actual_elem,
            sign: SignExtend::Zero,
        } if op.x86_hint.is_none() && vec == raw && actual_elem == elem => dst,
        _ => return None,
    };
    (op.guest_pc == guest_pc
        && single_definition_single_use(scalar, virtual_definitions, virtual_uses))
    .then_some(scalar)
}

#[allow(clippy::too_many_arguments)]
fn exact_masked_result_select(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    condition: VReg,
    scalar_result: VReg,
    encoding: X86EvexScalarFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let fallback_op = block.ops.get(index)?;
    let fallback = if encoding.zeroing {
        match fallback_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width,
            } if fallback_op.x86_hint.is_none() && width == element_width(encoding.elem)? => dst,
            _ => return None,
        }
    } else {
        match fallback_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem,
                sign: SignExtend::Zero,
            } if fallback_op.x86_hint.is_none()
                && xmm_index(&vec) == Some(encoding.destination)
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        }
    };
    if fallback_op.guest_pc != guest_pc
        || !single_definition_single_use(fallback, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let select = block.ops.get(index + 1)?;
    let selected = match select.kind {
        OpKind::Select {
            dst,
            cond,
            src_true,
            src_false,
            width,
        } if select.x86_hint.is_none()
            && cond == condition
            && src_true == scalar_result
            && src_false == fallback
            && width == element_width(encoding.elem)? =>
        {
            dst
        }
        _ => return None,
    };
    (select.guest_pc == guest_pc
        && single_definition_single_use(selected, virtual_definitions, virtual_uses))
    .then_some((selected, 2))
}

fn exact_unmasked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpArithmeticMemorySequence> {
    if encoding.writemask.is_some() || encoding.zeroing {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let upper_source = VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.source1)));
    let mut offset = 0usize;

    let first_scalar = if encoding.elem == VecElementType::F16 && encoding.opcode != 0x51 {
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: VecElementType::F16,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none() && vec == upper_source => dst,
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        Some(scalar)
    } else {
        None
    };

    let load_offset = offset;
    let load = block.ops.get(index + offset)?;
    let loaded = exact_load(load, encoding.memory_width)?;
    if load.guest_pc != guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let (source1, source2) = if let Some(first_scalar) = first_scalar {
        let first_broadcast = block.ops.get(index + offset)?;
        let first = exact_scalar_broadcast(first_broadcast, first_scalar, encoding.elem)?;
        if first_broadcast.guest_pc != guest_pc
            || !single_definition_single_use(first, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        let second_broadcast = block.ops.get(index + offset)?;
        let second = exact_scalar_broadcast(second_broadcast, loaded, encoding.elem)?;
        if second_broadcast.guest_pc != guest_pc
            || !single_definition_single_use(second, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        (first, second)
    } else {
        let broadcast = block.ops.get(index + offset)?;
        let source = exact_scalar_broadcast(broadcast, loaded, encoding.elem)?;
        let source_uses = if encoding.elem == VecElementType::F16 && encoding.opcode == 0x51 {
            2
        } else {
            1
        };
        if broadcast.guest_pc != guest_pc
            || !exact_virtual_definition_use(
                source,
                1,
                source_uses,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        offset += 1;
        (source, source)
    };

    let semantic = block.ops.get(index + offset)?;
    let raw = if encoding.elem == VecElementType::F16 {
        exact_fp16_semantic(semantic, source1, source2, encoding)?
    } else {
        exact_fp32_fp64_semantic(semantic, source2, None, encoding)?
    };
    if semantic.guest_pc != guest_pc {
        return None;
    }
    offset += 1;
    let scalar_result = exact_result_extract(
        block,
        index + offset,
        guest_pc,
        raw,
        encoding.elem,
        virtual_definitions,
        virtual_uses,
    )?;
    offset += 1;
    let consumed = exact_evex_scalar_result_tail(
        block,
        index,
        guest_pc,
        offset,
        scalar_result,
        upper_source,
        encoding.elem,
        encoding.destination,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexScalarFpArithmeticMemorySequence {
        consumed,
        load_offset,
        encoding,
    })
}

fn exact_masked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpArithmeticMemorySequence> {
    let mask = encoding.writemask?;
    let guest_pc = block.ops.get(index)?.guest_pc;
    let condition_uses = if encoding.opcode != 0x51 { 3 } else { 2 };
    let condition = exact_mask_condition(
        block,
        index,
        guest_pc,
        mask,
        condition_uses,
        virtual_definitions,
        virtual_uses,
    )?;
    let upper_source = VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.source1)));
    let mut offset = 1usize;

    let first_scalar = if encoding.elem == VecElementType::F16 && encoding.opcode != 0x51 {
        let extract = block.ops.get(index + offset)?;
        let raw = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: VecElementType::F16,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none() && vec == upper_source => dst,
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        let zero_op = block.ops.get(index + offset)?;
        let zero = match zero_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W16,
            } if zero_op.x86_hint.is_none() => dst,
            _ => return None,
        };
        if zero_op.guest_pc != guest_pc
            || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        let select = block.ops.get(index + offset)?;
        let selected = match select.kind {
            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width: OpWidth::W16,
            } if select.x86_hint.is_none()
                && cond == condition
                && src_true == raw
                && src_false == zero =>
            {
                dst
            }
            _ => return None,
        };
        if select.guest_pc != guest_pc
            || !single_definition_single_use(selected, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        Some(selected)
    } else {
        None
    };

    let inactive_source = if encoding.elem == VecElementType::F16 && encoding.opcode == 0x5E {
        0x3C00
    } else {
        0
    };
    let seed = block.ops.get(index + offset)?;
    let loaded = match seed.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(value),
            width,
        } if seed.x86_hint.is_none()
            && value == inactive_source
            && width == element_width(encoding.elem)? =>
        {
            dst
        }
        _ => return None,
    };
    if seed.guest_pc != guest_pc
        || !exact_virtual_definition_use(loaded, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    let load_offset = offset;
    let load = block.ops.get(index + offset)?;
    if load.guest_pc != guest_pc
        || !exact_predicated_load(load, loaded, condition, encoding.memory_width)
    {
        return None;
    }
    offset += 1;

    let (source1, source2) = if let Some(first_scalar) = first_scalar {
        let first_broadcast = block.ops.get(index + offset)?;
        let first = exact_scalar_broadcast(first_broadcast, first_scalar, encoding.elem)?;
        if first_broadcast.guest_pc != guest_pc
            || !single_definition_single_use(first, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        let second_broadcast = block.ops.get(index + offset)?;
        let second = exact_scalar_broadcast(second_broadcast, loaded, encoding.elem)?;
        if second_broadcast.guest_pc != guest_pc
            || !single_definition_single_use(second, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        (first, second)
    } else {
        let broadcast = block.ops.get(index + offset)?;
        let source = exact_scalar_broadcast(broadcast, loaded, encoding.elem)?;
        let source_uses = if encoding.elem == VecElementType::F16 && encoding.opcode == 0x51 {
            2
        } else {
            1
        };
        if broadcast.guest_pc != guest_pc
            || !exact_virtual_definition_use(
                source,
                1,
                source_uses,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        offset += 1;
        (source, source)
    };

    let semantic = block.ops.get(index + offset)?;
    let raw = if encoding.elem == VecElementType::F16 {
        exact_fp16_semantic(semantic, source1, source2, encoding)?
    } else {
        exact_fp32_fp64_semantic(
            semantic,
            source2,
            (encoding.opcode != 0x51).then_some(condition),
            encoding,
        )?
    };
    if semantic.guest_pc != guest_pc {
        return None;
    }
    offset += 1;
    let scalar_result = exact_result_extract(
        block,
        index + offset,
        guest_pc,
        raw,
        encoding.elem,
        virtual_definitions,
        virtual_uses,
    )?;
    offset += 1;
    let (selected, selected_ops) = exact_masked_result_select(
        block,
        index + offset,
        guest_pc,
        condition,
        scalar_result,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    offset += selected_ops;
    let consumed = exact_evex_scalar_result_tail(
        block,
        index,
        guest_pc,
        offset,
        selected,
        upper_source,
        encoding.elem,
        encoding.destination,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexScalarFpArithmeticMemorySequence {
        consumed,
        load_offset,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX scalar
/// arithmetic or square-root memory source.
///
/// Exact provenance binds the opcode map, element type, architectural
/// destination/source 1, LLIG image, writemask policy, helper address, dynamic
/// rounding, and the single destination commit. Matching is O(L) time and
/// O(L) auxiliary space for the fixed XMM lane count L <= 8; callers build
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_scalar_fp_arithmetic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpArithmeticMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if !exact_evex_memory_sequence_frontier(block, index, first.guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_scalar_fp_arithmetic_memory_encoding()?;
    let sequence = if encoding.writemask.is_some() {
        exact_masked_sequence(block, index, encoding, virtual_definitions, virtual_uses)?
    } else {
        exact_unmasked_sequence(block, index, encoding, virtual_definitions, virtual_uses)?
    };
    let address = match &block.ops.get(index + sequence.load_offset)?.kind {
        OpKind::Load { addr, .. } | OpKind::PredLoad { addr, .. } => addr,
        _ => return None,
    };
    exact_evex_memory_apx_frontier(block, index, first.guest_pc, address).then_some(sequence)
}
