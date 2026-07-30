//! Fail-closed helper-backed VEX floating-point flag-compare memory admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VCOMISS`/`VUCOMISS`/`VCOMISD`/`VUCOMISD`
/// memory-source decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexFpFlagCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) source1: u8,
    pub(crate) elem: VecElementType,
    pub(crate) signaling: bool,
    pub(crate) w: bool,
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

/// Validate one complete AVX scalar floating-point flag-comparison memory
/// decomposition.
///
/// Source-byte provenance binds the architectural source register, element
/// type, COMI/UCOMI exception policy, WIG encoding, reserved VEX.vvvv, and exact
/// 4- or 8-byte memory footprint. A generation-dependent VEX.L=1 source is
/// accepted only after exact validation and canonicalization to the
/// deterministic VEX.L=0 form. Both loaded virtuals must be closed
/// single-definition/single-use values, and no same-PC tail may remain
/// unconsumed.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_fp_flag_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexFpFlagCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
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

    let broadcast = block.ops.get(index + 1)?;
    let source2 = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if broadcast.guest_pc == load.guest_pc
            && broadcast.x86_hint.is_none()
            && scalar == loaded_scalar
            && broadcast_elem == elem =>
        {
            dst
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(source2, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(index + 2)?;
    if consumer.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86FpCompare {
        src1,
        src2,
        elem: consumer_elem,
        signaling,
        suppress_exceptions: false,
    } = consumer.kind
    else {
        return None;
    };
    let source1 = match src1 {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => index,
        _ => return None,
    };
    if src2 != source2 || consumer_elem != elem {
        return None;
    }

    let source_instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let instruction = source_instruction
        .vex_scalar_l1_canonical_l0()
        .unwrap_or(*source_instruction);
    let source_width = if instruction == *source_instruction {
        VecWidth::V128
    } else {
        VecWidth::V256
    };
    let (encoded_source1, encoded_elem, encoded_signaling, encoded_size, w) =
        instruction.vex_memory_fp_flag_compare_fields()?;
    if (
        encoded_source1,
        encoded_elem,
        encoded_signaling,
        encoded_size,
    ) != (source1, elem, signaling, memory_size)
    {
        return None;
    }
    let expected_prefix = if elem == VecElementType::F32 {
        X86SsePrefix::None
    } else {
        X86SsePrefix::OpSize
    };
    let opcode = if signaling { 0x2F } else { 0x2E };
    if consumer.x86_hint
        != Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: expected_prefix,
            opcode,
            width: source_width,
            w,
        })
    {
        return None;
    }

    Some(X86JitVexFpFlagCompareMemorySequence {
        consumed: 3,
        memory_size,
        source1,
        elem,
        signaling,
        w,
    })
}
