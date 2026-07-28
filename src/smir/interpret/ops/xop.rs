//! AMD XOP packed-vector execution.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp, X86XopPackedBitKind};
use crate::smir::ir::types::{SrcOperand, VecElementType, VecWidth};

impl SmirInterpreter {
    pub(crate) fn execute_op_xop(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86XopPackedBit {
            dst,
            src,
            count,
            elem,
            kind,
        } = &op.kind
        else {
            return self.execute_op_avx10(ctx, memory, op);
        };

        let input = Self::read_vec(ctx, *src);
        let counts = match count {
            SrcOperand::Reg(register) => Some(Self::read_vec(ctx, *register)),
            SrcOperand::Imm(_) => None,
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: op.guest_pc,
                    opcode: 0,
                });
                return Ok(());
            }
        };
        let fixed = match count {
            SrcOperand::Imm(value) if (0..=255).contains(value) => Some(*value as u8),
            SrcOperand::Reg(_) => None,
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: op.guest_pc,
                    opcode: 0,
                });
                return Ok(());
            }
        };
        let bits = elem.bytes() * 8;
        if !matches!(
            elem,
            VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        ) {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }
        let lanes = VecWidth::V128.lanes(*elem) as u8;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        let mut result = [0_u64; 16];
        for lane in 0..lanes {
            let value = Self::get_lane(&input, lane, bits);
            let raw_count = fixed.unwrap_or_else(|| {
                Self::get_lane(
                    counts.as_ref().expect("validated XOP count vector"),
                    lane,
                    bits,
                ) as u8
            });
            let signed_count = raw_count as i8;
            let amount = u32::from(signed_count.unsigned_abs()) & (u32::from(bits) - 1);
            let transformed = match (kind, signed_count.is_negative()) {
                (X86XopPackedBitKind::Rotate, false) => {
                    if bits == 64 {
                        value.rotate_left(amount)
                    } else {
                        ((value << amount)
                            | (value >> ((u32::from(bits) - amount) & (u32::from(bits) - 1))))
                            & mask
                    }
                }
                (X86XopPackedBitKind::Rotate, true) => {
                    if bits == 64 {
                        value.rotate_right(amount)
                    } else {
                        ((value >> amount)
                            | (value << ((u32::from(bits) - amount) & (u32::from(bits) - 1))))
                            & mask
                    }
                }
                (
                    X86XopPackedBitKind::LogicalShift | X86XopPackedBitKind::ArithmeticShift,
                    false,
                ) => (value << amount) & mask,
                (X86XopPackedBitKind::LogicalShift, true) => value >> amount,
                (X86XopPackedBitKind::ArithmeticShift, true) => {
                    let signed = if bits == 64 {
                        value as i64
                    } else {
                        ((value << (64 - bits)) as i64) >> (64 - bits)
                    };
                    ((signed >> amount) as u64) & mask
                }
            };
            Self::set_lane(&mut result, lane, bits, transformed);
        }
        Self::write_vec(ctx, *dst, result);
        Ok(())
    }
}
