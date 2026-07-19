//! SMIR emission helpers for Hexagon addressing and predicates

use crate::smir::lift::hexagon::*;
use std::collections::HashSet;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{HexDfOp, HexFpOp, HexFpRecipKind, OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

// Re-use the existing Hexagon decoder types
use crate::isa::hexagon::decode::{
    AddrMode, CmpKind, DecodedInsn, ExtendKind, MemOpKind, MemOpSrc, MemSign,
    MemWidth as HexMemWidth, ShiftKind,
};
// Direct opcode-level decoding for the ~900 scalar ops that decode to
// `DecodedInsn::Unknown` (handled only by the sem layer in cpu.rs). The lifter
// re-decodes such words via `decode_word` and emits SMIR for the regular
// scalar register ops; see `lift_unknown_op`.
use crate::isa::hexagon::opcode::{DecodedOp, Opcode, decode_word};

impl HexagonLifter {
    pub(crate) fn emit_creg_value_write(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        dst: VReg,
        src: VReg,
    ) {
        let src = if matches!(dst, VReg::Arch(ArchReg::Hexagon(HexagonReg::Gp))) {
            let masked = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(*op_id),
                addr,
                OpKind::And {
                    dst: masked,
                    src1: src,
                    src2: SrcOperand::Imm(0xffff_ffc0u32 as i64),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
            ));
            *op_id += 1;
            masked
        } else {
            src
        };

        ops.push(SmirOp::new(
            OpId(*op_id),
            addr,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W32,
            },
        ));
        *op_id += 1;
    }

    /// Emit `out = brev(src)` (`fbrev`/`fEA_BREVR`): reverse the LOW 16 bits of
    /// `src`, keeping the upper 16 bits intact. Matches `hex_brev` in cpu.rs.
    ///   lo16  = src & 0xffff
    ///   rev32 = reverse_bits32(lo16)   ; the 16 input bits land in bits 16..31
    ///   rev16 = rev32 >> 16            ; reversed value back in the low 16 bits
    ///   out   = (src & 0xffff0000) | rev16
    pub(crate) fn emit_brev(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        src: VReg,
    ) -> VReg {
        let lo16 = ctx.alloc_vreg();
        let rev32 = ctx.alloc_vreg();
        let rev16 = ctx.alloc_vreg();
        let hi = ctx.alloc_vreg();
        let out = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        push(OpKind::And {
            dst: lo16,
            src1: src,
            src2: SrcOperand::Imm(0xffff),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        push(OpKind::Rbit {
            dst: rev32,
            src: lo16,
            width: OpWidth::W32,
        });
        push(OpKind::Shr {
            dst: rev16,
            src: rev32,
            amount: SrcOperand::Imm(16),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        push(OpKind::And {
            dst: hi,
            src1: src,
            src2: SrcOperand::Imm(0xffff_0000u32 as i32 as i64),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        push(OpKind::Or {
            dst: out,
            src1: hi,
            src2: SrcOperand::Reg(rev16),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        out
    }

    /// Combine a Hexagon register PAIR (`R{even}:R{even+1}`) into a fresh 64-bit
    /// VReg: `result = R(even) | (R(even+1) << 32)`. Hexagon GPRs are 32-bit, so
    /// the low half is zero-extended into the W64 value. Returns the temp VReg.
    pub(crate) fn emit_combine_pair(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        even: u8,
    ) -> VReg {
        let hi = ctx.alloc_vreg();
        let out = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        push(OpKind::Shl {
            dst: hi,
            src: self.hex_reg(even + 1),
            amount: SrcOperand::Imm(32),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        });
        push(OpKind::Or {
            dst: out,
            src1: self.hex_reg(even),
            src2: SrcOperand::Reg(hi),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        });
        out
    }

    /// Emit the `spNloop0` loop-config side effects (after SA0/LC0 are written):
    ///   P3 = 0                              (`new_p[3] = Some(0)` in cpu.rs)
    ///   USR = (USR & ~(0x3<<8)) | ((n&0x3)<<8)   (`set_lpcfg(n)`: bits 9:8)
    /// `n` is the loop-config count (1/2/3). The `_cf` harness does not compare
    /// USR, so the LPCFG write is architecturally-faithful but invisible there;
    /// P3 IS compared (and must be 0).
    pub(crate) fn emit_sploop_lpcfg(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        n: u8,
    ) {
        let usr = VReg::Arch(ArchReg::Hexagon(HexagonReg::Usr));
        let cleared = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        // P3 = 0 (full predicate byte clear).
        push(OpKind::Mov {
            dst: self.hex_pred(3),
            src: SrcOperand::Imm(0),
            width: OpWidth::W32,
        });
        // USR &= ~(0x3 << 8)
        push(OpKind::And {
            dst: cleared,
            src1: usr,
            src2: SrcOperand::Imm(!(0x3i64 << 8)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        // USR = cleared | ((n & 0x3) << 8)
        push(OpKind::Or {
            dst: usr,
            src1: cleared,
            src2: SrcOperand::Imm((((n & 0x3) as i64) << 8)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
    }

    /// Emit the circular-buffer post-increment `base = fcirc_add(base, incr, M,
    /// CS)`, writing the result back into the GPR `base` (a `HexagonReg::R`
    /// VReg). `incr` is a VReg holding the (already byte-scaled) signed
    /// increment; `base_old` is a VReg snapshot of the base BEFORE this update
    /// (the EA already used it). Ports `hex_circ_add` in cpu.rs EXACTLY — both
    /// the common K==0/length>=4 branch and the legacy K!=0 branch:
    ///   length  = M & 0x1ffff
    ///   k       = (M >> 24) & 0xf
    ///   new_ptr = base_old + incr
    ///   k0      = (k == 0) && (length >= 4)
    ///   mask    = (1 << (k+2)) - 1
    ///   start   = k0 ? CS : (base_old & !mask)
    ///   end     = k0 ? CS + length : (start | (length & mask))
    ///   result  = new_ptr >= end ? new_ptr - length
    ///             : new_ptr < start ? new_ptr + length
    ///             : new_ptr
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_circ_add(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        base: VReg,
        base_old: VReg,
        modsel: u8,
        incr: VReg,
    ) {
        let m = self.hex_mod(modsel);
        let cs = self.hex_cs(modsel);
        let length = ctx.alloc_vreg();
        let k = ctx.alloc_vreg();
        let new_ptr = ctx.alloc_vreg();
        let k_is_zero = ctx.alloc_vreg();
        let len_ge_4 = ctx.alloc_vreg();
        let k0 = ctx.alloc_vreg();
        let shamt = ctx.alloc_vreg();
        let one_shl = ctx.alloc_vreg();
        let mask = ctx.alloc_vreg();
        let not_mask = ctx.alloc_vreg();
        let start_aligned = ctx.alloc_vreg();
        let len_masked = ctx.alloc_vreg();
        let end_aligned = ctx.alloc_vreg();
        let cs_plus_len = ctx.alloc_vreg();
        let start = ctx.alloc_vreg();
        let end = ctx.alloc_vreg();
        let ge_end = ctx.alloc_vreg();
        let lt_start = ctx.alloc_vreg();
        let minus_len = ctx.alloc_vreg();
        let plus_len = ctx.alloc_vreg();
        let wrapped_lo = ctx.alloc_vreg();
        let result = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        let w = OpWidth::W32;
        // length = M & 0x1ffff
        push(OpKind::And {
            dst: length,
            src1: m,
            src2: SrcOperand::Imm(0x1_ffff),
            width: w,
            flags: FlagUpdate::None,
        });
        // k = (M >> 24) & 0xf
        push(OpKind::Shr {
            dst: k,
            src: m,
            amount: SrcOperand::Imm(24),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::And {
            dst: k,
            src1: k,
            src2: SrcOperand::Imm(0xf),
            width: w,
            flags: FlagUpdate::None,
        });
        // new_ptr = base_old + incr
        push(OpKind::Add {
            dst: new_ptr,
            src1: base_old,
            src2: SrcOperand::Reg(incr),
            width: w,
            flags: FlagUpdate::None,
        });
        // k0 = (k == 0) & (length >= 4)
        push(OpKind::Cmp {
            src1: k,
            src2: SrcOperand::Imm(0),
            width: w,
        });
        push(OpKind::SetCC {
            dst: k_is_zero,
            cond: Condition::Eq,
            width: w,
        });
        push(OpKind::Cmp {
            src1: length,
            src2: SrcOperand::Imm(4),
            width: w,
        });
        push(OpKind::SetCC {
            dst: len_ge_4,
            cond: Condition::Uge,
            width: w,
        });
        push(OpKind::And {
            dst: k0,
            src1: k_is_zero,
            src2: SrcOperand::Reg(len_ge_4),
            width: w,
            flags: FlagUpdate::None,
        });
        // mask = (1 << (k+2)) - 1
        push(OpKind::Add {
            dst: shamt,
            src1: k,
            src2: SrcOperand::Imm(2),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Mov {
            dst: one_shl,
            src: SrcOperand::Imm(1),
            width: w,
        });
        push(OpKind::Shl {
            dst: one_shl,
            src: one_shl,
            amount: SrcOperand::Reg(shamt),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Sub {
            dst: mask,
            src1: one_shl,
            src2: SrcOperand::Imm(1),
            width: w,
            flags: FlagUpdate::None,
        });
        // not_mask = !mask
        push(OpKind::Xor {
            dst: not_mask,
            src1: mask,
            src2: SrcOperand::Imm(-1),
            width: w,
            flags: FlagUpdate::None,
        });
        // start_aligned = base_old & !mask
        push(OpKind::And {
            dst: start_aligned,
            src1: base_old,
            src2: SrcOperand::Reg(not_mask),
            width: w,
            flags: FlagUpdate::None,
        });
        // len_masked = length & mask ; end_aligned = start_aligned | len_masked
        push(OpKind::And {
            dst: len_masked,
            src1: length,
            src2: SrcOperand::Reg(mask),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Or {
            dst: end_aligned,
            src1: start_aligned,
            src2: SrcOperand::Reg(len_masked),
            width: w,
            flags: FlagUpdate::None,
        });
        // cs_plus_len = CS + length
        push(OpKind::Add {
            dst: cs_plus_len,
            src1: cs,
            src2: SrcOperand::Reg(length),
            width: w,
            flags: FlagUpdate::None,
        });
        // start = k0 ? CS : start_aligned ; end = k0 ? cs_plus_len : end_aligned
        // (Select reads CS via a temp copy so the arch reg isn't a Select operand
        //  width-clobber risk — CS is already W32.)
        push(OpKind::Select {
            dst: start,
            cond: k0,
            src_true: cs,
            src_false: start_aligned,
            width: w,
        });
        push(OpKind::Select {
            dst: end,
            cond: k0,
            src_true: cs_plus_len,
            src_false: end_aligned,
            width: w,
        });
        // ge_end = new_ptr >= end (unsigned)
        push(OpKind::Cmp {
            src1: new_ptr,
            src2: SrcOperand::Reg(end),
            width: w,
        });
        push(OpKind::SetCC {
            dst: ge_end,
            cond: Condition::Uge,
            width: w,
        });
        // lt_start = new_ptr < start (unsigned)
        push(OpKind::Cmp {
            src1: new_ptr,
            src2: SrcOperand::Reg(start),
            width: w,
        });
        push(OpKind::SetCC {
            dst: lt_start,
            cond: Condition::Ult,
            width: w,
        });
        // minus_len = new_ptr - length ; plus_len = new_ptr + length
        push(OpKind::Sub {
            dst: minus_len,
            src1: new_ptr,
            src2: SrcOperand::Reg(length),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Add {
            dst: plus_len,
            src1: new_ptr,
            src2: SrcOperand::Reg(length),
            width: w,
            flags: FlagUpdate::None,
        });
        // wrapped_lo = lt_start ? plus_len : new_ptr
        push(OpKind::Select {
            dst: wrapped_lo,
            cond: lt_start,
            src_true: plus_len,
            src_false: new_ptr,
            width: w,
        });
        // result = ge_end ? minus_len : wrapped_lo   (ge_end checked first)
        push(OpKind::Select {
            dst: result,
            cond: ge_end,
            src_true: minus_len,
            src_false: wrapped_lo,
            width: w,
        });
        // base = result
        push(OpKind::Mov {
            dst: base,
            src: SrcOperand::Reg(result),
            width: w,
        });
    }

    /// Emit `out = read_ireg(M[modsel]) << access_shift` (the `_pcr` increment).
    /// `read_ireg` (`fREAD_IREG`) packs an 11-bit signed value:
    ///   packed = ((M & 0xf0000000) >> 21) | ((M >> 17) & 0x7f)
    ///   ireg   = sign_extend_11(packed)
    ///   out    = ireg << access_shift
    /// Matches `hex_read_ireg` in cpu.rs.
    pub(crate) fn emit_read_ireg_shifted(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        modsel: u8,
        access_shift: u8,
    ) -> VReg {
        let m = self.hex_mod(modsel);
        let hi = ctx.alloc_vreg();
        let lo = ctx.alloc_vreg();
        let packed = ctx.alloc_vreg();
        let ireg = ctx.alloc_vreg();
        let out = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        let w = OpWidth::W32;
        // hi = (M & 0xf0000000) >> 21
        push(OpKind::And {
            dst: hi,
            src1: m,
            src2: SrcOperand::Imm(0xf000_0000u32 as i32 as i64),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Shr {
            dst: hi,
            src: hi,
            amount: SrcOperand::Imm(21),
            width: w,
            flags: FlagUpdate::None,
        });
        // lo = (M >> 17) & 0x7f
        push(OpKind::Shr {
            dst: lo,
            src: m,
            amount: SrcOperand::Imm(17),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::And {
            dst: lo,
            src1: lo,
            src2: SrcOperand::Imm(0x7f),
            width: w,
            flags: FlagUpdate::None,
        });
        // packed = hi | lo
        push(OpKind::Or {
            dst: packed,
            src1: hi,
            src2: SrcOperand::Reg(lo),
            width: w,
            flags: FlagUpdate::None,
        });
        // ireg = sign_extend_11(packed) = (packed << 21) >>(arith) 21
        push(OpKind::Shl {
            dst: ireg,
            src: packed,
            amount: SrcOperand::Imm(21),
            width: w,
            flags: FlagUpdate::None,
        });
        push(OpKind::Sar {
            dst: ireg,
            src: ireg,
            amount: SrcOperand::Imm(21),
            width: w,
            flags: FlagUpdate::None,
        });
        // out = ireg << access_shift
        if access_shift == 0 {
            push(OpKind::Mov {
                dst: out,
                src: SrcOperand::Reg(ireg),
                width: w,
            });
        } else {
            push(OpKind::Shl {
                dst: out,
                src: ireg,
                amount: SrcOperand::Imm(access_shift as i64),
                width: w,
                flags: FlagUpdate::None,
            });
        }
        out
    }

    /// Materialize the old effective address for a post-increment load into a
    /// temporary so later staged writeback cannot change the memory address.
    pub(crate) fn emit_postinc_load_ea(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        am: &AddrMode,
    ) -> Option<Address> {
        let base = match am {
            AddrMode::PostIncImm { base, .. }
            | AddrMode::PostIncReg { base, .. }
            | AddrMode::PostIncCircImm { base, .. }
            | AddrMode::PostIncCircReg { base, .. } => *base,
            AddrMode::PostIncBrev { base, .. } => {
                let ea = self.emit_brev(ops, op_id, addr, ctx, self.hex_reg(*base));
                return Some(Address::Direct(ea));
            }
            _ => return None,
        };

        let ea = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(*op_id),
            addr,
            OpKind::Mov {
                dst: ea,
                src: SrcOperand::Reg(self.hex_reg(base)),
                width: OpWidth::W32,
            },
        ));
        *op_id += 1;
        Some(Address::Direct(ea))
    }

    pub(crate) fn emit_postinc_update_value(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        am: &AddrMode,
    ) -> Option<(u8, VReg)> {
        match am {
            AddrMode::PostIncImm { base, offset } => {
                let offset = ctx.extend_imm(*offset);
                let update = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(*op_id),
                    addr,
                    OpKind::Add {
                        dst: update,
                        src1: self.hex_reg(*base),
                        src2: SrcOperand::Imm(offset as i64),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                ));
                *op_id += 1;
                Some((*base, update))
            }
            AddrMode::PostIncReg { base, modsel } | AddrMode::PostIncBrev { base, modsel } => {
                let update = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(*op_id),
                    addr,
                    OpKind::Add {
                        dst: update,
                        src1: self.hex_reg(*base),
                        src2: SrcOperand::Reg(self.hex_mod(*modsel)),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                ));
                *op_id += 1;
                Some((*base, update))
            }
            AddrMode::PostIncCircImm { base, modsel, incr } => {
                let update = ctx.alloc_vreg();
                let incr_reg = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(*op_id),
                    addr,
                    OpKind::Mov {
                        dst: incr_reg,
                        src: SrcOperand::Imm(*incr as i64),
                        width: OpWidth::W32,
                    },
                ));
                *op_id += 1;
                self.emit_circ_add(
                    ops,
                    op_id,
                    addr,
                    ctx,
                    update,
                    self.hex_reg(*base),
                    *modsel,
                    incr_reg,
                );
                Some((*base, update))
            }
            AddrMode::PostIncCircReg {
                base,
                modsel,
                shift,
            } => {
                let update = ctx.alloc_vreg();
                let incr_reg = self.emit_read_ireg_shifted(ops, op_id, addr, ctx, *modsel, *shift);
                self.emit_circ_add(
                    ops,
                    op_id,
                    addr,
                    ctx,
                    update,
                    self.hex_reg(*base),
                    *modsel,
                    incr_reg,
                );
                Some((*base, update))
            }
            _ => None,
        }
    }

    pub(crate) fn emit_commit_postinc_update(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        staged: Option<(u8, VReg)>,
    ) {
        if let Some((base, update)) = staged {
            ops.push(SmirOp::new(
                OpId(*op_id),
                addr,
                OpKind::Mov {
                    dst: self.hex_reg(base),
                    src: SrcOperand::Reg(update),
                    width: OpWidth::W32,
                },
            ));
            *op_id += 1;
        }
    }

    /// Emit the base-register UPDATE for a modifier / circular / bit-reverse
    /// post-increment load or store. `base` is the GPR index; `am` is the
    /// addressing mode (must be one of the PostInc{Reg,Brev,CircImm,CircReg}
    /// variants). Callers that emit this before a load destination write must
    /// materialize the old EA first; the base still holds its OLD value here.
    pub(crate) fn emit_mod_postinc(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        base: u8,
        am: &AddrMode,
    ) {
        let base_reg = self.hex_reg(base);
        match am {
            // memX(Rx++Mu) / memX(Rx++Mu:brev): Rx += raw M[modsel].
            AddrMode::PostIncReg { modsel, .. } | AddrMode::PostIncBrev { modsel, .. } => {
                let m = self.hex_mod(*modsel);
                ops.push(SmirOp::new(
                    OpId(*op_id),
                    addr,
                    OpKind::Add {
                        dst: base_reg,
                        src1: base_reg,
                        src2: SrcOperand::Reg(m),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                ));
                *op_id += 1;
            }
            // memX(Rx++#imm:circ(Mu)): Rx = circ_add(Rx, imm, M, CS). The (already
            // byte-scaled) immediate increment is materialised into a temp.
            AddrMode::PostIncCircImm { modsel, incr, .. } => {
                let incr_reg = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(*op_id),
                    addr,
                    OpKind::Mov {
                        dst: incr_reg,
                        src: SrcOperand::Imm(*incr as i64),
                        width: OpWidth::W32,
                    },
                ));
                *op_id += 1;
                self.emit_circ_add(ops, op_id, addr, ctx, base_reg, base_reg, *modsel, incr_reg);
            }
            // memX(Rx++I:circ(Mu)): Rx = circ_add(Rx, ireg(M)<<sh, M, CS).
            AddrMode::PostIncCircReg { modsel, shift, .. } => {
                let incr_reg = self.emit_read_ireg_shifted(ops, op_id, addr, ctx, *modsel, *shift);
                self.emit_circ_add(ops, op_id, addr, ctx, base_reg, base_reg, *modsel, incr_reg);
            }
            _ => unreachable!("emit_mod_postinc called on non-postinc-mod addr"),
        }
    }

    /// Emit a fresh vreg holding the BRANCH TRUTH for a predicate condition: the
    /// low bit of `P{pred}` (when `sense`) or its logical inverse (when not).
    /// Used by the conditional branch/jumpr/call lifts so the
    /// `ControlFlow::CondBranchReg`/`Select` consumers read the real value.
    pub(crate) fn emit_pred_truth(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        pred: u8,
        sense: bool,
    ) -> VReg {
        let cond_vreg = ctx.alloc_vreg();
        // The interpreter tests only the LOW BIT of the predicate; mask it so a
        // hardware-width predicate (0x00/0xff) maps to a clean 0/1 truth value.
        let masked = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        push(OpKind::And {
            dst: masked,
            src1: self.hex_pred(pred),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        if sense {
            push(OpKind::Mov {
                dst: cond_vreg,
                src: SrcOperand::Reg(masked),
                width: OpWidth::W32,
            });
        } else {
            // Invert: jump-if-false branches when the predicate is clear.
            push(OpKind::Xor {
                dst: cond_vreg,
                src1: masked,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            });
        }
        cond_vreg
    }

    /// Expand a 0/1 truth vreg to a FULL predicate byte (`f8BITSOF`: 0x00/0xff)
    /// and write it into predicate register `P{pred}`. This matches the Hexagon
    /// scalar-compare predicate byte (all 8 bits set on true). `Neg` turns 0/1
    /// into 0x00/0xffffffff; the `& 0xff` keeps the predicate byte.
    pub(crate) fn emit_pred_full(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        pred: u8,
        truth: VReg,
    ) {
        let neg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(*op_id),
            addr,
            OpKind::Neg {
                dst: neg,
                src: truth,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ));
        *op_id += 1;
        ops.push(SmirOp::new(
            OpId(*op_id),
            addr,
            OpKind::And {
                dst: self.hex_pred(pred),
                src1: neg,
                src2: SrcOperand::Imm(0xff),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ));
        *op_id += 1;
    }

    /// Emit a PREDICATE-GATED post-increment-immediate base update: the base
    /// register `base` advances by `inc` ONLY when `cond` (a 0/1 truth vreg)
    /// holds, else it is left unchanged. Mirrors the predicated-load/store
    /// CANCEL (no base advance on a false predicate). Implemented as a pure
    /// unconditional Add into a fresh `new_base` (no fault) followed by a
    /// `Select(base, cond, new_base, base_old)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_gated_postinc_imm(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        base: u8,
        inc: i64,
        cond: VReg,
    ) {
        let base_reg = self.hex_reg(base);
        let new_base = ctx.alloc_vreg();
        let mut push = |kind: OpKind| {
            ops.push(SmirOp::new(OpId(*op_id), addr, kind));
            *op_id += 1;
        };
        push(OpKind::Add {
            dst: new_base,
            src1: base_reg,
            src2: SrcOperand::Imm(inc),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        // base = cond ? new_base : base (unchanged).
        push(OpKind::Select {
            dst: base_reg,
            cond,
            src_true: new_base,
            src_false: base_reg,
            width: OpWidth::W32,
        });
    }

    /// Emit a fresh vreg holding `src & !0x3` (the hardware target alignment of
    /// indirect branches/calls).
    pub(crate) fn emit_align4(
        &self,
        ops: &mut Vec<SmirOp>,
        op_id: &mut u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
        src: VReg,
    ) -> VReg {
        let masked = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(*op_id),
            addr,
            OpKind::And {
                dst: masked,
                src1: src,
                src2: SrcOperand::Imm(!0x3i64),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ));
        *op_id += 1;
        masked
    }
}
