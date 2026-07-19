//! Data-movement op execution

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
    pub(crate) fn execute_op_data_movement(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // DATA MOVEMENT
            // ==================================================================
            OpKind::Mov { dst, src, width } => {
                let val = self.read_src_operand(ctx, src);
                Self::write_x86_partial(ctx, *dst, val, *width);
            }

            OpKind::CMove {
                dst,
                src,
                cond,
                width,
            } => {
                if ctx.flags.eval_condition(*cond) {
                    let val = ctx.read_vreg(*src) & width.mask();
                    Self::write_x86_partial(ctx, *dst, val, *width);
                }
            }

            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width,
            } => {
                let cond_val = ctx.read_vreg(*cond);
                let result = if cond_val != 0 {
                    ctx.read_vreg(*src_true)
                } else {
                    ctx.read_vreg(*src_false)
                };
                Self::write_x86_partial(ctx, *dst, result, *width);
            }

            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let raw = ctx.read_vreg(*src);
                let val = if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    (raw >> 8) & from_width.mask()
                } else {
                    raw & from_width.mask()
                };
                Self::write_x86_partial(ctx, *dst, val, *to_width);
            }

            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                let raw = ctx.read_vreg(*src);
                let val = if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    (raw >> 8) & from_width.mask()
                } else {
                    raw & from_width.mask()
                };
                let sign_bit = from_width.sign_bit();
                let extended = if (val & sign_bit) != 0 {
                    val | !from_width.mask()
                } else {
                    val
                };
                Self::write_x86_partial(ctx, *dst, extended, *to_width);
            }

            OpKind::Cwd { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let sign_bit = width.sign_bit();
                let result = if (val & sign_bit) != 0 {
                    width.mask()
                } else {
                    0
                };
                Self::write_x86_partial(ctx, *dst, result, *width);
            }

            OpKind::Truncate {
                dst,
                src,
                from_width: _,
                to_width,
            } => {
                let val = ctx.read_vreg(*src);
                ctx.write_vreg(*dst, val & to_width.mask());
            }

            OpKind::Lea { dst, addr } => {
                let effective_addr = self.compute_address(ctx, addr);
                ctx.write_vreg(*dst, effective_addr);
            }

            OpKind::Xchg { reg1, reg2, width } => {
                let v1 = ctx.read_vreg(*reg1) & width.mask();
                let v2 = ctx.read_vreg(*reg2) & width.mask();
                Self::write_x86_partial(ctx, *reg1, v2, *width);
                Self::write_x86_partial(ctx, *reg2, v1, *width);
            }

            _ => return self.execute_op_memory(ctx, memory, op),
        }

        Ok(())
    }
}
