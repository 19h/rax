//! Helper-backed lowering for legacy and VEX.128 XMM masked byte stores.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, MemWidth, OpWidth, SignExtend, VReg, X86Reg,
};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

impl X86_64Lowerer {
    fn emit_maskmovdqu_operand_snapshot(&mut self, data_index: u8, mask_index: u8) {
        if self.preserve_vector_mem_helpers {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_sse_mov_rm_disp(
                Some(0xF3),
                0x7F,
                PhysReg::Xmm(data_index),
                PhysReg::Rsp,
                0,
                DispSize::Auto,
            );
            emitter.emit_sse_mov_rm_disp(
                Some(0xF3),
                0x7F,
                PhysReg::Xmm(mask_index),
                PhysReg::Rsp,
                16,
                DispSize::Auto,
            );
            return;
        }

        // This region has no independently admitted native vector operation,
        // so the entry bridge leaves host XMM registers untouched. Copy the
        // two low 128-bit operands from their GuestRegs ZMM slots with GPR-only
        // instructions; PUSH/POP preserve the corresponding guest GPRs.
        self.code.emit_u8(0x50); // push rax
        self.code.emit_u8(0x56); // push rsi
        self.emit_load_state_ptr_rax();
        for (index, stack_offset) in [(data_index, 0i32), (mask_index, 16)] {
            for chunk in [0i32, 8] {
                let state_offset = X86_GUEST_ZMM_OFFSET + i32::from(index) * 64 + chunk;
                self.emit_struct_mov(PhysReg::Rax, 6, state_offset, false);
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(
                    PhysReg::Rsp,
                    16 + stack_offset + chunk,
                    PhysReg::Rsi,
                    OpWidth::W64,
                );
            }
        }
        self.code.emit_u8(0x5E); // pop rsi
        self.code.emit_u8(0x58); // pop rax
    }

    /// Fuse one exact `MASKMOVDQU`/`VMASKMOVDQU` expansion into sixteen
    /// ordered, conditionally executed 1-byte MMU-helper stores. Operand bytes
    /// are snapshotted before the first possible fault; later active lanes
    /// retain the instruction's architecturally ordered partial completion.
    pub(crate) fn try_lower_jit_maskmovdqu(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_maskmovdqu_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
        }
        self.emit_maskmovdqu_operand_snapshot(sequence.data_index, sequence.mask_index);

        for lane in 0..16u8 {
            let store_offset = if sequence.address_size_32 {
                4 + usize::from(lane) * 5
            } else {
                3 + usize::from(lane) * 4
            };
            let store = &block.ops[idx + store_offset];
            let lifted_addr = match &store.kind {
                OpKind::PredStore {
                    addr,
                    width: MemWidth::B1,
                    ..
                } => addr,
                _ => {
                    return Err(LowerError::InvalidOperand {
                        op: "XMM MASKMOVDQU".to_string(),
                        operand: "validated lane must end in its exact byte store".to_string(),
                    });
                }
            };
            let helper_addr = if sequence.address_size_32 {
                Some(match lifted_addr {
                    Address::BaseOffset { disp_size, .. } => Address::BaseOffset {
                        base: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                        offset: i64::from(lane),
                        disp_size: *disp_size,
                    },
                    Address::SegmentRel {
                        segment,
                        index: None,
                        scale: 1,
                        ..
                    } => Address::SegmentRel {
                        segment: *segment,
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
                        index: None,
                        scale: 1,
                        disp: i64::from(lane),
                    },
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "XMM MASKMOVDQU addr32".to_string(),
                            operand: "validated lane must use EDI with optional FS/GS".to_string(),
                        });
                    }
                })
            } else {
                None
            };
            let addr = helper_addr.as_ref().unwrap_or(lifted_addr);

            self.code.emit_u8(0x9C); // pushfq
            // Outer mask slot starts at [rsp+16]; PUSHFQ moves it to +24.
            self.code.emit_u8(0xF6);
            self.code.emit_u8(0x44);
            self.code.emit_u8(0x24);
            self.code.emit_u8(24 + lane);
            self.code.emit_u8(0x80);
            let inactive = self.emit_jcc_placeholder(X86Cond::E);
            self.code.emit_u8(0x9D); // popfq before a helper call

            let emit = if sequence.address_size_32 {
                Self::emit_jit_mem_op_addr32
            } else {
                Self::emit_jit_mem_op
            };
            emit(
                self,
                store.guest_pc,
                false,
                None,
                None,
                None,
                None,
                Some(16 + i32::from(lane)),
                addr,
                MemWidth::B1,
                SignExtend::Zero,
                32,
            )?;
            self.code.emit_u8(0xE9);
            let done = self.code.position();
            self.code.emit_u32(0);

            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x9D); // popfq on the inactive path
            self.patch_rel32_to_current(done)?;
        }

        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        Ok(Some(sequence.consumed))
    }
}
