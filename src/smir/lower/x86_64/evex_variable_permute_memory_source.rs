//! Helper-backed EVEX variable VPERMILPS/PD memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_variable_permute_memory_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX variable-permute width"),
        }
    }

    fn emit_evex_variable_permute_control_broadcast(
        &mut self,
        scratch: u8,
        elem: VecElementType,
        width: VecWidth,
    ) {
        self.emit_vec_rr(
            VecEncoding {
                kind: VecEncodingKind::Evex,
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: match elem {
                    VecElementType::F32 => 0x58,
                    VecElementType::F64 => 0x59,
                    _ => unreachable!("validated VPERMIL control element"),
                },
                width,
                w: elem == VecElementType::F64,
            },
            Self::evex_variable_permute_memory_phys_reg(scratch, width),
            PhysReg::Xmm(scratch),
            0,
        );
    }

    /// Fuse one exact EVEX variable VPERMILPS/PD memory decomposition.
    ///
    /// The class-E4NF memory read is unconditional and commits only the
    /// nonarchitectural helper slot. A borrowed ZMM register imports the full
    /// control vector; EVEX.b forms first replicate the imported low dword or
    /// qword with VPBROADCASTD/Q. The byte-validated register rewrite then
    /// performs the original merge/zero operation, after which the borrowed
    /// architectural register is restored completely.
    pub(crate) fn try_lower_jit_evex_variable_permute_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_variable_permute_memory_sequence(
                block,
                index,
                true,
                &self.x86_instruction_bytes,
                virtual_definitions,
                virtual_uses,
            )
        else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX variable VPERMIL memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }

        let address = match (&block.ops[index].kind, sequence.encoding.broadcast) {
            (OpKind::VLoad { addr, .. }, false) | (OpKind::Load { addr, .. }, true) => addr,
            _ => unreachable!("validated EVEX variable-permute memory operation"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            sequence.encoding.memory_size,
            true,
            true,
        )?;

        let encoding = sequence.encoding;
        let scratch = Self::evex_variable_permute_memory_phys_reg(encoding.scratch, encoding.width);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, encoding.width);
        if encoding.broadcast {
            self.emit_evex_variable_permute_control_broadcast(
                encoding.scratch,
                encoding.elem,
                encoding.width,
            );
        }
        self.code
            .emit_bytes(encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
