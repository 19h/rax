//! Native lowering for register-only x86 XADD.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86GprOperand, X86XaddOp};
use crate::smir::ir::types::OpWidth;
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

use super::{X86_64Lowerer, X86Emitter};

/// Validate the complete register-only XADD shape reconstructed by the strict
/// x86 lifter. Encoding hints are excluded because the operation itself
/// retains every semantic distinction, including legacy high-byte lanes.
pub(crate) fn x86_xadd_shape_valid(op: &SmirOp) -> bool {
    matches!(&op.kind, OpKind::X86Xadd(xadd) if op.x86_hint.is_none() && xadd.is_valid())
}

fn needs_state_snapshot(xadd: X86XaddOp) -> bool {
    [xadd.dst, xadd.src].into_iter().any(|operand| {
        operand
            .gpr_index()
            .is_some_and(|index| index >= 16 || matches!(index, 4 | 5))
    })
}

impl X86_64Lowerer {
    fn emit_xadd_registers(
        &mut self,
        xadd: X86XaddOp,
        dst: PhysReg,
        src: PhysReg,
    ) -> Result<(), LowerError> {
        if xadd.dst.high_byte || xadd.src.high_byte {
            let encode = |operand: X86GprOperand| {
                operand
                    .gpr_index()
                    .map(|index| index + u8::from(operand.high_byte) * 4)
            };
            let dst = encode(xadd.dst).ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: "invalid high-byte destination".into(),
            })?;
            let src = encode(xadd.src).ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: "invalid high-byte source".into(),
            })?;
            self.code.emit_bytes(&[0x0F, 0xC0, 0xC0 | (src << 3) | dst]);
            return Ok(());
        }

        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_rex_for_width(xadd.width, src, dst);
        emitter.code.emit_u8(0x0F);
        emitter.code.emit_u8(if xadd.width == OpWidth::W8 {
            0xC0
        } else {
            0xC1
        });
        emitter.emit_modrm_rr(src, dst);
        Ok(())
    }

    fn emit_load_xadd_operand(
        &mut self,
        operand: X86GprOperand,
        dst: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = operand
            .gpr_index()
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: "operand is not an architectural GPR".into(),
            })?;
        let offset = i32::from(index) * 8 + i32::from(operand.high_byte);
        let load_width = if operand.high_byte {
            OpWidth::W8
        } else {
            width
        };
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mov_rm(dst, PhysReg::Rax, offset, load_width);
        Ok(())
    }

    fn emit_store_xadd_operand(
        &mut self,
        operand: X86GprOperand,
        src: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = operand
            .gpr_index()
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: "operand is not an architectural GPR".into(),
            })?;
        if operand.high_byte {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rax, i32::from(index) * 8 + 1, src, OpWidth::W8);
            Ok(())
        } else {
            self.emit_store_gpr_slot_from_reg(index, src, width)
        }
    }

    fn lower_state_backed_xadd(&mut self, xadd: X86XaddOp) -> Result<(), LowerError> {
        let preserve_flags = xadd.flags == FlagUpdate::None;
        self.code.emit_u8(0x50); // push guest RAX before loading the state pointer
        self.emit_load_state_ptr_rax();
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq; guest RAX is at [rsp+8]
        }
        self.emit_spill_legacy_gprs_to_state_from_rax(if preserve_flags { 8 } else { 0 });

        self.emit_load_xadd_operand(xadd.dst, PhysReg::Rdx, xadd.width)?;
        self.emit_load_xadd_operand(xadd.src, PhysReg::Rdi, xadd.width)?;
        self.emit_xadd_registers(
            X86XaddOp {
                dst: X86GprOperand::low(crate::smir::ir::types::X86Reg::Rdx),
                src: X86GprOperand::low(crate::smir::ir::types::X86Reg::Rdi),
                width: xadd.width,
                flags: xadd.flags,
            },
            PhysReg::Rdx,
            PhysReg::Rdi,
        )?;

        // XADD leaves old DST in the source scratch and the sum in the
        // destination scratch. Source-first commit makes equal operands end in
        // the sum and also handles AL/AH-style aliases within one parent GPR.
        self.emit_store_xadd_operand(xadd.src, PhysReg::Rdi, xadd.width)?;
        self.emit_store_xadd_operand(xadd.dst, PhysReg::Rdx, xadd.width)?;

        if [xadd.dst, xadd.src]
            .into_iter()
            .any(|operand| operand.gpr_index() == Some(5))
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, 5 * 8, OpWidth::W64);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        if preserve_flags {
            self.code.emit_u8(0x9D); // popfq
        }
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_x86_xadd(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        let OpKind::X86Xadd(xadd) = &op.kind else {
            return Err(LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: "wrong operation kind".into(),
            });
        };
        if !x86_xadd_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86Xadd".into(),
                operand: format!("invalid register/lane/width/flags shape: {xadd:?}"),
            });
        }
        if needs_state_snapshot(*xadd) {
            return self.lower_state_backed_xadd(*xadd);
        }

        let dst = self.get_reg(xadd.dst.vreg())?;
        let src = self.get_reg(xadd.src.vreg())?;
        if xadd.flags == FlagUpdate::None {
            self.code.emit_u8(0x9C); // pushfq
        }
        self.emit_xadd_registers(*xadd, dst, src)?;
        if xadd.flags == FlagUpdate::None {
            self.code.emit_u8(0x9D); // popfq
        }
        Ok(())
    }
}
