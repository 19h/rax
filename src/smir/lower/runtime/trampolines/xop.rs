//! Exact AMD XOP packed-bit memory-source sequence validation.

use std::collections::HashMap;

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, SrcOperand, VReg, VecElementType, VecWidth, X86Reg};

pub(crate) fn x86_jit_mem_xop_source_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr) = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: VecWidth::V128,
        } if matches!(
            load.x86_hint,
            Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
        ) =>
        {
            (*temporary, addr)
        }
        _ => return None,
    };
    if !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let low_xmm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))));
    let valid = consumer.guest_pc == load.guest_pc
        && consumer.x86_hint.is_none()
        && match &consumer.kind {
            OpKind::X86XopPackedBit {
                dst,
                src,
                count,
                elem:
                    VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64,
                ..
            } => {
                low_xmm(dst)
                    && (low_xmm(src) || *src == temporary)
                    && match count {
                        SrcOperand::Reg(reg) => low_xmm(reg) || *reg == temporary,
                        SrcOperand::Imm(value) => (0..=255).contains(value),
                        _ => false,
                    }
                    && ((*src == temporary)
                        ^ matches!(count, SrcOperand::Reg(reg) if *reg == temporary))
            }
            _ => false,
        };
    valid.then_some(2)
}

/// Whether an executable region reads or writes XMM slots through the
/// state-backed XOP lowerer, including an exact helper-backed memory pair.
pub(crate) fn uses_x86_xop_state_excluding(
    function: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    function
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .any(|block| {
            if block
                .ops
                .iter()
                .any(crate::smir::lower::x86_64::x86_xop_packed_bit_shape_valid)
            {
                return true;
            }
            let mut definitions = HashMap::new();
            let mut uses = HashMap::new();
            for op in &block.ops {
                for reg in op.kind.dests() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *definitions.entry(reg).or_insert(0usize) += 1;
                    }
                }
                for reg in op.kind.source_vregs() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *uses.entry(reg).or_insert(0usize) += 1;
                    }
                }
            }
            (0..block.ops.len()).any(|index| {
                x86_jit_mem_xop_source_sequence_len(block, index, true, &definitions, &uses)
                    .is_some()
            })
        })
}
