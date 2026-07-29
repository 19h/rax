//! Fail-closed helper-backed VEX binary memory-source admission.

mod vex_horizontal_integer;
mod vex_integer_minmax;
mod vex_integer_multiply_add;
mod vex_integer_pack;
mod vex_interleave;
mod vex_pmul_high_word;
mod vex_pmul_low;
mod vex_shared_count_shift;
mod vex_widening_dword_multiply;

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VLaneOp,
    VReg, VecElementType, VecWidth, X86AesOp, X86FmaKind, X86FmaOrder, X86FpBinaryOp, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX/EVEX AES memory-source sequence consumed by the
/// helper-backed lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitAesMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    /// Architectural first source for binary rounds. Unary VAESIMC and
    /// VAESKEYGENASSIST receive the helper value as their first source.
    pub(crate) source1: Option<u8>,
    pub(crate) width: VecWidth,
    pub(crate) needs_aes: bool,
    pub(crate) needs_vaes: bool,
    pub(crate) needs_avx512vl: bool,
    pub(crate) supports_avx_ymm16: bool,
}

fn aes_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
        | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => Some(*index),
        _ => None,
    }
}

/// Validate one exact unmasked VEX/EVEX AES memory-source pair. The memory
/// value must be a single-definition/single-use virtual consumed immediately
/// by one architectural AES operation at the same guest PC. This lets the
/// lowerer replace the virtual with a borrowed vector register without making
/// any allocator-owned value observable.
///
/// The classifier is O(1); callers build definition/use maps once in O(N) time
/// and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_aes_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitAesMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width: consumer_width,
        op,
        imm,
    } = &consumer.kind
    else {
        return None;
    };
    if *consumer_width != width {
        return None;
    }
    let destination = aes_vector_index(dst, width)?;
    let source1 = match op {
        X86AesOp::Enc | X86AesOp::EncLast | X86AesOp::Dec | X86AesOp::DecLast
            if *imm == 0 && *src2 == Some(temporary) =>
        {
            Some(aes_vector_index(src1, width)?)
        }
        X86AesOp::InvMixColumns
            if width == VecWidth::V128
                && *imm == 0
                && *src1 == temporary
                && src2.is_none()
                && destination <= 15 =>
        {
            None
        }
        X86AesOp::KeygenAssist
            if width == VecWidth::V128
                && *src1 == temporary
                && src2.is_none()
                && destination <= 15 =>
        {
            None
        }
        _ => return None,
    };

    let high_register = destination >= 16 || source1.is_some_and(|source_index| source_index >= 16);
    // Intel SDM Vol. 2 specifies AES+AVX for VEX.128 round forms and
    // VAES for VEX.256 or EVEX round forms. The lowerer re-encodes every
    // unmasked low-register 128-bit round with VEX, irrespective of the guest
    // encoding. Unary VAESIMC/VAESKEYGENASSIST likewise require AES+AVX.
    let needs_aes = source1.is_none() || (width == VecWidth::V128 && !high_register);
    let needs_vaes = source1.is_some() && !needs_aes;
    Some(X86JitAesMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        needs_aes,
        needs_vaes,
        needs_avx512vl: width != VecWidth::V512 && high_register,
        supports_avx_ymm16: width != VecWidth::V512 && !high_register,
    })
}

/// Exact contiguous VEX binary memory-source sequence consumed by the
/// helper-backed lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexBinaryMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) width: VecWidth,
    pub(crate) map: X86VecMap,
    pub(crate) prefix: X86SsePrefix,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) needs_avx2: bool,
    pub(crate) needs_fma: bool,
}

fn low_vex_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexBinaryKind {
    Logic,
    IntegerArithmetic,
    IntegerCompare,
    FloatingPointArithmetic,
}

fn x86_jit_vex_packed_average_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(temporary, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::AvgRnd,
        signed: false,
        set_ovf: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *lanes != width.lanes(*elem) as u8
        || !matches!(elem, VecElementType::I8 | VecElementType::I16)
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, w) =
        instruction.vex_memory_packed_average_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
    ) != (destination, source1, *elem, width)
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F,
        prefix: X86SsePrefix::OpSize,
        opcode: if *elem == VecElementType::I8 {
            0xE0
        } else {
            0xE3
        },
        w,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}

fn x86_jit_vex_packed_sign_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
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
    if !virtual_single_definition_single_use(temporary, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Sign,
        signed: true,
        set_ovf: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *lanes != width.lanes(*elem) as u8
        || !matches!(
            elem,
            VecElementType::I8 | VecElementType::I16 | VecElementType::I32
        )
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, _encoded_w) =
        instruction.vex_memory_packed_sign_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
    ) != (destination, source1, *elem, width)
    {
        return None;
    }
    let opcode = match elem {
        VecElementType::I8 => 0x08,
        VecElementType::I16 => 0x09,
        VecElementType::I32 => 0x0A,
        _ => unreachable!("filtered packed-sign element type"),
    };

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VPSIGNB/W/D are WIG. Match both guest values but emit canonical W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}

fn x86_jit_vex_pmulhrsw_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
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
    if !virtual_single_definition_single_use(temporary, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem: VecElementType::I16,
        lanes,
        signed1: true,
        signed2: true,
        shift_left: 0,
        round: true,
        sat_bits: 0,
        out_shift: 15,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary || *lanes != width.lanes(VecElementType::I16) as u8 {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_width, _encoded_w) =
        instruction.vex_memory_pmulhrsw_fields()?;
    if (encoded_destination, encoded_source1, encoded_width) != (destination, source1, width) {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode: 0x0B,
        // VPMULHRSW is WIG. Match both guest values but emit canonical W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}

pub(super) fn vex_fma3_kind(opcode: u8) -> Option<X86FmaKind> {
    match opcode & 0x0F {
        0x06 => Some(X86FmaKind::AddSub),
        0x07 => Some(X86FmaKind::SubAdd),
        0x08 | 0x09 => Some(X86FmaKind::Add),
        0x0A | 0x0B => Some(X86FmaKind::Sub),
        0x0C | 0x0D => Some(X86FmaKind::NegativeMultiplyAdd),
        0x0E | 0x0F => Some(X86FmaKind::NegativeMultiplySub),
        _ => None,
    }
}

pub(super) fn vex_fma3_order(opcode: u8) -> Option<X86FmaOrder> {
    match opcode >> 4 {
        0x09 => Some(X86FmaOrder::Order132),
        0x0A => Some(X86FmaOrder::Order213),
        0x0B => Some(X86FmaOrder::Order231),
        _ => None,
    }
}

fn x86_jit_vex_packed_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
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
    if !virtual_single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }

    let fma = block.ops.get(index + 1)?;
    let OpKind::X86Fma(fma_op) = &fma.kind else {
        return None;
    };
    let raw = fma_op.dst;
    if fma.guest_pc != load.guest_pc
        || !virtual_single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || fma_op.src3 != loaded
        || fma_op.mask.is_some()
        || fma_op.round != FpRoundMode::Dynamic
    {
        return None;
    }
    let destination = low_vex_vector_index(&fma_op.src1, width)?;
    let source1 = low_vex_vector_index(&fma_op.src2, width)?;
    let expected_elem = if fma_op.elem == VecElementType::F64 {
        VecElementType::F64
    } else if fma_op.elem == VecElementType::F32 {
        VecElementType::F32
    } else {
        return None;
    };
    if fma_op.lanes != width.lanes(expected_elem) as u8 {
        return None;
    }

    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width: hint_width,
        w,
    }) = fma.x86_hint
    else {
        return None;
    };
    if hint_width != width
        || w != (expected_elem == VecElementType::F64)
        || fma_op.kind != vex_fma3_kind(opcode)?
        || fma_op.order != vex_fma3_order(opcode)?
    {
        return None;
    }

    let result = block.ops.get(index + 2)?;
    if result.guest_pc != load.guest_pc
        || result.x86_hint.is_some()
        || !matches!(
            result.kind,
            OpKind::VMov {
                dst,
                src,
                width: result_width,
            } if low_vex_vector_index(&dst, width) == Some(destination)
                && src == raw
                && result_width == width
        )
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_opcode, encoded_width, encoded_w) =
        instruction.vex_memory_packed_fma3_fields()?;
    if encoded_destination != destination
        || encoded_source1 != source1
        || encoded_opcode != opcode
        || encoded_width != width
        || encoded_w != w
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 3,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode,
        w,
        needs_avx2: false,
        needs_fma: true,
    })
}

fn x86_jit_vex_scalar_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded_scalar, memory_size, elem) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, 4, VecElementType::F32)
        }
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, 8, VecElementType::F64)
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    };

    let broadcast = block.ops.get(index + 1)?;
    let source_vector = match &broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none()
            && *scalar == loaded_scalar
            && *broadcast_elem == elem =>
        {
            *dst
        }
        _ => return None,
    };
    if !same_pc(1)
        || !virtual_single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let fma = block.ops.get(index + 2)?;
    let OpKind::X86Fma(fma_op) = &fma.kind else {
        return None;
    };
    let raw = fma_op.dst;
    if !same_pc(2)
        || !virtual_single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || fma_op.src3 != source_vector
        || fma_op.mask.is_some()
        || fma_op.elem != elem
        || fma_op.lanes != 1
        || fma_op.round != FpRoundMode::Dynamic
    {
        return None;
    }
    let destination = low_vex_vector_index(&fma_op.src1, VecWidth::V128)?;
    let source1 = low_vex_vector_index(&fma_op.src2, VecWidth::V128)?;
    let upper_source = fma_op.src1;
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width: hint_width,
        w,
    }) = fma.x86_hint
    else {
        return None;
    };
    if !matches!(hint_width, VecWidth::V128 | VecWidth::V256)
        || w != (elem == VecElementType::F64)
        || fma_op.kind != vex_fma3_kind(opcode)?
        || fma_op.order != vex_fma3_order(opcode)?
    {
        return None;
    }

    let result_extract = block.ops.get(index + 3)?;
    let scalar_result = match &result_extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } if result_extract.x86_hint.is_none() && *vec == raw && *extract_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(3)
        || !virtual_single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let mut upper_scalars = Vec::with_capacity(xmm_lanes - 1);
    for lane in 1..xmm_lanes {
        let offset = 3 + lane;
        let extract = block.ops.get(index + offset)?;
        let upper_scalar = match &extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && *vec == upper_source
                && usize::from(*extract_lane) == lane
                && *extract_elem == elem =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !virtual_single_definition_single_use(
                upper_scalar,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        upper_scalars.push(upper_scalar);
    }

    let zero_offset = 3 + xmm_lanes;
    let zero_op = block.ops.get(index + zero_offset)?;
    let zero = match &zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => *dst,
        _ => return None,
    };
    if !same_pc(zero_offset)
        || !virtual_single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let clear_offset = zero_offset + 1;
    let clear = block.ops.get(index + clear_offset)?;
    if clear.x86_hint.is_some()
        || !matches!(
            &clear.kind,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: broadcast_elem,
                lanes: 1,
            } if low_vex_vector_index(dst, VecWidth::V128) == Some(destination)
                && *scalar == zero
                && *broadcast_elem == elem
        )
        || !same_pc(clear_offset)
    {
        return None;
    }

    let low_insert_offset = clear_offset + 1;
    let low_insert = block.ops.get(index + low_insert_offset)?;
    if low_insert.x86_hint.is_some()
        || !matches!(
            &low_insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: 0,
                elem: insert_elem,
            } if low_vex_vector_index(dst, VecWidth::V128) == Some(destination)
                && dst == vec
                && *scalar == scalar_result
                && *insert_elem == elem
        )
        || !same_pc(low_insert_offset)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = low_insert_offset + lane;
        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || !matches!(
                &insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: insert_lane,
                    elem: insert_elem,
                } if low_vex_vector_index(dst, VecWidth::V128) == Some(destination)
                    && dst == vec
                    && *scalar == upper_scalar
                    && usize::from(*insert_lane) == lane
                    && *insert_elem == elem
            )
            || !same_pc(offset)
        {
            return None;
        }
    }

    let consumed = low_insert_offset + xmm_lanes;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_opcode, encoded_w) =
        instruction.vex_memory_scalar_fma3_fields()?;
    if encoded_destination != destination
        || encoded_source1 != source1
        || encoded_opcode != opcode
        || encoded_w != w
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed,
        memory_size,
        destination,
        source1,
        width: VecWidth::V128,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode,
        w,
        needs_avx2: false,
        needs_fma: true,
    })
}

fn vex_packed_fp_binary_encoding_valid(
    kind: &OpKind,
    map: X86VecMap,
    prefix: X86SsePrefix,
    opcode: u8,
) -> bool {
    let OpKind::X86FpBinary {
        mask,
        elem,
        lanes,
        op,
        round,
        suppress_exceptions,
        ..
    } = kind
    else {
        return false;
    };
    let expected_op = match opcode {
        0x58 => X86FpBinaryOp::Add,
        0x59 => X86FpBinaryOp::Mul,
        0x5C => X86FpBinaryOp::Sub,
        0x5D => X86FpBinaryOp::Min,
        0x5E => X86FpBinaryOp::Div,
        0x5F => X86FpBinaryOp::Max,
        _ => return false,
    };
    let expected_prefix = match elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64 => X86SsePrefix::OpSize,
        _ => return false,
    };
    map == X86VecMap::Map0F
        && prefix == expected_prefix
        && *op == expected_op
        && mask.is_none()
        && *round == FpRoundMode::Dynamic
        && !*suppress_exceptions
        && matches!(
            (elem, lanes),
            (VecElementType::F32, 4 | 8) | (VecElementType::F64, 2 | 4)
        )
}

/// Validate one exact helper-backed unmasked VEX.128/VEX.256 memory-source
/// sequence. Supported packed families include logic, arithmetic, average,
/// sign, multiply, interleave, saturating pack, min/max, horizontal, compare,
/// and FMA3; scalar binary32/binary64 arithmetic and FMA3 are also accepted.
/// Most packed families are two-op `VLoad`/consumer pairs. Shared-count shifts
/// use `VLoad`/`VExtractLane`/`X86PackedShift`; packed FMA3 and scalar forms
/// validate their complete multi-op chains. Families whose IR hints do not
/// retain the encoding require exact instruction-byte provenance. Scalar
/// arithmetic requires `VEX.L=0`; scalar FMA3 accepts ignored `VEX.L`.
/// Single-definition/single-use checks prevent the fused lowerer from hiding
/// any independently observable virtual value.
///
/// The classifier is O(1); callers build the definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_binary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    if !allow_mem {
        return None;
    }
    if let Some(sequence) = x86_jit_vex_packed_fma3_memory_sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = x86_jit_vex_packed_average_memory_sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = x86_jit_vex_packed_sign_memory_sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = x86_jit_vex_pmulhrsw_memory_sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_integer_multiply_add::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_interleave::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_integer_pack::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_shared_count_shift::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_integer_minmax::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_pmul_low::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_pmul_high_word::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_widening_dword_multiply::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    if let Some(sequence) = vex_horizontal_integer::sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    ) {
        return Some(sequence);
    }
    let load = block.ops.get(index)?;
    if matches!(load.kind, OpKind::Load { .. }) {
        if let Some(sequence) = x86_jit_vex_scalar_fma3_memory_sequence(
            block,
            index,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) {
            return Some(sequence);
        }
        return x86_jit_vex_scalar_fp_binary_memory_sequence(
            block,
            index,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        );
    }
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let (destination, source1, source2, consumer_width, binary_kind) = match &consumer.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, *width, VexBinaryKind::Logic),
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } => (
            dst,
            src1,
            src2,
            super::x86_vector_width_from_lanes(*elem, *lanes)?,
            VexBinaryKind::IntegerArithmetic,
        ),
        OpKind::X86FpBinary {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } => (
            dst,
            src1,
            src2,
            super::x86_vector_width_from_lanes(*elem, *lanes)?,
            VexBinaryKind::FloatingPointArithmetic,
        ),
        OpKind::VCmp {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } => (
            dst,
            src1,
            src2,
            super::x86_vector_width_from_lanes(*elem, *lanes)?,
            VexBinaryKind::IntegerCompare,
        ),
        _ => return None,
    };
    if *source2 != temporary || consumer_width != width {
        return None;
    }
    let destination = low_vex_vector_index(destination, width)?;
    let source1 = low_vex_vector_index(source1, width)?;
    let Some(X86OpHint::VexOp {
        map,
        pp: prefix,
        opcode,
        width: hint_width,
        w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if hint_width != width {
        return None;
    }
    let needs_avx2 = match binary_kind {
        VexBinaryKind::Logic => {
            if load.x86_hint.is_some()
                || map != X86VecMap::Map0F
                || !super::x86_vector_logic_encoding_valid(&consumer.kind, prefix, opcode, false, w)
            {
                return None;
            }
            let (needs_avx, needs_avx2, needs_avx512dq, needs_avx512vl) =
                super::x86_vector_logic_feature_requirements(consumer);
            if !needs_avx || needs_avx512dq || needs_avx512vl {
                return None;
            }
            needs_avx2
        }
        VexBinaryKind::IntegerArithmetic => {
            if load.x86_hint.is_some()
                || !super::x86_vector_integer_arithmetic_map_valid(&consumer.kind, map)
                || !super::x86_vector_integer_arithmetic_encoding_valid(
                    &consumer.kind,
                    prefix,
                    opcode,
                    false,
                    w,
                )
            {
                return None;
            }
            let (needs_avx, needs_avx2, needs_avx512vl) =
                super::x86_vector_integer_arithmetic_feature_requirements(consumer);
            if !needs_avx || needs_avx512vl {
                return None;
            }
            needs_avx2
        }
        VexBinaryKind::IntegerCompare => {
            let OpKind::VCmp { elem, cond, .. } = &consumer.kind else {
                unreachable!("VEX integer-compare classifier selected VCmp")
            };
            let expected_map = match elem {
                VecElementType::I8 | VecElementType::I16 | VecElementType::I32 => X86VecMap::Map0F,
                VecElementType::I64 => X86VecMap::Map0F38,
                _ => return None,
            };
            if load.x86_hint.is_some()
                || map != expected_map
                || !super::x86_vector_integer_compare_encoding_valid(*elem, *cond, prefix, opcode)
            {
                return None;
            }
            let (needs_sse41, needs_sse42, needs_avx, needs_avx2) =
                super::x86_vector_integer_compare_feature_requirements(consumer);
            if needs_sse41 || needs_sse42 || !needs_avx {
                return None;
            }
            let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
            if !instruction.is_vex_memory_fixed_integer_compare(
                destination,
                source1,
                *elem,
                *cond,
                width,
                w,
            ) {
                return None;
            }
            needs_avx2
        }
        VexBinaryKind::FloatingPointArithmetic => {
            if load.x86_hint != Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                || !vex_packed_fp_binary_encoding_valid(&consumer.kind, map, prefix, opcode)
            {
                return None;
            }
            false
        }
    };

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map,
        prefix,
        opcode,
        w,
        needs_avx2,
        needs_fma: false,
    })
}

fn virtual_single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

fn x86_jit_vex_scalar_fp_binary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded_scalar, memory_size, elem) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if x86_jit_mem_address_shape_valid(addr) => (*dst, 4, VecElementType::F32),
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if x86_jit_mem_address_shape_valid(addr) => (*dst, 8, VecElementType::F64),
        _ => return None,
    };
    if !virtual_single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    };

    let source_vector = match &block.ops.get(index + 1)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if *scalar == loaded_scalar && *broadcast_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(1)
        || !virtual_single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let binary = block.ops.get(index + 2)?;
    let (binary_result, source1) = match &binary.kind {
        OpKind::X86FpBinary {
            dst,
            src1,
            src2,
            mask: None,
            elem: binary_elem,
            lanes: 1,
            op,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        } if *src2 == source_vector
            && *binary_elem == elem
            && matches!(
                op,
                X86FpBinaryOp::Add
                    | X86FpBinaryOp::Mul
                    | X86FpBinaryOp::Sub
                    | X86FpBinaryOp::Min
                    | X86FpBinaryOp::Div
                    | X86FpBinaryOp::Max
            ) =>
        {
            (*dst, *src1)
        }
        _ => return None,
    };
    if !same_pc(2)
        || !virtual_single_definition_single_use(binary_result, virtual_definitions, virtual_uses)
    {
        return None;
    }
    let source1_index = low_vex_vector_index(&source1, VecWidth::V128)?;
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: prefix,
        opcode,
        width: VecWidth::V128,
        w,
    }) = binary.x86_hint
    else {
        return None;
    };
    let expected_prefix = match elem {
        VecElementType::F32 => X86SsePrefix::Rep,
        VecElementType::F64 => X86SsePrefix::Repne,
        _ => unreachable!("scalar binary classifier selected F32/F64"),
    };
    let expected_op = match opcode {
        0x58 => X86FpBinaryOp::Add,
        0x59 => X86FpBinaryOp::Mul,
        0x5C => X86FpBinaryOp::Sub,
        0x5D => X86FpBinaryOp::Min,
        0x5E => X86FpBinaryOp::Div,
        0x5F => X86FpBinaryOp::Max,
        _ => return None,
    };
    let OpKind::X86FpBinary { op, .. } = &binary.kind else {
        unreachable!("validated scalar FP binary operation")
    };
    if prefix != expected_prefix || *op != expected_op {
        return None;
    }

    let scalar_result = match &block.ops.get(index + 3)?.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } if *vec == binary_result && *extract_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(3)
        || !virtual_single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let mut upper_scalars = Vec::with_capacity(xmm_lanes - 1);
    for lane in 1..xmm_lanes {
        let offset = 3 + lane;
        let upper_scalar = match &block.ops.get(index + offset)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } if *vec == source1 && usize::from(*extract_lane) == lane && *extract_elem == elem => {
                *dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !virtual_single_definition_single_use(
                upper_scalar,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        upper_scalars.push(upper_scalar);
    }

    let zero_offset = 3 + xmm_lanes;
    let zero = match &block.ops.get(index + zero_offset)?.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        _ => return None,
    };
    if !same_pc(zero_offset)
        || !virtual_single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let clear_offset = zero_offset + 1;
    let destination = match &block.ops.get(index + clear_offset)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if *scalar == zero && *broadcast_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(clear_offset) {
        return None;
    }
    let destination_index = low_vex_vector_index(&destination, VecWidth::V128)?;

    let low_insert_offset = clear_offset + 1;
    if !matches!(
        &block.ops.get(index + low_insert_offset)?.kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: 0,
            elem: insert_elem,
        } if *dst == destination
            && *vec == destination
            && *scalar == scalar_result
            && *insert_elem == elem
    ) || !same_pc(low_insert_offset)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = low_insert_offset + lane;
        if !matches!(
            &block.ops.get(index + offset)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: insert_lane,
                elem: insert_elem,
            } if *dst == destination
                && *vec == destination
                && *scalar == upper_scalar
                && usize::from(*insert_lane) == lane
                && *insert_elem == elem
        ) || !same_pc(offset)
        {
            return None;
        }
    }

    let consumed = low_insert_offset + xmm_lanes;
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_pp, encoded_opcode, encoded_w) =
        instruction.vex_scalar_memory_fp_arithmetic_fields()?;
    let encoded_prefix = match encoded_pp {
        2 => X86SsePrefix::Rep,
        3 => X86SsePrefix::Repne,
        _ => return None,
    };
    if encoded_destination != destination_index
        || encoded_source1 != source1_index
        || encoded_prefix != prefix
        || encoded_opcode != opcode
        || encoded_w != w
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed,
        memory_size,
        destination: destination_index,
        source1: source1_index,
        width: VecWidth::V128,
        map: X86VecMap::Map0F,
        prefix,
        opcode,
        w,
        needs_avx2: false,
        needs_fma: false,
    })
}
