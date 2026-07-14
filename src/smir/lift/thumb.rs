//! Thumb (T16) and Thumb-2 (T32) scalar instruction lifter.
//!
//! This lifter shares scalar operation construction with the AArch64 lifter,
//! but enforces the architectural boundaries that are specific to AArch32
//! Thumb execution:
//!
//! - r13 and r14 are identity-mapped AArch32 GPRs (`X13`/`X14`), not the
//!   AArch64 SP/LR aliases;
//! - r15, IT-state predication, explicit conditional instructions, RRX, and
//!   flag-setting logical/move/shift forms fail closed;
//! - direct Thumb branches use the architectural `PC + 4` base and BL writes
//!   `(next_pc | 1)` to r14;
//! - T32 MOVT and bitfield encodings are translated using their T32 layouts;
//! - both 16-bit and 32-bit instruction lengths are retained by block lifting.

use std::collections::HashSet;

use crate::isa::arm::ExecutionState;
use crate::isa::arm::decoder::{
    DecodedInsn, Decoder, Mnemonic, Operand, Register, ShiftType, ThumbDecoder,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, ArmReg, FunctionId, GuestAddr, OpId, OpWidth, SourceArch, SrcOperand, VReg,
};
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::aarch64::Aarch64Lifter;
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

/// Fail-closed T16/T32 scalar lifter.
pub struct ThumbLifter {
    shared: Aarch64Lifter,
}

impl ThumbLifter {
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

    /// The generic AArch64 lifter interprets `Register::is_sp` as architectural
    /// AArch64 SP. Thumb's r13 is instead identity-mapped to host X13.
    fn normalize_regs(insn: &DecodedInsn) -> DecodedInsn {
        let mut normalized = insn.clone();
        for operand in &mut normalized.operands {
            match operand {
                Operand::ShiftedReg(shifted)
                    if shifted.shift_type == ShiftType::LSL && shifted.amount == 0 =>
                {
                    let reg = shifted.reg;
                    *operand = Operand::Reg(if reg.num == 13 {
                        Register::raw(13, false, false)
                    } else {
                        reg
                    });
                }
                Operand::Reg(reg) if reg.num == 13 => {
                    *reg = Register::raw(13, false, false);
                }
                Operand::ShiftedReg(shifted) if shifted.reg.num == 13 => {
                    shifted.reg = Register::raw(13, false, false);
                }
                Operand::ExtendedReg(extended) if extended.reg.num == 13 => {
                    extended.reg = Register::raw(13, false, false);
                }
                _ => {}
            }
        }
        normalized
    }

    fn rejects_hidden_state(insn: &DecodedInsn) -> bool {
        if insn.cond.is_some() || insn.mnemonic == Mnemonic::IT {
            return true;
        }
        insn.operands.iter().any(|operand| match operand {
            Operand::Reg(reg) => reg.num >= 15,
            Operand::ShiftedReg(shifted) => {
                shifted.reg.num >= 15
                    || shifted.shift_type == ShiftType::RRX
                    || shifted.amount >= 32
            }
            Operand::ExtendedReg(extended) => extended.reg.num >= 15,
            Operand::Mem(_) | Operand::RegList(_) => true,
            _ => false,
        })
    }

    fn shared_scalar_mnemonic(insn: &DecodedInsn) -> bool {
        use Mnemonic::*;

        match insn.mnemonic {
            ADD | ADDS | ADC | ADCS | SUB | SUBS | SBC | SBCS | CMP | CMN | NEG | NEGS | CLZ
            | RBIT | REV | REV16 | UDIV | SDIV | NOP => true,
            SXTB | SXTH | UXTB | UXTH => {
                matches!(insn.operands.as_slice(), [Operand::Reg(_), Operand::Reg(_)])
            }
            MOV => {
                !insn.sets_flags && !matches!(insn.operands.get(1), Some(Operand::ShiftedReg(_)))
            }
            AND | ORR | EOR | BIC | MUL => !insn.sets_flags,
            LSL | LSR | ASR | ROR => {
                !insn.sets_flags && matches!(insn.operands.get(2), Some(Operand::Imm(_)))
            }
            _ => false,
        }
    }

    fn operand_src(operand: &Operand) -> Result<SrcOperand, LiftError> {
        match operand {
            Operand::Reg(reg) if reg.num < 15 => Ok(SrcOperand::Reg(Self::reg(reg.num))),
            Operand::Imm(imm) => Ok(SrcOperand::Imm(imm.effective_value())),
            Operand::ShiftedReg(shifted)
                if shifted.reg.num < 15
                    && shifted.shift_type != ShiftType::RRX
                    && shifted.amount < 32 =>
            {
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
                    amount: shifted.amount,
                })
            }
            _ => Err(LiftError::Internal(
                "unsupported Thumb scalar source operand".to_string(),
            )),
        }
    }

    fn bitfield_fields(insn: &DecodedInsn, pc: GuestAddr) -> Result<(u8, u8, u8), LiftError> {
        let rn = ((insn.raw >> 16) & 0xf) as u8;
        let lsb = ((((insn.raw >> 12) & 0x7) << 2) | ((insn.raw >> 6) & 0x3)) as u8;
        let encoded_width = (insn.raw & 0x1f) as u8;
        if rn >= 15 && insn.mnemonic != Mnemonic::BFC {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "Thumb bitfield source PC".to_string(),
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
        if Self::rejects_hidden_state(insn) {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!(
                    "Thumb {:?} requires IT, PC, memory, or special shifter state",
                    insn.mnemonic
                ),
            });
        }

        let normalized = Self::normalize_regs(insn);
        if Self::shared_scalar_mnemonic(&normalized) {
            return self.shared.lift_insn_inner(&normalized, pc, ctx);
        }

        let mut ops = Vec::new();
        let control = match normalized.mnemonic {
            Mnemonic::MVN if !normalized.sets_flags => {
                let (Some(Operand::Reg(rd)), Some(source)) =
                    (normalized.operands.first(), normalized.operands.get(1))
                else {
                    return Err(LiftError::Internal(
                        "invalid Thumb MVN operands".to_string(),
                    ));
                };
                let source = match source {
                    Operand::Reg(rm) => rm,
                    Operand::ShiftedReg(shifted)
                        if shifted.shift_type == ShiftType::LSL && shifted.amount == 0 =>
                    {
                        &shifted.reg
                    }
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: "shifted Thumb MVN".to_string(),
                        });
                    }
                };
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Not {
                        dst: Self::reg(rd.num),
                        src: Self::reg(source.num),
                        width: OpWidth::W32,
                    },
                );
                ControlFlow::Fallthrough
            }
            Mnemonic::MOV if !normalized.sets_flags => {
                let [Operand::Reg(rd), Operand::ShiftedReg(shifted)] =
                    normalized.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid Thumb shifted MOV operands".to_string(),
                    ));
                };
                let dst = Self::reg(rd.num);
                let src = Self::reg(shifted.reg.num);
                let amount = SrcOperand::Imm(i64::from(shifted.amount));
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
                    normalized.operands.first(),
                    normalized.operands.get(1),
                    normalized.operands.get(2),
                ) else {
                    return Err(LiftError::Internal(
                        "invalid Thumb RSB operands".to_string(),
                    ));
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
                            flags: if normalized.sets_flags {
                                FlagUpdate::All
                            } else {
                                FlagUpdate::None
                            },
                        },
                    ),
                    SrcOperand::Imm(0) => Self::push(
                        &mut ops,
                        pc,
                        OpKind::Neg {
                            dst,
                            src: rn,
                            width: OpWidth::W32,
                            flags: if normalized.sets_flags {
                                FlagUpdate::All
                            } else {
                                FlagUpdate::None
                            },
                        },
                    ),
                    SrcOperand::Imm(imm) if !normalized.sets_flags => {
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
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: "Thumb shifted or flag-setting immediate RSB".to_string(),
                        });
                    }
                }
                ControlFlow::Fallthrough
            }
            Mnemonic::MLA | Mnemonic::MLS if !normalized.sets_flags => {
                let [
                    Operand::Reg(rd),
                    Operand::Reg(rn),
                    Operand::Reg(rm),
                    Operand::Reg(ra),
                ] = normalized.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid Thumb multiply-accumulate operands".to_string(),
                    ));
                };
                let kind = if normalized.mnemonic == Mnemonic::MLA {
                    OpKind::MulAdd {
                        dst: Self::reg(rd.num),
                        acc: Self::reg(ra.num),
                        src1: Self::reg(rn.num),
                        src2: Self::reg(rm.num),
                        width: OpWidth::W32,
                    }
                } else {
                    OpKind::MulSub {
                        dst: Self::reg(rd.num),
                        acc: Self::reg(ra.num),
                        src1: Self::reg(rn.num),
                        src2: Self::reg(rm.num),
                        width: OpWidth::W32,
                    }
                };
                Self::push(&mut ops, pc, kind);
                ControlFlow::Fallthrough
            }
            Mnemonic::UMULL | Mnemonic::SMULL if !normalized.sets_flags => {
                let [
                    Operand::Reg(lo),
                    Operand::Reg(hi),
                    Operand::Reg(rn),
                    Operand::Reg(rm),
                ] = normalized.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid Thumb long-multiply operands".to_string(),
                    ));
                };
                let kind = if normalized.mnemonic == Mnemonic::UMULL {
                    OpKind::MulU {
                        dst_lo: Self::reg(lo.num),
                        dst_hi: Some(Self::reg(hi.num)),
                        src1: Self::reg(rn.num),
                        src2: SrcOperand::Reg(Self::reg(rm.num)),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    }
                } else {
                    OpKind::MulS {
                        dst_lo: Self::reg(lo.num),
                        dst_hi: Some(Self::reg(hi.num)),
                        src1: Self::reg(rn.num),
                        src2: SrcOperand::Reg(Self::reg(rm.num)),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    }
                };
                Self::push(&mut ops, pc, kind);
                ControlFlow::Fallthrough
            }
            Mnemonic::MOVK => {
                let (Some(Operand::Reg(rd)), Some(Operand::Imm(imm))) =
                    (normalized.operands.first(), normalized.operands.get(1))
                else {
                    return Err(LiftError::Internal(
                        "invalid Thumb MOVT operands".to_string(),
                    ));
                };
                let dst = Self::reg(rd.num);
                let imm16 = imm.effective_value() as u32 & 0xffff;
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
            Mnemonic::UBFX | Mnemonic::SBFX => {
                let Some(Operand::Reg(rd)) = normalized.operands.first() else {
                    return Err(LiftError::Internal(
                        "invalid Thumb bitfield-extract operands".to_string(),
                    ));
                };
                let (rn, lsb, encoded_width) = Self::bitfield_fields(&normalized, pc)?;
                let width_bits = encoded_width + 1;
                if u16::from(lsb) + u16::from(width_bits) > 32 {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "Thumb bitfield-extract bounds".to_string(),
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
                        sign_extend: normalized.mnemonic == Mnemonic::SBFX,
                        op_width: OpWidth::W32,
                    },
                );
                ControlFlow::Fallthrough
            }
            Mnemonic::BFI | Mnemonic::BFC => {
                let Some(Operand::Reg(rd)) = normalized.operands.first() else {
                    return Err(LiftError::Internal(
                        "invalid Thumb bitfield-insert operands".to_string(),
                    ));
                };
                let (rn, lsb, msb) = Self::bitfield_fields(&normalized, pc)?;
                if msb < lsb {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "Thumb bitfield-insert bounds".to_string(),
                    });
                }
                let width_bits = msb - lsb + 1;
                let dst = Self::reg(rd.num);
                if normalized.mnemonic == Mnemonic::BFC {
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
            Mnemonic::B => {
                let Some(Operand::Label(offset)) = normalized.operands.first() else {
                    return Err(LiftError::Internal("invalid Thumb B operands".to_string()));
                };
                ControlFlow::Branch {
                    target: (pc as i64).wrapping_add(4).wrapping_add(*offset) as u64,
                }
            }
            Mnemonic::BL => {
                let Some(Operand::Label(offset)) = normalized.operands.first() else {
                    return Err(LiftError::Internal("invalid Thumb BL operands".to_string()));
                };
                Self::push(
                    &mut ops,
                    pc,
                    OpKind::Mov {
                        dst: Self::reg(14),
                        src: SrcOperand::Imm((pc.wrapping_add(4) | 1) as i64),
                        width: OpWidth::W32,
                    },
                );
                ControlFlow::Call {
                    target: CallTarget::GuestAddr(
                        (pc as i64).wrapping_add(4).wrapping_add(*offset) as u64,
                    ),
                }
            }
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("Thumb {:?}", normalized.mnemonic),
                });
            }
        };

        Ok((ops, control))
    }

    fn result(ops: Vec<SmirOp>, bytes_consumed: usize, control_flow: ControlFlow) -> LiftResult {
        let branch_targets = match &control_flow {
            ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => vec![*target],
            ControlFlow::Call {
                target: CallTarget::GuestAddr(target),
            } => vec![*target],
            _ => Vec::new(),
        };
        LiftResult {
            ops,
            bytes_consumed,
            control_flow,
            branch_targets,
        }
    }

    fn decode(bytes: &[u8], addr: GuestAddr) -> Result<DecodedInsn, LiftError> {
        if bytes.len() < 2 {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need: 2,
            });
        }
        let hw1 = u16::from_le_bytes(bytes[..2].try_into().unwrap());
        let need = if ThumbDecoder::is_32bit_instruction(hw1) {
            4
        } else {
            2
        };
        if bytes.len() < need {
            return Err(LiftError::Incomplete {
                addr,
                have: bytes.len(),
                need,
            });
        }
        Decoder::new(ExecutionState::Thumb)
            .decode(&bytes[..need])
            .map_err(|_| LiftError::InvalidEncoding {
                addr,
                bytes: bytes[..need].to_vec(),
            })
    }
}

impl Default for ThumbLifter {
    fn default() -> Self {
        Self::new()
    }
}

impl SmirLifter for ThumbLifter {
    fn source_arch(&self) -> SourceArch {
        SourceArch::Thumb
    }

    fn lift_insn(
        &mut self,
        addr: GuestAddr,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let insn = Self::decode(bytes, addr)?;
        ctx.guest_pc = addr;
        let bytes_consumed = insn.size as usize;
        let (ops, control) = self.lift_decoded(&insn, addr, ctx)?;
        Ok(Self::result(ops, bytes_consumed, control))
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
            let prefix = mem
                .read(pc, 2)
                .map_err(|error| LiftError::MemoryError { addr: pc, error })?;
            let hw1 = u16::from_le_bytes(prefix[..2].try_into().unwrap());
            let bytes = if ThumbDecoder::is_32bit_instruction(hw1) {
                mem.read(pc, 4)
                    .map_err(|error| LiftError::MemoryError { addr: pc, error })?
            } else {
                prefix
            };
            let result = self.lift_insn(pc, &bytes, ctx)?;
            let insn_pc = pc;
            pc = pc.wrapping_add(result.bytes_consumed as u64);
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
                ControlFlow::Return => Terminator::Return { values: Vec::new() },
                ControlFlow::Trap { kind } => Terminator::Trap { kind },
                ControlFlow::Syscall => Terminator::Trap {
                    kind: TrapKind::SystemCall,
                },
                ControlFlow::CondBranch { .. }
                | ControlFlow::CondBranchReg { .. }
                | ControlFlow::IndirectBranch { .. }
                | ControlFlow::IndirectBranchMem { .. } => {
                    return Err(LiftError::Unsupported {
                        addr: insn_pc,
                        mnemonic: "Thumb block terminator".to_string(),
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
            .map(|block| block.guest_pc.wrapping_add(4))
            .max()
            .unwrap_or(entry.wrapping_add(2));
        Ok(SmirFunction {
            id,
            entry: ctx.get_or_create_block(entry),
            blocks,
            locals: Vec::new(),
            guest_range: (min, max),
            calling_convention: CallingConv::GuestPreserveAll,
            attrs: FunctionAttrs::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lift(bytes: &[u8]) -> LiftResult {
        let mut lifter = ThumbLifter::new();
        let mut ctx = LiftContext::new(SourceArch::Thumb);
        lifter.lift_insn(0x1000, bytes, &mut ctx).unwrap()
    }

    #[test]
    fn lifts_mixed_t16_t32_scalar_matrix_with_exact_lengths() {
        let cases: &[(&[u8], usize, &str)] = &[
            (&[0x88, 0x18], 2, "adds-t16"),
            (&[0x63, 0x1f], 2, "subs-t16"),
            (&[0x75, 0x41], 2, "adcs-t16"),
            (&[0x87, 0x41], 2, "sbcs-t16"),
            (&[0x80, 0x29], 2, "cmp-t16"),
            (&[0xda, 0x42], 2, "cmn-t16"),
            (&[0x6c, 0x42], 2, "neg-t16"),
            (&[0xc8, 0x44], 2, "add-high-t16"),
            (&[0xda, 0x46], 2, "mov-high-t16"),
            (&[0x08, 0xba], 2, "rev-t16"),
            (&[0x01, 0xeb, 0xc2, 0x00], 4, "add-shift-t32"),
            (&[0x0c, 0xea, 0x70, 0x1b], 4, "and-ror-t32"),
            (&[0x09, 0xfb, 0x0a, 0xf8], 4, "mul-t32"),
            (&[0x04, 0xfb, 0x05, 0x63], 4, "mla-t32"),
            (&[0xa2, 0xfb, 0x03, 0x01], 4, "umull-t32"),
            (&[0xb9, 0xfb, 0xfa, 0xf8], 4, "udiv-t32"),
            (&[0xb2, 0xfa, 0x82, 0xf1], 4, "clz-t32"),
            (&[0x6a, 0xf3, 0x0f, 0x29], 4, "bfi-t32"),
            (&[0xcc, 0xf3, 0x06, 0x3b], 4, "ubfx-t32"),
            (&[0x4f, 0xfa, 0x83, 0xf2], 4, "sxtb-t32"),
            (&[0xcc, 0xf6, 0xfe, 0x27], 4, "movt-t32"),
        ];
        for (bytes, expected_len, label) in cases {
            let mut lifter = ThumbLifter::new();
            let mut ctx = LiftContext::new(SourceArch::Thumb);
            let result = lifter
                .lift_insn(0x1000, bytes, &mut ctx)
                .unwrap_or_else(|error| panic!("{label}: {error:?}"));
            assert_eq!(result.bytes_consumed, *expected_len, "{label}");
            assert!(!result.ops.is_empty(), "{label}");
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        }
    }

    #[test]
    fn uses_thumb_pc_plus_four_and_sets_thumb_link_bit() {
        let branch = lift(&[0x04, 0xe0]); // B +8; target = 0x100c.
        assert!(matches!(
            branch.control_flow,
            ControlFlow::Branch { target: 0x100c }
        ));

        let call = lift(&[0x00, 0xf0, 0x00, 0xf8]); // BL +0.
        assert!(matches!(
            call.control_flow,
            ControlFlow::Call {
                target: CallTarget::GuestAddr(0x1004)
            }
        ));
        assert!(matches!(
            call.ops.as_slice(),
            [SmirOp {
                kind: OpKind::Mov {
                    dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                    src: SrcOperand::Imm(0x1005),
                    width: OpWidth::W32,
                },
                ..
            }]
        ));
    }

    #[test]
    fn rejects_it_pc_memory_and_partial_flag_contracts() {
        let cases: &[&[u8]] = &[
            &[0x08, 0xbf],             // IT EQ
            &[0x00, 0x20],             // MOVS r0,#0: N/Z only
            &[0x08, 0x40],             // ANDS r0,r1: N/Z, preserve C/V
            &[0x48, 0x00],             // LSLS r0,r1,#1: N/Z/C, preserve V
            &[0x78, 0x46],             // MOV r0,pc
            &[0x08, 0x68],             // LDR r0,[r1]
            &[0x4f, 0xea, 0x31, 0x00], // RRX r0,r1
            &[0x00, 0xd0],             // BEQ +0: explicit predication
            &[0x01, 0xfa, 0x02, 0xf0], // LSL.W r0,r1,r2: low-8 count semantics
            &[0x4f, 0xfa, 0x93, 0xf2], // SXTB.W r2,r3,ROR #8
        ];
        for bytes in cases {
            let mut lifter = ThumbLifter::new();
            let mut ctx = LiftContext::new(SourceArch::Thumb);
            assert!(matches!(
                lifter.lift_insn(0x1000, bytes, &mut ctx),
                Err(LiftError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn reports_t16_and_t32_incomplete_input_exactly() {
        let mut lifter = ThumbLifter::new();
        let mut ctx = LiftContext::new(SourceArch::Thumb);
        assert!(matches!(
            lifter.lift_insn(0x1000, &[0], &mut ctx),
            Err(LiftError::Incomplete {
                have: 1,
                need: 2,
                ..
            })
        ));
        assert!(matches!(
            lifter.lift_insn(0x1000, &[0x01, 0xeb, 0xc2], &mut ctx),
            Err(LiftError::Incomplete {
                have: 3,
                need: 4,
                ..
            })
        ));
    }

    #[test]
    fn block_lifting_advances_over_mixed_t16_t32_widths() {
        struct Memory {
            base: GuestAddr,
            bytes: Vec<u8>,
        }

        impl MemoryReader for Memory {
            fn read(
                &self,
                addr: GuestAddr,
                size: usize,
            ) -> Result<Vec<u8>, crate::smir::ir::memory::MemoryError> {
                let offset = (addr - self.base) as usize;
                if offset + size > self.bytes.len() {
                    return Err(crate::smir::ir::memory::MemoryError::OutOfBounds { addr });
                }
                Ok(self.bytes[offset..offset + size].to_vec())
            }
        }

        let memory = Memory {
            base: 0x1000,
            bytes: vec![
                0x88, 0x18, // ADDS r0,r1,r2 (T16)
                0x01, 0xeb, 0xc2, 0x00, // ADD.W r0,r1,r2,LSL #3 (T32)
                0x00, 0xe0, // B +0 (T16), target 0x100a
            ],
        };
        let mut lifter = ThumbLifter::new();
        let mut ctx = LiftContext::new(SourceArch::Thumb);
        let block = lifter.lift_block(0x1000, &memory, &mut ctx).unwrap();

        assert_eq!(block.ops.len(), 2);
        assert_eq!(block.ops[0].guest_pc, 0x1000);
        assert_eq!(block.ops[1].guest_pc, 0x1002);
        assert!(matches!(
            block.terminator,
            Terminator::Branch { target } if target == ctx.get_or_create_block(0x100a)
        ));
    }
}
