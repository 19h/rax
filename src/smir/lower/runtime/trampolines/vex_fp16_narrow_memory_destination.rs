//! Fail-closed helper-backed F16C `VCVTPS2PH` memory destinations.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};
use crate::smir::ir::{X86InstructionBytes, X86VexFp16NarrowMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact canonical one-op decomposition consumed for one F16C VEX
/// `VCVTPS2PH` memory destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexFp16NarrowMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexFp16NarrowMemoryEncoding,
}

fn source(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated F16C VCVTPS2PH source width"),
    }))
}

/// Validate the exact one-op graph for F16C VEX `VCVTPS2PH` with an
/// 8-/16-byte memory destination.
///
/// Complete source-byte provenance binds VEX map/`pp`/`W`/`L`/`vvvv`, source
/// register, immediate rounding control, and the memory width represented by
/// `lanes`. The SMIR address must be fully state-backed so the helper can
/// perform the sole guest-memory commit. Classification is O(1) time and O(1)
/// auxiliary space.
pub(crate) fn x86_jit_vex_fp16_narrow_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> Option<X86JitVexFp16NarrowMemorySequence> {
    if !allow_mem {
        return None;
    }
    let op = block.ops.get(index)?;
    if (index != 0 && block.ops[index - 1].guest_pc == op.guest_pc)
        || block
            .ops
            .get(index + 1)
            .is_some_and(|next| next.guest_pc == op.guest_pc)
    {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, op.guest_pc))?;
    let encoding = instruction.vex_fp16_narrow_memory_encoding()?;
    let expected_hint = X86OpHint::VexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x1D,
        width: encoding.source_width,
        w: false,
    };
    if !matches!(
        &op.kind,
        OpKind::X86PackedFpConvertStore {
            addr,
            src,
            mask: None,
            lanes,
            round,
        } if *src == source(encoding.source, encoding.source_width)
            && *lanes == encoding.lanes
            && *round == encoding.round
            && x86_jit_mem_address_shape_valid(addr)
    ) || op.x86_hint != Some(expected_hint)
    {
        return None;
    }

    Some(X86JitVexFp16NarrowMemorySequence {
        consumed: 1,
        encoding,
    })
}
