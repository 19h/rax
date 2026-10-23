//! Packet-old scalar inputs and separately retained in-flight GPR outputs.

use super::*;

#[derive(Default)]
pub(super) struct PacketRegisters {
    old: [Option<VReg>; 32],
    new: [Option<VReg>; 32],
}

pub(super) fn register_index(register: VReg) -> Option<usize> {
    match register {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::R(index))) if index < 32 => Some(index as usize),
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Sp)) => Some(29),
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Fp)) => Some(30),
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Lr)) => Some(31),
        _ => None,
    }
}

fn architectural(index: usize) -> VReg {
    VReg::Arch(ArchReg::Hexagon(HexagonReg::R(index as u8)))
}

fn push(operations: &mut Vec<SmirOp>, pc: GuestAddr, kind: OpKind) {
    operations.push(SmirOp::new(OpId(operations.len() as u16), pc, kind));
}

pub(super) fn copy(operations: &mut Vec<SmirOp>, pc: GuestAddr, dst: VReg, src: VReg) {
    push(
        operations,
        pc,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W32,
        },
    );
}

impl PacketRegisters {
    pub(super) fn latest(&self, register: u8) -> Option<VReg> {
        self.new.get(register as usize).copied().flatten()
    }

    /// Only a written GPR needs a saved old value. Restore packet-old inputs
    /// around each instruction, then retain its result in SSA for explicit .new
    /// consumers and for the final architectural register file.
    pub(super) fn rewrite(
        &mut self,
        operations: Vec<SmirOp>,
        pc: GuestAddr,
        context: &mut LiftContext,
        final_word: bool,
    ) -> Result<Vec<SmirOp>, LiftError> {
        if final_word && self.new.iter().all(Option::is_none) {
            return Ok(operations);
        }
        let mut reads = [false; 32];
        let mut writes = [false; 32];
        let mut conditions = [None; 32];
        for operation in &operations {
            for source in operation.kind.source_vregs() {
                if let Some(index) = register_index(source) {
                    reads[index] = true;
                }
            }
            for destination in operation.kind.dests() {
                if let Some(index) = register_index(destination) {
                    writes[index] = true;
                    conditions[index] = match operation.kind {
                        OpKind::PredLoad { cond, .. } => Some((cond, true)),
                        OpKind::Select {
                            cond,
                            src_false,
                            src_true,
                            ..
                        } if register_index(src_false) == Some(index)
                            && register_index(src_true) != Some(index) =>
                        {
                            Some((cond, true))
                        }
                        OpKind::Select {
                            cond,
                            src_true,
                            src_false,
                            ..
                        } if register_index(src_true) == Some(index)
                            && register_index(src_false) != Some(index) =>
                        {
                            Some((cond, false))
                        }
                        _ => None,
                    };
                }
            }
        }
        for index in 0..32 {
            if writes[index] && self.new[index].is_some() && conditions[index].is_none() {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "multiple nonconditional Hexagon GPR writes".into(),
                });
            }
        }
        let mut result = Vec::with_capacity(operations.len() + 16);
        for index in 0..32 {
            if writes[index] && self.old[index].is_none() {
                let old = context.alloc_vreg();
                copy(&mut result, pc, old, architectural(index));
                self.old[index] = Some(old);
            }
            if reads[index] && self.new[index].is_some() {
                copy(
                    &mut result,
                    pc,
                    architectural(index),
                    self.old[index].unwrap(),
                );
            }
        }
        for mut operation in operations {
            operation.id = OpId(result.len() as u16);
            result.push(operation);
        }
        for index in 0..32 {
            if writes[index] {
                let computed = context.alloc_vreg();
                copy(&mut result, pc, computed, architectural(index));
                self.new[index] = Some(
                    if let (Some(previous), Some((condition, when_true))) =
                        (self.new[index], conditions[index])
                    {
                        let merged = context.alloc_vreg();
                        push(
                            &mut result,
                            pc,
                            OpKind::Select {
                                dst: merged,
                                cond: condition,
                                src_true: if when_true { computed } else { previous },
                                src_false: if when_true { previous } else { computed },
                                width: OpWidth::W32,
                            },
                        );
                        merged
                    } else {
                        computed
                    },
                );
            }
            if (reads[index] || writes[index])
                && let Some(latest) = self.new[index]
            {
                copy(&mut result, pc, architectural(index), latest);
            }
        }
        Ok(result)
    }
}
