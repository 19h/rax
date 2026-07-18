//! alu.rs

use crate::smir::lift::aarch64::*;
use std::collections::HashSet;

use crate::isa::arm::decoder::{
    AddressingMode, Condition as ArmCondition, DecodedInsn, ExtendType, FpRegSize, FpRegister,
    MemOffset, MemOperand, Mnemonic, Operand, Register, ShiftType,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader};

impl Aarch64Lifter {

    pub(crate) fn lift_add_sub_tags(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, rn, offset) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
            insn.operands.get(3),
        ) {
            (
                Some(Operand::Reg(rd)),
                Some(Operand::Reg(rn)),
                Some(Operand::Imm(offset)),
                Some(Operand::Imm(_tag_offset)),
            ) => (rd, rn, offset.effective_value()),
            _ => {
                return Err(LiftError::Internal(
                    "invalid ADDG/SUBG operands".to_string(),
                ));
            }
        };

        let adjusted = ctx.alloc_vreg();
        let src2 = SrcOperand::Imm(offset);
        let kind = if insn.mnemonic == Mnemonic::ADDG {
            OpKind::Add {
                dst: adjusted,
                src1: self.arm_reg(rn),
                src2,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            }
        } else {
            OpKind::Sub {
                dst: adjusted,
                src1: self.arm_reg(rn),
                src2,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            }
        };
        Self::push_lifted_op(ops, pc, kind);

        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: self.dst_reg(rd, ctx),
                src1: adjusted,
                src2: SrcOperand::Imm64(MTE_TAG_CLEAR_MASK),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        Ok(())
    }


    /// Convert shift type
    pub(crate) fn arm_shift(&self, shift: ShiftType) -> ShiftOp {
        match shift {
            ShiftType::LSL => ShiftOp::Lsl,
            ShiftType::LSR => ShiftOp::Lsr,
            ShiftType::ASR => ShiftOp::Asr,
            ShiftType::ROR | ShiftType::RRX => ShiftOp::Ror,
        }
    }


    pub(crate) fn arm_extend(&self, extend: ExtendType) -> ExtendOp {
        match extend {
            ExtendType::UXTB => ExtendOp::Uxtb,
            ExtendType::UXTH => ExtendOp::Uxth,
            ExtendType::UXTW => ExtendOp::Uxtw,
            ExtendType::UXTX => ExtendOp::Uxtx,
            ExtendType::SXTB => ExtendOp::Sxtb,
            ExtendType::SXTH => ExtendOp::Sxth,
            ExtendType::SXTW => ExtendOp::Sxtw,
            ExtendType::SXTX => ExtendOp::Sxtx,
        }
    }


    // ========================================================================
    // Operand Helpers
    // ========================================================================

    /// Convert memory operand to SMIR address
    pub(crate) fn mem_to_addr(&self, mem: &MemOperand, ctx: &mut LiftContext) -> (Address, Vec<SmirOp>) {
        let mut pre_ops = Vec::new();
        let pc = ctx.guest_pc;

        let addr = match &mem.offset {
            MemOffset::None => Address::Direct(self.arm_reg(&mem.base)),
            MemOffset::Imm(off) => Address::BaseOffset {
                base: self.arm_reg(&mem.base),
                offset: *off,
                disp_size: DispSize::Auto,
            },
            MemOffset::Reg(idx) => {
                let tmp = ctx.alloc_vreg();
                let width = self.reg_width(&mem.base);
                pre_ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Add {
                        dst: tmp,
                        src1: self.arm_reg(&mem.base),
                        src2: SrcOperand::Reg(self.arm_reg(idx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));
                Address::Direct(tmp)
            }
            MemOffset::ShiftedReg(sr) => {
                let tmp_shift = ctx.alloc_vreg();
                let tmp_addr = ctx.alloc_vreg();
                let width = self.reg_width(&mem.base);
                let amount = sr
                    .immediate_amount()
                    .expect("A64 memory shifts use an immediate amount");

                pre_ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Shl {
                        dst: tmp_shift,
                        src: self.arm_reg(&sr.reg),
                        amount: SrcOperand::Imm(i64::from(amount)),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));

                pre_ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Add {
                        dst: tmp_addr,
                        src1: self.arm_reg(&mem.base),
                        src2: SrcOperand::Reg(tmp_shift),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));

                Address::Direct(tmp_addr)
            }
            MemOffset::ExtendedReg(er) => {
                let tmp_ext = ctx.alloc_vreg();
                let tmp_addr = ctx.alloc_vreg();
                let width = self.reg_width(&mem.base);

                let (from_width, signed) = match er.extend_type {
                    ExtendType::UXTB => (OpWidth::W8, false),
                    ExtendType::UXTH => (OpWidth::W16, false),
                    ExtendType::UXTW => (OpWidth::W32, false),
                    ExtendType::UXTX => (OpWidth::W64, false),
                    ExtendType::SXTB => (OpWidth::W8, true),
                    ExtendType::SXTH => (OpWidth::W16, true),
                    ExtendType::SXTW => (OpWidth::W32, true),
                    ExtendType::SXTX => (OpWidth::W64, true),
                };

                if signed {
                    pre_ops.push(SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::SignExtend {
                            dst: tmp_ext,
                            src: self.arm_reg(&er.reg),
                            from_width,
                            to_width: width,
                        },
                    ));
                } else {
                    pre_ops.push(SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::ZeroExtend {
                            dst: tmp_ext,
                            src: self.arm_reg(&er.reg),
                            from_width,
                            to_width: width,
                        },
                    ));
                }

                if er.shift > 0 {
                    let tmp_shift = ctx.alloc_vreg();
                    pre_ops.push(SmirOp::new(
                        OpId(1),
                        pc,
                        OpKind::Shl {
                            dst: tmp_shift,
                            src: tmp_ext,
                            amount: SrcOperand::Imm(er.shift as i64),
                            width,
                            flags: FlagUpdate::None,
                        },
                    ));
                    pre_ops.push(SmirOp::new(
                        OpId(2),
                        pc,
                        OpKind::Add {
                            dst: tmp_addr,
                            src1: self.arm_reg(&mem.base),
                            src2: SrcOperand::Reg(tmp_shift),
                            width,
                            flags: FlagUpdate::None,
                        },
                    ));
                } else {
                    pre_ops.push(SmirOp::new(
                        OpId(1),
                        pc,
                        OpKind::Add {
                            dst: tmp_addr,
                            src1: self.arm_reg(&mem.base),
                            src2: SrcOperand::Reg(tmp_ext),
                            width,
                            flags: FlagUpdate::None,
                        },
                    ));
                }

                Address::Direct(tmp_addr)
            }
        };

        (addr, pre_ops)
    }


    pub(crate) fn indexed_access_addr(&self, mem: &MemOperand, addr: Address) -> Address {
        if matches!(
            mem.mode,
            AddressingMode::PreIndex | AddressingMode::PostIndex
        ) {
            Address::Direct(self.arm_reg(&mem.base))
        } else {
            addr
        }
    }


    pub(crate) fn push_mul_op(
        ops: &mut Vec<SmirOp>,
        pc: u64,
        dst_lo: VReg,
        dst_hi: Option<VReg>,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        signed: bool,
    ) {
        let src2 = SrcOperand::Reg(src2);
        let flags = FlagUpdate::None;
        if signed {
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::MulS {
                    dst_lo,
                    dst_hi,
                    src1,
                    src2,
                    width,
                    flags,
                },
            );
        } else {
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::MulU {
                    dst_lo,
                    dst_hi,
                    src1,
                    src2,
                    width,
                    flags,
                },
            );
        }
    }


    pub(crate) fn lift_crc32(
        &self,
        insn: &DecodedInsn,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
    ) {
        if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            let dst = self.dst_reg(rd, ctx);
            let crc = ctx.alloc_vreg();
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::Mov {
                    dst: crc,
                    src: SrcOperand::Reg(self.arm_reg(rn)),
                    width: OpWidth::W32,
                },
            );

            let data_bits = match insn.mnemonic {
                Mnemonic::CRC32B | Mnemonic::CRC32CB => 8,
                Mnemonic::CRC32H | Mnemonic::CRC32CH => 16,
                Mnemonic::CRC32W | Mnemonic::CRC32CW => 32,
                Mnemonic::CRC32X | Mnemonic::CRC32CX => 64,
                _ => return,
            };
            let poly = if matches!(
                insn.mnemonic,
                Mnemonic::CRC32CB | Mnemonic::CRC32CH | Mnemonic::CRC32CW | Mnemonic::CRC32CX
            ) {
                0x82f6_3b78
            } else {
                0xedb8_8320u32
            };
            let source_width = if data_bits == 64 {
                OpWidth::W64
            } else {
                OpWidth::W32
            };

            for byte in 0..(data_bits / 8) {
                let data_byte = ctx.alloc_vreg();
                if byte == 0 {
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Mov {
                            dst: data_byte,
                            src: SrcOperand::Reg(self.arm_reg(rm)),
                            width: source_width,
                        },
                    );
                } else {
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Shr {
                            dst: data_byte,
                            src: self.arm_reg(rm),
                            amount: SrcOperand::Imm((byte * 8) as i64),
                            width: source_width,
                            flags: FlagUpdate::None,
                        },
                    );
                }
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::And {
                        dst: data_byte,
                        src1: data_byte,
                        src2: SrcOperand::Imm(0xff),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Xor {
                        dst: crc,
                        src1: crc,
                        src2: SrcOperand::Reg(data_byte),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                );

                for _ in 0..8 {
                    let bit = ctx.alloc_vreg();
                    let mask = ctx.alloc_vreg();
                    let shifted = ctx.alloc_vreg();
                    let poly_bits = ctx.alloc_vreg();

                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::And {
                            dst: bit,
                            src1: crc,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Sub {
                            dst: mask,
                            src1: VReg::Imm(0),
                            src2: SrcOperand::Reg(bit),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: crc,
                            amount: SrcOperand::Imm(1),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::And {
                            dst: poly_bits,
                            src1: mask,
                            src2: SrcOperand::Imm(poly as i64),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Xor {
                            dst: crc,
                            src1: shifted,
                            src2: SrcOperand::Reg(poly_bits),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                }
            }

            Self::push_lifted_op(
                ops,
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(crc),
                    width: OpWidth::W32,
                },
            );
        }
    }


    pub(crate) fn lift_rev(
        &self,
        insn: &DecodedInsn,
        kind: RevKind,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, rn) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) => (rd, rn),
            _ => return Err(LiftError::Internal("invalid REV operands".to_string())),
        };

        let dst = self.dst_reg(rd, ctx);
        let src = self.arm_reg(rn);
        let width = self.reg_width(rd);

        match kind {
            RevKind::Full => {
                Self::push_lifted_op(ops, pc, OpKind::Bswap { dst, src, width });
            }
            RevKind::Halfwords => {
                let lo = ctx.alloc_vreg();
                let hi = ctx.alloc_vreg();
                let lo_shifted = ctx.alloc_vreg();
                let hi_shifted = ctx.alloc_vreg();
                let (lo_mask, hi_mask) = if width == OpWidth::W64 {
                    (0x00ff_00ff_00ff_00ff_u64, 0xff00_ff00_ff00_ff00_u64)
                } else {
                    (0x00ff_00ff_u64, 0xff00_ff00_u64)
                };

                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::And {
                        dst: lo,
                        src1: src,
                        src2: SrcOperand::Imm64(lo_mask as i64),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::And {
                        dst: hi,
                        src1: src,
                        src2: SrcOperand::Imm64(hi_mask as i64),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Shl {
                        dst: lo_shifted,
                        src: lo,
                        amount: SrcOperand::Imm(8),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Shr {
                        dst: hi_shifted,
                        src: hi,
                        amount: SrcOperand::Imm(8),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Or {
                        dst,
                        src1: lo_shifted,
                        src2: SrcOperand::Reg(hi_shifted),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
            }
            RevKind::Words => {
                if width == OpWidth::W32 {
                    Self::push_lifted_op(ops, pc, OpKind::Bswap { dst, src, width });
                } else {
                    let lo_rev = ctx.alloc_vreg();
                    let hi = ctx.alloc_vreg();
                    let hi_rev = ctx.alloc_vreg();
                    let hi_shifted = ctx.alloc_vreg();

                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Bswap {
                            dst: lo_rev,
                            src,
                            width: OpWidth::W32,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Shr {
                            dst: hi,
                            src,
                            amount: SrcOperand::Imm(32),
                            width,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Bswap {
                            dst: hi_rev,
                            src: hi,
                            width: OpWidth::W32,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Shl {
                            dst: hi_shifted,
                            src: hi_rev,
                            amount: SrcOperand::Imm(32),
                            width,
                            flags: FlagUpdate::None,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Or {
                            dst,
                            src1: hi_shifted,
                            src2: SrcOperand::Reg(lo_rev),
                            width,
                            flags: FlagUpdate::None,
                        },
                    );
                }
            }
        }

        Ok(())
    }


    pub(crate) fn lift_shift(
        &self,
        insn: &DecodedInsn,
        shift_op: ShiftOp,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(amount)) = (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            let dst = self.dst_reg(rd, ctx);
            let width = self.reg_width(rd);
            let flags = if insn.sets_flags {
                FlagUpdate::All
            } else {
                FlagUpdate::None
            };

            let amount_src = match amount {
                Operand::Imm(imm) => {
                    let value = if shift_op == ShiftOp::Lsl
                        && matches!(insn.operands.get(3), Some(Operand::Imm(_)))
                    {
                        i64::from(width.bits()) - imm.value
                    } else {
                        imm.value
                    };
                    SrcOperand::Imm(value)
                }
                Operand::Reg(r) => SrcOperand::Reg(self.arm_reg(r)),
                _ => return Err(LiftError::Internal("invalid shift amount".to_string())),
            };

            let kind = match shift_op {
                ShiftOp::Lsl => OpKind::Shl {
                    dst,
                    src: self.arm_reg(rn),
                    amount: amount_src,
                    width,
                    flags,
                },
                ShiftOp::Lsr => OpKind::Shr {
                    dst,
                    src: self.arm_reg(rn),
                    amount: amount_src,
                    width,
                    flags,
                },
                ShiftOp::Asr => OpKind::Sar {
                    dst,
                    src: self.arm_reg(rn),
                    amount: amount_src,
                    width,
                    flags,
                },
                ShiftOp::Ror | ShiftOp::Rrx => OpKind::Ror {
                    dst,
                    src: self.arm_reg(rn),
                    amount: amount_src,
                    width,
                    flags,
                },
            };

            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        }

        Ok(())
    }


    pub(crate) fn lift_extract(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        if let (
            Some(Operand::Reg(rd)),
            Some(Operand::Reg(rn)),
            Some(Operand::Reg(rm)),
            Some(Operand::Imm(lsb)),
        ) = (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
            insn.operands.get(3),
        ) {
            let dst = self.dst_reg(rd, ctx);
            let width = self.reg_width(rd);
            let amount = lsb.value;

            if amount == 0 {
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Mov {
                        dst,
                        src: SrcOperand::Reg(self.arm_reg(rm)),
                        width,
                    },
                );
                return Ok(());
            }

            let lo = ctx.alloc_vreg();
            let hi = ctx.alloc_vreg();
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::Shr {
                    dst: lo,
                    src: self.arm_reg(rm),
                    amount: SrcOperand::Imm(amount),
                    width,
                    flags: FlagUpdate::None,
                },
            );
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::Shl {
                    dst: hi,
                    src: self.arm_reg(rn),
                    amount: SrcOperand::Imm(i64::from(width.bits()) - amount),
                    width,
                    flags: FlagUpdate::None,
                },
            );
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::Or {
                    dst,
                    src1: lo,
                    src2: SrcOperand::Reg(hi),
                    width,
                    flags: FlagUpdate::None,
                },
            );
        }

        Ok(())
    }


    pub(crate) fn lift_bitfield(
        &self,
        insn: &DecodedInsn,
        kind: BitfieldKind,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let invalid = || LiftError::Internal("invalid bitfield operands".to_string());
        let (rd, rn, immr, imms) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
            insn.operands.get(3),
        ) {
            (
                Some(Operand::Reg(rd)),
                Some(Operand::Reg(rn)),
                Some(Operand::Imm(immr)),
                Some(Operand::Imm(imms)),
            ) => (rd, rn, immr.value, imms.value),
            _ => return Err(invalid()),
        };
        if !(0..=63).contains(&immr) || !(0..=63).contains(&imms) {
            return Err(invalid());
        }

        let dst = self.dst_reg(rd, ctx);
        let dst_in = self.arm_reg(rd);
        let src = self.arm_reg(rn);
        let width = self.reg_width(rd);
        let reg_bits = width.bits() as u8;
        let immr = immr as u8;
        let imms = imms as u8;
        if immr >= reg_bits || imms >= reg_bits {
            return Err(invalid());
        }

        match kind {
            BitfieldKind::Extract { sign_extend } => {
                if imms < immr {
                    return Err(invalid());
                }
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Bfx {
                        dst,
                        src,
                        lsb: immr,
                        width_bits: imms - immr + 1,
                        sign_extend,
                        op_width: width,
                    },
                );
            }
            BitfieldKind::Insert => {
                if imms >= immr {
                    return Err(invalid());
                }
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Bfi {
                        dst,
                        dst_in,
                        src,
                        lsb: reg_bits - immr,
                        width_bits: imms + 1,
                        op_width: width,
                    },
                );
            }
            BitfieldKind::InsertLow => {
                if imms < immr {
                    return Err(invalid());
                }
                let width_bits = imms - immr + 1;
                if width_bits == reg_bits {
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Reg(src),
                            width,
                        },
                    );
                } else {
                    let extracted = ctx.alloc_vreg();
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Bfx {
                            dst: extracted,
                            src,
                            lsb: immr,
                            width_bits,
                            sign_extend: false,
                            op_width: width,
                        },
                    );
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Bfi {
                            dst,
                            dst_in,
                            src: extracted,
                            lsb: 0,
                            width_bits,
                            op_width: width,
                        },
                    );
                }
            }
            BitfieldKind::InsertZero { sign_extend } => {
                if imms >= immr {
                    return Err(invalid());
                }
                let lsb = reg_bits - immr;
                let width_bits = imms + 1;
                let extracted = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Bfx {
                        dst: extracted,
                        src,
                        lsb: 0,
                        width_bits,
                        sign_extend,
                        op_width: width,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Shl {
                        dst,
                        src: extracted,
                        amount: SrcOperand::Imm(i64::from(lsb)),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
            }
        }

        Ok(())
    }


    pub(crate) fn lift_xaflag(&self, pc: u64, ops: &mut Vec<SmirOp>, ctx: &mut LiftContext) {
        let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

        let shl1 = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shl {
                dst: shl1,
                src: nzcv,
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let shl2 = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shl {
                dst: shl2,
                src: nzcv,
                amount: SrcOperand::Imm(2),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let has_c_or_z_as_n = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: has_c_or_z_as_n,
                src1: shl1,
                src2: SrcOperand::Reg(shl2),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let n_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::AndNot {
                dst: n_bit,
                src1: VReg::Imm(NZCV_N),
                src2: SrcOperand::Reg(has_c_or_z_as_n),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let z_raw = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: z_raw,
                src1: nzcv,
                src2: SrcOperand::Imm(NZCV_Z),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let z_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: z_bit,
                src1: z_raw,
                src2: SrcOperand::Reg(shl1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let shr1 = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shr {
                dst: shr1,
                src: nzcv,
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let c_or_z = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: c_or_z,
                src1: nzcv,
                src2: SrcOperand::Reg(shr1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let c_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: c_bit,
                src1: c_or_z,
                src2: SrcOperand::Imm(NZCV_C),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let shr2 = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shr {
                dst: shr2,
                src: nzcv,
                amount: SrcOperand::Imm(2),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let v_unmasked = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::AndNot {
                dst: v_unmasked,
                src1: shr2,
                src2: SrcOperand::Reg(shr1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let v_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: v_bit,
                src1: v_unmasked,
                src2: SrcOperand::Imm(NZCV_V),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let nz = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: nz,
                src1: n_bit,
                src2: SrcOperand::Reg(z_bit),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let cv = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: cv,
                src1: c_bit,
                src2: SrcOperand::Reg(v_bit),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let result = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: result,
                src1: nz,
                src2: SrcOperand::Reg(cv),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: nzcv,
                src: SrcOperand::Reg(result),
                width: OpWidth::W32,
            },
        );
    }


    pub(crate) fn lift_extend(
        &self,
        insn: &DecodedInsn,
        from_width: OpWidth,
        signed: bool,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) =
            (insn.operands.get(0), insn.operands.get(1))
        {
            let dst = self.dst_reg(rd, ctx);
            let to_width = self.reg_width(rd);

            let kind = if signed {
                OpKind::SignExtend {
                    dst,
                    src: self.arm_reg(rn),
                    from_width,
                    to_width,
                }
            } else {
                OpKind::ZeroExtend {
                    dst,
                    src: self.arm_reg(rn),
                    from_width,
                    to_width,
                }
            };

            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        }

        Ok(())
    }
}
