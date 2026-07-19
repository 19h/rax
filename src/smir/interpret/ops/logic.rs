//! Bitwise logical op execution

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_logic(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // BITWISE LOGICAL
            // ==================================================================
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a | b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a ^ b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                ctx.flags
                    .set_lazy_with_update(LazyFlags::logic(result, *width), *flags);
            }

            OpKind::Not { dst, src, width } => {
                let a = ctx.read_vreg(*src);
                let result = (!a) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Test { src1, src2, width } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & b) & width.mask();

                ctx.flags.set_lazy_logic(result, *width);
            }

            OpKind::AndNot {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = (a & !b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                let lazy = if matches!(flags, FlagUpdate::Specific(_)) {
                    LazyFlags::andn(result, *width)
                } else {
                    LazyFlags::logic(result, *width)
                };
                ctx.flags.set_lazy_with_update(lazy, *flags);
            }

            _ => return self.execute_op_shift(ctx, memory, op),
        }

        Ok(())
    }
}
