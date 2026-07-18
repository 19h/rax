//! bitmanip.rs

use crate::smir::lift::riscv::*;
use crate::isa::riscv::{
    Isa as RvIsa, Op as RvOp, Xlen as RvXlen, decode as rv_decode, rvc::decode_rvc as rv_decode_rvc,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, RvVectorState, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{SmirBlock, SmirFunction};

use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter};

impl RiscVLifter {

    /// Decode-driven lowering of OP-IMM bit-manipulation (Zbb/Zbs immediates
    /// and the unary count/extend/reverse ops). `Orc.b`/`Brev8` (no direct SMIR
    /// op) and crypto remain gaps.
    pub(crate) fn lift_zb_imm(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        use CryptoTerm::*;
        let d = rv_decode(insn, self.rv_xlen(), &self.decoder_isa());
        let rs1 = self.get_x_reg(d.rs1, ctx);
        let shamt = ((insn >> 20) & 0x3F) as i64; // 6-bit (RV64) bit/shift index
        let mut ops = Vec::new();
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let dst = match self.def_x_reg(d.rd, ctx) {
            Some(dst) => dst,
            None => return Ok((ops, ControlFlow::NextInsn)),
        };
        let w = self.op_width();
        let bit = 1i64.wrapping_shl(shamt as u32);

        match d.op {
            RvOp::Rori => ops.push(mk(
                ctx,
                OpKind::Ror {
                    dst,
                    src: rs1,
                    amount: SrcOperand::Imm(shamt),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Bclri => ops.push(mk(
                ctx,
                OpKind::AndNot {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Imm(bit),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Bseti => ops.push(mk(
                ctx,
                OpKind::Or {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Imm(bit),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Binvi => ops.push(mk(
                ctx,
                OpKind::Xor {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Imm(bit),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Bexti => {
                let s = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shr {
                        dst: s,
                        src: rs1,
                        amount: SrcOperand::Imm(shamt),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst,
                        src1: s,
                        src2: SrcOperand::Imm(1),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Clz => ops.push(mk(
                ctx,
                OpKind::Clz {
                    dst,
                    src: rs1,
                    width: w,
                },
            )),
            RvOp::Ctz => ops.push(mk(
                ctx,
                OpKind::Ctz {
                    dst,
                    src: rs1,
                    width: w,
                },
            )),
            RvOp::Cpop => ops.push(mk(
                ctx,
                OpKind::Popcnt {
                    dst,
                    src: rs1,
                    width: w,
                },
            )),
            RvOp::SextB => ops.push(mk(
                ctx,
                OpKind::SignExtend {
                    dst,
                    src: rs1,
                    from_width: OpWidth::W8,
                    to_width: w,
                },
            )),
            RvOp::SextH => ops.push(mk(
                ctx,
                OpKind::SignExtend {
                    dst,
                    src: rs1,
                    from_width: OpWidth::W16,
                    to_width: w,
                },
            )),
            RvOp::Rev8 => ops.push(mk(
                ctx,
                OpKind::Bswap {
                    dst,
                    src: rs1,
                    width: w,
                },
            )),
            // Brev8 (reverse bits within each byte) = bswap(rbit(x)): a full bit
            // reverse moves byte i's bits (reversed) to byte 7-i; the byte swap
            // moves them back, leaving each byte bit-reversed in place.
            RvOp::Brev8 => {
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Rbit {
                        dst: t,
                        src: rs1,
                        width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Bswap {
                        dst,
                        src: t,
                        width: w,
                    },
                ));
            }
            // SHA / SM3: xor-folds of rotates and a logical shift. 32-bit ops
            // (sha256/sm3) sign-extend the W32 result; sha512 are native W64.
            RvOp::Sha256Sig0 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(R, 7), (R, 18), (S, 3)],
                true,
            ),
            RvOp::Sha256Sig1 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(R, 17), (R, 19), (S, 10)],
                true,
            ),
            RvOp::Sha256Sum0 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(R, 2), (R, 13), (R, 22)],
                true,
            ),
            RvOp::Sha256Sum1 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(R, 6), (R, 11), (R, 25)],
                true,
            ),
            RvOp::Sha512Sig0 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(RW, 1), (RW, 8), (SW, 7)],
                false,
            ),
            RvOp::Sha512Sig1 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(RW, 19), (RW, 61), (SW, 6)],
                false,
            ),
            RvOp::Sha512Sum0 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(RW, 28), (RW, 34), (RW, 39)],
                false,
            ),
            RvOp::Sha512Sum1 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(RW, 14), (RW, 18), (RW, 41)],
                false,
            ),
            RvOp::Sm3p0 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(X, 0), (L, 9), (L, 17)],
                true,
            ),
            RvOp::Sm3p1 => self.crypto_xor3(
                ctx,
                &mut ops,
                addr,
                rs1,
                dst,
                &[(X, 0), (L, 15), (L, 23)],
                true,
            ),
            // AES-64 inverse-MixColumns (`aes64im`, unary) and the round-key
            // schedule step (`aes64ks1i`, with round number insn[23:20]) — S-box
            // table ops, computed bit-exactly by the RvIntCrypto op.
            RvOp::Aes64im | RvOp::Aes64ks1i => {
                let imm = match d.op {
                    RvOp::Aes64ks1i => ((insn >> 20) & 0xf) as u8,
                    _ => 0,
                };
                ops.push(mk(
                    ctx,
                    OpKind::RvIntCrypto {
                        dst,
                        src1: rs1,
                        src2: rs1,
                        op: d.op,
                        imm,
                        xlen: self.xlen,
                    },
                ));
            }
            _ => {
                return Err(LiftError::Unsupported {
                    addr,
                    mnemonic: format!("{:?}", d.op),
                });
            }
        }

        Ok((ops, ControlFlow::NextInsn))
    }


    /// Decode-driven lowering of OP-space bit-manipulation / conditional ops
    /// (Zba/Zbb/Zbs/Zicond). Uses the verified RISC-V decoder for the precise
    /// operation; unsupported ops (Zbc carry-less mul, crypto, xperm) return
    /// `Unsupported`.
    pub(crate) fn lift_zb_op(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let d = rv_decode(insn, self.rv_xlen(), &self.decoder_isa());
        let rs1 = self.get_x_reg(d.rs1, ctx);
        let rs2 = self.get_x_reg(d.rs2, ctx);
        let mut ops = Vec::new();
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let dst = match self.def_x_reg(d.rd, ctx) {
            Some(dst) => dst,
            None => return Ok((ops, ControlFlow::NextInsn)), // rd == x0: pure no-op
        };
        let w = self.op_width();

        // Helper: dst = min/max(rs1, rs2) using a compare + select.
        let mut minmax = |ctx: &mut LiftContext, ops: &mut Vec<SmirOp>, cond: Condition| {
            ops.push(mk(
                ctx,
                OpKind::Cmp {
                    src1: rs1,
                    src2: SrcOperand::Reg(rs2),
                    width: w,
                },
            ));
            let c = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::SetCC {
                    dst: c,
                    cond,
                    width: w,
                },
            ));
            ops.push(mk(
                ctx,
                OpKind::Select {
                    dst,
                    cond: c,
                    src_true: rs1,
                    src_false: rs2,
                    width: w,
                },
            ));
        };

        // Helper: shift-add  dst = (rs1 << sh) + rs2  (optionally zext.w rs1 first)
        let mut shadd = |ctx: &mut LiftContext, ops: &mut Vec<SmirOp>, sh: i64, uw: bool| {
            let base = if uw {
                let z = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::ZeroExtend {
                        dst: z,
                        src: rs1,
                        from_width: OpWidth::W32,
                        to_width: w,
                    },
                ));
                z
            } else {
                rs1
            };
            let s = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Shl {
                    dst: s,
                    src: base,
                    amount: SrcOperand::Imm(sh),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(mk(
                ctx,
                OpKind::Add {
                    dst,
                    src1: s,
                    src2: SrcOperand::Reg(rs2),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
        };

        // Helper: single-bit op  bit = 1 << (rs2 & (XLEN-1)); then apply.
        let mut bitop = |ctx: &mut LiftContext, ops: &mut Vec<SmirOp>, which: u8| {
            let one = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Mov {
                    dst: one,
                    src: SrcOperand::Imm(1),
                    width: w,
                },
            ));
            let bit = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Shl {
                    dst: bit,
                    src: one,
                    amount: SrcOperand::Reg(rs2),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            let k = match which {
                0 => OpKind::AndNot {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Reg(bit),
                    width: w,
                    flags: FlagUpdate::None,
                }, // bclr
                1 => OpKind::Or {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Reg(bit),
                    width: w,
                    flags: FlagUpdate::None,
                }, // bset
                _ => OpKind::Xor {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Reg(bit),
                    width: w,
                    flags: FlagUpdate::None,
                }, // binv
            };
            ops.push(mk(ctx, k));
        };

        // Helper: word op into a temp, then sign-extend W32 -> W64.
        let mut wordret =
            |ctx: &mut LiftContext, ops: &mut Vec<SmirOp>, inner: OpKind, tmp: VReg| {
                ops.push(mk(ctx, inner));
                ops.push(mk(
                    ctx,
                    OpKind::SignExtend {
                        dst,
                        src: tmp,
                        from_width: OpWidth::W32,
                        to_width: w,
                    },
                ));
            };

        match d.op {
            RvOp::Andn => ops.push(mk(
                ctx,
                OpKind::AndNot {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Reg(rs2),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Orn => {
                let n = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Not {
                        dst: n,
                        src: rs2,
                        width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst,
                        src1: rs1,
                        src2: SrcOperand::Reg(n),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Xnor => {
                let x = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Xor {
                        dst: x,
                        src1: rs1,
                        src2: SrcOperand::Reg(rs2),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Not {
                        dst,
                        src: x,
                        width: w,
                    },
                ));
            }
            RvOp::Rol => ops.push(mk(
                ctx,
                OpKind::Rol {
                    dst,
                    src: rs1,
                    amount: SrcOperand::Reg(rs2),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Ror => ops.push(mk(
                ctx,
                OpKind::Ror {
                    dst,
                    src: rs1,
                    amount: SrcOperand::Reg(rs2),
                    width: w,
                    flags: FlagUpdate::None,
                },
            )),
            RvOp::Rolw => {
                let t = ctx.alloc_vreg();
                wordret(
                    ctx,
                    &mut ops,
                    OpKind::Rol {
                        dst: t,
                        src: rs1,
                        amount: SrcOperand::Reg(rs2),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                    t,
                );
            }
            RvOp::Rorw => {
                let t = ctx.alloc_vreg();
                wordret(
                    ctx,
                    &mut ops,
                    OpKind::Ror {
                        dst: t,
                        src: rs1,
                        amount: SrcOperand::Reg(rs2),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                    t,
                );
            }
            RvOp::Min => minmax(ctx, &mut ops, Condition::Slt),
            RvOp::Minu => minmax(ctx, &mut ops, Condition::Ult),
            RvOp::Max => minmax(ctx, &mut ops, Condition::Sgt),
            RvOp::Maxu => minmax(ctx, &mut ops, Condition::Ugt),
            RvOp::SextB => ops.push(mk(
                ctx,
                OpKind::SignExtend {
                    dst,
                    src: rs1,
                    from_width: OpWidth::W8,
                    to_width: w,
                },
            )),
            RvOp::SextH => ops.push(mk(
                ctx,
                OpKind::SignExtend {
                    dst,
                    src: rs1,
                    from_width: OpWidth::W16,
                    to_width: w,
                },
            )),
            RvOp::ZextH => ops.push(mk(
                ctx,
                OpKind::ZeroExtend {
                    dst,
                    src: rs1,
                    from_width: OpWidth::W16,
                    to_width: w,
                },
            )),
            RvOp::Sh1add => shadd(ctx, &mut ops, 1, false),
            RvOp::Sh2add => shadd(ctx, &mut ops, 2, false),
            RvOp::Sh3add => shadd(ctx, &mut ops, 3, false),
            RvOp::Sh1addUw => shadd(ctx, &mut ops, 1, true),
            RvOp::Sh2addUw => shadd(ctx, &mut ops, 2, true),
            RvOp::Sh3addUw => shadd(ctx, &mut ops, 3, true),
            RvOp::AddUw => {
                let z = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::ZeroExtend {
                        dst: z,
                        src: rs1,
                        from_width: OpWidth::W32,
                        to_width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Add {
                        dst,
                        src1: z,
                        src2: SrcOperand::Reg(rs2),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Bclr => bitop(ctx, &mut ops, 0),
            RvOp::Bset => bitop(ctx, &mut ops, 1),
            RvOp::Binv => bitop(ctx, &mut ops, 2),
            RvOp::Bext => {
                let s = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shr {
                        dst: s,
                        src: rs1,
                        amount: SrcOperand::Reg(rs2),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst,
                        src1: s,
                        src2: SrcOperand::Imm(1),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::CzeroEqz => {
                // rd = (rs2 != 0) ? rs1 : 0
                ops.push(mk(
                    ctx,
                    OpKind::Cmp {
                        src1: rs2,
                        src2: SrcOperand::Imm(0),
                        width: w,
                    },
                ));
                let nz = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::SetCC {
                        dst: nz,
                        cond: Condition::Ne,
                        width: w,
                    },
                ));
                let zero = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Select {
                        dst,
                        cond: nz,
                        src_true: rs1,
                        src_false: zero,
                        width: w,
                    },
                ));
            }
            RvOp::CzeroNez => {
                // rd = (rs2 == 0) ? rs1 : 0
                ops.push(mk(
                    ctx,
                    OpKind::Cmp {
                        src1: rs2,
                        src2: SrcOperand::Imm(0),
                        width: w,
                    },
                ));
                let z = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::SetCC {
                        dst: z,
                        cond: Condition::Eq,
                        width: w,
                    },
                ));
                let zero = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Select {
                        dst,
                        cond: z,
                        src_true: rs1,
                        src_false: zero,
                        width: w,
                    },
                ));
            }
            // Pack: rd = (rs2[lo] << shift) | rs1[lo]. pack uses XLEN/2 halves
            // (zext.w on RV64), packh uses bytes, packw uses 16-bit halves with
            // a sign-extended 32-bit result.
            RvOp::Pack => {
                let half_width = if self.xlen == 64 {
                    OpWidth::W32
                } else {
                    OpWidth::W16
                };
                let half_bits = (self.xlen / 2) as i64;
                let a = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::ZeroExtend {
                        dst: a,
                        src: rs1,
                        from_width: half_width,
                        to_width: w,
                    },
                ));
                let b = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::ZeroExtend {
                        dst: b,
                        src: rs2,
                        from_width: half_width,
                        to_width: w,
                    },
                ));
                let bsh = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shl {
                        dst: bsh,
                        src: b,
                        amount: SrcOperand::Imm(half_bits),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst,
                        src1: a,
                        src2: SrcOperand::Reg(bsh),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Packh => {
                let a = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: a,
                        src1: rs1,
                        src2: SrcOperand::Imm(0xff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let b = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: b,
                        src1: rs2,
                        src2: SrcOperand::Imm(0xff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let bsh = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shl {
                        dst: bsh,
                        src: b,
                        amount: SrcOperand::Imm(8),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst,
                        src1: a,
                        src2: SrcOperand::Reg(bsh),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Packw => {
                let a = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: a,
                        src1: rs1,
                        src2: SrcOperand::Imm(0xffff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let b = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: b,
                        src1: rs2,
                        src2: SrcOperand::Imm(0xffff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let bsh = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shl {
                        dst: bsh,
                        src: b,
                        amount: SrcOperand::Imm(16),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: t,
                        src1: a,
                        src2: SrcOperand::Reg(bsh),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::SignExtend {
                        dst,
                        src: t,
                        from_width: OpWidth::W32,
                        to_width: w,
                    },
                ));
            }
            // Carry-less multiply, crossbar permute, AES / SM4 round and key
            // helpers — no clean SMIR primitive; computed bit-exactly by the
            // RvIntCrypto op. SM4/AES32 carry their `bs` field (insn[31:30]).
            RvOp::Clmul
            | RvOp::Clmulh
            | RvOp::Clmulr
            | RvOp::Xperm4
            | RvOp::Xperm8
            | RvOp::Sha512Sig0l
            | RvOp::Sha512Sig0h
            | RvOp::Sha512Sig1l
            | RvOp::Sha512Sig1h
            | RvOp::Sha512Sum0r
            | RvOp::Sha512Sum1r
            | RvOp::Sm4ed
            | RvOp::Sm4ks
            | RvOp::Aes32esi
            | RvOp::Aes32esmi
            | RvOp::Aes32dsi
            | RvOp::Aes32dsmi
            | RvOp::Aes64es
            | RvOp::Aes64esm
            | RvOp::Aes64ds
            | RvOp::Aes64dsm
            | RvOp::Aes64ks2 => {
                let imm = match d.op {
                    RvOp::Sm4ed
                    | RvOp::Sm4ks
                    | RvOp::Aes32esi
                    | RvOp::Aes32esmi
                    | RvOp::Aes32dsi
                    | RvOp::Aes32dsmi => ((insn >> 30) & 3) as u8,
                    _ => 0,
                };
                ops.push(mk(
                    ctx,
                    OpKind::RvIntCrypto {
                        dst,
                        src1: rs1,
                        src2: rs2,
                        op: d.op,
                        imm,
                        xlen: self.xlen,
                    },
                ));
            }
            _ => {
                return Err(LiftError::Unsupported {
                    addr,
                    mnemonic: format!("{:?}", d.op),
                });
            }
        }

        Ok((ops, ControlFlow::NextInsn))
    }


    /// Decode-driven lowering of OP-IMM-32 bit-manipulation (slli.uw, roriw,
    /// clzw/cpopw/ctzw).
    pub(crate) fn lift_zb_imm32(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let d = rv_decode(insn, self.rv_xlen(), &self.decoder_isa());
        let rs1 = self.get_x_reg(d.rs1, ctx);
        let mut ops = Vec::new();
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let dst = match self.def_x_reg(d.rd, ctx) {
            Some(dst) => dst,
            None => return Ok((ops, ControlFlow::NextInsn)),
        };
        let w = OpWidth::W64;
        // Word results that are intrinsically <= 32 bits (counts) are zero-safe;
        // shift/rotate word results must be sign-extended.
        let sext32 = |ctx: &mut LiftContext, ops: &mut Vec<SmirOp>, tmp: VReg| {
            ops.push(mk(
                ctx,
                OpKind::SignExtend {
                    dst,
                    src: tmp,
                    from_width: OpWidth::W32,
                    to_width: w,
                },
            ));
        };
        match d.op {
            RvOp::SlliUw => {
                let shamt = ((insn >> 20) & 0x3F) as i64; // 6-bit
                let z = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::ZeroExtend {
                        dst: z,
                        src: rs1,
                        from_width: OpWidth::W32,
                        to_width: w,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Shl {
                        dst,
                        src: z,
                        amount: SrcOperand::Imm(shamt),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::Roriw => {
                let shamt = ((insn >> 20) & 0x1F) as i64;
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Ror {
                        dst: t,
                        src: rs1,
                        amount: SrcOperand::Imm(shamt),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                ));
                sext32(ctx, &mut ops, t);
            }
            RvOp::Clzw => ops.push(mk(
                ctx,
                OpKind::Clz {
                    dst,
                    src: rs1,
                    width: OpWidth::W32,
                },
            )),
            RvOp::Ctzw => ops.push(mk(
                ctx,
                OpKind::Ctz {
                    dst,
                    src: rs1,
                    width: OpWidth::W32,
                },
            )),
            RvOp::Cpopw => ops.push(mk(
                ctx,
                OpKind::Popcnt {
                    dst,
                    src: rs1,
                    width: OpWidth::W32,
                },
            )),
            _ => {
                return Err(LiftError::Unsupported {
                    addr,
                    mnemonic: format!("{:?}", d.op),
                });
            }
        }
        Ok((ops, ControlFlow::NextInsn))
    }
}
