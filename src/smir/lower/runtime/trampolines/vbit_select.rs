//! Exact helper-backed vector-load plus VBitSelect sequence validation.

use std::collections::HashMap;

use super::{x86_jit_mem_address_shape_valid, x86_xop_memory_guards_match};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::x86_64::x86_vbit_select_reg_index;

pub(crate) fn x86_jit_mem_vbit_select_sequence_len(
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
    let (temporary, addr, load_width) = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: load_width @ (VecWidth::V128 | VecWidth::V256),
        } if matches!(
            load.x86_hint,
            Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
        ) =>
        {
            (*temporary, addr, *load_width)
        }
        _ => return None,
    };
    if !x86_jit_mem_address_shape_valid(addr)
        || !x86_xop_memory_guards_match(block, index, addr, load_width.bytes() as u8)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let valid = consumer.guest_pc == load.guest_pc
        && consumer.x86_hint.is_none()
        && match &consumer.kind {
            OpKind::VBitSelect {
                dst,
                mask,
                src_true,
                src_false,
                width,
            } => {
                *width == load_width
                    && [dst, src_true]
                        .into_iter()
                        .all(|reg| x86_vbit_select_reg_index(*reg, *width).is_some())
                    && (x86_vbit_select_reg_index(*mask, *width).is_some() || *mask == temporary)
                    && (x86_vbit_select_reg_index(*src_false, *width).is_some()
                        || *src_false == temporary)
                    && ((*mask == temporary) ^ (*src_false == temporary))
            }
            _ => false,
        };
    valid.then_some(2)
}

pub(crate) fn uses_x86_vbit_select_state_excluding(
    function: &crate::smir::ir::SmirFunction,
    excluded: &HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    function
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .any(|block| {
            if block
                .ops
                .iter()
                .any(crate::smir::lower::x86_64::x86_vbit_select_shape_valid)
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
                x86_jit_mem_vbit_select_sequence_len(block, index, true, &definitions, &uses)
                    .is_some()
            })
        })
}
