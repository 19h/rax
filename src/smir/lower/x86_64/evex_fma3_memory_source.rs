//! Helper-backed EVEX packed/scalar FMA3 memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, SignExtend, VReg, VecWidth};
use crate::smir::ir::{SmirBlock, X86EvexPackedFma3MemoryReplay};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse the exact scalar memory-source decomposition emitted for one
    /// unmasked or writemasked EVEX FMA3 instruction. The scalar MMU helper
    /// stages the complete 2/4/8-byte source in a 16-byte nonarchitectural
    /// host-stack slot. For a writemasked source, a live-host-K bit-0 test
    /// bypasses the helper completely when the access is architecturally
    /// suppressed. A byte-validated rewrite of the original instruction then
    /// consumes `[rsp]`, preserving native FMA, MXCSR, merge/zero masking,
    /// destination-lane, and upper-zeroing behavior without borrowing an
    /// architectural vector register.
    pub(crate) fn try_lower_jit_evex_scalar_fma3_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_scalar_fma3_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX scalar FMA3 memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX FMA3".to_string(),
            });
        }
        let load_index = index + sequence.load_offset;
        let address = match &block.ops[load_index].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == sequence.memory_width => addr,
            OpKind::PredLoad {
                addr,
                width,
                signed: SignExtend::Zero,
                ..
            } if *width == sequence.memory_width => addr,
            _ => unreachable!("validated EVEX scalar FMA3 sequence owns its scalar memory op"),
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        let inactive = if let Some(mask) = sequence.encoding.writemask {
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, 1, OpWidth::W64);
            }
            Some(self.emit_jcc_placeholder(X86Cond::E))
        } else {
            None
        };

        if inactive.is_some() {
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
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
            sequence.memory_width,
            SignExtend::Zero,
            16,
        )?;
        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let execute = self.code.position();
            self.code.emit_u32(0);
            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            self.patch_rel32_to_current(execute)?;
        }
        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }

        Ok(Some(sequence.consumed))
    }

    fn evex_fma3_memory_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX packed FMA3 vector width"),
        }
    }

    /// Fuse the exact packed EVEX FMA3 memory-source decomposition. A
    /// full-vector source uses the nonarchitectural vector transfer slot and a
    /// byte-validated register rewrite. A broadcast source uses at most one
    /// scalar helper load into a 16-byte host-stack slot and a byte-validated
    /// `[rsp]{1toN}` rewrite, without borrowing an architectural vector
    /// register. For a writemasked broadcast, a live applicable-lane test
    /// bypasses the helper when the scalar access is architecturally
    /// suppressed while the rewritten instruction still applies merge/zero
    /// masking and upper clearing. Either helper exits precisely before FMA
    /// execution on fault.
    pub(crate) fn try_lower_jit_evex_packed_fma3_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_packed_fma3_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX packed FMA3 memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX FMA3".to_string(),
            });
        }
        match sequence.encoding.replay {
            X86EvexPackedFma3MemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated EVEX packed FMA3 vector source starts with VLoad"),
                };
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    true,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    sequence.memory_size,
                    true,
                    true,
                )?;

                let scratch_reg = Self::evex_fma3_memory_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } => {
                let memory_index = index + sequence.memory_offset;
                let (address, memory_width) = match &block.ops[memory_index].kind {
                    OpKind::Load {
                        addr,
                        width,
                        sign: SignExtend::Zero,
                        ..
                    } => (addr, *width),
                    OpKind::PredLoad {
                        addr,
                        width,
                        signed: SignExtend::Zero,
                        ..
                    } => (addr, *width),
                    _ => {
                        unreachable!(
                            "validated EVEX packed FMA3 broadcast owns one scalar memory op"
                        )
                    }
                };
                debug_assert_eq!(memory_width.bytes(), sequence.memory_size);
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
                }
                let inactive = if let Some(mask) = sequence.encoding.writemask {
                    let lanes = sequence.encoding.width.lanes(sequence.encoding.elem);
                    debug_assert!(lanes <= 32, "packed FMA3 applicable opmask width");
                    let lane_mask = (1u64 << lanes) - 1;
                    self.code.emit_u8(0x9C); // pushfq
                    self.code.emit_u8(0x50); // push guest RAX
                    self.emit_opmask_mask_to_rax64(mask);
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        // W32 is required for the 32-lane FP16/ZMM mask:
                        // TEST r64,imm32 would sign-extend 0xFFFF_FFFF and
                        // incorrectly observe architecturally ignored high
                        // opmask bits.
                        emitter.emit_test_ri(PhysReg::Rax, lane_mask as i64, OpWidth::W32);
                    }
                    Some(self.emit_jcc_placeholder(X86Cond::E))
                } else {
                    None
                };

                if inactive.is_some() {
                    self.code.emit_u8(0x58); // pop guest RAX
                    self.code.emit_u8(0x9D); // restore exact pre-guard flags
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
                    memory_width,
                    SignExtend::Zero,
                    16,
                )?;
                if let Some(inactive) = inactive {
                    self.code.emit_u8(0xE9);
                    let execute = self.code.position();
                    self.code.emit_u32(0);
                    self.patch_rel32_to_current(inactive)?;
                    self.code.emit_u8(0x58); // pop guest RAX
                    self.code.emit_u8(0x9D); // restore exact pre-guard flags
                    self.patch_rel32_to_current(execute)?;
                }
                self.code.emit_bytes(stack_instruction.as_slice());
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
                }
            }
        }

        Ok(Some(sequence.consumed))
    }
}
