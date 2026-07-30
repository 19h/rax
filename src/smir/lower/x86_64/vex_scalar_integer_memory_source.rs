//! Helper-backed VEX `VMOVD`/`VMOVQ` scalar-integer memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86VexScalarIntegerMemoryKind;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact VEX.128 `VMOVD` or `VMOVQ` memory decomposition.
    ///
    /// A precise MMU helper transfers exactly 4 or 8 guest bytes through the
    /// nonarchitectural vector scratch. A canonical native VEX VMOVD/VMOVQ
    /// then moves the scalar between that trusted slot and the live
    /// architectural XMM register. Loads zero every destination bit above the
    /// scalar; stores preserve the entire architectural vector file.
    pub(crate) fn try_lower_jit_vex_scalar_integer_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_vex_scalar_integer_memory_sequence(
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
        let encoding = sequence.encoding;
        let address = match encoding.kind {
            X86VexScalarIntegerMemoryKind::Load => match &block.ops[index].kind {
                OpKind::Load { addr, .. } => addr,
                _ => unreachable!("validated VEX scalar-integer load starts with Load"),
            },
            X86VexScalarIntegerMemoryKind::Store => match &block.ops[index + 1].kind {
                OpKind::Store { addr, .. } => addr,
                _ => unreachable!("validated VEX scalar-integer store ends with Store"),
            },
        };
        self.emit_jit_vector_scratch_scalar_memory_transfer(
            block.ops[index].guest_pc,
            encoding.kind == X86VexScalarIntegerMemoryKind::Load,
            encoding.vector,
            address,
            encoding.memory_width,
        )?;
        Ok(Some(sequence.consumed))
    }

    /// Dispatch all exact helper-backed VEX scalar-move memory families.
    pub(crate) fn try_lower_jit_vex_scalar_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        if let Some(consumed) = self.try_lower_jit_vex_half_move_memory_source(
            block,
            index,
            virtual_definitions,
            virtual_uses,
        )? {
            return Ok(Some(consumed));
        }
        if let Some(consumed) = self.try_lower_jit_vex_half_move_memory_store(
            block,
            index,
            virtual_definitions,
            virtual_uses,
        )? {
            return Ok(Some(consumed));
        }
        if let Some(consumed) = self.try_lower_jit_vex_scalar_fp_memory_source(
            block,
            index,
            virtual_definitions,
            virtual_uses,
        )? {
            return Ok(Some(consumed));
        }
        self.try_lower_jit_vex_scalar_integer_memory_source(
            block,
            index,
            virtual_definitions,
            virtual_uses,
        )
    }
}
