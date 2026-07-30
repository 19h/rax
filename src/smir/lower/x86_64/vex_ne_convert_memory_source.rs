//! Helper-backed AVX_NE_CONVERT VEX memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{DispSize, MemWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn emit_vex_ne_convert_stack_store(
        &mut self,
        register: PhysReg,
        width: VecWidth,
        displacement: i32,
    ) {
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_vex_prefix(
            X86VecMap::Map0F,
            X86SsePrefix::Rep,
            width,
            false,
            register.vec_ext(),
            0,
            PhysReg::Rsp.vec_ext(),
            0,
        );
        emitter.code.emit_u8(0x7F); // VMOVDQU [rsp+disp],xmm/ymm
        emitter.emit_modrm_mem_disp(register, PhysReg::Rsp, displacement, DispSize::Auto);
    }

    /// Fuse one exact AVX_NE_CONVERT memory-source decomposition.
    ///
    /// A 2-byte broadcast source is loaded directly into a 16-byte
    /// nonarchitectural host-stack slot. A 16-/32-byte source first crosses
    /// the precise vector helper boundary, then a borrowed low vector
    /// register copies the helper result into a 16-/32-byte host-stack slot
    /// and is restored completely. The byte-validated original instruction
    /// consumes `[rsp]`, preserving its memory-only encoding, conversion
    /// semantics, destination, and VEX zero-upper effect. A helper fault exits
    /// before any architectural vector state is changed.
    pub(crate) fn try_lower_jit_vex_ne_convert_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_ne_convert_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let encoding = sequence.encoding;
        let address = match &block.ops[index].kind {
            OpKind::Load {
                addr,
                width: MemWidth::B2,
                sign: SignExtend::Zero,
                ..
            } if encoding.kind.broadcast() => addr,
            OpKind::VLoad { addr, width, .. }
                if !encoding.kind.broadcast() && *width == encoding.width =>
            {
                addr
            }
            _ => unreachable!("validated AVX_NE_CONVERT sequence owns its exact memory load"),
        };

        if encoding.kind.broadcast() {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
            }
            self.emit_jit_mem_op(
                block.ops[index].guest_pc,
                true,
                None,
                Some(16),
                None,
                None,
                None,
                address,
                MemWidth::B2,
                SignExtend::Zero,
                16,
            )?;
        } else {
            self.emit_jit_vector_mem_helper(
                block.ops[index].guest_pc,
                true,
                X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                address,
                encoding.memory_size,
                true,
                true,
            )?;

            let frame_size = encoding.width.bytes() as i32;
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -frame_size);
            }
            let scratch = match encoding.width {
                VecWidth::V128 => PhysReg::Xmm(encoding.scratch),
                VecWidth::V256 => PhysReg::Ymm(encoding.scratch),
                _ => unreachable!("validated AVX_NE_CONVERT source width"),
            };
            self.code.emit_u8(0x50); // push guest RAX; stack source is at [rsp+8]
            self.emit_load_state_ptr_rax();
            self.emit_jit_vector_scratch_load(scratch, encoding.width);
            self.emit_vex_ne_convert_stack_store(scratch, encoding.width, 8);
            self.emit_jit_vector_scratch_restore(encoding.scratch);
            self.code.emit_u8(0x58); // pop guest RAX
        }

        self.code.emit_bytes(encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(
                PhysReg::Rsp,
                PhysReg::Rsp,
                if encoding.kind.broadcast() {
                    16
                } else {
                    encoding.width.bytes() as i32
                },
            );
        }
        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(encoding.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
