//! Fail-closed helper-backed EVEX `VCVTPS2PH` memory destinations.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};
use crate::smir::ir::{SmirBlock, X86EvexFp16NarrowMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact canonical one-op decomposition consumed for one EVEX `VCVTPS2PH`
/// memory destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFp16NarrowMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86EvexFp16NarrowMemoryEncoding,
}

fn source(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX VCVTPS2PH source width"),
    }))
}

/// Validate the exact one-op graph for EVEX `VCVTPS2PH` with an 8-/16-/32-byte
/// memory destination.
///
/// Complete byte provenance binds map/`pp`/`W`/`L'L`/`vvvv`, source, opmask,
/// immediate rounding control, APX address state, and the E11 destination
/// extent represented by `lanes`. The state-backed address is retained for the
/// helper's sole guest-memory commit. Classification is O(1) time and O(1)
/// auxiliary space.
pub(crate) fn x86_jit_evex_fp16_narrow_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> Option<X86JitEvexFp16NarrowMemorySequence> {
    if !allow_mem {
        return None;
    }
    let op = block.ops.get(index)?;
    let guest_pc = op.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc)
        || !no_following_same_pc(block, index, 1, guest_pc)
    {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_fp16_narrow_memory_encoding()?;
    let expected_hint = X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x1D,
        width: encoding.source_width,
        w: false,
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let address = match &op.kind {
        OpKind::X86PackedFpConvertStore {
            addr,
            src,
            mask,
            lanes,
            round,
        } if *src == source(encoding.source, encoding.source_width)
            && *mask == expected_mask
            && *lanes == encoding.lanes
            && *round == encoding.round
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            addr
        }
        _ => return None,
    };
    if op.x86_hint != Some(expected_hint)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }

    Some(X86JitEvexFp16NarrowMemorySequence {
        consumed: 1,
        encoding,
    })
}
