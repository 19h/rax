//! Bit-manipulation op execution

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
    pub(crate) fn execute_op_bit(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // BIT MANIPULATION
            // ==================================================================
            OpKind::Bt { src, index, width } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Bts {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val | (1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Btr {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val & !(1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Btc {
                dst,
                src,
                index,
                width,
            } => {
                let val = ctx.read_vreg(*src);
                let idx = self.read_src_operand(ctx, index) & (width.bits() as u64 - 1);
                let result = val ^ (1u64 << idx);

                Self::write_gpr(ctx, *dst, result & width.mask(), *width);

                ctx.flags.lazy = Some(LazyFlags {
                    op: LazyFlagOp::Bt,
                    result: 0,
                    left: val,
                    right: idx,
                    width: *width,
                    high: 0,
                });
            }

            OpKind::Bsf {
                dst,
                src,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    0 // ZF will be set
                } else {
                    val.trailing_zeros() as u64
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // BSF defines only ZF; retain the emulator's deterministic
                    // values for architecturally undefined status flags.
                    ctx.flags.materialize_all();
                    ctx.flags.materialized.zf = val == 0;
                }
            }

            OpKind::Bsr {
                dst,
                src,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    0 // ZF will be set
                } else {
                    (width.bits() - 1 - val.leading_zeros()) as u64
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    // BSR has the same ZF-only architectural flag contract as
                    // BSF. Preserve every other materialized status flag.
                    ctx.flags.materialize_all();
                    ctx.flags.materialized.zf = val == 0;
                }
            }

            OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let control = ctx.read_vreg(*control);
                let start = (control & 0xff) as u32;
                let len = ((control >> 8) & 0xff) as u32;
                let bits = width.bits();
                let result = if start >= bits || len == 0 {
                    0
                } else {
                    let shifted = src >> start;
                    if len >= bits {
                        shifted
                    } else {
                        shifted & ((1u64 << len) - 1)
                    }
                };
                let result = result & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_bextr(result, *width);
                }
            }

            OpKind::Bzhi {
                dst,
                src,
                index,
                width,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let index = (ctx.read_vreg(*index) & 0xff) as u32;
                let bits = width.bits();
                let result = if index >= bits {
                    src
                } else {
                    src & ((1u64 << index) - 1)
                };
                let result = result & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.set_lazy_bzhi(u64::from(index), result, *width);
                }
            }

            OpKind::X86Bls {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let result = match kind {
                    X86BlsKind::Blsr => src & src.wrapping_sub(1),
                    X86BlsKind::Blsmsk => src ^ src.wrapping_sub(1),
                    X86BlsKind::Blsi => src.wrapping_neg() & src,
                } & width.mask();
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    match kind {
                        X86BlsKind::Blsr => ctx.flags.set_lazy_blsr(src, result, *width),
                        X86BlsKind::Blsmsk => ctx.flags.set_lazy_blsmsk(src, result, *width),
                        X86BlsKind::Blsi => ctx.flags.set_lazy_blsi(src, result, *width),
                    }
                }
            }

            OpKind::X86Adx {
                dst,
                src1,
                src2,
                width,
                kind,
                flags,
            } => {
                let left = ctx.read_vreg(*src1) & width.mask();
                let right = ctx.read_vreg(*src2) & width.mask();
                let carry_in = match kind {
                    X86AdxKind::Adcx => ctx.flags.get_cf(),
                    X86AdxKind::Adox => ctx.flags.get_of(),
                };
                let full = u128::from(left) + u128::from(right) + u128::from(carry_in);
                let result = (full as u64) & width.mask();
                let carry_out = full > u128::from(width.mask());
                Self::write_gpr(ctx, *dst, result, *width);

                if flags.updates_any() {
                    ctx.flags.materialize_all();
                    match kind {
                        X86AdxKind::Adcx => ctx.flags.materialized.cf = carry_out,
                        X86AdxKind::Adox => ctx.flags.materialized.of = carry_out,
                    }
                }
            }

            OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let mask = ctx.read_vreg(*mask) & width.mask();
                let mut result = 0u64;
                let mut src_bit = 0u32;
                for bit in 0..width.bits() {
                    if ((mask >> bit) & 1) != 0 {
                        if ((src >> src_bit) & 1) != 0 {
                            result |= 1u64 << bit;
                        }
                        src_bit += 1;
                    }
                }
                Self::write_gpr(ctx, *dst, result & width.mask(), *width);
            }

            OpKind::Pext {
                dst,
                src,
                mask,
                width,
            } => {
                let src = ctx.read_vreg(*src) & width.mask();
                let mask = ctx.read_vreg(*mask) & width.mask();
                let mut result = 0u64;
                let mut dst_bit = 0u32;
                for bit in 0..width.bits() {
                    if ((mask >> bit) & 1) != 0 {
                        if ((src >> bit) & 1) != 0 {
                            result |= 1u64 << dst_bit;
                        }
                        dst_bit += 1;
                    }
                }
                Self::write_gpr(ctx, *dst, result & width.mask(), *width);
            }

            OpKind::Clz { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let extra_bits = 64 - width.bits();
                let result = (val.leading_zeros() - extra_bits) as u64;
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Ctz { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let result = if val == 0 {
                    width.bits() as u64
                } else {
                    val.trailing_zeros() as u64
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Popcnt { dst, src, width } => {
                let val = ctx.read_vreg(*src) & width.mask();
                Self::write_gpr(ctx, *dst, val.count_ones() as u64, *width);
            }

            OpKind::X86Count {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                // Read before writing so architectural source/destination
                // aliasing remains exact for all three legacy forms.
                let val = ctx.read_vreg(*src) & width.mask();
                let result = match kind {
                    X86CountKind::Popcnt => val.count_ones() as u64,
                    X86CountKind::Tzcnt => {
                        if val == 0 {
                            width.bits() as u64
                        } else {
                            val.trailing_zeros() as u64
                        }
                    }
                    X86CountKind::Lzcnt => {
                        let extra_bits = 64 - width.bits();
                        (val.leading_zeros() - extra_bits) as u64
                    }
                };
                Self::write_gpr(ctx, *dst, result, *width);

                let requested = flags.as_set();
                if !requested.is_empty() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = None;
                    match kind {
                        X86CountKind::Popcnt => {
                            if requested.contains(FlagSet::CF) {
                                ctx.flags.materialized.cf = false;
                            }
                            if requested.contains(FlagSet::ZF) {
                                ctx.flags.materialized.zf = val == 0;
                            }
                            if requested.contains(FlagSet::SF) {
                                ctx.flags.materialized.sf = false;
                            }
                            if requested.contains(FlagSet::OF) {
                                ctx.flags.materialized.of = false;
                            }
                            if requested.contains(FlagSet::PF) {
                                ctx.flags.materialized.pf = false;
                            }
                            if requested.contains(FlagSet::AF) {
                                ctx.flags.materialized.af = false;
                            }
                        }
                        X86CountKind::Tzcnt | X86CountKind::Lzcnt => {
                            if requested.contains(FlagSet::CF) {
                                ctx.flags.materialized.cf = val == 0;
                            }
                            if requested.contains(FlagSet::ZF) {
                                ctx.flags.materialized.zf = result == 0;
                            }
                        }
                    }
                }
            }

            OpKind::Bswap { dst, src, width } => {
                let val = ctx.read_vreg(*src);
                let result = match width {
                    OpWidth::W16 => (val as u16).swap_bytes() as u64,
                    OpWidth::W32 => (val as u32).swap_bytes() as u64,
                    OpWidth::W64 => val.swap_bytes(),
                    _ => val,
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Rbit { dst, src, width } => {
                let val = ctx.read_vreg(*src);
                let result = match width {
                    OpWidth::W32 => (val as u32).reverse_bits() as u64,
                    OpWidth::W64 => val.reverse_bits(),
                    _ => val,
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            OpKind::Bfx {
                dst,
                src,
                lsb,
                width_bits,
                sign_extend,
                op_width,
            } => {
                let val = ctx.read_vreg(*src);
                let mask = (1u64 << *width_bits) - 1;
                let extracted = (val >> *lsb) & mask;

                let result = if *sign_extend && (*width_bits > 0) {
                    let sign_bit = 1u64 << (*width_bits - 1);
                    if (extracted & sign_bit) != 0 {
                        extracted | !mask
                    } else {
                        extracted
                    }
                } else {
                    extracted
                };

                ctx.write_vreg(*dst, result & op_width.mask());
            }

            OpKind::Bfi {
                dst,
                dst_in,
                src,
                lsb,
                width_bits,
                op_width,
            } => {
                let dest_val = ctx.read_vreg(*dst_in);
                let src_val = ctx.read_vreg(*src);
                let mask = ((1u64 << *width_bits) - 1) << *lsb;
                let result = (dest_val & !mask) | ((src_val << *lsb) & mask);
                ctx.write_vreg(*dst, result & op_width.mask());
            }

            _ => return self.execute_op_data_movement(ctx, memory, op),
        }

        Ok(())
    }
}
