//! Exact lowering for the legacy Group-2 `/6` SAL alias.

use super::{ShiftRegOp, X86_64Lowerer, X86Emitter};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{ArchReg, DispSize, OpWidth, SrcOperand, VReg, X86Reg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

const AF_RFLAGS: i64 = 1 << 4;

fn legacy_gpr(reg: &VReg) -> bool {
    matches!(
        reg,
        VReg::Arch(ArchReg::X86(x86))
            if x86.gpr_index().is_some_and(|index| index < 16)
    )
}

/// Validate exactly the register-only shapes emitted for legacy Group-2 `/6`.
///
/// The unoptimized shape is an in-place SHL with the raw immediate byte or
/// architectural CL. A masked-zero count may optimize to an in-place MOV. The
/// global operation whitelist intentionally remains closed because the hint is
/// x86-specific and memory/high-byte graphs require separate provenance-backed
/// paths.
pub(crate) fn x86_shift_group6_shape_valid(op: &SmirOp) -> bool {
    if !matches!(op.x86_hint, Some(X86OpHint::ShiftGroup6)) {
        return false;
    }

    match &op.kind {
        OpKind::Shl {
            dst,
            src,
            amount,
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::All,
        } => {
            dst == src
                && legacy_gpr(dst)
                && match amount {
                    SrcOperand::Imm(value) => (0..=i64::from(u8::MAX)).contains(value),
                    SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::Rcx))) => true,
                    _ => false,
                }
        }
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } => dst == src && legacy_gpr(dst),
        _ => false,
    }
}

impl X86_64Lowerer {
    /// Lower one validated `/6` SHL shape and apply RAX's deterministic policy
    /// for the architecturally undefined AF output: preserve it for a masked
    /// count of zero and clear it for every nonzero masked count.
    pub(crate) fn lower_x86_shift_group6(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_shift_group6_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "legacy Group-2 /6 SAL".to_string(),
                operand: format!("invalid hinted SMIR shape: {:?}", op.kind),
            });
        }
        let OpKind::Shl {
            dst,
            src,
            amount,
            width,
            flags,
        } = &op.kind
        else {
            return Err(LowerError::InvalidOperand {
                op: "legacy Group-2 /6 SAL".to_string(),
                operand: "optimized no-op MOV must use data-movement lowering".to_string(),
            });
        };

        let dynamic_count = matches!(amount, SrcOperand::Reg(_));
        if dynamic_count {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rcx);
        }

        self.lower_state_backed_gpr_shift(*dst, *src, amount, *width, *flags, ShiftRegOp::Shl)?;

        let count_mask: u8 = if *width == OpWidth::W64 { 0x3F } else { 0x1F };
        if dynamic_count {
            // Stack on entry: saved original RCX. Preserve the complete result
            // image while TEST classifies the original CL, including dst=RCX.
            self.code.emit_u8(0x9C); // pushfq
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mi_disp(
                    PhysReg::Rsp,
                    8,
                    DispSize::Auto,
                    i64::from(count_mask),
                    OpWidth::W8,
                );
            }
            let masked_zero = self.emit_jcc_placeholder(super::X86Cond::E);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mi_disp(
                    4,
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    !AF_RFLAGS,
                    OpWidth::W64,
                );
            }
            self.patch_rel32_to_current(masked_zero)?;
            self.code.emit_u8(0x9D); // popfq: corrected or unchanged result image
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard original RCX
        } else if let SrcOperand::Imm(value) = amount {
            if (*value as u8) & count_mask != 0 {
                self.code.emit_u8(0x9C); // pushfq
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_alu_mi_disp(
                        4,
                        PhysReg::Rsp,
                        0,
                        DispSize::Auto,
                        !AF_RFLAGS,
                        OpWidth::W64,
                    );
                }
                self.code.emit_u8(0x9D); // popfq: AF-cleared result image
            }
        }

        Ok(())
    }
}
