//! Native lowering for register- and memory-destination `CMPXCHG`.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86CmpxchgOp, X86GprOperand};
#[cfg(feature = "smir-jit")]
use crate::smir::lower::runtime::X86JitScalarValue;

/// Validate the complete register-only CMPXCHG shape reconstructed by the
/// strict x86 lifter. Encoding hints are excluded because explicit operands
/// retain the only non-register identity, the legacy high-byte lane.
pub(crate) fn x86_cmpxchg_shape_valid(op: &SmirOp) -> bool {
    matches!(&op.kind, OpKind::X86Cmpxchg(cmpxchg) if op.x86_hint.is_none() && cmpxchg.is_valid())
}

fn cmpxchg_needs_state_snapshot(cmpxchg: X86CmpxchgOp) -> bool {
    cmpxchg.dst == cmpxchg.src
        || [cmpxchg.dst, cmpxchg.src].into_iter().any(|operand| {
            operand
                .gpr_index()
                .is_some_and(|index| index >= 16 || matches!(index, 4 | 5))
        })
}

impl X86_64Lowerer {
    fn emit_cmpxchg_registers(&mut self, width: OpWidth, dst: PhysReg, src: PhysReg) {
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_rex_for_width(width, src, dst);
        emitter.code.emit_u8(0x0F);
        emitter
            .code
            .emit_u8(if width == OpWidth::W8 { 0xB0 } else { 0xB1 });
        emitter.emit_modrm_rr(src, dst);
    }

    fn emit_load_cmpxchg_operand(
        &mut self,
        operand: X86GprOperand,
        dst: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = operand
            .gpr_index()
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "operand is not an architectural GPR".into(),
            })?;
        let offset = i32::from(index) * 8 + i32::from(operand.high_byte);
        let load_width = if operand.high_byte {
            OpWidth::W8
        } else {
            width
        };
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mov_rm(dst, PhysReg::Rcx, offset, load_width);
        Ok(())
    }

    fn emit_store_cmpxchg_operand(
        &mut self,
        operand: X86GprOperand,
        src: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = operand
            .gpr_index()
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "operand is not an architectural GPR".into(),
            })?;
        let offset = i32::from(index) * 8 + i32::from(operand.high_byte);
        let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if operand.high_byte {
                OpWidth::W8
            } else {
                width
            }
        } else {
            OpWidth::W64
        };
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mov_mr(PhysReg::Rcx, offset, src, commit_width);
        Ok(())
    }

    fn lower_state_backed_cmpxchg(&mut self, cmpxchg: X86CmpxchgOp) -> Result<(), LowerError> {
        let dst_index = cmpxchg
            .dst
            .gpr_index()
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "destination is not an architectural GPR".into(),
            })?;
        let preserve_flags = cmpxchg.flags == FlagUpdate::None;
        self.code.emit_u8(0x50); // push guest RAX before loading the state pointer
        self.emit_load_state_ptr_rax();
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq; guest RAX is at [rsp+8]
        }
        self.emit_spill_legacy_gprs_to_state_from_rax(if preserve_flags { 8 } else { 0 });

        // RCX retains the state pointer while RAX takes its architectural role.
        // Every operand is snapshotted before the native instruction commits.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rcx, 0, cmpxchg.width);
        }
        self.emit_load_cmpxchg_operand(cmpxchg.dst, PhysReg::Rdx, cmpxchg.width)?;
        self.emit_load_cmpxchg_operand(cmpxchg.src, PhysReg::Rdi, cmpxchg.width)?;
        if !preserve_flags {
            // Intel defines every arithmetic flag from accumulator - DST.
            // Translated x86-64 hosts have published the reverse subtraction
            // for CMPXCHG, so preserve an explicit CMP image around the exact
            // conditional register transition. CMPXCHG consumes no flags.
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_cmp_rr(PhysReg::Rax, PhysReg::Rdx, cmpxchg.width);
            emitter.code.emit_u8(0x9C); // pushfq
        }
        self.emit_cmpxchg_registers(cmpxchg.width, PhysReg::Rdx, PhysReg::Rdi);

        // CMPXCHG conditionally writes exactly one architectural destination.
        // Keeping the commits on separate paths is required when DST aliases
        // RAX and preserves upper halves on every no-op path.
        let mismatch = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_store_cmpxchg_operand(cmpxchg.dst, PhysReg::Rdx, cmpxchg.width)?;
        self.code.emit_u8(0xE9); // jmp .done
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(mismatch)?;
        self.emit_store_cmpxchg_operand(
            X86GprOperand::low(X86Reg::Rax),
            PhysReg::Rax,
            cmpxchg.width,
        )?;
        self.patch_rel32_to_current(done)?;

        if dst_index == 5 {
            self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
        }
        self.emit_reload_all(PhysReg::Rcx);
        // Restore either the incoming image (`None`) or the explicit
        // accumulator-minus-destination image (`All`).
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    fn lower_direct_cmpxchg(&mut self, cmpxchg: X86CmpxchgOp) -> Result<(), LowerError> {
        if cmpxchg.dst.high_byte || cmpxchg.src.high_byte {
            let encode = |operand: X86GprOperand| {
                operand
                    .gpr_index()
                    .map(|index| index + u8::from(operand.high_byte) * 4)
            };
            let dst = encode(cmpxchg.dst).ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "invalid high-byte destination".into(),
            })?;
            let src = encode(cmpxchg.src).ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "invalid high-byte source".into(),
            })?;

            if cmpxchg.flags == FlagUpdate::All {
                // Publish AL-DST explicitly around the exact transition.
                self.code.emit_bytes(&[0x3A, 0xC0 | dst]); // cmp al,r/m8
            }
            self.code.emit_u8(0x9C); // save specified or incoming flags
            self.code.emit_bytes(&[0x0F, 0xB0, 0xC0 | (src << 3) | dst]);
            self.code.emit_u8(0x9D);
            return Ok(());
        }

        let dst = self.get_reg(cmpxchg.dst.vreg())?;
        let src = self.get_reg(cmpxchg.src.vreg())?;
        if cmpxchg.flags == FlagUpdate::All {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_cmp_rr(PhysReg::Rax, dst, cmpxchg.width);
        }
        // CMPXCHG consumes no flags. Preserve either the explicit
        // accumulator-minus-destination image or the incoming image around
        // hosts whose CMPXCHG flag direction is not architectural.
        self.code.emit_u8(0x9C); // pushfq
        self.emit_cmpxchg_registers(cmpxchg.width, dst, src);
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    pub(crate) fn lower_x86_cmpxchg(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        let OpKind::X86Cmpxchg(cmpxchg) = &op.kind else {
            return Err(LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: "wrong operation kind".into(),
            });
        };
        if !x86_cmpxchg_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86Cmpxchg".into(),
                operand: format!("invalid register/lane/width/flags shape: {cmpxchg:?}"),
            });
        }
        if cmpxchg_needs_state_snapshot(*cmpxchg) {
            self.lower_state_backed_cmpxchg(*cmpxchg)
        } else {
            self.lower_direct_cmpxchg(*cmpxchg)
        }
    }

    /// Lower the fused memory `CMPXCHG`.
    ///
    /// Layout of the flag-neutral caller frame:
    ///   `[rsp+0]`  zero-extended memory operand written by the load helper
    ///   `[rsp+8]`  staged replacement value
    ///   `[rsp+24]` complete architectural RAX
    ///
    /// The architectural flags come from a single `CMP` against the staged
    /// memory operand; the helper call on the matching path preserves them, and
    /// every other instruction in the sequence is `MOV`/`LEA`/`Jcc`. The
    /// accumulator write-back is a branch rather than `CMOVcc` because SMIR's
    /// `CMove` writes only when the condition holds, whereas a 32-bit host
    /// `CMOVcc` would zero-extend the destination unconditionally.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_cmpxchg(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_cmpxchg_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let width = sequence.width;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            emitter.emit_mov_mr(PhysReg::Rsp, 24, PhysReg::Rax, OpWidth::W64);
        }
        // Stage the replacement value while every guest register is still live.
        match sequence.source {
            X86JitScalarValue::Register(source) => {
                let source = self.get_reg(source)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rsp, 8, source, OpWidth::W64);
            }
            X86JitScalarValue::Immediate(value) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(PhysReg::Rax, value, OpWidth::W64);
                emitter.emit_mov_mr(PhysReg::Rsp, 8, PhysReg::Rax, OpWidth::W64);
                emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
            }
        }

        self.emit_jit_mem_op(
            sequence.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            sequence.addr,
            sequence.mem_width,
            SignExtend::Zero,
            32,
        )?;

        // Publish the architectural comparison. RAX carries the accumulator
        // value; its guest content is restored below without touching flags.
        match sequence.accumulator {
            X86JitScalarValue::Register(accumulator) => {
                let accumulator = self.get_reg(accumulator)?;
                if accumulator != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(PhysReg::Rax, accumulator, OpWidth::W64);
                }
            }
            X86JitScalarValue::Immediate(value) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(PhysReg::Rax, value, OpWidth::W64);
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mem_disp(
                0x38,
                PhysReg::Rax,
                PhysReg::Rsp,
                0,
                DispSize::Auto,
                width,
                X86AluEncoding::RegRm,
            );
            // Flag-neutral restore of the architectural accumulator.
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
        }

        let mismatch = self.emit_jcc_placeholder(X86Cond::Ne);
        self.emit_jit_mem_op(
            sequence.guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(24),
            sequence.addr,
            sequence.mem_width,
            SignExtend::Zero,
            32,
        )?;
        self.code.emit_u8(0xE9); // jmp .done
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(mismatch)?;
        if sequence.writes_accumulator {
            // Architecturally the accumulator takes the memory operand only on
            // a mismatch, with ordinary partial-register write semantics.
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 0, width);
        }
        self.patch_rel32_to_current(done)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(Some(sequence.consumed))
    }
}
