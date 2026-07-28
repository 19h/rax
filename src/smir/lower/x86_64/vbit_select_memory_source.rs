//! Helper-backed vector-load plus VBitSelect lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, x86_vbit_select_reg_index};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET, X86_JIT_VECTOR_SCRATCH_INDEX,
};

impl X86_64Lowerer {
    pub(crate) fn try_lower_jit_vbit_select_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_vbit_select_sequence_len(
            block,
            index,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        if self.native_vector_state_active && !self.preserve_vector_mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "VBitSelect memory source in a physical-vector region requires vector-preserving MMU helpers"
                    .to_string(),
            });
        }
        let (temporary, address, width) = match &block.ops[index].kind {
            OpKind::VLoad {
                dst, addr, width, ..
            } => (*dst, addr, *width),
            _ => unreachable!("validated VBitSelect memory sequence starts with VLoad"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            width.bytes(),
            true,
            self.preserve_vector_mem_helpers,
        )?;

        let OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            ..
        } = &block.ops[index + 1].kind
        else {
            unreachable!("validated VBitSelect memory sequence consumer")
        };
        let slot = |reg: VReg| {
            X86_GUEST_ZMM_OFFSET + i32::from(x86_vbit_select_reg_index(reg, width).unwrap()) * 64
        };
        let operand_offset = |reg: VReg| {
            if reg == temporary {
                X86_GUEST_VECTOR_SCRATCH_OFFSET
            } else {
                slot(reg)
            }
        };
        let physical_inputs: Vec<u8> = [*mask, *src_true, *src_false]
            .into_iter()
            .filter(|reg| *reg != temporary)
            .map(|reg| x86_vbit_select_reg_index(reg, width).unwrap())
            .collect();
        self.emit_x86_vbit_select_state(
            x86_vbit_select_reg_index(*dst, width).unwrap(),
            operand_offset(*mask),
            slot(*src_true),
            operand_offset(*src_false),
            width,
            &physical_inputs,
        )?;
        Ok(Some(consumed))
    }
}
