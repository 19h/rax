//! AArch32 (A32/ARM-state) instruction lifter.
//!
//! A32 scalar integer instructions use the same 32-bit SMIR operations and
//! architectural NZCV positions as AArch64.  This lifter deliberately shares
//! the mature scalar operation construction in [`super::aarch64`] while
//! enforcing A32-specific invariants before delegation:
//!
//! - r13 and r14 remain ordinary identity-mapped GPRs (`X13`/`X14`), not the
//!   AArch64 host SP/LR aliases;
//! - r15 reads/writes, predication, RRX, and the LSR/ASR-#0 encodings remain
//!   fail-closed until their pipeline/conditional/shifter semantics can be
//!   represented without hidden native state;
//! - A32 branch targets use the architectural `PC + 8` base;
//! - A32-only reverse-subtract, multiply-accumulate, and MOVW/MOVT forms are
//!   translated explicitly.

use std::collections::HashSet;

use crate::isa::arm::decoder::{Aarch32Decoder, DecodedInsn, Mnemonic, Operand, ShiftType};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, ArmReg, BlockId, FunctionId, GuestAddr, OpId, OpWidth, SourceArch, SrcOperand, VReg,
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

    fn operand_src(operand: &Operand) -> Result<SrcOperand, LiftError> {
        match operand {
            Operand::Reg(reg) if reg.num < 15 => Ok(SrcOperand::Reg(Self::reg(reg.num))),
            Operand::Imm(imm) => Ok(SrcOperand::Imm(imm.effective_value())),
            Operand::ShiftedReg(shifted)
                if shifted.reg.num < 15
                    && shifted.shift_type != ShiftType::RRX
                    && !(shifted.amount == 0
                        && matches!(shifted.shift_type, ShiftType::LSR | ShiftType::ASR)) =>
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
                "unsupported A32 scalar source operand".to_string(),
            )),
        }
    }

    fn rejects_hidden_state(insn: &DecodedInsn) -> bool {
        if insn.cond.is_some() {
            return true;
        }
        insn.operands.iter().any(|operand| match operand {
            Operand::Reg(reg) => reg.num >= 15,
            Operand::ShiftedReg(shifted) => {
                shifted.reg.num >= 15
                    || shifted.shift_type == ShiftType::RRX
                    || (shifted.amount == 0
                        && matches!(shifted.shift_type, ShiftType::LSR | ShiftType::ASR))
            }
            _ => false,
        })
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
        if Self::rejects_hidden_state(insn) {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!(
                    "A32 {:?} requires PC, predication, or special shifter state",
                    insn.mnemonic
                ),
            });
        }

        if Self::shared_scalar_mnemonic(insn) {
            return self.shared.lift_insn_inner(insn, pc, ctx);
        }

        let mut ops = Vec::new();
        let control = match insn.mnemonic {
            Mnemonic::MOV if !insn.sets_flags => {
                let [Operand::Reg(rd), Operand::ShiftedReg(shifted)] = insn.operands.as_slice()
                else {
                    return Err(LiftError::Internal(
                        "invalid A32 shifted MOV operands".to_string(),
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
                ControlFlow::Branch {
                    target: (pc as i64).wrapping_add(8).wrapping_add(*offset) as u64,
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
                        src: SrcOperand::Imm(pc.wrapping_add(4) as i64),
                        width: OpWidth::W32,
                    },
                );
                ControlFlow::Call {
                    target: CallTarget::GuestAddr(
                        (pc as i64).wrapping_add(8).wrapping_add(*offset) as u64,
                    ),
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
