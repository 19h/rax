//! Helper-backed AMD XOP packed-bit memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86XopStateCount, x86_low_xmm_index};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SrcOperand, VReg};
use crate::smir::lower::{
    LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET, X86_JIT_VECTOR_SCRATCH_INDEX,
};

impl X86_64Lowerer {
    pub(crate) fn try_lower_jit_xop_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_xop_source_sequence_len(
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
                op: "XOP memory source in a physical-vector region requires vector-preserving MMU helpers"
                    .to_string(),
            });
        }
        let (temporary, address) = match &block.ops[index].kind {
            OpKind::VLoad { dst, addr, .. } => (*dst, addr),
            _ => unreachable!("validated XOP memory sequence starts with VLoad"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            16,
            true,
            self.preserve_vector_mem_helpers,
        )?;

        let OpKind::X86XopPackedBit {
            dst,
            src,
            count,
            elem,
            kind,
        } = &block.ops[index + 1].kind
        else {
            unreachable!("validated XOP memory sequence consumer")
        };
        let source_offset = if *src == temporary {
            X86_GUEST_VECTOR_SCRATCH_OFFSET
        } else {
            X86_GUEST_ZMM_OFFSET + i32::from(x86_low_xmm_index(*src).unwrap()) * 64
        };
        let count = match count {
            SrcOperand::Reg(reg) if *reg == temporary => {
                X86XopStateCount::Memory(X86_GUEST_VECTOR_SCRATCH_OFFSET)
            }
            SrcOperand::Reg(reg) => X86XopStateCount::Memory(
                X86_GUEST_ZMM_OFFSET + i32::from(x86_low_xmm_index(*reg).unwrap()) * 64,
            ),
            SrcOperand::Imm(value) => X86XopStateCount::Immediate(*value as u8),
            _ => unreachable!("validated XOP memory count"),
        };
        self.emit_x86_xop_packed_bit_state(
            x86_low_xmm_index(*dst).unwrap(),
            source_offset,
            count,
            *elem,
            *kind,
        )?;
        Ok(Some(consumed))
    }
}
