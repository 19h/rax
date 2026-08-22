//! fp.rs

use crate::isa::riscv::{
    Op as RvOp, Xlen as RvXlen, decode as rv_decode, rvc::decode_rvc as rv_decode_rvc,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, RvVectorState, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{SmirBlock, SmirFunction};
use crate::smir::lift::riscv::*;

use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl RiscVLifter {
    /// FP load/store (FLW/FLD/FLH/FSW/FSD/FSH). Loads NaN-box narrower values.
    /// Vector loads/stores (same opcodes) are gaps.
    pub(crate) fn lift_fp_ldst(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = rv_decode(insn, xl, &self.decoder_isa());
        let mut ops = Vec::new();
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        let base = self.get_x_reg(d.rs1, ctx);
        let address = Address::BaseOffset {
            base,
            offset: d.imm,
            disp_size: DispSize::Auto,
        };
        // (load?, width, nan-box mask of the *upper* bits)
        let (is_load, width, boxmask): (bool, MemWidth, i64) = match d.op {
            RvOp::Flw => (true, MemWidth::B4, 0xffff_ffff_0000_0000u64 as i64),
            RvOp::Fld => (true, MemWidth::B8, 0),
            RvOp::Flh => (true, MemWidth::B2, 0xffff_ffff_ffff_0000u64 as i64),
            RvOp::Fsw => (false, MemWidth::B4, 0),
            RvOp::Fsd => (false, MemWidth::B8, 0),
            RvOp::Fsh => (false, MemWidth::B2, 0),
            _ => {
                // Vector load/store share the 0x07/0x27 major opcodes (the
                // mop/lumop fields distinguish them) — opaque RvVector.
                if !d.is_illegal() {
                    return self.emit_rv_vector(insn, &d, addr, ctx);
                }
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }
        };
        if is_load {
            let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
            if boxmask == 0 {
                ops.push(mk(
                    ctx,
                    OpKind::Load {
                        dst: fd,
                        addr: address,
                        width,
                        sign: SignExtend::Zero,
                    },
                ));
            } else {
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Load {
                        dst: t,
                        addr: address,
                        width,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: fd,
                        src1: t,
                        src2: SrcOperand::Imm(boxmask),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
        } else {
            let fs = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rs2)));
            ops.push(mk(
                ctx,
                OpKind::Store {
                    src: fs,
                    addr: address,
                    width,
                },
            ));
        }
        Ok((ops, ControlFlow::NextInsn))
    }

    /// OP-FP (0x53): only the fflags/rounding-free ops — FP<->int bit moves
    /// (FMV.*) and sign injection (FSGNJ/N/X). All arithmetic / convert /
    /// compare / classify ops are gaps (need SMIR FP fflags support).
    pub(crate) fn lift_op_fp(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = rv_decode(insn, xl, &self.decoder_isa());
        let mut ops = Vec::new();
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        let w = OpWidth::W64;
        let getf = |reg: u8, _ctx: &mut LiftContext| VReg::Arch(ArchReg::RiscV(RiscVReg::F(reg)));

        // Sign-injection helper: fd = nanbox | (fs1 & ~signbit) | sign(fs1, fs2).
        // mode 0 = fsgnj (sign of fs2), 1 = fsgnjn (~sign of fs2), 2 = fsgnjx
        // (sign fs1 ^ fs2).
        // Canonicalize a narrow (.S/.H) operand: if it is not properly
        // NaN-boxed (upper bits all-1) it reads as the canonical NaN.
        let unbox = |ctx: &mut LiftContext,
                     ops: &mut Vec<SmirOp>,
                     f: VReg,
                     sh: i64,
                     himask: i64,
                     lomask: i64,
                     cn: i64|
         -> VReg {
            let hi = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Shr {
                    dst: hi,
                    src: f,
                    amount: SrcOperand::Imm(sh),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(mk(
                ctx,
                OpKind::Cmp {
                    src1: hi,
                    src2: SrcOperand::Imm(himask),
                    width: w,
                },
            ));
            let boxed = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::SetCC {
                    dst: boxed,
                    cond: Condition::Eq,
                    width: w,
                },
            ));
            let lo = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::And {
                    dst: lo,
                    src1: f,
                    src2: SrcOperand::Imm(lomask),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            let cnr = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Mov {
                    dst: cnr,
                    src: SrcOperand::Imm(cn),
                    width: w,
                },
            ));
            let u = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Select {
                    dst: u,
                    cond: boxed,
                    src_true: lo,
                    src_false: cnr,
                    width: w,
                },
            ));
            u
        };
        let mut sgnj = |ctx: &mut LiftContext,
                        ops: &mut Vec<SmirOp>,
                        fs1: VReg,
                        fs2: VReg,
                        signbit: i64,
                        boxmask: i64,
                        mode: u8| {
            let a = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::And {
                    dst: a,
                    src1: fs1,
                    src2: SrcOperand::Imm(!signbit),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            let sb = ctx.alloc_vreg();
            match mode {
                0 => ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: sb,
                        src1: fs2,
                        src2: SrcOperand::Imm(signbit),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                )),
                1 => {
                    // ~fs2 & signbit  ==  (fs2 & signbit) ^ signbit
                    let tmp = ctx.alloc_vreg();
                    ops.push(mk(
                        ctx,
                        OpKind::And {
                            dst: tmp,
                            src1: fs2,
                            src2: SrcOperand::Imm(signbit),
                            width: w,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(mk(
                        ctx,
                        OpKind::Xor {
                            dst: sb,
                            src1: tmp,
                            src2: SrcOperand::Imm(signbit),
                            width: w,
                            flags: FlagUpdate::None,
                        },
                    ));
                }
                _ => {
                    let x = ctx.alloc_vreg();
                    ops.push(mk(
                        ctx,
                        OpKind::Xor {
                            dst: x,
                            src1: fs1,
                            src2: SrcOperand::Reg(fs2),
                            width: w,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(mk(
                        ctx,
                        OpKind::And {
                            dst: sb,
                            src1: x,
                            src2: SrcOperand::Imm(signbit),
                            width: w,
                            flags: FlagUpdate::None,
                        },
                    ));
                }
            }
            let c = ctx.alloc_vreg();
            ops.push(mk(
                ctx,
                OpKind::Or {
                    dst: c,
                    src1: a,
                    src2: SrcOperand::Reg(sb),
                    width: w,
                    flags: FlagUpdate::None,
                },
            ));
            let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
            if boxmask == 0 {
                ops.push(mk(
                    ctx,
                    OpKind::Mov {
                        dst: fd,
                        src: SrcOperand::Reg(c),
                        width: w,
                    },
                ));
            } else {
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: fd,
                        src1: c,
                        src2: SrcOperand::Imm(boxmask),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
        };
        const SB_D: i64 = 0x8000_0000_0000_0000u64 as i64;
        const BOX_S: i64 = 0xffff_ffff_0000_0000u64 as i64;
        const BOX_H: i64 = 0xffff_ffff_ffff_0000u64 as i64;

        match d.op {
            // FP -> int bit moves.
            RvOp::FmvXW => {
                let fs = getf(d.rs1, ctx);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    ops.push(mk(
                        ctx,
                        OpKind::SignExtend {
                            dst,
                            src: fs,
                            from_width: OpWidth::W32,
                            to_width: w,
                        },
                    ));
                }
            }
            RvOp::FmvXH => {
                let fs = getf(d.rs1, ctx);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    ops.push(mk(
                        ctx,
                        OpKind::SignExtend {
                            dst,
                            src: fs,
                            from_width: OpWidth::W16,
                            to_width: w,
                        },
                    ));
                }
            }
            RvOp::FmvXD => {
                let fs = getf(d.rs1, ctx);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    ops.push(mk(
                        ctx,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Reg(fs),
                            width: w,
                        },
                    ));
                }
            }
            RvOp::FmvhXD => {
                let fs = getf(d.rs1, ctx);
                let high = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shr {
                        dst: high,
                        src: fs,
                        amount: SrcOperand::Imm(32),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    ops.push(mk(
                        ctx,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Reg(high),
                            width: w,
                        },
                    ));
                }
            }
            // int -> FP bit moves (NaN-box narrow values).
            RvOp::FmvWX => {
                let xs = self.get_x_reg(d.rs1, ctx);
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: t,
                        src1: xs,
                        src2: SrcOperand::Imm(0xffff_ffff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: fd,
                        src1: t,
                        src2: SrcOperand::Imm(BOX_S),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::FmvHX => {
                let xs = self.get_x_reg(d.rs1, ctx);
                let t = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: t,
                        src1: xs,
                        src2: SrcOperand::Imm(0xffff),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: fd,
                        src1: t,
                        src2: SrcOperand::Imm(BOX_H),
                        width: w,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            RvOp::FmvDX => {
                let xs = self.get_x_reg(d.rs1, ctx);
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Mov {
                        dst: fd,
                        src: SrcOperand::Reg(xs),
                        width: w,
                    },
                ));
            }
            RvOp::FmvpDX => {
                let low_src = self.get_x_reg(d.rs1, ctx);
                let high_src = self.get_x_reg(d.rs2, ctx);
                let low = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::And {
                        dst: low,
                        src1: low_src,
                        src2: SrcOperand::Imm(0xffff_ffff),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                let shifted = ctx.alloc_vreg();
                ops.push(mk(
                    ctx,
                    OpKind::Shl {
                        dst: shifted,
                        src: high_src,
                        amount: SrcOperand::Imm(32),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Or {
                        dst: fd,
                        src1: low,
                        src2: SrcOperand::Reg(shifted),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            // Sign injection. Only .D is lifted: .S/.H must canonicalize an
            // improperly-NaN-boxed operand to the canonical NaN first, which
            // needs extra unbox conditionals — gapped for now.
            RvOp::FsgnjD | RvOp::FsgnjnD | RvOp::FsgnjxD => {
                let m = match d.op {
                    RvOp::FsgnjD => 0,
                    RvOp::FsgnjnD => 1,
                    _ => 2,
                };
                let fs1 = getf(d.rs1, ctx);
                let fs2 = getf(d.rs2, ctx);
                sgnj(ctx, &mut ops, fs1, fs2, SB_D, 0, m);
            }
            RvOp::FsgnjS | RvOp::FsgnjnS | RvOp::FsgnjxS => {
                let m = match d.op {
                    RvOp::FsgnjS => 0,
                    RvOp::FsgnjnS => 1,
                    _ => 2,
                };
                let fs1 = getf(d.rs1, ctx);
                let fs2 = getf(d.rs2, ctx);
                let u1 = unbox(
                    ctx,
                    &mut ops,
                    fs1,
                    32,
                    0xffff_ffff,
                    0xffff_ffff,
                    0x7fc0_0000,
                );
                let u2 = unbox(
                    ctx,
                    &mut ops,
                    fs2,
                    32,
                    0xffff_ffff,
                    0xffff_ffff,
                    0x7fc0_0000,
                );
                sgnj(ctx, &mut ops, u1, u2, 0x8000_0000u64 as i64, BOX_S, m);
            }
            RvOp::FsgnjH | RvOp::FsgnjnH | RvOp::FsgnjxH => {
                let m = match d.op {
                    RvOp::FsgnjH => 0,
                    RvOp::FsgnjnH => 1,
                    _ => 2,
                };
                let fs1 = getf(d.rs1, ctx);
                let fs2 = getf(d.rs2, ctx);
                let u1 = unbox(ctx, &mut ops, fs1, 16, 0xffff_ffff_ffff, 0xffff, 0x7e00);
                let u2 = unbox(ctx, &mut ops, fs2, 16, 0xffff_ffff_ffff, 0xffff, 0x7e00);
                sgnj(ctx, &mut ops, u1, u2, 0x8000, BOX_H, m);
            }
            // Classify (fflags-free): canonicalize narrow operands, then build
            // the 10-bit class mask. (FCLASS does not depend on rounding.)
            RvOp::FclassD => {
                let fs = getf(d.rs1, ctx);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    self.emit_fclass(ctx, &mut ops, addr, fs, 52, 11, dst);
                }
            }
            RvOp::FclassS => {
                let fs = getf(d.rs1, ctx);
                let u = unbox(ctx, &mut ops, fs, 32, 0xffff_ffff, 0xffff_ffff, 0x7fc0_0000);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    self.emit_fclass(ctx, &mut ops, addr, u, 23, 8, dst);
                }
            }
            RvOp::FclassH => {
                let fs = getf(d.rs1, ctx);
                let u = unbox(ctx, &mut ops, fs, 16, 0xffff_ffff_ffff, 0xffff, 0x7e00);
                if let Some(dst) = self.def_x_reg(d.rd, ctx) {
                    self.emit_fclass(ctx, &mut ops, addr, u, 10, 5, dst);
                }
            }
            // Zfa load-immediate: `rs1` is a 32-entry table index (NOT a
            // register), so materialise the constant directly and NaN-box it.
            RvOp::FliS | RvOp::FliD | RvOp::FliH => {
                use crate::isa::riscv::float as ff;
                let (bits, boxmask) = match d.op {
                    RvOp::FliS => (ff::fli(ff::F32, d.rs1) as u32 as i64, BOX_S),
                    RvOp::FliH => (ff::fli(ff::F16, d.rs1) as u16 as i64, BOX_H),
                    _ => (ff::fli(ff::F64, d.rs1) as i64, 0),
                };
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Mov {
                        dst: fd,
                        src: SrcOperand::Imm(bits | boxmask),
                        width: w,
                    },
                ));
            }
            // All remaining OP-FP arithmetic / convert / compare / min-max /
            // round ops are computed bit-exactly via the RvFp op (fflags + NaN
            // canonicalisation + dynamic rounding).
            _ => {
                if d.is_illegal() {
                    return Err(LiftError::InvalidEncoding {
                        addr,
                        bytes: insn.to_le_bytes().to_vec(),
                    });
                }
                self.emit_rvfp(&d, addr, ctx, &mut ops);
            }
        }
        Ok((ops, ControlFlow::NextInsn))
    }

    /// Lift the FMA family (opcodes 0x43/0x47/0x4b/0x4f) via the bit-exact RvFp
    /// op. The decoder maps each to the concrete `Fmadd/Fmsub/Fnmsub/Fnmadd`
    /// `.S/.D/.H` opcode; the operand sign flips live in `eval_scalar_fp`.
    pub(crate) fn lift_fp_fma(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = rv_decode(insn, xl, &self.decoder_isa());
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let mut ops = Vec::new();
        self.emit_rvfp(&d, addr, ctx, &mut ops);
        Ok((ops, ControlFlow::NextInsn))
    }
}
