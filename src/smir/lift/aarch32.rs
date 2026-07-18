//! AArch32 (A32/ARM-state) instruction lifter.
//!
//! A32 scalar integer instructions use the same 32-bit SMIR operations and
//! architectural NZCV positions as AArch64.  This lifter deliberately shares
//! the mature scalar operation construction in [`super::aarch64`] while
//! enforcing A32-specific invariants before delegation:
//!
//! - r13 and r14 remain ordinary identity-mapped GPRs (`X13`/`X14`), not the
//!   AArch64 host SP/LR aliases;
//! - r15 data-register reads/writes, predicated data operations, RRX, and the
//!   LSR/ASR-#0 encodings remain fail-closed until their pipeline/conditional/
//!   shifter semantics can be represented without hidden native state;
//! - the complete 16-opcode A32 data-processing register-shifted-register
//!   space uses one compound SMIR operation with an exact low-byte count,
//!   read-before-write aliasing, shifter carry, and arithmetic NZCV contract;
//! - scalar LDM/STM and PUSH/POP forms without r15, user-bank transfer, or
//!   constrained base/list aliases expand into ordered B4 helper operations;
//! - literal scalar loads freeze the architectural `PC + 8` effective address
//!   into W32 absolute-address IR, including subtracting and wrapping forms;
//! - immediate/scaled-register LDRD/STRD forms over an even R0-R13 pair use
//!   pair memory IR so a second-word load fault cannot publish either result;
//! - unconditional and condition-code A32 branch targets use the architectural
//!   `PC + 8` base and become explicit SMIR control-flow edges; all PC
//!   arithmetic wraps modulo 2^32;
//! - BX over r0-r14 becomes a register-indirect SMIR terminator whose
//!   interworking state change is handled by the gated runtime exit path;
//! - BLX immediate/register forms preserve the return-state link bit and carry
//!   the callee execution state explicitly; BLX LR snapshots old LR before the
//!   link write;
//! - A32-only reverse-subtract, multiply-accumulate, and MOVW/MOVT forms are
//!   translated explicitly.

use std::collections::HashSet;

use crate::isa::arm::decoder::{
    Aarch32Decoder, AddressingMode, Condition as ArmCondition, DecodedInsn, MemOffset, MemOperand,
    Mnemonic, Operand, ShiftType,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{ArmDpRegShiftKind, OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, BlockId, Condition, DispSize, FunctionId, GuestAddr, MemWidth, OpId,
    OpWidth, ShiftOp, SignExtend, SourceArch, SrcOperand, VReg,
};
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::aarch64::Aarch64Lifter;
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

/// Fail-closed A32 scalar lifter.
pub struct Aarch32Lifter {
    shared: Aarch64Lifter,
}

impl Aarch32Lifter {
    pub fn new() -> Self {
        Self {
            shared: Aarch64Lifter::strict(),
        }
    }

    #[inline]
    fn reg(num: u8) -> VReg {
        VReg::Arch(ArchReg::Arm(ArmReg::X(num)))
    }

    fn push(ops: &mut Vec<SmirOp>, pc: GuestAddr, kind: OpKind) {
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
    }

    fn pc32(pc: GuestAddr) -> Result<u32, LiftError> {
        u32::try_from(pc).map_err(|_| LiftError::Unsupported {
            addr: pc,
            mnemonic: "A32 guest PC outside the 32-bit address space".to_string(),
        })
    }

    fn add_pc_offset(
        pc: GuestAddr,
        pipeline_bias: u32,
        offset: i64,
    ) -> Result<GuestAddr, LiftError> {
        let pc = Self::pc32(pc)?;
        Ok(u64::from(
            pc.wrapping_add(pipeline_bias).wrapping_add(offset as u32),
        ))
    }

    fn next_pc(pc: GuestAddr, bytes: usize) -> Result<GuestAddr, LiftError> {
        let bytes = u32::try_from(bytes)
            .map_err(|_| LiftError::Internal("A32 instruction length exceeds u32".to_string()))?;
        Ok(u64::from(Self::pc32(pc)?.wrapping_add(bytes)))
    }

    fn operand_src(operand: &Operand) -> Result<SrcOperand, LiftError> {
        match operand {
            Operand::Reg(reg) if reg.num < 15 => Ok(SrcOperand::Reg(Self::reg(reg.num))),
            Operand::Imm(imm) => Ok(SrcOperand::Imm(imm.effective_value())),
            Operand::ShiftedReg(shifted)
                if shifted.reg.num < 15
                    && shifted.shift_type != ShiftType::RRX
                    && matches!(
                        shifted.immediate_amount(),
                        Some(amount)
                            if !(amount == 0
                                && matches!(shifted.shift_type, ShiftType::LSR | ShiftType::ASR))
                    ) =>
            {
                let amount = shifted
                    .immediate_amount()
                    .expect("guard requires immediate A32 scalar shift");
                let shift = match shifted.shift_type {
                    ShiftType::LSL => crate::smir::ir::types::ShiftOp::Lsl,
                    ShiftType::LSR => crate::smir::ir::types::ShiftOp::Lsr,
                    ShiftType::ASR => crate::smir::ir::types::ShiftOp::Asr,
                    ShiftType::ROR => crate::smir::ir::types::ShiftOp::Ror,
                    ShiftType::RRX => unreachable!(),
                };
                Ok(SrcOperand::Shifted {
                    reg: Self::reg(shifted.reg.num),
                    shift,
                    amount,
                })
            }
            _ => Err(LiftError::Internal(
                "unsupported A32 scalar source operand".to_string(),
            )),
        }
    }

    fn rejects_hidden_state(insn: &DecodedInsn) -> bool {
        // A condition-code B has no predicated data effects: it is represented
        // directly as a two-edge SMIR terminator. Every other conditional A32
        // instruction still requires instruction-level commit suppression.
        if insn.cond.is_some() && insn.mnemonic != Mnemonic::B {
            return true;
        }
        insn.operands.iter().any(|operand| match operand {
            Operand::Reg(reg) => reg.num >= 15,
            Operand::ShiftedReg(shifted) => {
                shifted.reg.num >= 15
                    || shifted.shift_type == ShiftType::RRX
                    || matches!(shifted.amount_register(), Some(reg) if reg.num >= 15)
                    || matches!(
                        shifted.immediate_amount(),
                        Some(0) if matches!(shifted.shift_type, ShiftType::LSR | ShiftType::ASR)
                    )
            }
            Operand::Mem(mem) => {
                mem.base.num >= 15
                    || match &mem.offset {
                        MemOffset::None | MemOffset::Imm(_) => false,
                        MemOffset::Reg(reg) => reg.num >= 15,
                        MemOffset::ShiftedReg(shifted) => {
                            shifted.reg.num >= 15
                                || shifted.shift_type == ShiftType::RRX
                                || shifted.amount_register().is_some()
                                || matches!(
                                    shifted.immediate_amount(),
                                    Some(0)
                                        if matches!(
                                            shifted.shift_type,
                                            ShiftType::LSR | ShiftType::ASR | ShiftType::ROR
                                        )
                                )
                        }
                        MemOffset::ExtendedReg(extended) => extended.reg.num >= 15,
                    }
            }
            _ => false,
        })
    }

    /// Lift the complete A32 data-processing register-shifted-register space.
    fn lift_dp_register_shift(
        insn: &DecodedInsn,
        pc: GuestAddr,
        ops: &mut Vec<SmirOp>,
    ) -> Result<bool, LiftError> {
        if insn.state != crate::isa::arm::ExecutionState::Aarch32
            || (insn.raw >> 25) & 0x7 != 0
            || (insn.raw >> 4) & 1 == 0
            || (insn.raw >> 7) & 1 != 0
        {
            return Ok(false);
        }

        let opcode = ((insn.raw >> 21) & 0xf) as u8;
        let kind =
            ArmDpRegShiftKind::from_opcode(opcode).expect("four-bit A32 data-processing opcode");
        let encoded_s = (insn.raw >> 20) & 1 != 0;
        let expected_mnemonic = match (kind, encoded_s) {
            (ArmDpRegShiftKind::And, false) => Mnemonic::AND,
            (ArmDpRegShiftKind::And, true) => Mnemonic::ANDS,
            (ArmDpRegShiftKind::Eor, false) => Mnemonic::EOR,
            (ArmDpRegShiftKind::Eor, true) => Mnemonic::EORS,
            (ArmDpRegShiftKind::Sub, false) => Mnemonic::SUB,
            (ArmDpRegShiftKind::Sub, true) => Mnemonic::SUBS,
            (ArmDpRegShiftKind::Rsb, false) => Mnemonic::RSB,
            (ArmDpRegShiftKind::Rsb, true) => Mnemonic::RSBS,
            (ArmDpRegShiftKind::Add, false) => Mnemonic::ADD,
            (ArmDpRegShiftKind::Add, true) => Mnemonic::ADDS,
            (ArmDpRegShiftKind::Adc, false) => Mnemonic::ADC,
            (ArmDpRegShiftKind::Adc, true) => Mnemonic::ADCS,
            (ArmDpRegShiftKind::Sbc, false) => Mnemonic::SBC,
            (ArmDpRegShiftKind::Sbc, true) => Mnemonic::SBCS,
            (ArmDpRegShiftKind::Rsc, false) => Mnemonic::RSC,
            (ArmDpRegShiftKind::Rsc, true) => Mnemonic::RSCS,
            (ArmDpRegShiftKind::Tst, _) => Mnemonic::TST,
            (ArmDpRegShiftKind::Teq, _) => Mnemonic::TEQ,
            (ArmDpRegShiftKind::Cmp, _) => Mnemonic::CMP,
            (ArmDpRegShiftKind::Cmn, _) => Mnemonic::CMN,
            (ArmDpRegShiftKind::Orr, false) => Mnemonic::ORR,
            (ArmDpRegShiftKind::Orr, true) => Mnemonic::ORRS,
            (ArmDpRegShiftKind::Mov, false) => Mnemonic::MOV,
            (ArmDpRegShiftKind::Mov, true) => Mnemonic::MOVS,
            (ArmDpRegShiftKind::Bic, false) => Mnemonic::BIC,
            (ArmDpRegShiftKind::Bic, true) => Mnemonic::BICS,
            (ArmDpRegShiftKind::Mvn, false) => Mnemonic::MVN,
            (ArmDpRegShiftKind::Mvn, true) => Mnemonic::MVNS,
        };
        if insn.mnemonic != expected_mnemonic {
            return Ok(false);
        }
        let encoded_rn = ((insn.raw >> 16) & 0xf) as u8;
        let encoded_rd = ((insn.raw >> 12) & 0xf) as u8;

        let (dst, rn, shifted) = match (kind.writes_result(), kind.uses_rn()) {
            (true, true) => {
                let [
                    Operand::Reg(rd),
                    Operand::Reg(rn),
                    Operand::ShiftedReg(shifted),
                ] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "malformed A32 register-shifted data-processing operands".to_string(),
                    ));
                };
                (Some(rd), Some(rn), shifted)
            }
            (true, false) => {
                let [Operand::Reg(rd), Operand::ShiftedReg(shifted)] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "malformed A32 register-shifted move operands".to_string(),
                    ));
                };
                (Some(rd), None, shifted)
            }
            (false, true) => {
                let [Operand::Reg(rn), Operand::ShiftedReg(shifted)] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "malformed A32 register-shifted test operands".to_string(),
                    ));
                };
                (None, Some(rn), shifted)
            }
            (false, false) => unreachable!(),
        };

        let Some(rs) = shifted.amount_register() else {
            return Ok(false);
        };
        let fixed_fields_valid =
            (kind.writes_result() || encoded_rd == 0) && (kind.uses_rn() || encoded_rn == 0);
        let flags_valid = if kind.writes_result() {
            insn.sets_flags == encoded_s
        } else {
            encoded_s && insn.sets_flags
        };
        if !fixed_fields_valid
            || !flags_valid
            || dst.is_some_and(|reg| reg.num >= 15)
            || rn.is_some_and(|reg| reg.num >= 15)
            || shifted.reg.num >= 15
            || rs.num >= 15
            || shifted.shift_type == ShiftType::RRX
        {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 register-shifted data processing uses reserved fields or PC"
                    .to_string(),
            });
        }

        let shift = match shifted.shift_type {
            ShiftType::LSL => crate::smir::ir::types::ShiftOp::Lsl,
            ShiftType::LSR => crate::smir::ir::types::ShiftOp::Lsr,
            ShiftType::ASR => crate::smir::ir::types::ShiftOp::Asr,
            ShiftType::ROR => crate::smir::ir::types::ShiftOp::Ror,
            ShiftType::RRX => unreachable!(),
        };
        let flags = if encoded_s || !kind.writes_result() {
            if kind.is_logical() {
                FlagUpdate::Specific(
                    crate::smir::ir::flags::FlagSet::SF
                        .union(crate::smir::ir::flags::FlagSet::ZF)
                        .union(crate::smir::ir::flags::FlagSet::CF),
                )
            } else {
                FlagUpdate::Specific(crate::smir::ir::flags::FlagSet::NZCV)
            }
        } else {
            FlagUpdate::None
        };
        Self::push(
            ops,
            pc,
            OpKind::ArmDpRegShift {
                kind,
                dst: dst.map(|reg| Self::reg(reg.num)),
                rn: rn.map(|reg| Self::reg(reg.num)),
                rm: Self::reg(shifted.reg.num),
                rs: Self::reg(rs.num),
                shift,
                flags,
            },
        );
        Ok(true)
    }

    fn branch_condition(cond: ArmCondition, pc: GuestAddr) -> Result<Condition, LiftError> {
        let cond = match cond {
            ArmCondition::EQ => Condition::Eq,
            ArmCondition::NE => Condition::Ne,
            ArmCondition::CS => Condition::Uge,
            ArmCondition::CC => Condition::Ult,
            ArmCondition::MI => Condition::Negative,
            ArmCondition::PL => Condition::Positive,
            ArmCondition::VS => Condition::Overflow,
            ArmCondition::VC => Condition::NoOverflow,
            ArmCondition::HI => Condition::Ugt,
            ArmCondition::LS => Condition::Ule,
            ArmCondition::GE => Condition::Sge,
            ArmCondition::LT => Condition::Slt,
            ArmCondition::GT => Condition::Sgt,
            ArmCondition::LE => Condition::Sle,
            ArmCondition::AL | ArmCondition::NV => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "A32 conditional branch uses reserved AL/NV condition".to_string(),
                });
            }
        };
        Ok(cond)
    }

    fn memory_kind(mnemonic: Mnemonic) -> Option<(bool, MemWidth, SignExtend)> {
        match mnemonic {
            Mnemonic::LDR => Some((true, MemWidth::B4, SignExtend::Zero)),
            Mnemonic::LDRB => Some((true, MemWidth::B1, SignExtend::Zero)),
            Mnemonic::LDRH => Some((true, MemWidth::B2, SignExtend::Zero)),
            Mnemonic::LDRSB => Some((true, MemWidth::B1, SignExtend::Sign)),
            Mnemonic::LDRSH => Some((true, MemWidth::B2, SignExtend::Sign)),
            Mnemonic::STR => Some((false, MemWidth::B4, SignExtend::Zero)),
            Mnemonic::STRB => Some((false, MemWidth::B1, SignExtend::Zero)),
            Mnemonic::STRH => Some((false, MemWidth::B2, SignExtend::Zero)),
            _ => None,
        }
    }

    fn literal_load(insn: &DecodedInsn, pc: GuestAddr) -> Result<Option<OpKind>, LiftError> {
        let Some((true, width, sign)) = Self::memory_kind(insn.mnemonic) else {
            return Ok(None);
        };
        let [Operand::Reg(rt), Operand::Mem(mem)] = insn.operands.as_slice() else {
            return Ok(None);
        };
        let MemOperand {
            base,
            offset: MemOffset::Imm(offset),
            mode: AddressingMode::Offset,
        } = mem
        else {
            return Ok(None);
        };
        if rt.num >= 15 || base.num != 15 {
            return Ok(None);
        }
        if insn.cond.is_some() {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "predicated A32 literal load".to_string(),
            });
        }
        let address = Self::pc32(pc)?.wrapping_add(8).wrapping_add(*offset as u32);
        Ok(Some(OpKind::Load {
            dst: Self::reg(rt.num),
            addr: Address::Absolute(u64::from(address)),
            width,
            sign,
        }))
    }

    fn shift_op(shift: ShiftType) -> Result<ShiftOp, LiftError> {
        match shift {
            ShiftType::LSL => Ok(ShiftOp::Lsl),
            ShiftType::LSR => Ok(ShiftOp::Lsr),
            ShiftType::ASR => Ok(ShiftOp::Asr),
            ShiftType::ROR => Ok(ShiftOp::Ror),
            ShiftType::RRX => Err(LiftError::Internal(
                "A32 memory RRX escaped the hidden-state gate".to_string(),
            )),
        }
    }

    fn memory_writeback(insn: &DecodedInsn, mem: &MemOperand) -> Result<Option<OpKind>, LiftError> {
        if mem.mode == AddressingMode::Offset {
            return Ok(None);
        }

        let base = Self::reg(mem.base.num);
        let (subtract, src2) = match &mem.offset {
            MemOffset::None => return Ok(None),
            MemOffset::Imm(offset) if *offset < 0 => (true, SrcOperand::Imm(offset.wrapping_neg())),
            MemOffset::Imm(offset) => (false, SrcOperand::Imm(*offset)),
            MemOffset::Reg(index) => (
                ((insn.raw >> 23) & 1) == 0,
                SrcOperand::Reg(Self::reg(index.num)),
            ),
            MemOffset::ShiftedReg(shifted) => {
                let amount = shifted.immediate_amount().ok_or_else(|| {
                    LiftError::Internal(
                        "A32 memory offset has register-specified shift".to_string(),
                    )
                })?;
                (
                    ((insn.raw >> 23) & 1) == 0,
                    SrcOperand::Shifted {
                        reg: Self::reg(shifted.reg.num),
                        shift: Self::shift_op(shifted.shift_type)?,
                        amount,
                    },
                )
            }
            MemOffset::ExtendedReg(_) => {
                return Err(LiftError::Internal(
                    "A32 memory extended-register offset".to_string(),
                ));
            }
        };
        let kind = if subtract {
            OpKind::Sub {
                dst: base,
                src1: base,
                src2,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            }
        } else {
            OpKind::Add {
                dst: base,
                src1: base,
                src2,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            }
        };
        Ok(Some(kind))
    }

    fn memory_address(
        insn: &DecodedInsn,
        mem: &MemOperand,
        pc: GuestAddr,
    ) -> Result<Address, LiftError> {
        let base = Self::reg(mem.base.num);
        if mem.mode == AddressingMode::PostIndex {
            return Ok(Address::Direct(base));
        }

        match &mem.offset {
            MemOffset::None | MemOffset::Imm(0) => Ok(Address::Direct(base)),
            MemOffset::Imm(offset) => Ok(Address::BaseOffset {
                base,
                offset: *offset,
                disp_size: DispSize::Auto,
            }),
            MemOffset::Reg(index) if ((insn.raw >> 23) & 1) != 0 => Ok(Address::BaseIndexScale {
                base: Some(base),
                index: Self::reg(index.num),
                scale: 1,
                disp: 0,
                disp_size: DispSize::Auto,
            }),
            MemOffset::ShiftedReg(shifted)
                if ((insn.raw >> 23) & 1) != 0
                    && shifted.shift_type == ShiftType::LSL
                    && matches!(shifted.immediate_amount(), Some(amount) if amount <= 3) =>
            {
                let amount = shifted
                    .immediate_amount()
                    .expect("guard requires immediate A32 memory shift");
                Ok(Address::BaseIndexScale {
                    base: Some(base),
                    index: Self::reg(shifted.reg.num),
                    scale: 1 << amount,
                    disp: 0,
                    disp_size: DispSize::Auto,
                })
            }
            _ => Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 pre/offset register address not representable without a temporary"
                    .to_string(),
            }),
        }
    }

    fn lift_memory(
        &self,
        insn: &DecodedInsn,
        pc: GuestAddr,
        ops: &mut Vec<SmirOp>,
    ) -> Result<(), LiftError> {
        let Some((is_load, width, sign)) = Self::memory_kind(insn.mnemonic) else {
            return Err(LiftError::Internal(
                "invalid A32 scalar memory mnemonic".to_string(),
            ));
        };
        let [Operand::Reg(rt), Operand::Mem(mem)] = insn.operands.as_slice() else {
            return Err(LiftError::Internal(
                "invalid A32 scalar memory operands".to_string(),
            ));
        };
        let writeback = Self::memory_writeback(insn, mem)?;
        if is_load && writeback.is_some() && rt.num == mem.base.num {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 load writeback aliases its destination".to_string(),
            });
        }
        let addr = Self::memory_address(insn, mem, pc)?;
        let kind = if is_load {
            OpKind::Load {
                dst: Self::reg(rt.num),
                addr,
                width,
                sign,
            }
        } else {
            OpKind::Store {
                src: Self::reg(rt.num),
                addr,
                width,
            }
        };
        Self::push(ops, pc, kind);
        // Both pre- and post-index writeback follow the helper access. A helper
        // fault exits from the memory op, so the writeback remains uncommitted.
        if let Some(writeback) = writeback {
            Self::push(ops, pc, writeback);
        }
        Ok(())
    }

    fn lift_double_memory(
        &self,
        insn: &DecodedInsn,
        pc: GuestAddr,
        ops: &mut Vec<SmirOp>,
    ) -> Result<(), LiftError> {
        let [Operand::Reg(rt), Operand::Mem(mem)] = insn.operands.as_slice() else {
            return Err(LiftError::Internal(
                "invalid A32 double-transfer operands".to_string(),
            ));
        };
        if rt.num >= 14 || rt.num & 1 != 0 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 double transfer requires an even R0-R13 pair".to_string(),
            });
        }
        let is_load = insn.mnemonic == Mnemonic::LDP;
        let rt2 = rt.num + 1;
        let writeback = Self::memory_writeback(insn, mem)?;
        if writeback.is_some() && (mem.base.num == rt.num || mem.base.num == rt2) {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 double transfer has a constrained base/pair alias".to_string(),
            });
        }
        let addr = Self::memory_address(insn, mem, pc)?;
        Self::push(
            ops,
            pc,
            if is_load {
                OpKind::LoadPair {
                    dst1: Self::reg(rt.num),
                    dst2: Self::reg(rt2),
                    addr,
                    width: MemWidth::B4,
                }
            } else {
                OpKind::StorePair {
                    src1: Self::reg(rt.num),
                    src2: Self::reg(rt2),
                    addr,
                    width: MemWidth::B4,
                }
            },
        );
        if let Some(writeback) = writeback {
            Self::push(ops, pc, writeback);
        }
        Ok(())
    }

    fn multiple_kind(mnemonic: Mnemonic) -> Option<(bool, bool, bool)> {
        use Mnemonic::*;

        match mnemonic {
            LDM | LDMIA | POP => Some((true, true, false)),
            LDMIB => Some((true, true, true)),
            LDMDA => Some((true, false, false)),
            LDMDB => Some((true, false, true)),
            STM | STMIA => Some((false, true, false)),
            STMIB => Some((false, true, true)),
            STMDA => Some((false, false, false)),
            STMDB | PUSH => Some((false, false, true)),
            _ => None,
        }
    }

    fn lift_multiple_memory(
        &self,
        insn: &DecodedInsn,
        pc: GuestAddr,
        ops: &mut Vec<SmirOp>,
    ) -> Result<(), LiftError> {
        let Some((is_load, increment, before)) = Self::multiple_kind(insn.mnemonic) else {
            return Err(LiftError::Internal(
                "invalid A32 multiple-transfer mnemonic".to_string(),
            ));
        };
        let push_pop = matches!(insn.mnemonic, Mnemonic::PUSH | Mnemonic::POP);
        let (base_num, list) = match insn.operands.as_slice() {
            [Operand::RegList(list)] if push_pop => (13, list),
            [Operand::Reg(base), Operand::RegList(list)] if !push_pop => (base.num, list),
            _ => {
                return Err(LiftError::Internal(
                    "invalid A32 multiple-transfer operands".to_string(),
                ));
            }
        };
        let mask = list.mask;
        let writeback = push_pop || ((insn.raw >> 21) & 1) != 0;

        if base_num >= 15 || mask == 0 || list.contains(15) || ((insn.raw >> 22) & 1) != 0 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 multiple transfer requires PC, user-bank, or empty-list semantics"
                    .to_string(),
            });
        }
        if (is_load && list.contains(base_num))
            || (!is_load && writeback && list.contains(base_num))
        {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 multiple transfer has a constrained base/list alias".to_string(),
            });
        }

        let base = Self::reg(base_num);
        let count = i64::from(list.count());
        let low_offset = match (increment, before) {
            (true, false) => 0,
            (true, true) => 4,
            (false, false) => 4 - count * 4,
            (false, true) => -count * 4,
        };

        for (ordinal, reg_num) in list.iter().enumerate() {
            let offset = low_offset + ordinal as i64 * 4;
            let addr = if offset == 0 {
                Address::Direct(base)
            } else {
                Address::BaseOffset {
                    base,
                    offset,
                    disp_size: DispSize::Auto,
                }
            };
            Self::push(
                ops,
                pc,
                if is_load {
                    OpKind::Load {
                        dst: Self::reg(reg_num),
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    }
                } else {
                    OpKind::Store {
                        src: Self::reg(reg_num),
                        addr,
                        width: MemWidth::B4,
                    }
                },
            );
        }

        if writeback {
            let delta = count * 4;
            Self::push(
                ops,
                pc,
                if increment {
                    OpKind::Add {
                        dst: base,
                        src1: base,
                        src2: SrcOperand::Imm(delta),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    }
                } else {
                    OpKind::Sub {
                        dst: base,
                        src1: base,
                        src2: SrcOperand::Imm(delta),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    }
                },
            );
        }
        Ok(())
    }

    fn shared_scalar_mnemonic(insn: &DecodedInsn) -> bool {
        use Mnemonic::*;

        match insn.mnemonic {
            ADD | ADDS | ADC | ADCS | SUB | SUBS | SBC | SBCS | CMP | CMN | CLZ | RBIT | REV
            | REV16 | UDIV | SDIV | NOP => true,
            MOV => {
                !insn.sets_flags && !matches!(insn.operands.get(1), Some(Operand::ShiftedReg(_)))
            }
            AND | ORR | EOR | BIC | MVN | MUL => !insn.sets_flags,
            _ => false,
        }
    }

    fn bitfield_fields(insn: &DecodedInsn, pc: GuestAddr) -> Result<(u8, u8, u8), LiftError> {
        let rn = (insn.raw & 0xf) as u8;
        let lsb = ((insn.raw >> 7) & 0x1f) as u8;
        let encoded_width = ((insn.raw >> 16) & 0x1f) as u8;
        if rn >= 15 && insn.mnemonic != Mnemonic::BFC {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "A32 bitfield source PC".to_string(),
            });
        }
        Ok((rn, lsb, encoded_width))
    }

    fn lift_decoded(
        &self,
        insn: &DecodedInsn,
        pc: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        if let Some(literal) = Self::literal_load(insn, pc)? {
            let mut ops = Vec::new();
            Self::push(&mut ops, pc, literal);
            return Ok((ops, ControlFlow::Fallthrough));
        }
        if Self::rejects_hidden_state(insn) {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!(
                    "A32 {:?} requires PC, predication, or special shifter state",
                    insn.mnemonic
                ),
            });
        }

        let mut ops = Vec::new();
        if Self::lift_dp_register_shift(insn, pc, &mut ops)? {
            return Ok((ops, ControlFlow::Fallthrough));
        }
        if Self::shared_scalar_mnemonic(insn) {
            return self.shared.lift_insn_inner(insn, pc, ctx);
        }

        let control = match insn.mnemonic {
            Mnemonic::LDR
            | Mnemonic::LDRB
            | Mnemonic::LDRH
            | Mnemonic::LDRSB
            | Mnemonic::LDRSH
            | Mnemonic::STR
            | Mnemonic::STRB
            | Mnemonic::STRH => {
                self.lift_memory(insn, pc, &mut ops)?;
                ControlFlow::Fallthrough
            }
            Mnemonic::LDP | Mnemonic::STP => {
                self.lift_double_memory(insn, pc, &mut ops)?;
                ControlFlow::Fallthrough
            }
            Mnemonic::LDM
            | Mnemonic::LDMIA
            | Mnemonic::LDMIB
            | Mnemonic::LDMDA
            | Mnemonic::LDMDB
            | Mnemonic::STM
            | Mnemonic::STMIA
            | Mnemonic::STMIB
            | Mnemonic::STMDA
            | Mnemonic::STMDB
            | Mnemonic::PUSH
            | Mnemonic::POP => {
                self.lift_multiple_memory(insn, pc, &mut ops)?;
                ControlFlow::Fallthrough
            }
            Mnemonic::MOV if !insn.sets_flags => {
                let [Operand::Reg(rd), Operand::ShiftedReg(shifted)] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid A32 shifted MOV operands".to_string(),
                    ));
                };
                let dst = Self::reg(rd.num);
                let src = Self::reg(shifted.reg.num);
                let amount =
                    SrcOperand::Imm(i64::from(shifted.immediate_amount().ok_or_else(|| {
                        LiftError::Internal(
                            "A32 MOV register shift escaped dedicated lifting".to_string(),
                        )
                    })?));
                let flags = FlagUpdate::None;
                let kind = match shifted.shift_type {
                    ShiftType::LSL => OpKind::Shl {
                        dst,
                        src,
                        amount,
                        width: OpWidth::W32,
                        flags,
                    },
                    ShiftType::LSR => OpKind::Shr {
                        dst,
                        src,
                        amount,
                        width: OpWidth::W32,
                        flags,
                    },
                    ShiftType::ASR => OpKind::Sar {
                        dst,
                        src,
                        amount,
                        width: OpWidth::W32,
                        flags,
                    },
                    ShiftType::ROR => OpKind::Ror {
                        dst,
                        src,
                        amount,
                        width: OpWidth::W32,
                        flags,
                    },
                    ShiftType::RRX => unreachable!(),
                };
                Self::push(&mut ops, pc, kind);
                ControlFlow::Fallthrough
            }
            Mnemonic::RSB | Mnemonic::RSBS => {
                let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(operand2)) = (
                    insn.operands.first(),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) else {
                    return Err(LiftError::Internal("invalid A32 RSB operands".to_string()));
                };
                let dst = Self::reg(rd.num);
                let rn = Self::reg(rn.num);
                match Self::operand_src(operand2)? {
                    SrcOperand::Reg(src) => Self::push(
                        &mut ops,
                        pc,
                        OpKind::Sub {
                            dst,
                            src1: src,
                            src2: SrcOperand::Reg(rn),
                            width: OpWidth::W32,
                            flags: if insn.sets_flags {
                                FlagUpdate::All
                            } else {
                                FlagUpdate::None
                            },
                        },
                    ),
                    SrcOperand::Imm(imm) if !insn.sets_flags => {
                        Self::push(
                            &mut ops,
                            pc,
                            OpKind::Neg {
                                dst,
                                src: rn,
                                width: OpWidth::W32,
                                flags: FlagUpdate::None,
                            },
                        );
                        Self::push(
                            &mut ops,
                            pc,
                            OpKind::Add {
                                dst,
                                src1: dst,
                                src2: SrcOperand::Imm(imm),
                                width: OpWidth::W32,
                                flags: FlagUpdate::None,
                            },
                        );
                    }
                    SrcOperand::Shifted { .. } if !insn.sets_flags && dst != rn => {
                        Self::push(
                            &mut ops,
                            pc,
                            OpKind::Mov {
                                dst,
                                src: Self::operand_src(operand2)?,
                                width: OpWidth::W32,
                            },
                        );
                        Self::push(
                            &mut ops,
                            pc,
                            OpKind::Sub {
                                dst,
                                src1: dst,
                                src2: SrcOperand::Reg(rn),
                                width: OpWidth::W32,
                                flags: FlagUpdate::None,
                            },
                        );
                    }
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: "A32 flag-setting or aliased shifted RSB".to_string(),
                        });
                    }
                }
                ControlFlow::Fallthrough
            }
            Mnemonic::MLA | Mnemonic::MLS if !insn.sets_flags => {
                let [
                    Operand::Reg(rd),
                    Operand::Reg(rm),
                    Operand::Reg(rs),
                    Operand::Reg(rn),
                ] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid A32 multiply-accumulate operands".to_string(),
                    ));
                };
                let kind = if insn.mnemonic == Mnemonic::MLA {
                    OpKind::MulAdd {
                        dst: Self::reg(rd.num),
                        acc: Self::reg(rn.num),
                        src1: Self::reg(rm.num),
                        src2: Self::reg(rs.num),
                        width: OpWidth::W32,
                    }
                } else {
                    OpKind::MulSub {
                        dst: Self::reg(rd.num),
                        acc: Self::reg(rn.num),
                        src1: Self::reg(rm.num),
                        src2: Self::reg(rs.num),
                        width: OpWidth::W32,
                    }
                };
                Self::push(&mut ops, pc, kind);
                ControlFlow::Fallthrough
            }
            Mnemonic::UMULL | Mnemonic::SMULL if !insn.sets_flags => {
                let [
                    Operand::Reg(lo),
                    Operand::Reg(hi),
                    Operand::Reg(rm),
                    Operand::Reg(rs),
                ] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid A32 long-multiply operands".to_string(),
                    ));
                };
                let args = (
                    Self::reg(lo.num),
                    Some(Self::reg(hi.num)),
                    Self::reg(rm.num),
                    SrcOperand::Reg(Self::reg(rs.num)),
                    OpWidth::W32,
                    FlagUpdate::None,
                );
                let kind = if insn.mnemonic == Mnemonic::UMULL {
                    OpKind::MulU {
                        dst_lo: args.0,
                        dst_hi: args.1,
                        src1: args.2,
                        src2: args.3,
                        width: args.4,
                        flags: args.5,
                    }
                } else {
                    OpKind::MulS {
                        dst_lo: args.0,
                        dst_hi: args.1,
                        src1: args.2,
                        src2: args.3,
                        width: args.4,
                        flags: args.5,
                    }
                };
                Self::push(&mut ops, pc, kind);
                ControlFlow::Fallthrough
            }
            Mnemonic::UBFX | Mnemonic::SBFX => {
                let Some(Operand::Reg(rd)) = insn.operands.first() else {
                    return Err(LiftError::Internal(
                        "invalid A32 bitfield-extract operands".to_string(),
                    ));
                };
                let (rn, lsb, encoded_width) = Self::bitfield_fields(insn, pc)?;
                let width_bits = encoded_width + 1;
                if u16::from(lsb) + u16::from(width_bits) > 32 {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "A32 bitfield-extract bounds".to_string(),
                    });
                }
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Bfx {
                        dst: Self::reg(rd.num),
                        src: Self::reg(rn),
                        lsb,
                        width_bits,
                        sign_extend: insn.mnemonic == Mnemonic::SBFX,
                        op_width: OpWidth::W32,
                    },
                );
                ControlFlow::Fallthrough
            }
            Mnemonic::BFI | Mnemonic::BFC => {
                let Some(Operand::Reg(rd)) = insn.operands.first() else {
                    return Err(LiftError::Internal(
                        "invalid A32 bitfield-insert operands".to_string(),
                    ));
                };
                let (rn, lsb, msb) = Self::bitfield_fields(insn, pc)?;
                if msb < lsb {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "A32 bitfield-insert bounds".to_string(),
                    });
                }
                let width_bits = msb - lsb + 1;
                let dst = Self::reg(rd.num);
                if insn.mnemonic == Mnemonic::BFC {
                    let field_mask = if width_bits == 32 {
                        u32::MAX
                    } else {
                        ((1u32 << width_bits) - 1) << lsb
                    };
                    Self::push(
                        &mut ops,
                        pc,
                        OpKind::And {
                            dst,
                            src1: dst,
                            src2: SrcOperand::Imm(i64::from(!field_mask)),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                } else {
                    Self::push(
                        &mut ops,
                        pc,
                        OpKind::Bfi {
                            dst,
                            dst_in: dst,
                            src: Self::reg(rn),
                            lsb,
                            width_bits,
                            op_width: OpWidth::W32,
                        },
                    );
                }
                ControlFlow::Fallthrough
            }
            Mnemonic::MOVZ => {
                let Some(Operand::Reg(rd)) = insn.operands.first() else {
                    return Err(LiftError::Internal("invalid A32 MOVW operands".to_string()));
                };
                let imm16 = (((insn.raw >> 16) & 0xf) << 12) | (insn.raw & 0xfff);
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Mov {
                        dst: Self::reg(rd.num),
                        src: SrcOperand::Imm(i64::from(imm16)),
                        width: OpWidth::W32,
                    },
                );
                ControlFlow::Fallthrough
            }
            Mnemonic::MOVK => {
                let Some(Operand::Reg(rd)) = insn.operands.first() else {
                    return Err(LiftError::Internal("invalid A32 MOVT operands".to_string()));
                };
                let dst = Self::reg(rd.num);
                let imm16 = (((insn.raw >> 16) & 0xf) << 12) | (insn.raw & 0xfff);
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::And {
                        dst,
                        src1: dst,
                        src2: SrcOperand::Imm(0xffff),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                );
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Or {
                        dst,
                        src1: dst,
                        src2: SrcOperand::Imm(i64::from(imm16 << 16)),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                );
                ControlFlow::Fallthrough
            }
            Mnemonic::B => {
                let Some(Operand::Label(offset)) = insn.operands.first() else {
                    return Err(LiftError::Internal("invalid A32 B operands".to_string()));
                };
                let target = Self::add_pc_offset(pc, 8, *offset)?;
                if let Some(cond) = insn.cond {
                    ControlFlow::CondBranch {
                        cond: Self::branch_condition(cond, pc)?,
                        target,
                        fallthrough: Self::next_pc(pc, 4)?,
                    }
                } else {
                    ControlFlow::Branch { target }
                }
            }
            Mnemonic::BL => {
                let Some(Operand::Label(offset)) = insn.operands.first() else {
                    return Err(LiftError::Internal("invalid A32 BL operands".to_string()));
                };
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Mov {
                        dst: Self::reg(14),
                        src: SrcOperand::Imm(Self::next_pc(pc, 4)? as i64),
                        width: OpWidth::W32,
                    },
                );
                ControlFlow::Call {
                    target: CallTarget::GuestAddr(Self::add_pc_offset(pc, 8, *offset)?),
                }
            }
            Mnemonic::BLX => match insn.operands.first() {
                Some(Operand::Label(offset)) => {
                    Self::push(
                        &mut ops,
                        pc,
                        OpKind::Mov {
                            dst: Self::reg(14),
                            src: SrcOperand::Imm(Self::next_pc(pc, 4)? as i64),
                            width: OpWidth::W32,
                        },
                    );
                    ControlFlow::Call {
                        target: CallTarget::GuestAddrInterworking {
                            addr: Self::add_pc_offset(pc, 8, *offset)?,
                            thumb: true,
                        },
                    }
                }
                Some(Operand::Reg(rm)) => {
                    // BLX LR must consume the old LR before installing its return
                    // address. Preserve that data dependency explicitly in SMIR;
                    // other registers remain unchanged by the link write.
                    let target = if rm.num == 14 {
                        let snapshot = ctx.alloc_vreg();
                        Self::push(
                            &mut ops,
                            pc,
                            OpKind::Mov {
                                dst: snapshot,
                                src: SrcOperand::Reg(Self::reg(14)),
                                width: OpWidth::W32,
                            },
                        );
                        snapshot
                    } else {
                        Self::reg(rm.num)
                    };
                    Self::push(
                        &mut ops,
                        pc,
                        OpKind::Mov {
                            dst: Self::reg(14),
                            src: SrcOperand::Imm(Self::next_pc(pc, 4)? as i64),
                            width: OpWidth::W32,
                        },
                    );
                    ControlFlow::Call {
                        target: CallTarget::IndirectInterworking(target),
                    }
                }
                _ => {
                    return Err(LiftError::Internal("invalid A32 BLX operands".to_string()));
                }
            },
            Mnemonic::BX => {
                let Some(Operand::Reg(rm)) = insn.operands.first() else {
                    return Err(LiftError::Internal("invalid A32 BX operands".to_string()));
                };
                ControlFlow::IndirectBranch {
                    target: Self::reg(rm.num),
                }
            }
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("A32 {:?}", insn.mnemonic),
                });
            }
        };

        Ok((ops, control))
    }

    fn result(ops: Vec<SmirOp>, bytes_consumed: usize, control_flow: ControlFlow) -> LiftResult {
        let branch_targets = match &control_flow {
            ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => vec![*target],
            ControlFlow::CondBranch {
                target,
                fallthrough,
                ..
            } => vec![*target, *fallthrough],
            ControlFlow::CondBranchReg {
                taken, not_taken, ..
            } => vec![*taken, *not_taken],
            ControlFlow::Call {
                target: CallTarget::GuestAddr(target),
            } => vec![*target],
            ControlFlow::Call {
                target: CallTarget::GuestAddrInterworking { addr, .. },
            } => vec![*addr],
            _ => Vec::new(),
        };
        LiftResult {
            ops,
            bytes_consumed,
            control_flow,
            branch_targets,
        }
    }
}

impl Default for Aarch32Lifter {
    fn default() -> Self {
        Self::new()
    }
}

impl SmirLifter for Aarch32Lifter {
    fn source_arch(&self) -> SourceArch {
        SourceArch::Aarch32
    }

    fn lift_insn(
        &mut self,
        addr: GuestAddr,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        Self::pc32(addr)?;
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 4,
            });
        }
        let raw = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let insn = Aarch32Decoder::decode(raw).map_err(|_| LiftError::InvalidEncoding {
            addr,
            bytes: bytes[..4].to_vec(),
        })?;
        ctx.guest_pc = addr;
        let (ops, control) = self.lift_decoded(&insn, addr, ctx)?;
        Ok(Self::result(ops, 4, control))
    }

    fn lift_block(
        &mut self,
        addr: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirBlock, LiftError> {
        let block_id = ctx.get_or_create_block(addr);
        let mut ops = Vec::new();
        let mut pc = addr;

        loop {
            let bytes = mem
                .read(pc, 4)
                .map_err(|error| LiftError::MemoryError { addr: pc, error })?;
            let result = self.lift_insn(pc, &bytes, ctx)?;
            let insn_pc = pc;
            pc = Self::next_pc(pc, result.bytes_consumed)?;
            for mut op in result.ops {
                op.id = OpId(ops.len() as u16);
                ops.push(op);
            }
            if !result.control_flow.ends_block() {
                continue;
            }

            let terminator = match result.control_flow {
                ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
                    Terminator::Branch {
                        target: ctx.get_or_create_block(target),
                    }
                }
                ControlFlow::Call { target } => Terminator::Call {
                    target,
                    args: Vec::new(),
                    continuation: ctx.get_or_create_block(pc),
                },
                ControlFlow::CondBranch {
                    cond,
                    target,
                    fallthrough,
                } => {
                    let cond_vreg = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        insn_pc,
                        OpKind::TestCondition {
                            dst: cond_vreg,
                            cond,
                        },
                    ));
                    Terminator::CondBranch {
                        cond: cond_vreg,
                        true_target: ctx.get_or_create_block(target),
                        false_target: ctx.get_or_create_block(fallthrough),
                    }
                }
                ControlFlow::Return => Terminator::Return { values: Vec::new() },
                ControlFlow::IndirectBranch { target } => Terminator::IndirectBranch {
                    target,
                    possible_targets: Vec::new(),
                },
                ControlFlow::Trap { kind } => Terminator::Trap { kind },
                ControlFlow::Syscall => Terminator::Trap {
                    kind: TrapKind::SystemCall,
                },
                ControlFlow::CondBranchReg { .. } | ControlFlow::IndirectBranchMem { .. } => {
                    return Err(LiftError::Unsupported {
                        addr: insn_pc,
                        mnemonic: "A32 block terminator".to_string(),
                    });
                }
                ControlFlow::Fallthrough | ControlFlow::NextInsn => unreachable!(),
            };
            return Ok(SmirBlock {
                id: block_id,
                guest_pc: addr,
                phis: Vec::new(),
                ops,
                terminator,
                exec_count: 0,
            });
        }
    }

    fn lift_function(
        &mut self,
        entry: GuestAddr,
        mem: &dyn MemoryReader,
        ctx: &mut LiftContext,
    ) -> Result<SmirFunction, LiftError> {
        let id = FunctionId(ctx.known_functions.len() as u32);
        ctx.known_functions.insert(entry, id);
        let mut worklist = vec![entry];
        let mut visited = HashSet::new();
        let mut blocks = Vec::new();

        while let Some(addr) = worklist.pop() {
            if !visited.insert(addr) {
                continue;
            }
            let block = self.lift_block(addr, mem, ctx)?;
            for successor in block.successors() {
                if let Some((&successor_addr, _)) =
                    ctx.block_cache.iter().find(|(_, id)| **id == successor)
                {
                    if !visited.contains(&successor_addr) {
                        worklist.push(successor_addr);
                    }
                }
            }
            blocks.push(block);
        }

        let min = blocks
            .iter()
            .map(|block| block.guest_pc)
            .min()
            .unwrap_or(entry);
        let max = blocks
            .iter()
            .map(|block| {
                block
                    .guest_pc
                    .wrapping_add((block.ops.len().max(1) * 4) as u64)
            })
            .max()
            .unwrap_or(entry.wrapping_add(4));
        Ok(SmirFunction {
            id,
            entry: ctx.get_or_create_block(entry),
            blocks,
            locals: Vec::new(),
            guest_range: (min, max),
            calling_convention: CallingConv::GuestPreserveAll,
            attrs: FunctionAttrs::default(),
            x86_instruction_bytes: std::collections::HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::flags::FlagSet;

    fn lift(raw: u32) -> LiftResult {
        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        lifter
            .lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx)
            .unwrap()
    }

    #[test]
    fn lifts_a32_scalar_integer_matrix() {
        let cases = [
            (0xe081_0002, "add"),
            (0xe054_3385, "subs-shifted"),
            (0xe2a7_60ff, "adc-immediate"),
            (0xe0c9_800a, "sbc"),
            (0xe26c_b007, "rsb"),
            (0xe002_14e3, "and-ror"),
            (0xe385_4102, "orr-immediate"),
            (0xe027_6008, "eor"),
            (0xe1ca_900b, "bic"),
            (0xe1a0_0241, "mov-asr"),
            (0xe1e0_2003, "mvn"),
            (0xe004_0695, "mul"),
            (0xe027_a998, "mla"),
            (0xe06b_109c, "mls"),
            (0xe16f_2f13, "clz"),
            (0xe6bf_4f35, "rev"),
            (0xe6bf_6fb7, "rev16"),
            (0xe6ff_8f39, "rbit"),
            (0xe7cb_021f, "bfc"),
            (0xe7cf_1412, "bfi"),
            (0xe7e6_3654, "ubfx"),
            (0xe7a7_5856, "sbfx"),
            (0xe730_fa11, "udiv"),
            (0xe713_fb14, "sdiv"),
            (0xe30b_aeef, "movw"),
            (0xe34c_aafe, "movt"),
        ];
        for (raw, label) in cases {
            let result = lift(raw);
            assert!(
                !result.ops.is_empty(),
                "{label} must produce a concrete SMIR operation"
            );
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        }
    }

    fn encode_dp_register_shift(
        opcode: u8,
        set_flags: bool,
        rd: u8,
        rn: u8,
        rm: u8,
        rs: u8,
        shift: u8,
    ) -> u32 {
        0xe000_0000
            | (u32::from(opcode) << 21)
            | (u32::from(set_flags) << 20)
            | (u32::from(rn) << 16)
            | (u32::from(rd) << 12)
            | (u32::from(rs) << 8)
            | (u32::from(shift) << 5)
            | (1 << 4)
            | u32::from(rm)
    }

    #[test]
    fn lifts_complete_a32_data_processing_register_shift_opcode_space() {
        let nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);
        for opcode in 0_u8..16 {
            let kind = ArmDpRegShiftKind::from_opcode(opcode).unwrap();
            let rd = if kind.writes_result() { 2 } else { 0 };
            let rn = if kind.uses_rn() { 1 } else { 0 };
            let flag_modes: &[bool] = if kind.writes_result() {
                &[false, true]
            } else {
                &[true]
            };
            for &set_flags in flag_modes {
                for (shift_bits, expected_shift) in [
                    (0_u8, ShiftOp::Lsl),
                    (1, ShiftOp::Lsr),
                    (2, ShiftOp::Asr),
                    (3, ShiftOp::Ror),
                ] {
                    let raw = encode_dp_register_shift(opcode, set_flags, rd, rn, 4, 3, shift_bits);
                    let result = lift(raw);
                    assert_eq!(result.ops.len(), 1, "raw={raw:#010x}");
                    assert!(
                        matches!(
                            result.ops[0].kind,
                            OpKind::ArmDpRegShift {
                                kind: actual_kind,
                                dst,
                                rn: actual_rn,
                                rm,
                                rs,
                                shift,
                                flags,
                            } if actual_kind == kind
                                && dst == kind.writes_result().then(|| Aarch32Lifter::reg(2))
                                && actual_rn == kind.uses_rn().then(|| Aarch32Lifter::reg(1))
                                && rm == Aarch32Lifter::reg(4)
                                && rs == Aarch32Lifter::reg(3)
                                && shift == expected_shift
                                && flags == if set_flags {
                                    FlagUpdate::Specific(if kind.is_logical() { nzc } else { FlagSet::NZCV })
                                } else {
                                    FlagUpdate::None
                                }
                        ),
                        "raw={raw:#010x}: {:?}",
                        result.ops[0].kind
                    );
                }
            }
        }
    }

    #[test]
    fn a32_data_processing_register_shift_rejects_pc_and_noncanonical_fixed_fields() {
        let cases = [
            encode_dp_register_shift(4, false, 15, 1, 2, 3, 0),
            encode_dp_register_shift(4, false, 0, 15, 2, 3, 0),
            encode_dp_register_shift(4, false, 0, 1, 15, 3, 0),
            encode_dp_register_shift(4, false, 0, 1, 2, 15, 0),
            encode_dp_register_shift(8, true, 1, 1, 2, 3, 0),
            encode_dp_register_shift(13, false, 1, 1, 2, 3, 0),
        ];
        for raw in cases {
            let mut lifter = Aarch32Lifter::new();
            let mut ctx = LiftContext::new(SourceArch::Aarch32);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                    Err(LiftError::Unsupported { .. })
                ),
                "raw={raw:#010x}"
            );
        }
    }

    #[test]
    fn a32_register_shift_recognition_does_not_steal_miscellaneous_encodings() {
        for raw in [
            0xe16f_2f13_u32, // CLZ r2,r3
            0xe12f_ff1e,     // BX lr
            0xe12f_ff3e,     // BLX lr
        ] {
            let result = lift(raw);
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| !matches!(op.kind, OpKind::ArmDpRegShift { .. })),
                "raw={raw:#010x}"
            );
        }
    }

    #[test]
    fn lifts_a32_scalar_memory_width_sign_and_store_matrix() {
        let cases = [
            (0xe591_0004, MemWidth::B4, SignExtend::Zero, true), // ldr r0,[r1,#4]
            (0xe5d3_2002, MemWidth::B1, SignExtend::Zero, true), // ldrb r2,[r3,#2]
            (0xe1d5_40b2, MemWidth::B2, SignExtend::Zero, true), // ldrh r4,[r5,#2]
            (0xe1d7_60d1, MemWidth::B1, SignExtend::Sign, true), // ldrsb r6,[r7,#1]
            (0xe1d9_80f2, MemWidth::B2, SignExtend::Sign, true), // ldrsh r8,[r9,#2]
            (0xe58b_0004, MemWidth::B4, SignExtend::Zero, false), // str r0,[r11,#4]
            (0xe5ca_2001, MemWidth::B1, SignExtend::Zero, false), // strb r2,[r10,#1]
            (0xe1cc_40b2, MemWidth::B2, SignExtend::Zero, false), // strh r4,[r12,#2]
        ];

        for (raw, width, sign, is_load) in cases {
            let result = lift(raw);
            assert_eq!(result.ops.len(), 1, "{raw:#010x}");
            match &result.ops[0].kind {
                OpKind::Load {
                    width: actual_width,
                    sign: actual_sign,
                    ..
                } if is_load => {
                    assert_eq!(*actual_width, width, "{raw:#010x}");
                    assert_eq!(*actual_sign, sign, "{raw:#010x}");
                }
                OpKind::Store {
                    width: actual_width,
                    ..
                } if !is_load => assert_eq!(*actual_width, width, "{raw:#010x}"),
                other => panic!("unexpected memory lift for {raw:#010x}: {other:?}"),
            }
        }
    }

    #[test]
    fn memory_writeback_follows_access_and_register_offsets_fail_closed() {
        let pre = lift(0xe5b1_0004); // ldr r0,[r1,#4]!
        assert!(matches!(
            pre.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::Load {
                        addr: Address::BaseOffset { offset: 4, .. },
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::Add {
                        dst,
                        src1,
                        src2: SrcOperand::Imm(4),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                    ..
                }
            ] if dst == src1 && *dst == Aarch32Lifter::reg(1)
        ));

        let post_sub = lift(0xe611_0002); // ldr r0,[r1],-r2
        assert!(matches!(
            post_sub.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::Load {
                        addr: Address::Direct(base),
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::Sub {
                        dst,
                        src1,
                        src2: SrcOperand::Reg(index),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                    ..
                }
            ] if *base == Aarch32Lifter::reg(1)
                && dst == src1
                && *dst == Aarch32Lifter::reg(1)
                && *index == Aarch32Lifter::reg(2)
        ));

        let scaled = lift(0xe791_0102); // ldr r0,[r1,r2,lsl #2]
        assert!(matches!(
            &scaled.ops[0].kind,
            OpKind::Load {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    ..
                },
                ..
            } if *base == Aarch32Lifter::reg(1) && *index == Aarch32Lifter::reg(2)
        ));

        for raw in [
            0xe711_0002u32, // ldr r0,[r1,-r2] needs a non-clobbering address temp
            0xe7b1_0122,    // ldr r0,[r1,r2,lsr #2]! needs an address temp
            0xe5b1_1004,    // ldr r1,[r1,#4]! has constrained alias semantics
        ] {
            let mut lifter = Aarch32Lifter::new();
            let mut ctx = LiftContext::new(SourceArch::Aarch32);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                    Err(LiftError::Unsupported { .. })
                ),
                "{raw:#010x}"
            );
        }
    }

    #[test]
    fn lifts_a32_literal_load_width_sign_add_subtract_and_wrap_matrix() {
        for (raw, dst, address, width, sign) in [
            (0xe59f_0004, 0, 0x100c, MemWidth::B4, SignExtend::Zero),
            (0xe51f_1004, 1, 0x1004, MemWidth::B4, SignExtend::Zero),
            (0xe5df_2003, 2, 0x100b, MemWidth::B1, SignExtend::Zero),
            (0xe55f_3003, 3, 0x1005, MemWidth::B1, SignExtend::Zero),
            (0xe1df_40b2, 4, 0x100a, MemWidth::B2, SignExtend::Zero),
            (0xe15f_50d1, 5, 0x1007, MemWidth::B1, SignExtend::Sign),
            (0xe1df_60f2, 6, 0x100a, MemWidth::B2, SignExtend::Sign),
            (0xe59f_7fff, 7, 0x2007, MemWidth::B4, SignExtend::Zero),
            (0xe51f_8fff, 8, 0x0009, MemWidth::B4, SignExtend::Zero),
            (0xe1df_9fbf, 9, 0x1107, MemWidth::B2, SignExtend::Zero),
            (0xe15f_afbf, 10, 0x0f09, MemWidth::B2, SignExtend::Zero),
            (0xe1df_bfdf, 11, 0x1107, MemWidth::B1, SignExtend::Sign),
            (0xe15f_cfdf, 12, 0x0f09, MemWidth::B1, SignExtend::Sign),
            (0xe1df_dfff, 13, 0x1107, MemWidth::B2, SignExtend::Sign),
            (0xe15f_efff, 14, 0x0f09, MemWidth::B2, SignExtend::Sign),
        ] {
            let result = lift(raw);
            assert!(
                matches!(
                    result.ops.as_slice(),
                    [SmirOp {
                        kind: OpKind::Load {
                            dst: actual_dst,
                            addr: Address::Absolute(actual_address),
                            width: actual_width,
                            sign: actual_sign,
                        },
                        ..
                    }] if *actual_dst == Aarch32Lifter::reg(dst)
                        && *actual_address == address
                        && *actual_width == width
                        && *actual_sign == sign
                ),
                "{raw:#010x}: {result:?}"
            );
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        }

        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        for (pc, raw, expected) in [
            (0xffff_fffc, 0xe59f_0004u32, 8),
            (0xffff_fffc, 0xe51f_0008u32, 0xffff_fffc),
        ] {
            let result = lifter.lift_insn(pc, &raw.to_le_bytes(), &mut ctx).unwrap();
            assert!(matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::Load {
                        addr: Address::Absolute(address),
                        ..
                    },
                    ..
                }] if *address == expected
            ));
        }

        for raw in [
            0x059f_0004u32, // predicated literal load needs conditional commit
            0xe59f_f004,    // load-to-PC is control flow
            0xe58f_0004,    // PC-relative store is outside the literal-load subset
            0xe5bf_0004,    // writeback from PC is invalid for this subset
            0xe79f_0001,    // register-offset PC base is not a literal form
        ] {
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                    Err(LiftError::Unsupported { .. })
                ),
                "{raw:#010x}"
            );
        }
        assert!(matches!(
            lifter.lift_insn(
                u64::from(u32::MAX) + 1,
                &0xe59f_0004u32.to_le_bytes(),
                &mut ctx,
            ),
            Err(LiftError::Unsupported { .. })
        ));
    }

    #[test]
    fn lifts_a32_multiple_transfers_in_register_and_address_order() {
        fn offset(op: &SmirOp) -> i64 {
            let addr = match &op.kind {
                OpKind::Load { addr, .. } | OpKind::Store { addr, .. } => addr,
                other => panic!("expected transfer, got {other:?}"),
            };
            match addr {
                Address::Direct(_) => 0,
                Address::BaseOffset { offset, .. } => *offset,
                other => panic!("unexpected multiple-transfer address {other:?}"),
            }
        }

        for (raw, expected_offsets, label) in [
            (0xe8aa_0005, [0, 4], "stmia r10!,{r0,r2}"),
            (0xe9aa_0005, [4, 8], "stmib r10!,{r0,r2}"),
            (0xe82a_0005, [-4, 0], "stmda r10!,{r0,r2}"),
            (0xe92a_0005, [-8, -4], "stmdb r10!,{r0,r2}"),
        ] {
            let result = lift(raw);
            assert_eq!(result.ops.len(), 3, "{label}");
            assert_eq!(offset(&result.ops[0]), expected_offsets[0], "{label}");
            assert_eq!(offset(&result.ops[1]), expected_offsets[1], "{label}");
            assert!(matches!(
                &result.ops[0].kind,
                OpKind::Store { src, .. } if *src == Aarch32Lifter::reg(0)
            ));
            assert!(matches!(
                &result.ops[1].kind,
                OpKind::Store { src, .. } if *src == Aarch32Lifter::reg(2)
            ));
        }

        let load = lift(0xe8ba_002a); // ldmia r10!,{r1,r3,r5}
        assert_eq!(load.ops.len(), 4);
        assert_eq!(
            load.ops[..3].iter().map(offset).collect::<Vec<_>>(),
            vec![0, 4, 8]
        );
        assert!(matches!(
            &load.ops[3].kind,
            OpKind::Add {
                dst,
                src1,
                src2: SrcOperand::Imm(12),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            } if dst == src1 && *dst == Aarch32Lifter::reg(10)
        ));

        let push = lift(0xe92d_4011); // push {r0,r4,lr}
        assert_eq!(
            push.ops[..3].iter().map(offset).collect::<Vec<_>>(),
            vec![-12, -8, -4]
        );
        assert!(matches!(
            &push.ops[3].kind,
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(12),
                ..
            } if dst == src1 && *dst == Aarch32Lifter::reg(13)
        ));
    }

    #[test]
    fn a32_multiple_transfers_reject_hidden_and_constrained_forms() {
        for raw in [
            0xe8bd_8001u32, // pop {r0,pc}: interworking control flow
            0xe8fd_0003,    // ldmia sp!,{r0,r1}^: user-bank transfer
            0xe8b1_0000,    // ldmia r1!,{}: architecturally constrained empty list
            0xe8b1_0002,    // ldmia r1!,{r1}: load/base alias
            0xe8a1_0002,    // stmia r1!,{r1}: store/writeback alias
            0xe8bf_0001,    // ldmia pc!,{r0}: pipeline PC base
        ] {
            let mut lifter = Aarch32Lifter::new();
            let mut ctx = LiftContext::new(SourceArch::Aarch32);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                    Err(LiftError::Unsupported { .. })
                ),
                "{raw:#010x}"
            );
        }
    }

    #[test]
    fn lifts_a32_double_transfers_with_pair_atomicity_and_writeback() {
        let load = lift(0xe1c2_00d8); // ldrd r0,r1,[r2,#8]
        assert!(matches!(
            load.ops.as_slice(),
            [SmirOp {
                kind: OpKind::LoadPair {
                    dst1,
                    dst2,
                    addr: Address::BaseOffset {
                        base,
                        offset: 8,
                        ..
                    },
                    width: MemWidth::B4,
                },
                ..
            }] if *dst1 == Aarch32Lifter::reg(0)
                && *dst2 == Aarch32Lifter::reg(1)
                && *base == Aarch32Lifter::reg(2)
        ));

        let store = lift(0xe1e4_20f8); // strd r2,r3,[r4,#8]!
        assert!(matches!(
            store.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::StorePair {
                        src1,
                        src2,
                        addr: Address::BaseOffset { offset: 8, .. },
                        width: MemWidth::B4,
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::Add {
                        dst,
                        src1: base,
                        src2: SrcOperand::Imm(8),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                    ..
                }
            ] if *src1 == Aarch32Lifter::reg(2)
                && *src2 == Aarch32Lifter::reg(3)
                && dst == base
                && *dst == Aarch32Lifter::reg(4)
        ));
    }

    #[test]
    fn a32_double_transfers_reject_odd_pc_and_writeback_alias_pairs() {
        for raw in [
            0xe1c2_10d8u32, // ldrd odd r1 pair
            0xe1c2_e0d8,    // ldrd r14,r15 pair
            0xe1e0_00d8,    // ldrd r0,r1,[r0,#8]!: base aliases pair
            0xe1ef_20f8,    // strd r2,r3,[pc,#8]!: pipeline PC base
        ] {
            let mut lifter = Aarch32Lifter::new();
            let mut ctx = LiftContext::new(SourceArch::Aarch32);
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                    Err(LiftError::Unsupported { .. })
                ),
                "{raw:#010x}"
            );
        }
    }

    #[test]
    fn uses_a32_pc_plus_eight_for_direct_branches() {
        let result = lift(0xea00_0002); // B +8; architectural target = 0x1010.
        assert!(matches!(
            result.control_flow,
            ControlFlow::Branch { target: 0x1010 }
        ));
        assert_eq!(result.branch_targets, vec![0x1010]);
    }

    #[test]
    fn lifts_a32_bx_for_every_non_pc_register_and_block_terminator() {
        for rm in 0_u32..15 {
            let result = lift(0xe12f_ff10 | rm);
            assert!(result.ops.is_empty());
            assert!(result.branch_targets.is_empty());
            assert!(matches!(
                result.control_flow,
                ControlFlow::IndirectBranch { target }
                    if target == Aarch32Lifter::reg(rm as u8)
            ));
        }
        let mut lifter = Aarch32Lifter::new();
        let mut reject_ctx = LiftContext::new(SourceArch::Aarch32);
        assert!(matches!(
            lifter.lift_insn(0x1000, &0xe12f_ff1fu32.to_le_bytes(), &mut reject_ctx),
            Err(LiftError::Unsupported { .. })
        ));

        struct Memory([u8; 4]);
        impl MemoryReader for Memory {
            fn read(
                &self,
                addr: GuestAddr,
                size: usize,
            ) -> Result<Vec<u8>, crate::smir::ir::memory::MemoryError> {
                if addr != 0x1000 || size != 4 {
                    return Err(crate::smir::ir::memory::MemoryError::OutOfBounds { addr });
                }
                Ok(self.0.to_vec())
            }
        }

        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        let block = lifter
            .lift_block(0x1000, &Memory(0xe12f_ff1eu32.to_le_bytes()), &mut ctx)
            .unwrap();
        assert!(block.ops.is_empty());
        assert!(matches!(
            block.terminator,
            Terminator::IndirectBranch {
                target: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                ref possible_targets,
            } if possible_targets.is_empty()
        ));
    }

    #[test]
    fn lifts_a32_blx_immediate_and_every_register_with_old_lr_snapshot() {
        for (raw, target) in [(0xfa00_0000_u32, 0x1008), (0xfb00_0000, 0x100a)] {
            let result = lift(raw);
            assert_eq!(result.branch_targets, vec![target]);
            assert!(matches!(
                result.control_flow,
                ControlFlow::Call {
                    target: CallTarget::GuestAddrInterworking {
                        addr,
                        thumb: true,
                    }
                } if addr == target
            ));
            assert!(matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::Mov {
                        dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                        src: SrcOperand::Imm(0x1004),
                        width: OpWidth::W32,
                    },
                    ..
                }]
            ));
        }

        for rm in 0_u32..15 {
            let result = lift(0xe12f_ff30 | rm);
            assert!(result.branch_targets.is_empty());
            match (rm, result.ops.as_slice(), result.control_flow) {
                (
                    14,
                    [
                        SmirOp {
                            kind:
                                OpKind::Mov {
                                    dst: snapshot,
                                    src: SrcOperand::Reg(source),
                                    width: OpWidth::W32,
                                },
                            ..
                        },
                        SmirOp {
                            kind:
                                OpKind::Mov {
                                    dst: link,
                                    src: SrcOperand::Imm(0x1004),
                                    width: OpWidth::W32,
                                },
                            ..
                        },
                    ],
                    ControlFlow::Call {
                        target: CallTarget::IndirectInterworking(target),
                    },
                ) => {
                    assert!(matches!(snapshot, VReg::Virtual(_)));
                    assert_eq!(*source, Aarch32Lifter::reg(14));
                    assert_eq!(*link, Aarch32Lifter::reg(14));
                    assert_eq!(target, *snapshot);
                }
                (
                    _,
                    [
                        SmirOp {
                            kind:
                                OpKind::Mov {
                                    dst,
                                    src: SrcOperand::Imm(0x1004),
                                    width: OpWidth::W32,
                                },
                            ..
                        },
                    ],
                    ControlFlow::Call {
                        target: CallTarget::IndirectInterworking(target),
                    },
                ) => {
                    assert_eq!(*dst, Aarch32Lifter::reg(14));
                    assert_eq!(target, Aarch32Lifter::reg(rm as u8));
                }
                other => panic!("unexpected BLX r{rm} lift: {other:?}"),
            }
        }

        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        assert!(matches!(
            lifter.lift_insn(0x1000, &0xe12f_ff3fu32.to_le_bytes(), &mut ctx),
            Err(LiftError::Unsupported { .. })
        ));
    }

    #[test]
    fn a32_branch_and_link_pc_arithmetic_wraps_modulo_2_pow_32() {
        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);

        let branch = lifter
            .lift_insn(0xffff_fffc, &0xea00_0000u32.to_le_bytes(), &mut ctx)
            .unwrap(); // B +0: 0xffff_fffc + 8 wraps to 4.
        assert!(matches!(
            branch.control_flow,
            ControlFlow::Branch { target: 4 }
        ));

        let cond = lifter
            .lift_insn(0xffff_fffc, &0x0a00_0000u32.to_le_bytes(), &mut ctx)
            .unwrap(); // BEQ +0; fallthrough wraps to 0.
        assert!(matches!(
            cond.control_flow,
            ControlFlow::CondBranch {
                target: 4,
                fallthrough: 0,
                ..
            }
        ));

        let call = lifter
            .lift_insn(0xffff_fffc, &0xeb00_0000u32.to_le_bytes(), &mut ctx)
            .unwrap(); // BL +0; target 4, link 0.
        assert!(matches!(
            call.control_flow,
            ControlFlow::Call {
                target: CallTarget::GuestAddr(4)
            }
        ));
        assert!(matches!(
            call.ops.as_slice(),
            [SmirOp {
                kind: OpKind::Mov {
                    dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W32,
                },
                ..
            }]
        ));

        let exchange = lifter
            .lift_insn(0xffff_fffc, &0xfb00_0000u32.to_le_bytes(), &mut ctx)
            .unwrap(); // BLX +2; target 6, ARM link 0.
        assert!(matches!(
            exchange.control_flow,
            ControlFlow::Call {
                target: CallTarget::GuestAddrInterworking {
                    addr: 6,
                    thumb: true,
                }
            }
        ));
        assert!(matches!(
            exchange.ops.as_slice(),
            [SmirOp {
                kind: OpKind::Mov {
                    src: SrcOperand::Imm(0),
                    ..
                },
                ..
            }]
        ));

        assert!(matches!(
            lifter.lift_insn(
                u64::from(u32::MAX) + 1,
                &0xea00_0000u32.to_le_bytes(),
                &mut ctx,
            ),
            Err(LiftError::Unsupported { .. })
        ));
    }

    #[test]
    fn lifts_every_a32_branch_condition_with_pc_plus_eight_and_fallthrough() {
        let conditions = [
            Condition::Eq,
            Condition::Ne,
            Condition::Uge,
            Condition::Ult,
            Condition::Negative,
            Condition::Positive,
            Condition::Overflow,
            Condition::NoOverflow,
            Condition::Ugt,
            Condition::Ule,
            Condition::Sge,
            Condition::Slt,
            Condition::Sgt,
            Condition::Sle,
        ];

        for (bits, expected) in conditions.into_iter().enumerate() {
            let raw = ((bits as u32) << 28) | 0x0a00_0002;
            let result = lift(raw);
            assert!(matches!(
                result.control_flow,
                ControlFlow::CondBranch {
                    cond,
                    target: 0x1010,
                    fallthrough: 0x1004,
                } if cond == expected
            ));
            assert_eq!(result.branch_targets, vec![0x1010, 0x1004]);
        }
    }

    #[test]
    fn a32_block_materializes_only_a_foldable_branch_condition() {
        struct Memory([u8; 4]);

        impl MemoryReader for Memory {
            fn read(
                &self,
                addr: GuestAddr,
                size: usize,
            ) -> Result<Vec<u8>, crate::smir::ir::memory::MemoryError> {
                if addr != 0x1000 || size != 4 {
                    return Err(crate::smir::ir::memory::MemoryError::OutOfBounds { addr });
                }
                Ok(self.0.to_vec())
            }
        }

        let memory = Memory(0x1a00_0000u32.to_le_bytes()); // BNE +0.
        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        let block = lifter.lift_block(0x1000, &memory, &mut ctx).unwrap();
        let taken = ctx.get_or_create_block(0x1008);
        let not_taken = ctx.get_or_create_block(0x1004);

        assert!(matches!(
            block.ops.as_slice(),
            [SmirOp {
                guest_pc: 0x1000,
                kind: OpKind::TestCondition {
                    dst,
                    cond: Condition::Ne,
                },
                ..
            }] if matches!(dst, VReg::Virtual(_))
        ));
        assert!(matches!(
            block.terminator,
            Terminator::CondBranch {
                cond: VReg::Virtual(_),
                true_target,
                false_target,
            } if true_target == taken && false_target == not_taken
        ));
    }

    #[test]
    fn rejects_predication_pc_and_special_shifter_state() {
        let raws: [u32; 8] = [
            0x1081_0002, // ADDNE r0,r1,r2
            0xe081_000f, // ADD r0,r1,pc
            0xe1a0_0061, // RRX r0,r1
            0xe1a0_0021, // LSR r0,r1,#32 (encoded amount zero)
            0xe7e6_365f, // UBFX r3,pc,#12,#7
            0xe037_a998, // MLAS r7,r8,r9,r10
            0xe7c3_1412, // BFI r1,r2,#8 with msb below lsb
            0xe7ff_0851, // UBFX r0,r1,#16,#32 exceeds register width
        ];
        for raw in raws {
            let mut lifter = Aarch32Lifter::new();
            let mut ctx = LiftContext::new(SourceArch::Aarch32);
            assert!(matches!(
                lifter.lift_insn(0x1000, &raw.to_le_bytes(), &mut ctx),
                Err(LiftError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn incomplete_input_is_reported_without_decoder_access() {
        let mut lifter = Aarch32Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch32);
        assert!(matches!(
            lifter.lift_insn(0x1000, &[0, 1, 2], &mut ctx),
            Err(LiftError::Incomplete {
                have: 3,
                need: 4,
                ..
            })
        ));
    }
}
