//! Fail-closed helper-backed VEX `VMPSADBW` memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VMPSADBW` memory-source decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexMpsadbwMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) width: VecWidth,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
}

fn low_vex_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

/// Validate the complete two-op `VLoad`/`VMpsadbw` decomposition for one VEX
/// memory source. Source-byte provenance binds both architectural inputs, the
/// destination, vector width, imm8, and ignored W bit. The load may retain its
/// original unaligned hint or an aligned hint established by alignment
/// inference. Its virtual destination must have exactly one definition and one
/// use, and the two operations must comprise the complete guest instruction.
///
/// Classification is O(1). Callers build definition/use maps once in O(N) time
/// and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_mpsadbw_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexMpsadbwMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(
                load.x86_hint,
                Some(X86OpHint::VecAlign(
                    X86VecAlign::Unaligned | X86VecAlign::Aligned
                ))
            ) && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if (index != 0 && block.ops[index - 1].guest_pc == load.guest_pc)
        || consumer.guest_pc != load.guest_pc
        || consumer.x86_hint.is_some()
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::VMpsadbw {
        dst,
        src1,
        src2,
        mask: None,
        width: consumer_width,
        imm,
        zeroing: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != loaded || *consumer_width != width {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_width, immediate, w) =
        instruction.vex_memory_mpsadbw_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_width,
        immediate,
    ) != (destination, source1, width, *imm)
    {
        return None;
    }

    Some(X86JitVexMpsadbwMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        immediate,
        w,
    })
}
