//! ARM scalar NZCV uses the interpreter's normalized no-carry/borrow CF image.

use crate::smir::ir::context::{ArchRegState, SmirContext};
use crate::smir::ir::flags::{FlagSet, MaterializedFlags};
use crate::smir::ir::ops::OpKind;

use super::SmirInterpreter;

impl SmirInterpreter {
    pub(super) fn arm_alu_prior_flags(
        ctx: &mut SmirContext,
        kind: &OpKind,
    ) -> Option<MaterializedFlags> {
        let scalar_alu = matches!(
            kind,
            OpKind::Add { .. }
                | OpKind::Sub { .. }
                | OpKind::Adc { .. }
                | OpKind::Sbb { .. }
                | OpKind::Neg { .. }
                | OpKind::Cmp { .. }
                | OpKind::Cmn { .. }
                | OpKind::And { .. }
                | OpKind::Or { .. }
                | OpKind::Xor { .. }
                | OpKind::AndNot { .. }
                | OpKind::Test { .. }
        );
        if !matches!(ctx.arch_regs, ArchRegState::Aarch64(_))
            || !scalar_alu
            || kind.flags_written().intersection(FlagSet::NZCV).is_empty()
        {
            return None;
        }

        ctx.flags.materialize_all();
        Some(ctx.flags.materialized)
    }

    pub(super) fn commit_arm_alu_flags(
        ctx: &mut SmirContext,
        kind: &OpKind,
        prior: MaterializedFlags,
    ) {
        let written = kind.flags_written();
        ctx.flags.materialize_all();
        let flags = &mut ctx.flags.materialized;

        // FlagState conditions and MaterializedFlags::{from,to}_nzcv store
        // CF as !PSTATE.C. Subtraction already produces borrow; addition and
        // logical kernels produce carry and require inversion at this boundary.
        let produces_carry = matches!(
            kind,
            OpKind::Add { .. }
                | OpKind::Adc { .. }
                | OpKind::Cmn { .. }
                | OpKind::And { .. }
                | OpKind::Or { .. }
                | OpKind::Xor { .. }
                | OpKind::AndNot { .. }
                | OpKind::Test { .. }
        );
        if written.contains(FlagSet::CF) {
            if produces_carry {
                flags.cf = !flags.cf;
            }
        } else {
            flags.cf = prior.cf;
        }
        if !written.contains(FlagSet::ZF) {
            flags.zf = prior.zf;
        }
        if !written.contains(FlagSet::SF) {
            flags.sf = prior.sf;
        }
        if !written.contains(FlagSet::OF) {
            flags.of = prior.of;
        }
        if let ArchRegState::Aarch64(regs) = &mut ctx.arch_regs {
            regs.nzcv = flags.to_nzcv();
        }
    }
}
