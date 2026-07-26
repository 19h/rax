//! State-backed x86 LEA lowering.
//!
//! `LEA` with a guest RSP/RBP or APX EGPR operand cannot use the ordinary
//! register-to-register lowering: the native region runs on the host stack, so
//! hardware RSP holds the host stack pointer and hardware RBP holds the native
//! frame pointer. Those guest values live in the `GuestRegs` file instead.
//!
//! This lowering spills the legacy GPRs into that file, rebuilds the effective
//! address from the resulting coherent snapshot, and commits the architectural
//! destination back into its slot. LEA performs no memory access and updates no
//! flags, so the whole sequence is built exclusively from `MOV`/`LEA` and is
//! flag-preserving end to end.

use crate::smir::ir::types::{Address, OpWidth, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::x86_64::{X86_64Lowerer, X86Emitter};

impl X86_64Lowerer {
    pub(crate) fn lower_state_backed_gpr_lea(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let invalid = |operand: &str| LowerError::InvalidOperand {
            op: "state-backed LEA".to_string(),
            operand: operand.to_string(),
        };
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(invalid("unsupported destination width"));
        }
        let dst_idx = Self::x86_gpr_index(dst)
            .ok_or_else(|| invalid("destination is not an architectural x86 GPR"))?;
        let gpr_index = |reg: VReg| {
            Self::x86_gpr_index(reg)
                .ok_or_else(|| invalid("address operand is not an architectural x86 GPR"))
        };

        // Resolve every architectural operand BEFORE emitting, so a rejected
        // shape cannot leave a half-emitted spill sequence behind.
        enum Shape {
            BaseDisp {
                base: u8,
                disp: i32,
            },
            BaseIndex {
                base: Option<u8>,
                index: u8,
                scale: u8,
                disp: i32,
            },
        }
        let shape = match addr {
            Address::Direct(base) => Shape::BaseDisp {
                base: gpr_index(*base)?,
                disp: 0,
            },
            Address::BaseOffset { base, offset, .. } => Shape::BaseDisp {
                base: gpr_index(*base)?,
                disp: i32::try_from(*offset)
                    .map_err(|_| invalid("displacement is not encodable as imm32"))?,
            },
            Address::BaseIndexScale {
                base,
                index,
                scale,
                disp,
                ..
            } => {
                if !matches!(scale, 1 | 2 | 4 | 8) {
                    return Err(invalid("invalid address scale"));
                }
                Shape::BaseIndex {
                    base: match base {
                        Some(base) => Some(gpr_index(*base)?),
                        None => None,
                    },
                    index: gpr_index(*index)?,
                    scale: *scale,
                    disp: *disp,
                }
            }
            _ => return Err(invalid("unsupported effective-address form")),
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            match shape {
                Shape::BaseDisp { base, disp } => {
                    emitter.emit_mov_rm(
                        PhysReg::Rdx,
                        PhysReg::Rax,
                        i32::from(base) * 8,
                        OpWidth::W64,
                    );
                    // The architectural destination width selects the operand
                    // size of the address computation: W64 keeps the full
                    // effective address, W32 truncates and zero-extends, W16
                    // truncates and preserves the destination's upper bits.
                    emitter.emit_lea_disp_width(
                        PhysReg::Rdx,
                        PhysReg::Rdx,
                        disp,
                        crate::smir::ir::types::DispSize::Auto,
                        width,
                    );
                }
                Shape::BaseIndex {
                    base,
                    index,
                    scale,
                    disp,
                } => {
                    if let Some(base) = base {
                        emitter.emit_mov_rm(
                            PhysReg::Rdx,
                            PhysReg::Rax,
                            i32::from(base) * 8,
                            OpWidth::W64,
                        );
                    }
                    emitter.emit_mov_rm(
                        PhysReg::Rdi,
                        PhysReg::Rax,
                        i32::from(index) * 8,
                        OpWidth::W64,
                    );
                    emitter.emit_lea_sib_disp_width(
                        PhysReg::Rdx,
                        base.map(|_| PhysReg::Rdx),
                        PhysReg::Rdi,
                        scale,
                        disp,
                        crate::smir::ir::types::DispSize::Auto,
                        width,
                    );
                }
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        // Keep the prologue's saved guest RBP coherent with the state slot so
        // the epilogue POP returns the updated architectural value.
        if dst_idx == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }
}
