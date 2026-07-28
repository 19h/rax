//! Fail-closed helper-backed VEX/EVEX VPCLMULQDQ memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VpclmulqdqMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VPCLMULQDQ memory-source decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVpclmulqdqMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86VpclmulqdqMemoryEncoding,
}

fn vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
        | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => Some(*index),
        _ => None,
    }
}

fn unique_virtual(reg: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(reg, VReg::Virtual(_))
        .then_some(reg)
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
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(reg, count)| virtual_definitions.get(reg) == Some(count))
        && local_uses
            .iter()
            .all(|(reg, count)| virtual_uses.get(reg) == Some(count))
}

/// Validate the complete 9-, 14-, or 24-op decomposition emitted for one
/// unmasked VEX/EVEX VPCLMULQDQ memory source. Exact instruction provenance
/// binds widths, architectural operands, immediate selectors, and the native
/// register-source rewrite. Every temporary defined inside the sequence must
/// have exactly the definitions and uses visible inside it.
///
/// The instruction-defined maximum of four independent 128-bit blocks bounds
/// classification to O(1) time and O(1) auxiliary space. Callers build the
/// global definition/use maps once in O(N) time and O(V) space for N operations
/// and V virtual registers.
pub(crate) fn x86_jit_vpclmulqdq_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVpclmulqdqMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .vpclmulqdq_memory_encoding()?;
    if encoding.width != width {
        return None;
    }

    let blocks = (width.bytes() / 16) as usize;
    let consumed = 4 + 5 * blocks;
    let sequence = block.ops.get(index..index.checked_add(consumed)?)?;
    if sequence
        .iter()
        .skip(1)
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + consumed)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let source1 = VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(encoding.source1),
        VecWidth::V256 => X86Reg::Ymm(encoding.source1),
        VecWidth::V512 => X86Reg::Zmm(encoding.source1),
        _ => unreachable!("validated VPCLMULQDQ width"),
    }));
    let destination = VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(encoding.destination),
        VecWidth::V256 => X86Reg::Ymm(encoding.destination),
        VecWidth::V512 => X86Reg::Zmm(encoding.destination),
        _ => unreachable!("validated VPCLMULQDQ width"),
    }));
    if vector_index(&source1, width) != Some(encoding.source1)
        || vector_index(&destination, width) != Some(encoding.destination)
    {
        return None;
    }

    let mut seen = HashSet::new();
    unique_virtual(loaded, &mut seen)?;
    let mut products = Vec::with_capacity(blocks);
    for block_index in 0..blocks {
        let extract_index = index + 1 + block_index * 3;
        let lhs_op = &block.ops[extract_index];
        let OpKind::VExtractLane {
            dst: lhs,
            vec: lhs_vec,
            lane: lhs_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } = lhs_op.kind
        else {
            return None;
        };
        let lhs = unique_virtual(lhs, &mut seen)?;
        if lhs_vec != source1 || lhs_lane != block_index as u8 * 2 + (encoding.immediate & 1) {
            return None;
        }

        let rhs_op = &block.ops[extract_index + 1];
        let OpKind::VExtractLane {
            dst: rhs,
            vec: rhs_vec,
            lane: rhs_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } = rhs_op.kind
        else {
            return None;
        };
        let rhs = unique_virtual(rhs, &mut seen)?;
        if rhs_vec != loaded || rhs_lane != block_index as u8 * 2 + ((encoding.immediate >> 4) & 1)
        {
            return None;
        }

        let product_op = &block.ops[extract_index + 2];
        let OpKind::ClMul {
            dst: low,
            dst_hi: Some(high),
            src1: SrcOperand::Reg(product_lhs),
            src2: SrcOperand::Reg(product_rhs),
            elem_bits: 64,
            lanes: 1,
            acc: false,
        } = product_op.kind
        else {
            return None;
        };
        let low = unique_virtual(low, &mut seen)?;
        let high = unique_virtual(high, &mut seen)?;
        if product_lhs != lhs || product_rhs != rhs {
            return None;
        }
        products.push((low, high));
    }

    let zero_index = index + 1 + blocks * 3;
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = block.ops[zero_index].kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, &mut seen)?;
    let OpKind::VBroadcast {
        dst: output,
        scalar,
        elem: VecElementType::I64,
        lanes,
    } = block.ops[zero_index + 1].kind
    else {
        return None;
    };
    let output = unique_virtual(output, &mut seen)?;
    if scalar != zero || u32::from(lanes) != width.bytes() / 8 {
        return None;
    }

    let insert_index = zero_index + 2;
    for (block_index, (low, high)) in products.into_iter().enumerate() {
        for (offset, product) in [(0u8, low), (1, high)] {
            let OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane,
                elem: VecElementType::I64,
            } = block.ops[insert_index + block_index * 2 + usize::from(offset)].kind
            else {
                return None;
            };
            if dst != output
                || vec != output
                || scalar != product
                || lane != block_index as u8 * 2 + offset
            {
                return None;
            }
        }
    }

    let OpKind::VMov {
        dst,
        src,
        width: result_width,
    } = block.ops[index + consumed - 1].kind
    else {
        return None;
    };
    if dst != destination || src != output || result_width != width {
        return None;
    }
    if !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitVpclmulqdqMemorySequence {
        consumed,
        memory_size: width.bytes(),
        encoding,
    })
}
