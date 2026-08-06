//! Fail-closed helper-backed EVEX floating-point flag-compare admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexFpFlagCompareMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
    single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX scalar floating-point flag-comparison memory
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFpFlagCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexFpFlagCompareMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn expected_hint(encoding: X86EvexFpFlagCompareMemoryEncoding) -> Option<X86OpHint> {
    let width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    let (pp, w) = match encoding.elem {
        VecElementType::F16 => return None,
        VecElementType::F32 => (X86SsePrefix::None, false),
        VecElementType::F64 => (X86SsePrefix::OpSize, true),
        _ => return None,
    };
    Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp,
        opcode: if encoding.signaling { 0x2F } else { 0x2E },
        width,
        w,
    })
}

/// Validate the exact O0/O1/O2 three-op decomposition for one EVEX
/// `VCOMISS`/`VCOMISD`/`VCOMISH` or `VUCOMISS`/`VUCOMISD`/`VUCOMISH` memory
/// source.
///
/// Complete source-byte provenance binds precision, source register,
/// COMI/UCOMI invalid-exception policy, LLIG image, reserved controls, and the
/// unconditional Type-E3NF 2/4/8-byte access. The address must agree exactly
/// with any APX guard, both internal virtuals are globally single-definition
/// and single-use, and no same-PC operation may remain outside the sequence.
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_evex_fp_flag_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpFlagCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_fp_flag_compare_memory_encoding()?;
    let (loaded_scalar, address) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, addr)
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let memory_source = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: 1,
        } if broadcast.guest_pc == guest_pc
            && broadcast.x86_hint.is_none()
            && scalar == loaded_scalar
            && elem == encoding.elem =>
        {
            dst
        }
        _ => return None,
    };
    if !single_definition_single_use(memory_source, virtual_definitions, virtual_uses) {
        return None;
    }

    let compare = block.ops.get(index + 2)?;
    if compare.guest_pc != guest_pc
        || compare.x86_hint != expected_hint(encoding)
        || !matches!(
            compare.kind,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
                suppress_exceptions: false,
            } if src1 == xmm(encoding.source1)
                && src2 == memory_source
                && elem == encoding.elem
                && signaling == encoding.signaling
        )
    {
        return None;
    }

    let consumed = 3;
    if !no_following_same_pc(block, index, consumed, guest_pc) {
        return None;
    }
    Some(X86JitEvexFpFlagCompareMemorySequence {
        consumed,
        address_offset: 0,
        encoding,
    })
}
