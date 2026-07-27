//! Helper-backed VEX packed-binary memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86VecMap};
use crate::smir::ir::types::{DispSize, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET, X86_JIT_VECTOR_SCRATCH_INDEX,
};

impl X86_64Lowerer {
    fn vex_binary_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX binary width"),
        }
    }

    fn emit_vex_scratch_load(&mut self, register: PhysReg, width: VecWidth, offset: i32) {
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_vex_prefix(
            X86VecMap::Map0F,
            crate::smir::ir::ops::X86SsePrefix::Rep,
            width,
            false,
            register.vec_ext(),
            0,
            PhysReg::Rax.vec_ext(),
            0,
        );
        emitter.code.emit_u8(0x6F); // VMOVDQU xmm/ymm, m128/m256
        emitter.emit_modrm_mem_disp(register, PhysReg::Rax, offset, DispSize::Disp32);
    }

    /// Restore the complete architectural register borrowed as the transfer
    /// carrier. The AVX-only bridge owns only YMM0-YMM15; the general bridge
    /// owns complete ZMM state and therefore requires a 512-bit restore.
    fn emit_vex_binary_scratch_restore(&mut self, index: u8) {
        if self.avx_ymm16_vector_state {
            self.emit_vex_scratch_load(
                PhysReg::Ymm(index),
                VecWidth::V256,
                X86_GUEST_ZMM_OFFSET + i32::from(index) * 64,
            );
        } else {
            let register = PhysReg::Zmm(index);
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_evex_prefix(
                X86VecMap::Map0F,
                crate::smir::ir::ops::X86SsePrefix::Rep,
                VecWidth::V512,
                true,
                register.vec_ext(),
                0,
                PhysReg::Rax.vec_ext(),
                register.vec_ext2(),
                0,
                PhysReg::Rax.vec_ext2(),
                0,
            );
            emitter.code.emit_u8(0x6F); // VMOVDQU64 zmm, m512
            emitter.emit_modrm_mem_disp(
                register,
                PhysReg::Rax,
                X86_GUEST_ZMM_OFFSET + i32::from(index) * 64,
                DispSize::Disp32,
            );
        }
    }

    /// Fuse one exact `VLoad` plus VEX packed logic, integer add/subtract, or
    /// binary floating-point operation. The MMU helper commits only a
    /// nonarchitectural transfer slot. One low vector register not named by the
    /// guest instruction carries that value for the native operation and is
    /// restored in full before continuation.
    pub(crate) fn try_lower_jit_vex_binary_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_binary_memory_sequence(
            block,
            index,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX binary sequence starts with VLoad"),
        };
        let byte_size = match sequence.width {
            VecWidth::V128 => 16,
            VecWidth::V256 => 32,
            _ => unreachable!("validated VEX binary width"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            byte_size,
            true,
            true,
        )?;

        let scratch_index = (0..16u8)
            .find(|candidate| *candidate != sequence.destination && *candidate != sequence.source1)
            .expect("two VEX operands leave at least fourteen scratch registers");
        let scratch = Self::vex_binary_phys_reg(scratch_index, sequence.width);
        let destination = Self::vex_binary_phys_reg(sequence.destination, sequence.width);
        let source1 = Self::vex_binary_phys_reg(sequence.source1, sequence.width);

        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.emit_vex_scratch_load(scratch, sequence.width, X86_GUEST_VECTOR_SCRATCH_OFFSET);
        self.emit_vec_rrr(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F,
                pp: sequence.prefix,
                opcode: sequence.opcode,
                width: sequence.width,
                w: sequence.w,
            },
            destination,
            source1,
            scratch,
        );
        self.emit_vex_binary_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop rax

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
