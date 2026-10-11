//! Width-aware scalar operand transforms.

use crate::smir::ir::context::SmirContext;
use crate::smir::ir::types::{OpWidth, ShiftOp, SrcOperand};

use super::SmirInterpreter;

impl SmirInterpreter {
    /// Apply scalar shifts within the operation width. In particular, W-form
    /// right shifts and rotations cannot observe an X register's upper half.
    pub(crate) fn read_src_operand_width(
        &self,
        ctx: &SmirContext,
        src: &SrcOperand,
        width: OpWidth,
    ) -> u64 {
        let SrcOperand::Shifted { reg, shift, amount } = src else {
            return self.read_src_operand(ctx, src) & width.mask();
        };
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return self.read_src_operand(ctx, src) & width.mask();
        }

        let value = ctx.read_vreg(*reg) & width.mask();
        let amount = u32::from(*amount);
        let result = match shift {
            ShiftOp::Lsl => value.checked_shl(amount).unwrap_or(0),
            ShiftOp::Lsr => value.checked_shr(amount).unwrap_or(0),
            ShiftOp::Asr => {
                let signed = self.sign_extend(value, width) as i64;
                signed.checked_shr(amount).unwrap_or(signed >> 63) as u64
            }
            ShiftOp::Ror => {
                let rotation = amount % width.bits();
                let reverse = (width.bits() - rotation) % width.bits();
                (value >> rotation) | (value << reverse)
            }
            ShiftOp::Rrx => return self.read_src_operand(ctx, src) & width.mask(),
        };
        result & width.mask()
    }
}
