//! Fail-closed helper-backed VEX variable-blend memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecCmpCond, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexVariableBlendMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous five-op decomposition consumed for one helper-backed VEX
/// variable-blend memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexVariableBlendMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexVariableBlendMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX variable-blend width"),
    }))
}

fn unique_virtual(register: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(register, VReg::Virtual(_))
        .then_some(register)
        .filter(|candidate| seen.insert(*candidate))
}

fn local_virtual_counts_match(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_insert(0usize) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(register, count)| virtual_definitions.get(register) == Some(count))
        && local_uses
            .iter()
            .all(|(register, count)| virtual_uses.get(register) == Some(count))
}

/// Validate the complete five-op decomposition emitted for one `VBLENDVPS`,
/// `VBLENDVPD`, or `VPBLENDVB` memory source.
///
/// Source-byte provenance binds W/L, destination, both data sources, explicit
/// mask, element width, lane count, and memory width to the graph. Each of the
/// four virtual values is distinct and has every definition and use inside the
/// sequence. Runtime is O(1); callers construct definition/use maps once in
/// O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_variable_blend_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexVariableBlendMemorySequence> {
    if !allow_mem {
        return None;
    }
    let sequence = block.ops.get(index..index.checked_add(5)?)?;
    let load = &sequence[0];
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(
                load.x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            ) && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if sequence
        .iter()
        .skip(1)
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + sequence.len())
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_variable_blend_memory_encoding()?;
    if encoding.width != width {
        return None;
    }

    let mut seen = HashSet::new();
    let loaded = unique_virtual(loaded, &mut seen)?;
    let zero = match sequence[1].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => unique_virtual(dst, &mut seen)?,
        _ => return None,
    };
    let zero_vector = match sequence[2].kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if scalar == zero
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            unique_virtual(dst, &mut seen)?
        }
        _ => return None,
    };
    let selection_mask = match sequence[3].kind {
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond: VecCmpCond::Lt,
            elem,
            lanes,
        } if src1 == vector_reg(encoding.mask, encoding.width)
            && src2 == zero_vector
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            unique_virtual(dst, &mut seen)?
        }
        _ => return None,
    };
    if !matches!(
        sequence[4].kind,
        OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            width: selected_width,
        } if dst == vector_reg(encoding.destination, encoding.width)
            && mask == selection_mask
            && src_true == loaded
            && src_false == vector_reg(encoding.source1, encoding.width)
            && selected_width == encoding.width
    ) || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexVariableBlendMemorySequence {
        consumed: sequence.len(),
        encoding,
    })
}
