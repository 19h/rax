//! Helper-backed VPCOM vector-load plus state-backed comparison lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, x86_state_vcmp_reg_index};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET, X86_JIT_VECTOR_SCRATCH_INDEX,
};

impl X86_64Lowerer {
    pub(crate) fn try_lower_jit_vpcom_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_vpcom_sequence_len(
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
                op: "VPCOM memory source in a physical-vector region requires vector-preserving MMU helpers"
                    .to_string(),
            });
        }
        let (address, width) = match &block.ops[index].kind {
            OpKind::VLoad { addr, width, .. } => (addr, *width),
            _ => unreachable!("validated VPCOM memory sequence starts with VLoad"),
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

        let OpKind::VCmp {
            dst,
            src1,
            cond,
            elem,
            lanes,
            ..
        } = block.ops[index + 1].kind
        else {
            unreachable!("validated VPCOM memory sequence consumer")
        };
        let dst_index = x86_state_vcmp_reg_index(dst).unwrap();
        let src1_index = x86_state_vcmp_reg_index(src1).unwrap();
        self.emit_x86_state_vcmp(
            dst_index,
            X86_GUEST_ZMM_OFFSET + i32::from(src1_index) * 64,
            X86_GUEST_VECTOR_SCRATCH_OFFSET,
            elem,
            lanes,
            cond,
            &[src1_index],
        )?;
        Ok(Some(consumed))
    }
}
