//! Exact helper-backed VPCOM vector-load plus comparison validation.

use std::collections::HashMap;

use super::{x86_jit_mem_address_shape_valid, x86_xop_memory_guards_match};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::x86_64::{
    x86_state_vcmp_element_width, x86_state_vcmp_reg_index, x86_state_vcmp_shape_valid,
};

pub(crate) fn x86_jit_mem_vpcom_sequence_len(
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
        || !x86_xop_memory_guards_match(block, index, addr, 16)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let valid = consumer.guest_pc == load.guest_pc
        && matches!(consumer.x86_hint, Some(X86OpHint::XopVpcom))
        && match consumer.kind {
            OpKind::VCmp {
                dst,
                src1,
                src2,
                elem,
                lanes,
                ..
            } => {
                src2 == temporary
                    && x86_state_vcmp_reg_index(dst).is_some()
                    && x86_state_vcmp_reg_index(src1).is_some()
                    && x86_state_vcmp_element_width(elem, lanes).is_some()
            }
            _ => false,
        };
    valid.then_some(2)
}

pub(crate) fn uses_x86_state_vcmp_excluding(
    function: &crate::smir::ir::SmirFunction,
    excluded: &HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    function
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .any(|block| {
            if block.ops.iter().any(x86_state_vcmp_shape_valid) {
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
                x86_jit_mem_vpcom_sequence_len(block, index, true, &definitions, &uses).is_some()
            })
        })
}
