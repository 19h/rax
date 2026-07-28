//! Helper-backed x86 VEX/APX scalar BMI memory-source lowering.

use std::collections::HashMap;

use super::*;

#[derive(Clone, Copy)]
enum JitMemBmiKind {
    AndNot {
        other: u8,
        defined_rflags_mask: Option<i64>,
    },
    Bls {
        kind: X86BlsKind,
        defined_rflags_mask: Option<i64>,
    },
    Bzhi {
        control: u8,
        defined_rflags_mask: Option<i64>,
    },
    Bextr {
        control: u8,
        defined_rflags_mask: Option<i64>,
    },
    Pdep {
        source: u8,
    },
    Pext {
        source: u8,
    },
    Rorx {
        amount: u8,
    },
}

impl X86_64Lowerer {
    /// Fuse the exact scalar `Load` + BMI consumer pair emitted by the VEX/APX
    /// lifters. The load helper stages a zero-extended scalar in a 16-byte
    /// caller-owned stack frame and snapshots every architectural GPR in
    /// `GuestRegs`. The consumer then reads all operands before committing its
    /// destination, which makes every destination/source alias and the full
    /// 32-register APX GPR namespace safe.
    ///
    /// A load fault removes the caller-owned frame and exits at the load's
    /// precise guest PC before the destination or RFLAGS change.
    pub(crate) fn try_lower_jit_mem_bmi_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_bmi_source_sequence_len(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        let load = &block.ops[idx];
        let (addr, mem_width) = match &load.kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } => (addr, *width),
            _ => unreachable!("validated BMI memory source starts with Load"),
        };
        let consumer = &block.ops[idx + 1];
        let (dst, width, kind) = match &consumer.kind {
            OpKind::AndNot {
                dst,
                src2: SrcOperand::Reg(other),
                width,
                flags,
                ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::AndNot {
                    other: Self::x86_gpr_index(*other)
                        .expect("validated ANDN second source is an x86 GPR"),
                    defined_rflags_mask: match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!("validated ANDN has exact flag policy"),
                    },
                },
            ),
            OpKind::X86Bls {
                dst,
                width,
                kind,
                flags,
                ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Bls {
                    kind: *kind,
                    defined_rflags_mask: match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!("validated BLS has exact flag policy"),
                    },
                },
            ),
            OpKind::Bzhi {
                dst,
                index,
                width,
                flags,
                ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Bzhi {
                    control: Self::x86_gpr_index(*index)
                        .expect("validated BZHI index is an x86 GPR"),
                    defined_rflags_mask: match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!("validated BZHI has exact flag policy"),
                    },
                },
            ),
            OpKind::Bextr {
                dst,
                control,
                width,
                flags,
                ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Bextr {
                    control: Self::x86_gpr_index(*control)
                        .expect("validated BEXTR control is an x86 GPR"),
                    defined_rflags_mask: match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x841),
                        _ => unreachable!("validated BEXTR has exact flag policy"),
                    },
                },
            ),
            OpKind::Pdep {
                dst, src, width, ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Pdep {
                    source: Self::x86_gpr_index(*src).expect("validated PDEP source is an x86 GPR"),
                },
            ),
            OpKind::Pext {
                dst, src, width, ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Pext {
                    source: Self::x86_gpr_index(*src).expect("validated PEXT source is an x86 GPR"),
                },
            ),
            OpKind::Ror {
                dst,
                amount: SrcOperand::Imm(amount),
                width,
                ..
            } => (
                *dst,
                *width,
                JitMemBmiKind::Rorx {
                    amount: u8::try_from(*amount)
                        .expect("validated RORX immediate is an unsigned byte"),
                },
            ),
            _ => unreachable!("validated BMI memory source has an exact consumer"),
        };
        let dst_index = Self::x86_gpr_index(dst).expect("validated BMI destination is an x86 GPR");

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_mem_op(
            load.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            addr,
            mem_width,
            SignExtend::Zero,
            16,
        )?;

        // The successful helper path has already made GuestRegs authoritative.
        // Preserve guest RAX, load the state pointer, and stage every input
        // before a possibly aliasing destination can be committed.
        self.code.emit_u8(0x50);
        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match kind {
                JitMemBmiKind::AndNot { other, .. } => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 8, width);
                    emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(other) * 8, width);
                }
                JitMemBmiKind::Bls { .. } | JitMemBmiKind::Rorx { .. } => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 8, width);
                }
                JitMemBmiKind::Bzhi { control, .. } | JitMemBmiKind::Bextr { control, .. } => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 8, width);
                    emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(control) * 8, width);
                }
                JitMemBmiKind::Pdep { source } | JitMemBmiKind::Pext { source } => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(source) * 8, width);
                    emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rsp, 8, width);
                }
            }
        }

        self.code.emit_u8(0x9C); // pushfq: preserve undefined or all status flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match kind {
                JitMemBmiKind::AndNot { .. } => {
                    emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::R8, width);
                    emitter.emit_not(PhysReg::Rdx, width);
                    emitter.emit_and_rr(PhysReg::Rdx, PhysReg::Rdi, width);
                }
                JitMemBmiKind::Bls { kind, .. } => {
                    emitter.emit_vex_bls_rr(kind, PhysReg::Rdx, PhysReg::Rdi, width);
                }
                JitMemBmiKind::Bzhi { .. } => {
                    emitter.emit_vex_bmi_rr(0xF5, PhysReg::Rdx, PhysReg::Rdi, PhysReg::R8, width);
                }
                JitMemBmiKind::Bextr { .. } => {
                    emitter.emit_vex_bmi_rr(0xF7, PhysReg::Rdx, PhysReg::Rdi, PhysReg::R8, width);
                }
                JitMemBmiKind::Pdep { .. } => {
                    emitter.emit_vex_bmi_rr_pp(
                        0xF5,
                        X86SsePrefix::Repne,
                        PhysReg::Rdx,
                        PhysReg::R8,
                        PhysReg::Rdi,
                        width,
                    );
                }
                JitMemBmiKind::Pext { .. } => {
                    emitter.emit_vex_bmi_rr_pp(
                        0xF5,
                        X86SsePrefix::Rep,
                        PhysReg::Rdx,
                        PhysReg::R8,
                        PhysReg::Rdi,
                        width,
                    );
                }
                JitMemBmiKind::Rorx { amount } => {
                    emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::Rdi, width);
                    emitter.emit_ror_ri(PhysReg::Rdx, amount, width);
                }
            }
        }
        match kind {
            JitMemBmiKind::AndNot {
                defined_rflags_mask,
                ..
            }
            | JitMemBmiKind::Bls {
                defined_rflags_mask,
                ..
            }
            | JitMemBmiKind::Bzhi {
                defined_rflags_mask,
                ..
            }
            | JitMemBmiKind::Bextr {
                defined_rflags_mask,
                ..
            } => self.finish_bmi_flags(PhysReg::Rdx, defined_rflags_mask),
            JitMemBmiKind::Pdep { .. }
            | JitMemBmiKind::Pext { .. }
            | JitMemBmiKind::Rorx { .. } => self.code.emit_u8(0x9D),
        }

        self.emit_store_gpr_slot_from_reg(dst_index, PhysReg::Rdx, width)?;
        if dst_index == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }

        Ok(Some(consumed))
    }
}
