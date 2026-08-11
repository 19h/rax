//! compressed.rs

use crate::isa::riscv::{
    Isa as RvIsa, Op as RvOp, Xlen as RvXlen, decode as rv_decode, rvc::decode_rvc as rv_decode_rvc,
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
    /// Zcmp double moves have two architecturally simultaneous register
    /// assignments. The s-register encoding maps only to x8, x9, and x18-x23,
    /// so neither assignment aliases a0/x10 or a1/x11.
    pub(crate) fn lift_zcmp_move(
        &mut self,
        op: RvOp,
        r1s: u8,
        r2s: u8,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let (dst1, src1, dst2, src2) = match op {
            RvOp::CmMvsa01 => (r1s, 10, r2s, 11),
            RvOp::CmMva01s => (10, r1s, 11, r2s),
            _ => {
                return Err(LiftError::Internal(format!(
                    "Zcmp move lift received unexpected operation {op:?}"
                )));
            }
        };
        let width = self.op_width();
        let ops = [(dst1, src1), (dst2, src2)]
            .into_iter()
            .map(|(dst, src)| {
                SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst: self
                            .def_x_reg(dst, ctx)
                            .expect("Zcmp move destinations are nonzero"),
                        src: SrcOperand::Reg(self.get_x_reg(src, ctx)),
                        width,
                    },
                )
            })
            .collect();
        Ok((ops, ControlFlow::NextInsn))
    }

    /// Expand one Zcmp PUSH/POP macro into its architecturally ordered memory
    /// sequence. Earlier stores or register loads intentionally remain visible
    /// if a later access faults; the final SP/a0/control-flow updates occur only
    /// after every access succeeds.
    pub(crate) fn lift_zcmp_stack(
        &mut self,
        decoded: &crate::isa::riscv::Insn,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let count = match decoded.rd {
            4 => 1,
            5 => 2,
            6 => 3,
            7..=14 => usize::from(decoded.rd - 3),
            15 => 13,
            _ => {
                return Err(LiftError::Internal(format!(
                    "Zcmp stack lift received invalid register list {}",
                    decoded.rd
                )));
            }
        };
        const REGS: [u8; 13] = [1, 8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];
        let slot_size = i64::from(self.xlen / 8);
        let stack_adj = decoded.imm;
        let sp = self.get_x_reg(2, ctx);
        let width = self.op_width();
        let mem_width = if self.xlen == 32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        let mut ops = Vec::with_capacity(count + 3);

        match decoded.op {
            RvOp::CmPush => {
                for (slot, register) in REGS[..count].iter().rev().copied().enumerate() {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Store {
                            src: self.get_x_reg(register, ctx),
                            addr: Address::BaseOffset {
                                base: sp,
                                offset: -((slot as i64 + 1) * slot_size),
                                disp_size: DispSize::Auto,
                            },
                            width: mem_width,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Sub {
                        dst: self
                            .def_x_reg(2, ctx)
                            .expect("the stack pointer is nonzero"),
                        src1: sp,
                        src2: SrcOperand::Imm(stack_adj),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));
                Ok((ops, ControlFlow::NextInsn))
            }
            RvOp::CmPop | RvOp::CmPopRet | RvOp::CmPopRetz => {
                for (slot, register) in REGS[..count].iter().rev().copied().enumerate() {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Load {
                            dst: self
                                .def_x_reg(register, ctx)
                                .expect("Zcmp stack registers are nonzero"),
                            addr: Address::BaseOffset {
                                base: sp,
                                offset: stack_adj - (slot as i64 + 1) * slot_size,
                                disp_size: DispSize::Auto,
                            },
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Add {
                        dst: self
                            .def_x_reg(2, ctx)
                            .expect("the stack pointer is nonzero"),
                        src1: sp,
                        src2: SrcOperand::Imm(stack_adj),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));
                if decoded.op == RvOp::CmPopRetz {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Mov {
                            dst: self.def_x_reg(10, ctx).expect("a0 is a nonzero register"),
                            src: SrcOperand::Imm(0),
                            width,
                        },
                    ));
                }
                if matches!(decoded.op, RvOp::CmPopRet | RvOp::CmPopRetz) {
                    let target = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::And {
                            dst: target,
                            src1: self.get_x_reg(1, ctx),
                            src2: SrcOperand::Imm(!1i64),
                            width,
                            flags: FlagUpdate::None,
                        },
                    ));
                    Ok((ops, ControlFlow::IndirectBranch { target }))
                } else {
                    Ok((ops, ControlFlow::NextInsn))
                }
            }
            _ => Err(LiftError::Internal(format!(
                "Zcmp stack lift received unexpected operation {:?}",
                decoded.op
            ))),
        }
    }

    /// Zcmt table jumps fetch one XLEN-wide target from instruction memory.
    /// The generic SMIR memory bridge supplies the bytes; the production
    /// dispatcher reclassifies a failed helper read as an instruction-access
    /// fault, matching the architectural second-fetch semantics.
    pub(crate) fn lift_zcmt(
        &mut self,
        decoded: &crate::isa::riscv::Insn,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        if !matches!(decoded.op, RvOp::CmJt | RvOp::CmJalt) {
            return Err(LiftError::Internal(format!(
                "Zcmt lift received unexpected operation {:?}",
                decoded.op
            )));
        }
        let width = self.op_width();
        let target = ctx.alloc_vreg();
        let aligned = ctx.alloc_vreg();
        let mut ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Load {
                dst: target,
                addr: Address::BaseOffset {
                    base: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x017))),
                    offset: decoded.imm * i64::from(self.xlen / 8),
                    disp_size: DispSize::Auto,
                },
                width: if self.xlen == 32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                sign: SignExtend::Zero,
            },
        )];
        if decoded.op == RvOp::CmJalt {
            let return_addr = addr.wrapping_add(2)
                & if self.xlen == 32 {
                    u64::from(u32::MAX)
                } else {
                    u64::MAX
                };
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst: self
                        .def_x_reg(1, ctx)
                        .expect("the link register is nonzero"),
                    src: SrcOperand::Imm64(return_addr as i64),
                    width,
                },
            ));
        }
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::And {
                dst: aligned,
                src1: target,
                src2: SrcOperand::Imm(!1i64),
                width,
                flags: FlagUpdate::None,
            },
        ));
        Ok((ops, ControlFlow::IndirectBranch { target: aligned }))
    }

    // Compressed FP load/store (c.fld/c.fsd/c.fldsp/c.fsdsp). Doubles need no
    // NaN-boxing; decoded through the rax decoder for the resolved operands.
    pub(crate) fn lift_c_fp_ldst(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = crate::isa::riscv::decode::decode_compressed(insn, xl, &self.decoder_isa());
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let base = self.get_x_reg(d.rs1, ctx);
        let address = Address::BaseOffset {
            base,
            offset: d.imm,
            disp_size: DispSize::Auto,
        };
        let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
        let mut ops = Vec::new();
        match d.op {
            RvOp::Fld => {
                let fd = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rd)));
                ops.push(mk(
                    ctx,
                    OpKind::Load {
                        dst: fd,
                        addr: address,
                        width: MemWidth::B8,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            RvOp::Fsd => {
                let fs = VReg::Arch(ArchReg::RiscV(RiscVReg::F(d.rs2)));
                ops.push(mk(
                    ctx,
                    OpKind::Store {
                        src: fs,
                        addr: address,
                        width: MemWidth::B8,
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

    // Zcb quadrant-0 byte/half loads and stores (c.lbu/lhu/lh/sb/sh). Decoded
    // through the rax decoder for the precise op and resolved rd'/rs1'/rs2'/imm.
    pub(crate) fn lift_c_zcb_ldst(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = crate::isa::riscv::decode::decode_compressed(insn, xl, &self.decoder_isa());
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        let base = self.get_x_reg(d.rs1, ctx);
        let address = Address::BaseOffset {
            base,
            offset: d.imm,
            disp_size: DispSize::Auto,
        };
        let mut ops = Vec::new();
        let (width, sign, is_store) = match d.op {
            RvOp::Lbu => (MemWidth::B1, SignExtend::Zero, false),
            RvOp::Lhu => (MemWidth::B2, SignExtend::Zero, false),
            RvOp::Lh => (MemWidth::B2, SignExtend::Sign, false),
            RvOp::Sb => (MemWidth::B1, SignExtend::Zero, true),
            RvOp::Sh => (MemWidth::B2, SignExtend::Zero, true),
            _ => {
                return Err(LiftError::Unsupported {
                    addr,
                    mnemonic: format!("{:?}", d.op),
                });
            }
        };
        if is_store {
            let src = self.get_x_reg(d.rs2, ctx);
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Store {
                    src,
                    addr: address,
                    width,
                },
            ));
        } else if let Some(dst) = self.def_x_reg(d.rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Load {
                    dst,
                    addr: address,
                    width,
                    sign,
                },
            ));
        }
        Ok((ops, ControlFlow::NextInsn))
    }

    // C.ADDI4SPN: rd' = sp + nzuimm
    pub(crate) fn lift_c_addi4spn(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::creg(((insn >> 2) & 0x7) as u8);
        let nzuimm = ((((insn >> 5) & 1) << 3)
            | (((insn >> 6) & 1) << 2)
            | (((insn >> 7) & 0xF) << 6)
            | (((insn >> 11) & 0x3) << 4)) as i64;

        if nzuimm == 0 {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }

        let sp = self.get_x_reg(2, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Add {
                    dst,
                    src1: sp,
                    src2: SrcOperand::Imm(nzuimm),
                    width: self.op_width(),
                    flags: FlagUpdate::None,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.LW: rd' = mem[rs1' + uimm]
    pub(crate) fn lift_c_lw(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::creg(((insn >> 2) & 0x7) as u8);
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let uimm = ((((insn >> 5) & 1) << 6)
            | (((insn >> 6) & 1) << 2)
            | (((insn >> 10) & 0x7) << 3)) as i64;

        let base = self.get_x_reg(rs1, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Load {
                    dst,
                    addr: Address::BaseOffset {
                        base,
                        offset: uimm,
                        disp_size: DispSize::Auto,
                    },
                    width: MemWidth::B4,
                    sign: SignExtend::Sign,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.LD: rd' = mem[rs1' + uimm] (RV64)
    pub(crate) fn lift_c_ld(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::creg(((insn >> 2) & 0x7) as u8);
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let uimm = ((((insn >> 5) & 0x3) << 6) | (((insn >> 10) & 0x7) << 3)) as i64;

        let base = self.get_x_reg(rs1, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Load {
                    dst,
                    addr: Address::BaseOffset {
                        base,
                        offset: uimm,
                        disp_size: DispSize::Auto,
                    },
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.SW: mem[rs1' + uimm] = rs2'
    pub(crate) fn lift_c_sw(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs2 = Self::creg(((insn >> 2) & 0x7) as u8);
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let uimm = ((((insn >> 5) & 1) << 6)
            | (((insn >> 6) & 1) << 2)
            | (((insn >> 10) & 0x7) << 3)) as i64;

        let base = self.get_x_reg(rs1, ctx);
        let src = self.get_x_reg(rs2, ctx);

        let ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Store {
                src,
                addr: Address::BaseOffset {
                    base,
                    offset: uimm,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B4,
            },
        )];

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.SD: mem[rs1' + uimm] = rs2' (RV64)
    pub(crate) fn lift_c_sd(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs2 = Self::creg(((insn >> 2) & 0x7) as u8);
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let uimm = ((((insn >> 5) & 0x3) << 6) | (((insn >> 10) & 0x7) << 3)) as i64;

        let base = self.get_x_reg(rs1, ctx);
        let src = self.get_x_reg(rs2, ctx);

        let ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Store {
                src,
                addr: Address::BaseOffset {
                    base,
                    offset: uimm,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B8,
            },
        )];

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.ADDI / C.NOP
    pub(crate) fn lift_c_addi(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let imm = {
            let imm5 = ((insn >> 12) & 1) as i8;
            let imm4_0 = ((insn >> 2) & 0x1F) as i8;
            ((((imm5 << 5) | imm4_0) << 2) >> 2) as i64 // sign-extend from bit 5
        };

        if rd == 0 {
            // C.NOP
            return Ok((
                vec![SmirOp::new(ctx.next_op_id(), addr, OpKind::Nop)],
                ControlFlow::NextInsn,
            ));
        }

        let rs1 = self.get_x_reg(rd, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Add {
                    dst,
                    src1: rs1,
                    src2: SrcOperand::Imm(imm),
                    width: self.op_width(),
                    flags: FlagUpdate::None,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.ADDIW (RV64)
    pub(crate) fn lift_c_addiw(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let imm = {
            let imm5 = ((insn >> 12) & 1) as i8;
            let imm4_0 = ((insn >> 2) & 0x1F) as i8;
            ((((imm5 << 5) | imm4_0) << 2) >> 2) as i64 // sign-extend from bit 5
        };

        let rs1 = self.get_x_reg(rd, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Add {
                    dst: tmp,
                    src1: rs1,
                    src2: SrcOperand::Imm(imm),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::SignExtend {
                    dst,
                    src: tmp,
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.JAL (RV32 only)
    pub(crate) fn lift_c_jal(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let imm = self.c_j_offset(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        let return_addr = addr + 2;

        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(1, ctx) {
            // ra
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(return_addr as i64),
                    width: self.op_width(),
                },
            ));
        }

        Ok((ops, ControlFlow::DirectBranch(target)))
    }

    // C.LI
    pub(crate) fn lift_c_li(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let imm = {
            let imm5 = ((insn >> 12) & 1) as i8;
            let imm4_0 = ((insn >> 2) & 0x1F) as i8;
            ((((imm5 << 5) | imm4_0) << 2) >> 2) as i64 // sign-extend from bit 5
        };

        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(imm),
                    width: self.op_width(),
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.LUI / C.ADDI16SP
    pub(crate) fn lift_c_lui_addi16sp(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;

        let mut ops = Vec::new();

        if rd == 2 {
            // C.ADDI16SP
            let imm = {
                let bit9 = ((insn >> 12) & 1) as i16;
                let bit4 = ((insn >> 6) & 1) as i16;
                let bit6 = ((insn >> 5) & 1) as i16;
                let bit8_7 = ((insn >> 3) & 0x3) as i16;
                let bit5 = ((insn >> 2) & 1) as i16;
                let raw = (bit9 << 9) | (bit8_7 << 7) | (bit6 << 6) | (bit5 << 5) | (bit4 << 4);
                ((raw << 6) >> 6) as i64 // Sign-extend from 10 bits
            };

            if imm == 0 {
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }

            let sp = self.get_x_reg(2, ctx);
            if let Some(dst) = self.def_x_reg(2, ctx) {
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Add {
                        dst,
                        src1: sp,
                        src2: SrcOperand::Imm(imm),
                        width: self.op_width(),
                        flags: FlagUpdate::None,
                    },
                ));
            }
        } else {
            // C.LUI
            let nzimm = {
                let bit17 = ((insn >> 12) & 1) as i32;
                let bits16_12 = ((insn >> 2) & 0x1F) as i32;
                let raw = (bit17 << 17) | (bits16_12 << 12);
                ((raw << 14) >> 14) as i64 // Sign-extend from 18 bits
            };

            if nzimm == 0 {
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }

            if let Some(dst) = self.def_x_reg(rd, ctx) {
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(nzimm),
                        width: self.op_width(),
                    },
                ));
            }
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.SRLI, C.SRAI, C.ANDI, C.SUB, C.XOR, C.OR, C.AND, C.SUBW, C.ADDW
    pub(crate) fn lift_c_misc_alu(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::creg(((insn >> 7) & 0x7) as u8);
        let funct2 = (insn >> 10) & 0x3;

        let rs1 = self.get_x_reg(rd, ctx);
        let mut ops = Vec::new();

        match funct2 {
            0b00 | 0b01 => {
                // C.SRLI / C.SRAI
                let shamt = ((((insn >> 12) & 1) << 5) | ((insn >> 2) & 0x1F)) as i64;
                if let Some(dst) = self.def_x_reg(rd, ctx) {
                    let kind = if funct2 == 0b00 {
                        OpKind::Shr {
                            dst,
                            src: rs1,
                            amount: SrcOperand::Imm(shamt),
                            width: self.op_width(),
                            flags: FlagUpdate::None,
                        }
                    } else {
                        OpKind::Sar {
                            dst,
                            src: rs1,
                            amount: SrcOperand::Imm(shamt),
                            width: self.op_width(),
                            flags: FlagUpdate::None,
                        }
                    };
                    ops.push(SmirOp::new(ctx.next_op_id(), addr, kind));
                }
            }
            0b10 => {
                // C.ANDI
                let imm = {
                    let imm5 = ((insn >> 12) & 1) as i8;
                    let imm4_0 = ((insn >> 2) & 0x1F) as i8;
                    ((((imm5 << 5) | imm4_0) << 2) >> 2) as i64 // sign-extend from bit 5
                };
                if let Some(dst) = self.def_x_reg(rd, ctx) {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::And {
                            dst,
                            src1: rs1,
                            src2: SrcOperand::Imm(imm),
                            width: self.op_width(),
                            flags: FlagUpdate::None,
                        },
                    ));
                }
            }
            0b11 => {
                // Register-register ops
                let rs2 = Self::creg(((insn >> 2) & 0x7) as u8);
                let rs2_val = self.get_x_reg(rs2, ctx);
                let funct2b = (insn >> 5) & 0x3;
                let funct1 = (insn >> 12) & 1;

                if let Some(dst) = self.def_x_reg(rd, ctx) {
                    if funct1 == 0 {
                        let kind = match funct2b {
                            0b00 => OpKind::Sub {
                                dst,
                                src1: rs1,
                                src2: SrcOperand::Reg(rs2_val),
                                width: self.op_width(),
                                flags: FlagUpdate::None,
                            },
                            0b01 => OpKind::Xor {
                                dst,
                                src1: rs1,
                                src2: SrcOperand::Reg(rs2_val),
                                width: self.op_width(),
                                flags: FlagUpdate::None,
                            },
                            0b10 => OpKind::Or {
                                dst,
                                src1: rs1,
                                src2: SrcOperand::Reg(rs2_val),
                                width: self.op_width(),
                                flags: FlagUpdate::None,
                            },
                            0b11 => OpKind::And {
                                dst,
                                src1: rs1,
                                src2: SrcOperand::Reg(rs2_val),
                                width: self.op_width(),
                                flags: FlagUpdate::None,
                            },
                            _ => unreachable!(),
                        };
                        ops.push(SmirOp::new(ctx.next_op_id(), addr, kind));
                    } else if self.xlen == 64 {
                        let w = self.op_width();
                        match funct2b {
                            // C.SUBW / C.ADDW: W32 op then sign-extend.
                            0b00 | 0b01 => {
                                let tmp = ctx.alloc_vreg();
                                let k = if funct2b == 0b00 {
                                    OpKind::Sub {
                                        dst: tmp,
                                        src1: rs1,
                                        src2: SrcOperand::Reg(rs2_val),
                                        width: OpWidth::W32,
                                        flags: FlagUpdate::None,
                                    }
                                } else {
                                    OpKind::Add {
                                        dst: tmp,
                                        src1: rs1,
                                        src2: SrcOperand::Reg(rs2_val),
                                        width: OpWidth::W32,
                                        flags: FlagUpdate::None,
                                    }
                                };
                                ops.push(SmirOp::new(ctx.next_op_id(), addr, k));
                                ops.push(SmirOp::new(
                                    ctx.next_op_id(),
                                    addr,
                                    OpKind::SignExtend {
                                        dst,
                                        src: tmp,
                                        from_width: OpWidth::W32,
                                        to_width: OpWidth::W64,
                                    },
                                ));
                            }
                            // Zcb c.mul.
                            0b10 if self.extensions.zcb => ops.push(SmirOp::new(
                                ctx.next_op_id(),
                                addr,
                                OpKind::MulS {
                                    dst_lo: dst,
                                    dst_hi: None,
                                    src1: rs1,
                                    src2: SrcOperand::Reg(rs2_val),
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            )),
                            // Zcb unary: c.zext.b/sext.b/zext.h/sext.h/zext.w/not.
                            0b11 if self.extensions.zcb => {
                                let sub = (insn >> 2) & 0x7;
                                let k = match sub {
                                    0b000 => OpKind::And {
                                        dst,
                                        src1: rs1,
                                        src2: SrcOperand::Imm(0xff),
                                        width: w,
                                        flags: FlagUpdate::None,
                                    },
                                    0b001 => OpKind::SignExtend {
                                        dst,
                                        src: rs1,
                                        from_width: OpWidth::W8,
                                        to_width: w,
                                    },
                                    0b010 => OpKind::ZeroExtend {
                                        dst,
                                        src: rs1,
                                        from_width: OpWidth::W16,
                                        to_width: w,
                                    },
                                    0b011 => OpKind::SignExtend {
                                        dst,
                                        src: rs1,
                                        from_width: OpWidth::W16,
                                        to_width: w,
                                    },
                                    0b100 => OpKind::ZeroExtend {
                                        dst,
                                        src: rs1,
                                        from_width: OpWidth::W32,
                                        to_width: w,
                                    },
                                    0b101 => OpKind::Xor {
                                        dst,
                                        src1: rs1,
                                        src2: SrcOperand::Imm(-1),
                                        width: w,
                                        flags: FlagUpdate::None,
                                    },
                                    _ => {
                                        return Err(LiftError::Unsupported {
                                            addr,
                                            mnemonic: format!("c.zcb sub={sub:#05b}"),
                                        });
                                    }
                                };
                                ops.push(SmirOp::new(ctx.next_op_id(), addr, k));
                            }
                            _ => {
                                return Err(LiftError::InvalidEncoding {
                                    addr,
                                    bytes: insn.to_le_bytes().to_vec(),
                                });
                            }
                        }
                    } else {
                        return Err(LiftError::InvalidEncoding {
                            addr,
                            bytes: insn.to_le_bytes().to_vec(),
                        });
                    }
                }
            }
            _ => unreachable!(),
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.J
    pub(crate) fn lift_c_j(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let imm = self.c_j_offset(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        Ok((vec![], ControlFlow::DirectBranch(target)))
    }

    // C.BEQZ
    pub(crate) fn lift_c_beqz(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let imm = self.c_branch_offset(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        let fallthrough = addr + 2;

        let rs1_val = self.get_x_reg(rs1, ctx);
        let mut ops = Vec::new();

        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Cmp {
                src1: rs1_val,
                src2: SrcOperand::Imm(0),
                width: self.op_width(),
            },
        ));

        let cond_reg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::SetCC {
                dst: cond_reg,
                cond: Condition::Eq,
                width: OpWidth::W8,
            },
        ));

        Ok((
            ops,
            ControlFlow::CondBranchReg {
                cond: cond_reg,
                taken: target,
                not_taken: fallthrough,
            },
        ))
    }

    // C.BNEZ
    pub(crate) fn lift_c_bnez(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs1 = Self::creg(((insn >> 7) & 0x7) as u8);
        let imm = self.c_branch_offset(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        let fallthrough = addr + 2;

        let rs1_val = self.get_x_reg(rs1, ctx);
        let mut ops = Vec::new();

        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Cmp {
                src1: rs1_val,
                src2: SrcOperand::Imm(0),
                width: self.op_width(),
            },
        ));

        let cond_reg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::SetCC {
                dst: cond_reg,
                cond: Condition::Ne,
                width: OpWidth::W8,
            },
        ));

        Ok((
            ops,
            ControlFlow::CondBranchReg {
                cond: cond_reg,
                taken: target,
                not_taken: fallthrough,
            },
        ))
    }

    // C.SLLI
    pub(crate) fn lift_c_slli(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let shamt = ((((insn >> 12) & 1) << 5) | ((insn >> 2) & 0x1F)) as i64;

        let rs1 = self.get_x_reg(rd, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Shl {
                    dst,
                    src: rs1,
                    amount: SrcOperand::Imm(shamt),
                    width: self.op_width(),
                    flags: FlagUpdate::None,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.LWSP
    pub(crate) fn lift_c_lwsp(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let uimm = ((((insn >> 12) & 1) << 5)
            | (((insn >> 4) & 0x7) << 2)
            | (((insn >> 2) & 0x3) << 6)) as i64;

        if rd == 0 {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }

        let sp = self.get_x_reg(2, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Load {
                    dst,
                    addr: Address::BaseOffset {
                        base: sp,
                        offset: uimm,
                        disp_size: DispSize::Auto,
                    },
                    width: MemWidth::B4,
                    sign: SignExtend::Sign,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.LDSP (RV64)
    pub(crate) fn lift_c_ldsp(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let uimm = ((((insn >> 12) & 1) << 5)
            | (((insn >> 5) & 0x3) << 3)
            | (((insn >> 2) & 0x7) << 6)) as i64;

        if rd == 0 {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }

        let sp = self.get_x_reg(2, ctx);
        let mut ops = Vec::new();

        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Load {
                    dst,
                    addr: Address::BaseOffset {
                        base: sp,
                        offset: uimm,
                        disp_size: DispSize::Auto,
                    },
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ));
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.JR, C.MV, C.JALR, C.ADD
    pub(crate) fn lift_c_jr_mv_add(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = ((insn >> 7) & 0x1F) as u8;
        let rs2 = ((insn >> 2) & 0x1F) as u8;
        let bit12 = (insn >> 12) & 1;

        let mut ops = Vec::new();

        if bit12 == 0 {
            if rs2 == 0 {
                // C.JR
                if rd == 0 {
                    return Err(LiftError::InvalidEncoding {
                        addr,
                        bytes: insn.to_le_bytes().to_vec(),
                    });
                }
                // C.JR == JALR x0, rd, 0: the target clears bit 0.
                let rs1 = self.get_x_reg(rd, ctx);
                let aligned = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::And {
                        dst: aligned,
                        src1: rs1,
                        src2: SrcOperand::Imm(!1i64),
                        width: self.op_width(),
                        flags: FlagUpdate::None,
                    },
                ));
                return Ok((ops, ControlFlow::IndirectBranch { target: aligned }));
            } else {
                // C.MV
                let rs2_val = self.get_x_reg(rs2, ctx);
                if let Some(dst) = self.def_x_reg(rd, ctx) {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Reg(rs2_val),
                            width: self.op_width(),
                        },
                    ));
                }
            }
        } else {
            if rs2 == 0 && rd == 0 {
                // C.EBREAK
                ops.push(SmirOp::new(ctx.next_op_id(), addr, OpKind::Breakpoint));
            } else if rs2 == 0 {
                // C.JALR == JALR x1, rd, 0: link to x1, target clears bit 0.
                let rs1 = self.get_x_reg(rd, ctx);
                let aligned = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::And {
                        dst: aligned,
                        src1: rs1,
                        src2: SrcOperand::Imm(!1i64),
                        width: self.op_width(),
                        flags: FlagUpdate::None,
                    },
                ));
                let return_addr = addr + 2;

                if let Some(ra) = self.def_x_reg(1, ctx) {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Mov {
                            dst: ra,
                            src: SrcOperand::Imm(return_addr as i64),
                            width: self.op_width(),
                        },
                    ));
                }

                return Ok((ops, ControlFlow::IndirectBranch { target: aligned }));
            } else {
                // C.ADD
                let rs1 = self.get_x_reg(rd, ctx);
                let rs2_val = self.get_x_reg(rs2, ctx);
                if let Some(dst) = self.def_x_reg(rd, ctx) {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Add {
                            dst,
                            src1: rs1,
                            src2: SrcOperand::Reg(rs2_val),
                            width: self.op_width(),
                            flags: FlagUpdate::None,
                        },
                    ));
                }
            }
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.SWSP
    pub(crate) fn lift_c_swsp(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs2 = ((insn >> 2) & 0x1F) as u8;
        let uimm = ((((insn >> 9) & 0xF) << 2) | (((insn >> 7) & 0x3) << 6)) as i64;

        let sp = self.get_x_reg(2, ctx);
        let src = self.get_x_reg(rs2, ctx);

        let ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Store {
                src,
                addr: Address::BaseOffset {
                    base: sp,
                    offset: uimm,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B4,
            },
        )];

        Ok((ops, ControlFlow::NextInsn))
    }

    // C.SDSP (RV64)
    pub(crate) fn lift_c_sdsp(
        &mut self,
        insn: u16,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs2 = ((insn >> 2) & 0x1F) as u8;
        let uimm = ((((insn >> 10) & 0x7) << 3) | (((insn >> 7) & 0x7) << 6)) as i64;

        let sp = self.get_x_reg(2, ctx);
        let src = self.get_x_reg(rs2, ctx);

        let ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Store {
                src,
                addr: Address::BaseOffset {
                    base: sp,
                    offset: uimm,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B8,
            },
        )];

        Ok((ops, ControlFlow::NextInsn))
    }
}
