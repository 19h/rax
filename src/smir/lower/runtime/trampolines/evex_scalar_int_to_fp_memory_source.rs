//! Fail-closed helper-backed EVEX scalar integer-to-FP memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexScalarIntToFpMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_virtual_definition_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact two-op EVEX scalar integer-to-FP memory decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarIntToFpMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86EvexScalarIntToFpMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn expected_hint(encoding: X86EvexScalarIntToFpMemoryEncoding) -> Option<X86OpHint> {
    if encoding.elem == VecElementType::F16 {
        return None;
    }
    let pp = match encoding.pp {
        2 => X86SsePrefix::Rep,
        3 => X86SsePrefix::Repne,
        _ => return None,
    };
    let width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp,
        opcode: encoding.opcode,
        width,
        w: encoding.w,
    })
}

fn exact_conversion(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexScalarIntToFpMemoryEncoding,
) -> bool {
    matches!(
        op.kind,
        OpKind::X86IntToFp {
            dst,
            merge,
            src,
            elem,
            int_width,
            signed,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        } if op.x86_hint == expected_hint(encoding)
            && dst == xmm(encoding.destination)
            && merge == xmm(encoding.merge)
            && src == loaded
            && elem == encoding.elem
            && int_width == encoding.int_width
            && signed == encoding.signed
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// `VCVT{,U}SI2{SS,SD,SH}` scalar memory source.
///
/// Exact byte provenance binds format, signedness, W-selected source width,
/// destination and merge registers, LLIG image, dynamic rounding, exception
/// policy, APX address guard, and guest-PC frontier. Matching is O(1) time and
/// space; callers construct definition/use maps once in O(N) time and O(V)
/// space for N operations and V virtual registers.
pub(crate) fn x86_jit_evex_scalar_int_to_fp_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarIntToFpMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if !exact_evex_memory_sequence_frontier(block, index, load.guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .evex_scalar_int_to_fp_memory_encoding()?;
    let expected_sign = if encoding.signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let (loaded, address) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign,
        } if load.x86_hint.is_none()
            && *width == encoding.memory_width
            && *sign == expected_sign
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, addr)
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let conversion = block.ops.get(index + 1)?;
    if conversion.guest_pc != load.guest_pc
        || !exact_conversion(conversion, loaded, encoding)
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, load.guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexScalarIntToFpMemorySequence {
        consumed: 2,
        encoding,
    })
}
