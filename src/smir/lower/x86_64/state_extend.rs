//! State-backed x86 GPR sign/zero extension admission and lowering.

use super::*;

pub(crate) fn x86_state_backed_gpr_extend_candidate(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::ZeroExtend { dst, src, .. } | OpKind::SignExtend { dst, src, .. }
            if x86_state_backed_arch_gpr(dst) || x86_state_backed_arch_gpr(src)
    )
}

pub(crate) fn x86_state_backed_gpr_extend_valid(op: &SmirOp) -> bool {
    let gpr_index = |reg: &VReg| match reg {
        VReg::Arch(ArchReg::X86(x86)) => x86.gpr_index(),
        _ => None,
    };
    let state_backed = |index: u8| index >= 16 || matches!(index, 4 | 5);
    let widths_valid = |from: OpWidth, to: OpWidth| {
        matches!(
            (from, to),
            (OpWidth::W8, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                | (OpWidth::W16, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                | (OpWidth::W32, OpWidth::W64)
        )
    };

    let (dst, src, from_width, to_width) = match &op.kind {
        OpKind::ZeroExtend {
            dst,
            src,
            from_width,
            to_width,
        }
        | OpKind::SignExtend {
            dst,
            src,
            from_width,
            to_width,
        } => (dst, src, *from_width, *to_width),
        _ => return false,
    };
    let (Some(dst_index), Some(src_index)) = (gpr_index(dst), gpr_index(src)) else {
        return false;
    };
    if !x86_state_backed_gpr_extend_candidate(op)
        || !widths_valid(from_width, to_width)
        || !(state_backed(dst_index) || state_backed(src_index))
    {
        return false;
    }

    match op.x86_hint {
        None => !(from_width == OpWidth::W8 && matches!(src_index, 4..=7)),
        Some(X86OpHint::RexByteReg) => from_width == OpWidth::W8,
        Some(X86OpHint::LegacyHighByteReg) => {
            src_index <= 3
                && dst_index <= 7
                && from_width == OpWidth::W8
                && matches!(to_width, OpWidth::W16 | OpWidth::W32)
        }
        Some(_) => false,
    }
}

impl X86_64Lowerer {
    pub(crate) fn lower_state_backed_gpr_extend(
        &mut self,
        dst: VReg,
        src: VReg,
        from_width: OpWidth,
        to_width: OpWidth,
        signed: bool,
        legacy_high_byte: bool,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed MOVX".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed MOVX".to_string(),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        if to_width == OpWidth::W16 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                i32::from(dst_idx) * 8,
                OpWidth::W64,
            );
        }

        let source_offset = i32::from(src_idx) * 8 + i32::from(legacy_high_byte);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if from_width == to_width {
                // Same-width sign/zero extension is a bitwise copy. Use the
                // architecturally documented MOV spelling in generated host
                // code instead of relying on 66 0F BF/B7 acceptance.
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, source_offset, from_width);
            } else if signed {
                emitter.emit_movsx_rm_disp(
                    PhysReg::Rdx,
                    PhysReg::Rax,
                    source_offset,
                    DispSize::Auto,
                    from_width,
                    to_width,
                );
            } else {
                emitter.emit_movzx_rm_disp(
                    PhysReg::Rdx,
                    PhysReg::Rax,
                    source_offset,
                    DispSize::Auto,
                    from_width,
                    to_width,
                );
            }
        }

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, to_width)?;
        if dst_idx == 5 {
            let commit_width = if to_width == OpWidth::W16 {
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
