//! Fail-closed helper-backed EVEX `VPSHUFBITQMB` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, X86Reg};
use crate::smir::ir::{
    X86EvexVpshufbitqmbMemoryEncoding, X86EvexVpshufbitqmbMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// `VPSHUFBITQMB` memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexVpshufbitqmbMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexVpshufbitqmbMemoryEncoding,
}

/// Validate the complete O0/O1/O2 decomposition emitted for one
/// `VPSHUFBITQMB` memory source.
///
/// Exact provenance binds the Full-Mem tuple, width, K destination and
/// writemask, first vector source, helper address, byte-granular masked-load
/// graph, sole `VShuffleBitQM` consumer, APX guard, and same-PC frontier.
/// Classification is O(L) time and O(1) auxiliary space for L <= 64 bytes;
/// callers build definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_vpshufbitqmb_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexVpshufbitqmbMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_vpshufbitqmb_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexVpshufbitqmbMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexVpshufbitqmbMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: VecElementType::I8,
        writemask: encoding.writemask,
        zeroing: false,
        vector_load_hint: None,
        form,
        memory_source_uses: 1,
    };
    let destination = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.destination)));
    let writemask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |op, memory_source| {
            op.x86_hint.is_none()
                && matches!(
                    op.kind,
                    OpKind::VShuffleBitQM {
                        dst,
                        src,
                        indices,
                        mask,
                        width,
                    } if dst == destination
                        && vector_index(&src, encoding.width) == Some(encoding.source1)
                        && indices == memory_source
                        && mask == writemask
                        && width == encoding.width
                )
        },
    )?;
    Some(X86JitEvexVpshufbitqmbMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
