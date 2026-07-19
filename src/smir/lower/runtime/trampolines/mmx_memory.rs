//! Fail-closed admission for helper-backed MMX memory transfers.

use std::collections::HashMap;

use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg, VecWidth, X86Reg};

/// Exact legacy MMX MOVD/MOVQ scalar-memory encoding selected for lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MmxScalarMemoryTransferEncoding {
    pub(crate) is_load: bool,
    pub(crate) opcode: u8,
    pub(crate) mm_index: u8,
    pub(crate) mem_width: MemWidth,
    pub(crate) rex_w: bool,
}

/// Exact contiguous lifted sequence consumed by helper-backed lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MmxScalarMemoryTransferSequence {
    pub(crate) consumed: usize,
    pub(crate) memory_offset: usize,
    pub(crate) marker_offset: usize,
    pub(crate) encoding: X86MmxScalarMemoryTransferEncoding,
}

fn mm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Mm(index @ 0..=7))) => Some(index),
        _ => None,
    }
}

fn is_enter_mmx_marker(op: &SmirOp) -> bool {
    matches!(
        op.kind,
        OpKind::X86X87Control {
            kind: crate::smir::ir::ops::X86X87ControlKind::EnterMmx,
            addr: None,
        }
    ) && op.x86_hint.is_none()
}

/// Replace the lifted scalar temporary with RAX in a clone, then reuse the
/// register-register MMX validator as the semantic and encoding oracle. The
/// clone is never lowered or executed.
fn x86_mmx_scalar_memory_transfer_encoding(
    op: &SmirOp,
    temporary: VReg,
) -> Option<X86MmxScalarMemoryTransferEncoding> {
    let mut canonical = op.clone();
    let (is_load, mm, width) = match &mut canonical.kind {
        OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper: false,
        } if *src == temporary && mm_index(*dst).is_some() => {
            *src = VReg::Arch(ArchReg::X86(X86Reg::Rax));
            (true, *dst, *width)
        }
        OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper: false,
        } if *dst == temporary && mm_index(*src).is_some() => {
            *dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
            (false, *src, *width)
        }
        _ => return None,
    };
    if !super::is_x86_native_mmx_op(&canonical) {
        return None;
    }
    let opcode = match canonical.x86_hint {
        Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        }) if opcode == if is_load { 0x6E } else { 0x7E } => opcode,
        _ => return None,
    };
    let (mem_width, rex_w) = match width {
        OpWidth::W32 => (MemWidth::B4, false),
        OpWidth::W64 => (MemWidth::B8, true),
        _ => return None,
    };
    Some(X86MmxScalarMemoryTransferEncoding {
        is_load,
        opcode,
        mm_index: mm_index(mm)?,
        mem_width,
        rex_w,
    })
}

/// Admit only the legacy `0F 6F /r` and `0F 7F /r` MMX MOVQ memory forms.
/// Virtual V64 temporaries remain ineligible because they have no stable state
/// slot across the Rust MMU helper boundary.
pub fn x86_jit_mmx_mem_shape_valid(op: &SmirOp) -> bool {
    let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
    match (&op.kind, op.x86_hint) {
        (
            OpKind::VLoad {
                dst,
                addr,
                width: VecWidth::V64,
            },
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x6F,
            }),
        ) => mm(dst) && super::x86_jit_mem_address_shape_valid(addr),
        (
            OpKind::VStore {
                src,
                addr,
                width: VecWidth::V64,
            },
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x7F,
            }),
        ) => mm(src) && super::x86_jit_mem_address_shape_valid(addr),
        _ => false,
    }
}

/// Validate exact helper-backed `MOVD/MOVQ mm, m32/m64` and
/// `MOVD/MOVQ m32/m64, mm` lifted chains. The memory access must retain the
/// architectural width and fault before the MMX-state marker is committed.
pub(crate) fn x86_jit_mmx_scalar_memory_transfer_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86MmxScalarMemoryTransferSequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;

    if let OpKind::Load {
        dst: temporary @ VReg::Virtual(_),
        addr,
        width,
        sign: SignExtend::Zero,
    } = &first.kind
    {
        if first.x86_hint.is_some()
            || !super::x86_jit_mem_address_shape_valid(addr)
            || virtual_definitions.get(temporary) != Some(&1)
            || virtual_uses.get(temporary) != Some(&1)
        {
            return None;
        }
        let second = block.ops.get(index + 1)?;
        let third = block.ops.get(index + 2)?;
        if second.guest_pc != first.guest_pc || third.guest_pc != first.guest_pc {
            return None;
        }
        let (marker_offset, operation) = if is_enter_mmx_marker(second) {
            (1, third)
        } else if is_enter_mmx_marker(third) {
            (2, second)
        } else {
            return None;
        };
        let encoding = x86_mmx_scalar_memory_transfer_encoding(operation, *temporary)?;
        if !encoding.is_load || encoding.mem_width != *width {
            return None;
        }
        return Some(X86MmxScalarMemoryTransferSequence {
            consumed: 3,
            memory_offset: 0,
            marker_offset,
            encoding,
        });
    }

    let temporary = match &first.kind {
        OpKind::X86MovdQ {
            dst: temporary @ VReg::Virtual(_),
            ..
        } => *temporary,
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }
    let encoding = x86_mmx_scalar_memory_transfer_encoding(first, temporary)?;
    if encoding.is_load {
        return None;
    }
    let store = block.ops.get(index + 1)?;
    let marker = block.ops.get(index + 2)?;
    let addr = match &store.kind {
        OpKind::Store { src, addr, width }
            if *src == temporary && *width == encoding.mem_width && store.x86_hint.is_none() =>
        {
            addr
        }
        _ => return None,
    };
    if store.guest_pc != first.guest_pc
        || marker.guest_pc != first.guest_pc
        || !super::x86_jit_mem_address_shape_valid(addr)
        || !is_enter_mmx_marker(marker)
    {
        return None;
    }
    Some(X86MmxScalarMemoryTransferSequence {
        consumed: 3,
        memory_offset: 1,
        marker_offset: 2,
        encoding,
    })
}

pub(crate) fn x86_jit_mmx_scalar_memory_transfer_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_mmx_scalar_memory_transfer_sequence(
        block,
        index,
        allow_mem,
        virtual_definitions,
        virtual_uses,
    )
    .map(|sequence| sequence.consumed)
}
