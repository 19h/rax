//! Integer arithmetic op execution

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86GprOperand, X86OpHint, X86ThreeDNowKind,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_arithmetic(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // INTEGER ARITHMETIC
            // ==================================================================
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_add(b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_add(a, b, result, *width);
                }
            }

            OpKind::X86Xadd(xadd) => {
                let read = |operand: X86GprOperand| {
                    let value = ctx.read_vreg(operand.vreg());
                    if operand.high_byte {
                        (value >> 8) & 0xFF
                    } else {
                        value & xadd.width.mask()
                    }
                };
                let old_dst = read(xadd.dst);
                let old_src = read(xadd.src);
                let sum = old_dst.wrapping_add(old_src) & xadd.width.mask();
                let write = |ctx: &mut SmirContext, operand: X86GprOperand, value: u64| {
                    if operand.high_byte {
                        let old_parent = ctx.read_vreg(operand.vreg());
                        ctx.write_vreg(
                            operand.vreg(),
                            (old_parent & !0xFF00) | ((value & 0xFF) << 8),
                        );
                    } else {
                        Self::write_x86_partial(ctx, operand.vreg(), value, xadd.width);
                    }
                };
                write(ctx, xadd.src, old_dst);
                write(ctx, xadd.dst, sum);
                if xadd.flags.updates_any() {
                    ctx.flags.set_lazy_add(old_dst, old_src, sum, xadd.width);
                }
            }

            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_sub(b) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_sub(a, b, result, *width);
                }
            }

            OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let cf = if ctx.flags.get_cf() { 1u64 } else { 0 };
                let result = a.wrapping_add(b).wrapping_add(cf) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // Original operands + carry-in: CF/AF/OF must account for the
                    // carry (folding cf into `b` loses the carry-out).
                    ctx.flags.set_lazy_adc(a, b, cf, result, *width);
                }
            }

            OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let cf = if ctx.flags.get_cf() { 1u64 } else { 0 };
                let result = a.wrapping_sub(b).wrapping_sub(cf) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_sbb(a, b, cf, result, *width);
                }
            }

            OpKind::Neg {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = (0u64.wrapping_sub(a)) & width.mask();

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Neg,
                        result,
                        left: a,
                        right: 0,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Inc {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = a.wrapping_add(1) & width.mask();
                Self::write_x86_partial(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags::inc(a, result, *width));
                }
            }

            OpKind::Dec {
                dst,
                src,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src);
                let result = a.wrapping_sub(1) & width.mask();
                Self::write_x86_partial(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags::dec(a, result, *width));
                }
            }

            OpKind::Cmp { src1, src2, width } => {
                let a = ctx.read_vreg(*src1);
                let b = self.read_src_operand(ctx, src2);
                let result = a.wrapping_sub(b) & width.mask();

                ctx.flags.set_lazy_sub(a, b, result, *width);
            }

            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = self.read_src_operand(ctx, src2) & width.mask();

                let (result_lo, result_hi) = match width {
                    OpWidth::W8 => {
                        let r = (a as u16) * (b as u16);
                        ((r & 0xFF) as u64, ((r >> 8) & 0xFF) as u64)
                    }
                    OpWidth::W16 => {
                        let r = (a as u32) * (b as u32);
                        ((r & 0xFFFF) as u64, ((r >> 16) & 0xFFFF) as u64)
                    }
                    OpWidth::W32 => {
                        let r = (a as u64) * (b as u64);
                        (r & 0xFFFF_FFFF, (r >> 32) & 0xFFFF_FFFF)
                    }
                    OpWidth::W64 => {
                        let r = (a as u128) * (b as u128);
                        (r as u64, (r >> 64) as u64)
                    }
                    OpWidth::W128 => {
                        // 128-bit multiply not supported
                        (a.wrapping_mul(b), 0)
                    }
                };

                if *width == OpWidth::W8 {
                    // 8-bit MUL: the full 16-bit product lives in AX (AH:AL);
                    // DX is untouched. Merge the 16-bit product into AX.
                    Self::write_gpr(ctx, *dst_lo, result_lo | (result_hi << 8), OpWidth::W16);
                } else {
                    Self::write_gpr(ctx, *dst_lo, result_lo, *width);
                    if let Some(hi) = dst_hi {
                        Self::write_gpr(ctx, *hi, result_hi, *width);
                    }
                }

                ctx.flags.set_lazy_with_update(
                    LazyFlags {
                        op: LazyFlagOp::Mul,
                        result: result_lo,
                        left: a,
                        right: b,
                        width: *width,
                        high: result_hi,
                    },
                    *flags,
                );
            }

            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                let a = self.sign_extend(ctx.read_vreg(*src1), *width);
                let b = self.sign_extend(self.read_src_operand(ctx, src2), *width);

                let (result_lo, result_hi) = match width {
                    OpWidth::W8 => {
                        let r = (a as i8 as i16) * (b as i8 as i16);
                        ((r as u16 & 0xFF) as u64, (((r as u16) >> 8) & 0xFF) as u64)
                    }
                    OpWidth::W16 => {
                        let r = (a as i16 as i32) * (b as i16 as i32);
                        (
                            (r as u32 & 0xFFFF) as u64,
                            (((r as u32) >> 16) & 0xFFFF) as u64,
                        )
                    }
                    OpWidth::W32 => {
                        let r = (a as i32 as i64) * (b as i32 as i64);
                        ((r as u64 & 0xFFFF_FFFF), ((r as u64) >> 32) & 0xFFFF_FFFF)
                    }
                    OpWidth::W64 => {
                        let r = (a as i64 as i128) * (b as i64 as i128);
                        (r as u64, (r >> 64) as u64)
                    }
                    OpWidth::W128 => ((a as i64).wrapping_mul(b as i64) as u64, 0),
                };

                if *width == OpWidth::W8 {
                    // 8-bit IMUL: the full 16-bit product lives in AX (AH:AL);
                    // DX is untouched.
                    Self::write_gpr(ctx, *dst_lo, result_lo | (result_hi << 8), OpWidth::W16);
                } else {
                    Self::write_gpr(ctx, *dst_lo, result_lo, *width);
                    if let Some(hi) = dst_hi {
                        Self::write_gpr(ctx, *hi, result_hi, *width);
                    }
                }

                ctx.flags.set_lazy_with_update(
                    LazyFlags {
                        // Signed: CF/OF iff the product isn't the sign-extension
                        // of the low half (distinct from unsigned Mul's high!=0).
                        op: LazyFlagOp::Imul,
                        result: result_lo,
                        left: a as u64,
                        right: b as u64,
                        width: *width,
                        high: result_hi,
                    },
                    *flags,
                );
            }

            OpKind::MulAdd {
                dst,
                acc,
                src1,
                src2,
                width,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = ctx.read_vreg(*src2) & width.mask();
                let c = ctx.read_vreg(*acc) & width.mask();
                let result = c.wrapping_add(a.wrapping_mul(b)) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::MulSub {
                dst,
                acc,
                src1,
                src2,
                width,
            } => {
                let a = ctx.read_vreg(*src1) & width.mask();
                let b = ctx.read_vreg(*src2) & width.mask();
                let c = ctx.read_vreg(*acc) & width.mask();
                let result = c.wrapping_sub(a.wrapping_mul(b)) & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
                ..
            } => {
                let mask = width.mask();
                let b = (self.read_src_operand(ctx, src2) & mask) as u128;
                if b == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                // x86 DIV divides the double-width RDX:RAX (AX for 8-bit) by the
                // operand; non-x86 contexts have no high half (single-width div).
                let lo = ctx.read_vreg(*src1) & mask;
                let is_x86 = matches!(ctx.arch_regs, ArchRegState::X86_64(_));
                let dividend: u128 = if !is_x86 {
                    lo as u128
                } else if *width == OpWidth::W8 {
                    (ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax)) & 0xFFFF) as u128
                } else {
                    let hi = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rdx)) & mask;
                    ((hi as u128) << width.bits()) | (lo as u128)
                };
                let q = dividend / b;
                let r = dividend % b;
                if q > mask as u128 {
                    // Quotient overflow -> #DE.
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let (q, r) = (q as u64, r as u64);
                if is_x86 && *width == OpWidth::W8 {
                    // 8-bit: quotient -> AL, remainder -> AH.
                    let rax = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax));
                    let new = (rax & !0xFFFF) | ((r & 0xFF) << 8) | (q & 0xFF);
                    ctx.write_arch_reg(ArchReg::X86(X86Reg::Rax), new);
                } else {
                    Self::write_gpr(ctx, *quot, q, *width);
                    if let Some(rem_reg) = rem {
                        Self::write_gpr(ctx, *rem_reg, r, *width);
                    }
                }
            }

            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
                ..
            } => {
                let mask = width.mask();
                let bits = width.bits();
                let b = self.sign_extend(self.read_src_operand(ctx, src2), *width) as i64 as i128;
                if b == 0 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let is_x86 = matches!(ctx.arch_regs, ArchRegState::X86_64(_));
                // Signed double-width dividend: RDX:RAX (AX for 8-bit) on x86.
                let dividend: i128 = if !is_x86 {
                    self.sign_extend(ctx.read_vreg(*src1), *width) as i64 as i128
                } else if *width == OpWidth::W8 {
                    ((ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax)) & 0xFFFF) as u16) as i16 as i128
                } else {
                    let lo = ctx.read_vreg(*src1) & mask;
                    let hi = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rdx)) & mask;
                    let combined = ((hi as u128) << bits) | (lo as u128);
                    Self::sext128(combined, bits * 2)
                };
                let q = dividend.wrapping_div(b);
                let r = dividend.wrapping_rem(b);
                // x86 IDIV raises #DE when the quotient does not fit. Non-x86
                // users of DivS are single-width operations and keep the
                // wrapping MIN / -1 result.
                if is_x86 {
                    let qmax = (1i128 << (bits - 1)) - 1;
                    let qmin = -(1i128 << (bits - 1));
                    if q < qmin || q > qmax {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                }
                let (q, r) = ((q as u64) & mask, (r as u64) & mask);
                if is_x86 && *width == OpWidth::W8 {
                    let rax = ctx.read_arch_reg(ArchReg::X86(X86Reg::Rax));
                    let new = (rax & !0xFFFF) | ((r & 0xFF) << 8) | (q & 0xFF);
                    ctx.write_arch_reg(ArchReg::X86(X86Reg::Rax), new);
                } else {
                    Self::write_gpr(ctx, *quot, q, *width);
                    if let Some(rem_reg) = rem {
                        Self::write_gpr(ctx, *rem_reg, r, *width);
                    }
                }
            }

            _ => return self.execute_op_logic(ctx, memory, op),
        }

        Ok(())
    }
}
