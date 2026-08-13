//! Shift and rotate op execution

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
    pub(crate) fn execute_op_shift(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // SHIFTS AND ROTATES
            // ==================================================================
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    0
                } else {
                    (val << amt) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Shl,
                            result,
                            left: val,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                    // RAX assigns the architecturally undefined `/6` SAL AF
                    // output deterministically: every nonzero masked count
                    // clears it, while count zero leaves all flags unchanged.
                    if matches!(x86_hint, Some(X86OpHint::ShiftGroup6))
                        && flags.as_set().contains(FlagSet::AF)
                    {
                        ctx.flags.materialized.af = false;
                    }
                }
            }

            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    0
                } else {
                    (val >> amt) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Shr,
                            result,
                            left: val,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                }
            }

            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                // Mask to the operand width BEFORE sign-extending, or stale upper
                // register bits leak into both the shifted-out bits and the sign.
                let val = self.sign_extend(ctx.read_vreg(*src) & width.mask(), *width);
                let count_mask = Self::scalar_shift_count_mask(ctx.source_arch, *width);
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let result = if amt >= width.bits() as u64 {
                    if (val as i64) < 0 { width.mask() } else { 0 }
                } else {
                    ((val as i64 >> amt) as u64) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // A masked shift count of 0 leaves all status flags unchanged.
                if amt != 0 {
                    ctx.flags.set_lazy_with_update(
                        LazyFlags {
                            op: LazyFlagOp::Sar,
                            result,
                            left: val as u64,
                            right: amt,
                            width: *width,
                            high: 0,
                        },
                        *flags,
                    );
                }
            }

            OpKind::Shld {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let left = ctx.read_vreg(*dst) & width.mask();
                let right = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    left
                } else {
                    ((left << amt) | (right >> (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // The deterministic no-op cases (zero or a masked subword count above
                // the operand width) preserve flags; otherwise CF is the last bit out
                // of the destination's top.
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Shld,
                        result,
                        left,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Shrd {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let left = ctx.read_vreg(*dst) & width.mask();
                let right = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    left
                } else {
                    ((left >> amt) | (right << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // The deterministic no-op cases (zero or a masked subword count above
                // the operand width) preserve flags; otherwise CF is the last bit out
                // of the destination's bottom.
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Shrd,
                        result,
                        left,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::X86NddDoubleShift {
                dst,
                base,
                fill,
                amount,
                width,
                left,
                flags,
            } => {
                let base = ctx.read_vreg(*base) & width.mask();
                let fill = ctx.read_vreg(*fill) & width.mask();
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let amt = self.read_src_operand(ctx, amount) & count_mask;
                let defined = amt != 0 && amt <= bits;
                let result = if !defined {
                    base
                } else if *left {
                    ((base << amt) | (fill >> (bits - amt))) & width.mask()
                } else {
                    ((base >> amt) | (fill << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);
                if defined && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: if *left {
                            LazyFlagOp::Shld
                        } else {
                            LazyFlagOp::Shrd
                        },
                        result,
                        left: base,
                        right: amt,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                // x86 masks the count to 5 bits (6 for 64-bit); the rotation
                // amount is that masked count mod the width.
                let cmask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = self.read_src_operand(ctx, amount) & cmask;
                let amt = masked % bits;
                let result = if amt == 0 {
                    val
                } else {
                    ((val << amt) | (val >> (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // CF/OF update iff the MASKED count != 0 — even when the rotation
                // amount (masked mod width) is 0, e.g. ROL r16 by 16. `right`
                // carries the masked count so OF keys on masked==1.
                if masked != 0 && flags.updates_any() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rotate,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let bits = width.bits() as u64;
                let cmask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = self.read_src_operand(ctx, amount) & cmask;
                let amt = masked % bits;
                let result = if amt == 0 {
                    val
                } else {
                    ((val >> amt) | (val << (bits - amt))) & width.mask()
                };

                Self::write_gpr(ctx, *dst, result, *width);

                // CF/OF update iff the MASKED count != 0 (see Rol).
                if masked != 0 && flags.updates_any() {
                    ctx.flags.materialize_all();
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Ror,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: 0,
                    });
                }
            }

            OpKind::ArmRegShift {
                dst,
                src,
                amount,
                shift,
                width,
                flags,
            } => {
                use crate::isa::arm::aarch32::cpu::shift_c;
                use crate::isa::arm::decoder::ShiftType;

                debug_assert_eq!(*width, OpWidth::W32);
                let value = ctx.read_vreg(*src) as u32;
                let count = (self.read_src_operand(ctx, amount) & 0xff) as u32;
                ctx.flags.materialize_all();
                let carry_in = ctx.flags.materialized.cf;
                let shift_type = match shift {
                    crate::smir::ir::types::ShiftOp::Lsl => ShiftType::LSL,
                    crate::smir::ir::types::ShiftOp::Lsr => ShiftType::LSR,
                    crate::smir::ir::types::ShiftOp::Asr => ShiftType::ASR,
                    crate::smir::ir::types::ShiftOp::Ror => ShiftType::ROR,
                    crate::smir::ir::types::ShiftOp::Rrx => ShiftType::RRX,
                };
                let (result, carry) = shift_c(value, shift_type, count, carry_in);
                Self::write_gpr(ctx, *dst, u64::from(result), OpWidth::W32);

                let updated = flags.as_set();
                if updated.contains(FlagSet::SF) {
                    ctx.flags.materialized.sf = result & 0x8000_0000 != 0;
                }
                if updated.contains(FlagSet::ZF) {
                    ctx.flags.materialized.zf = result == 0;
                }
                if updated.contains(FlagSet::CF) {
                    ctx.flags.materialized.cf = carry;
                }
            }

            OpKind::ArmDpRegShift {
                kind,
                dst,
                rn,
                rm,
                rs,
                shift,
                flags,
            } => {
                use crate::isa::arm::aarch32::cpu::{add_with_carry, shift_c};
                use crate::isa::arm::decoder::ShiftType;
                use crate::smir::ir::ops::ArmDpRegShiftKind;

                ctx.flags.materialize_all();
                let carry_in = ctx.flags.materialized.cf;
                let value = ctx.read_vreg(*rm) as u32;
                let count = (ctx.read_vreg(*rs) & 0xff) as u32;
                let shift_type = match shift {
                    crate::smir::ir::types::ShiftOp::Lsl => ShiftType::LSL,
                    crate::smir::ir::types::ShiftOp::Lsr => ShiftType::LSR,
                    crate::smir::ir::types::ShiftOp::Asr => ShiftType::ASR,
                    crate::smir::ir::types::ShiftOp::Ror => ShiftType::ROR,
                    crate::smir::ir::types::ShiftOp::Rrx => ShiftType::RRX,
                };
                let (shifted, shifter_carry) = shift_c(value, shift_type, count, carry_in);
                let lhs = rn.map(|reg| ctx.read_vreg(reg) as u32).unwrap_or(0);

                let (result, arithmetic_flags) = match kind {
                    ArmDpRegShiftKind::And | ArmDpRegShiftKind::Tst => (lhs & shifted, None),
                    ArmDpRegShiftKind::Eor | ArmDpRegShiftKind::Teq => (lhs ^ shifted, None),
                    ArmDpRegShiftKind::Orr => (lhs | shifted, None),
                    ArmDpRegShiftKind::Mov => (shifted, None),
                    ArmDpRegShiftKind::Bic => (lhs & !shifted, None),
                    ArmDpRegShiftKind::Mvn => (!shifted, None),
                    ArmDpRegShiftKind::Sub | ArmDpRegShiftKind::Cmp => {
                        let (result, carry, overflow) = add_with_carry(lhs, !shifted, 1);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Rsb => {
                        let (result, carry, overflow) = add_with_carry(shifted, !lhs, 1);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Add | ArmDpRegShiftKind::Cmn => {
                        let (result, carry, overflow) = add_with_carry(lhs, shifted, 0);
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Adc => {
                        let (result, carry, overflow) =
                            add_with_carry(lhs, shifted, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Sbc => {
                        let (result, carry, overflow) =
                            add_with_carry(lhs, !shifted, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                    ArmDpRegShiftKind::Rsc => {
                        let (result, carry, overflow) =
                            add_with_carry(shifted, !lhs, u32::from(carry_in));
                        (result, Some((carry, overflow)))
                    }
                };

                if let Some(dst) = dst {
                    Self::write_gpr(ctx, *dst, u64::from(result), OpWidth::W32);
                }

                let updated = flags.as_set();
                if updated.contains(FlagSet::SF) {
                    ctx.flags.materialized.sf = result & 0x8000_0000 != 0;
                }
                if updated.contains(FlagSet::ZF) {
                    ctx.flags.materialized.zf = result == 0;
                }
                if updated.contains(FlagSet::CF) {
                    ctx.flags.materialized.cf = arithmetic_flags
                        .map(|(carry, _)| carry)
                        .unwrap_or(shifter_carry);
                }
                if updated.contains(FlagSet::OF) {
                    debug_assert!(arithmetic_flags.is_some());
                    if let Some((_, overflow)) = arithmetic_flags {
                        ctx.flags.materialized.of = overflow;
                    }
                }
            }

            OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count = self.read_src_operand(ctx, amount);
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = count & count_mask;
                ctx.flags.materialize_all();
                let (result, carry, effective) =
                    Self::x86_rcl(val, count, ctx.flags.materialized.cf, *width);

                Self::write_gpr(ctx, *dst, result, *width);

                if effective != 0 && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rcl,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: u64::from(carry),
                    });
                }
            }

            OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                let val = ctx.read_vreg(*src) & width.mask();
                let count = self.read_src_operand(ctx, amount);
                let bits = width.bits() as u64;
                let count_mask = if bits == 64 { 0x3F } else { 0x1F };
                let masked = count & count_mask;
                ctx.flags.materialize_all();
                let (result, carry, effective) =
                    Self::x86_rcr(val, count, ctx.flags.materialized.cf, *width);

                Self::write_gpr(ctx, *dst, result, *width);

                if effective != 0 && flags.updates_any() {
                    ctx.flags.lazy = Some(LazyFlags {
                        op: LazyFlagOp::Rcr,
                        result,
                        left: val,
                        right: masked,
                        width: *width,
                        high: u64::from(carry),
                    });
                }
            }

            // Hexagon bidirectional register-amount shift (S2_{asl,asr,lsr,lsl}
            // _r_r and the pair forms via a W64 temp). The count is the sign-
            // extension of the low 7 bits of `amount` to [-64, 63]; a negative
            // count reverses the shift direction. All arithmetic is performed in
            // i128/u128 with the spec's two-step `>> (n-1) >> 1` / `<< (n-1) << 1`
            // idiom so a `|count| == 64` shift never triggers Rust shift overflow.
            OpKind::BidirShift {
                dst,
                src,
                amount,
                kind,
                width,
            } => {
                let bits = width.bits();
                let raw = self.read_src_operand(ctx, src) & width.mask();
                // sxtn7(amount): sign-extend the low 7 bits to [-64, 63].
                let cnt = {
                    let low7 = (self.read_src_operand(ctx, amount) & 0x7f) as i64;
                    ((low7 << 57) >> 57) as i64
                };
                let result: u64 = match kind {
                    // arithmetic left (asl): + shifts left, - shifts (arith)right.
                    0 => {
                        let s = Self::sext128(raw as u128, bits);
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (s >> n) >> 1
                        } else {
                            s << (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // arithmetic right (asr): + shifts (arith)right, - shifts left.
                    1 => {
                        let s = Self::sext128(raw as u128, bits);
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (s << n) << 1
                        } else {
                            s >> (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // logical left (lsl): + shifts left, - shifts (logical)right.
                    2 => {
                        let u = raw as u128;
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (u >> n) >> 1
                        } else {
                            u << (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                    // logical right (lsr): + shifts (logical)right, - shifts left.
                    _ => {
                        let u = raw as u128;
                        let r = if cnt < 0 {
                            let n = (-cnt) as u32 - 1;
                            (u << n) << 1
                        } else {
                            u >> (cnt as u32)
                        };
                        r as u64 & width.mask()
                    }
                };
                Self::write_gpr(ctx, *dst, result, *width);
            }

            // Hexagon saturating clamp (`fSATN`/`fSATUN`) with the USR:OVF sticky
            // overflow bit. The source temp is read and sign-extended from the
            // operation `width` (the lifter feeds an already-sign-extended wide
            // value), clamped to a `sat_bits` signed/unsigned range, and the
            // (truncated) result stored. When the value was actually clamped and
            // `set_ovf` is set, USR bit 0 is OR-ed in (sticky, other bits kept).
            OpKind::SatN {
                dst,
                src,
                sat_bits,
                signed,
                set_ovf,
                width,
            } => {
                // Read the source and sign-extend from `width` to a full i64 so
                // the clamp compares signed magnitudes correctly.
                let raw = self.read_src_operand(ctx, src);
                let val = Self::sext128(raw as u128, width.bits()) as i64;
                let n = *sat_bits as u32;
                let (lo, hi) = if *signed {
                    (-(1i64 << (n - 1)), (1i64 << (n - 1)) - 1)
                } else {
                    (0i64, (1i64 << n) - 1)
                };
                let (clamped, ovf) = if val < lo {
                    (lo, true)
                } else if val > hi {
                    (hi, true)
                } else {
                    (val, false)
                };
                if ovf && *set_ovf {
                    Self::set_hex_ovf(ctx);
                }
                // Store the clamped value's low `width` bits (two's-complement
                // low bits for a negative signed-clamp result).
                Self::write_gpr(ctx, *dst, (clamped as u64) & width.mask(), *width);
            }

            // Carry-less (GF(2)) polynomial multiply — Hexagon
            // `pmpyw`/`vpmpyh` (+ `_acc`) and x86 PCLMULQDQ.
            OpKind::ClMul {
                dst,
                dst_hi,
                src1,
                src2,
                elem_bits,
                lanes,
                acc,
            } => {
                // Carry-less product of two `bits`-wide operands: XOR-accumulate
                // of the shifted partial products (no carries; sign irrelevant).
                #[inline]
                pub(crate) fn clmul(a: u64, b: u64, bits: u32) -> u128 {
                    let mut prod: u128 = 0;
                    for k in 0..bits {
                        if (b >> k) & 1 == 1 {
                            prod ^= u128::from(a) << k;
                        }
                    }
                    prod
                }
                let a = self.read_src_operand(ctx, src1);
                let b = self.read_src_operand(ctx, src2);
                let bits = *elem_bits as u32;
                let elem_mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let result_mask = if *lanes == 1 {
                    elem_mask
                } else {
                    u64::from(u32::MAX)
                };
                let (mut lo, mut hi): (u64, u64) = if *lanes == 1 {
                    // One product split at the element boundary: 32x32 for
                    // Hexagon pmpyw, 64x64 for x86 PCLMULQDQ.
                    let p = clmul(a & elem_mask, b & elem_mask, bits);
                    (
                        (p & u128::from(elem_mask)) as u64,
                        ((p >> bits) & u128::from(elem_mask)) as u64,
                    )
                } else {
                    // vpmpyh: two 16x16 -> 32-bit products, interleaved:
                    //   lo.h0=p0.lo, lo.h1=p1.lo, hi.h0=p0.hi, hi.h1=p1.hi.
                    let x0 = a & 0xffff;
                    let x1 = (a >> 16) & 0xffff;
                    let y0 = b & 0xffff;
                    let y1 = (b >> 16) & 0xffff;
                    let p0 = (clmul(x0, y0, bits) & 0xffff_ffff) as u64;
                    let p1 = (clmul(x1, y1, bits) & 0xffff_ffff) as u64;
                    let lo = (p0 & 0xffff) | ((p1 & 0xffff) << 16);
                    let hi = ((p0 >> 16) & 0xffff) | (((p1 >> 16) & 0xffff) << 16);
                    (lo, hi)
                };
                if *acc {
                    lo ^= ctx.read_vreg(*dst) & result_mask;
                    if let Some(h) = dst_hi {
                        hi ^= ctx.read_vreg(*h) & result_mask;
                    }
                }
                let width = if bits == 64 && *lanes == 1 {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                };
                Self::write_gpr(ctx, *dst, lo & result_mask, width);
                if let Some(h) = dst_hi {
                    Self::write_gpr(ctx, *h, hi & result_mask, width);
                }
            }

            OpKind::Crc32C {
                dst,
                crc,
                data,
                data_width,
            } => {
                // Reflected Castagnoli recurrence. Register byte 0 is consumed
                // first, matching x86's little-endian source interpretation.
                const POLY_REFLECTED: u32 = 0x82F6_3B78;
                let mut value = ctx.read_vreg(*crc) as u32;
                let input = ctx.read_vreg(*data);
                for byte in 0..(data_width.bits() / 8) {
                    value ^= ((input >> (byte * 8)) & 0xFF) as u32;
                    for _ in 0..8 {
                        value = (value >> 1) ^ (POLY_REFLECTED & 0u32.wrapping_sub(value & 1));
                    }
                }
                // Both r32 and r64 instruction forms architecturally clear the
                // destination's high 32 bits.
                Self::write_gpr(ctx, *dst, u64::from(value), OpWidth::W64);
            }

            // `M7_wcmpy*` — 32x32 wide complex multiply with an i128 accumulator,
            // `:<<1` scale (>>31), optional `:rnd`, and signed-32 saturation.
            OpKind::CmpyW128Sat {
                dst,
                rss_lo,
                rss_hi,
                rtt_lo,
                rtt_hi,
                w0,
                w1,
                w2,
                w3,
                add,
                rnd,
            } => {
                // Reconstruct the two register pairs (even = low word, odd = high
                // word) and select a signed 32-bit word from each.
                let rss = (ctx.read_vreg(*rss_lo) & 0xffff_ffff)
                    | ((ctx.read_vreg(*rss_hi) & 0xffff_ffff) << 32);
                let rtt = (ctx.read_vreg(*rtt_lo) & 0xffff_ffff)
                    | ((ctx.read_vreg(*rtt_hi) & 0xffff_ffff) << 32);
                #[inline]
                pub(crate) fn word(src: u64, n: u8) -> i128 {
                    ((src >> (n as u32 * 32)) as u32 as i32) as i128
                }
                let term0 = word(rss, *w0) * word(rtt, *w1);
                let term1 = word(rss, *w2) * word(rtt, *w3);
                let mut accv: i128 = if *add { term0 + term1 } else { term0 - term1 };
                if *rnd {
                    accv += 0x4000_0000i128;
                }
                let shifted = accv >> 31; // arithmetic shift of the signed accumulator
                // Saturate to signed 32 bits with the sticky USR:OVF bit.
                let lo = i32::MIN as i128;
                let hi = i32::MAX as i128;
                let (clamped, ovf) = if shifted < lo {
                    (lo, true)
                } else if shifted > hi {
                    (hi, true)
                } else {
                    (shifted, false)
                };
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
                Self::write_gpr(
                    ctx,
                    *dst,
                    (clamped as i64 as u64) & 0xffff_ffff,
                    OpWidth::W32,
                );
            }

            // `S2_asl_r_r_sat` / `S2_asr_r_r_sat` — register-amount saturating
            // shift implementing `fSAT_ORIG_SHL` (port of sem/shift.rs).
            OpKind::SatOrigShl {
                dst,
                src,
                amount,
                right,
                width,
            } => {
                let src_v = self.read_src_operand(ctx, src) as u32;
                // shamt = fSXTN(7,32, amount): sign-extend the low 7 bits to i32.
                let raw = self.read_src_operand(ctx, amount) as u32;
                let sh = ((raw as i32) << 25) >> 25;
                let orig_i = src_v as i32 as i64;

                // fSAT_ORIG_SHL(a, orig): saturate `a` to s32 honoring orig's
                // sign. NOTE: the sem's `ctx.sat_n(a, 32)` ALSO sets USR:OVF
                // whenever it clamps (a < INT_MIN or a > INT_MAX), independent of
                // the sign-flip / special cases below — so OVF is set on any
                // clamp, then again (idempotently) on a sign flip / orig>0&&a==0.
                #[inline]
                pub(crate) fn sat_orig_shl(ctx: &mut SmirContext, a: i64, orig: u32) -> u32 {
                    let orig_s = orig as i32;
                    // sat_n(a, 32): clamp to [INT_MIN, INT_MAX], setting OVF on clamp.
                    let sat = if a < i32::MIN as i64 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MIN
                    } else if a > i32::MAX as i64 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MAX
                    } else {
                        a as i32
                    };
                    if (sat ^ orig_s) < 0 {
                        // sign flipped -> saturate toward ORIG's extreme
                        let v = if orig_s < 0 { i32::MIN } else { i32::MAX };
                        SmirInterpreter::set_hex_ovf(ctx);
                        v as u32
                    } else if orig_s > 0 && a == 0 {
                        SmirInterpreter::set_hex_ovf(ctx);
                        i32::MAX as u32
                    } else {
                        sat as u32
                    }
                }

                let result: u32 = if !*right {
                    // asl_r_r_sat: positive count = left (saturating).
                    if sh < 0 {
                        // fBIDIR_ASHIFTL with negative amount -> arithmetic right.
                        (((orig_i >> ((-sh) - 1)) >> 1) as i64) as u32
                    } else {
                        let a = orig_i << sh;
                        sat_orig_shl(ctx, a, src_v)
                    }
                } else {
                    // asr_r_r_sat: negative count = left (saturating).
                    if sh < 0 {
                        let a = (orig_i << ((-sh) - 1)) << 1;
                        sat_orig_shl(ctx, a, src_v)
                    } else {
                        ((orig_i >> sh) as i64) as u32
                    }
                };
                Self::write_gpr(ctx, *dst, (result as u64) & width.mask(), *width);
            }

            _ => return self.execute_op_bit(ctx, memory, op),
        }

        Ok(())
    }
}
