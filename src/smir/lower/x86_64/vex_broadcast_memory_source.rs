//! Helper-backed VEX memory-broadcast lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_broadcast_destination(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX memory-broadcast width"),
        }
    }

    fn emit_vex_broadcast_128_block(
        &mut self,
        destination: PhysReg,
        scratch_index: u8,
        integer: bool,
    ) {
        self.emit_vec_rrr_imm(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: if integer { 0x38 } else { 0x18 },
                width: VecWidth::V256,
                w: false,
            },
            destination,
            PhysReg::Ymm(scratch_index),
            PhysReg::Xmm(scratch_index),
            1,
        );
    }

    fn emit_vex_floating_scalar_broadcast(
        &mut self,
        destination: PhysReg,
        scratch_index: u8,
        elem: VecElementType,
        width: VecWidth,
    ) {
        if width == VecWidth::V256 {
            self.emit_vex_broadcast_128_block(destination, scratch_index, false);
        }
        let source = if width == VecWidth::V256 {
            destination
        } else {
            PhysReg::Xmm(scratch_index)
        };
        self.emit_vec_rrr_imm(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F,
                pp: match elem {
                    VecElementType::F32 => X86SsePrefix::None,
                    VecElementType::F64 => X86SsePrefix::OpSize,
                    _ => unreachable!("validated floating VEX memory broadcast"),
                },
                opcode: 0xC6,
                width,
                w: false,
            },
            destination,
            source,
            source,
            0,
        );
    }

    /// Fuse one exact VEX memory-broadcast decomposition. The helper reads
    /// exactly 1, 2, 4, 8, or 16 bytes into a zero-padded nonarchitectural
    /// transfer slot before any destination mutation. A borrowed XMM register
    /// carries the value and is restored completely after the non-faulting
    /// register sequence.
    ///
    /// Floating memory broadcasts remain AVX-only: scalar forms synthesize
    /// replication with VSHUFPS/VSHUFPD, and 128-bit tuple forms use
    /// VINSERTF128. Integer forms use their AVX2 register opcodes or
    /// VINSERTI128. This preserves the guest instructions' feature boundary.
    pub(crate) fn try_lower_jit_vex_broadcast_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_broadcast_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated VEX memory broadcast starts with Lea"),
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

        let scratch_index = (0..16u8)
            .find(|candidate| *candidate != sequence.destination)
            .expect("one VEX destination leaves fifteen scratch registers");
        let destination = Self::vex_broadcast_destination(sequence.destination, sequence.width);

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        // All covered sources are at most 128 bits. VEX.128 VMOVDQU imports
        // the zero-padded helper slot and clears the scratch YMM upper half.
        self.emit_jit_vector_scratch_load(PhysReg::Xmm(scratch_index), VecWidth::V128);
        match (sequence.source_lanes, sequence.elem) {
            (4, VecElementType::F32) => {
                self.emit_vex_broadcast_128_block(destination, scratch_index, false);
            }
            (4, VecElementType::I32) => {
                self.emit_vex_broadcast_128_block(destination, scratch_index, true);
            }
            (1, VecElementType::F32 | VecElementType::F64) => {
                self.emit_vex_floating_scalar_broadcast(
                    destination,
                    scratch_index,
                    sequence.elem,
                    sequence.width,
                );
            }
            (
                1,
                VecElementType::I8
                | VecElementType::I16
                | VecElementType::I32
                | VecElementType::I64,
            ) => {
                self.emit_vec_rr(
                    VecEncoding {
                        kind: VecEncodingKind::Vex,
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: sequence.opcode,
                        width: sequence.width,
                        w: false,
                    },
                    destination,
                    PhysReg::Xmm(scratch_index),
                    0,
                );
            }
            _ => unreachable!("validated VEX memory-broadcast shape"),
        }
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
