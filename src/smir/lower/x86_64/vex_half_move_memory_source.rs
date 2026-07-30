//! Helper-backed VEX high/low 64-bit lane memory lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact VEX.128 `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD`
    /// memory-source decomposition.
    ///
    /// The precise MMU helper commits only the nonarchitectural vector
    /// transfer slot. A borrowed XMM register carries its low qword. High-lane
    /// loads map directly to `VMOVLHPS`; low-lane loads first duplicate the
    /// helper qword into the scratch high lane and then use `VMOVHLPS`. These
    /// AVX bit transfers are independent of the packed-single/packed-double
    /// mnemonic and WIG bit. The borrowed register is restored completely.
    pub(crate) fn try_lower_jit_vex_half_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_half_move_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index + 1].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated VEX half-move sequence contains an 8-byte load"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            8,
            true,
            true,
        )?;

        let fields = sequence.encoding;
        let scratch_index = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two VEX operands leave at least fourteen scratch registers");
        let scratch = PhysReg::Xmm(scratch_index);
        let destination = PhysReg::Xmm(fields.destination);
        let source1 = PhysReg::Xmm(fields.source1);
        let encoding = |opcode| VecEncoding {
            kind: VecEncodingKind::Vex,
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode,
            width: VecWidth::V128,
            w: false,
        };

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);
        if fields.memory_lane == 0 {
            // VMOVLHPS scratch,scratch,scratch duplicates scratch[63:0] into
            // scratch[127:64], which VMOVHLPS then selects into dst[63:0].
            self.emit_vec_rrr(encoding(0x16), scratch, scratch, scratch);
            self.emit_vec_rrr(encoding(0x12), destination, source1, scratch);
        } else {
            // VMOVLHPS dst,source1,scratch copies scratch[63:0] to dst[127:64].
            self.emit_vec_rrr(encoding(0x16), destination, source1, scratch);
        }
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(fields.destination);
        }
        Ok(Some(sequence.consumed))
    }

    /// Fuse one exact VEX.128 `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD`
    /// memory-destination decomposition.
    ///
    /// A native VEX lane store first copies the selected source qword into the
    /// nonarchitectural transfer slot at a trusted host address. The precise
    /// MMU helper then performs exactly one 8-byte guest-memory write or
    /// deoptimizes at the instruction PC without modifying architectural
    /// vector state.
    pub(crate) fn try_lower_jit_vex_half_move_memory_store(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_half_move_store_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index + 1].kind {
            OpKind::Store { addr, .. } => addr,
            _ => unreachable!("validated VEX half-move sequence contains an 8-byte store"),
        };
        let fields = sequence.encoding;

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_qword_store(PhysReg::Xmm(fields.source), fields.memory_lane);
        self.code.emit_u8(0x58); // pop guest RAX

        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            false,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            8,
            false,
            true,
        )?;
        Ok(Some(sequence.consumed))
    }
}
