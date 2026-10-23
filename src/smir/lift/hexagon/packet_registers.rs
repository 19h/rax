//! Packet-old scalar inputs and retained in-flight GPR/GP/modifier outputs.

use super::*;

const REGISTER_COUNT: usize = 37;

pub(super) struct PacketRegisters {
    old: [Option<VReg>; REGISTER_COUNT],
    new: [Option<VReg>; REGISTER_COUNT],
}

impl Default for PacketRegisters {
    fn default() -> Self {
        Self {
            old: [None; REGISTER_COUNT],
            new: [None; REGISTER_COUNT],
        }
    }
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

fn packet_index(register: VReg) -> Option<usize> {
    register_index(register).or_else(|| match register {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Gp)) => Some(32),
        VReg::Arch(ArchReg::Hexagon(HexagonReg::M(index))) if index < 2 => {
            Some(33 + index as usize)
        }
        VReg::Arch(ArchReg::Hexagon(HexagonReg::Cs(index))) if index < 2 => {
            Some(35 + index as usize)
        }
        _ => None,
    })
}

fn architectural(index: usize) -> VReg {
    let register = match index {
        0..32 => HexagonReg::R(index as u8),
        32 => HexagonReg::Gp,
        33..35 => HexagonReg::M((index - 33) as u8),
        35..37 => HexagonReg::Cs((index - 35) as u8),
        _ => unreachable!("packet scalar register index"),
    };
    VReg::Arch(ArchReg::Hexagon(register))
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

    /// Only a written scalar register needs a saved old value. Restore packet-old inputs
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
        let mut reads = [false; REGISTER_COUNT];
        let mut writes = [false; REGISTER_COUNT];
        let mut conditions = [None; REGISTER_COUNT];
        for operation in &operations {
            for source in operation.kind.source_vregs() {
                if let Some(index) = packet_index(source) {
                    reads[index] = true;
                }
            }
            for destination in operation.kind.dests() {
                if let Some(index) = packet_index(destination) {
                    writes[index] = true;
                    conditions[index] = match operation.kind {
                        OpKind::PredLoad { cond, .. } => Some((cond, true)),
                        OpKind::Select {
                            cond,
                            src_false,
                            src_true,
                            ..
                        } if packet_index(src_false) == Some(index)
                            && packet_index(src_true) != Some(index) =>
                        {
                            Some((cond, true))
                        }
                        OpKind::Select {
                            cond,
                            src_true,
                            src_false,
                            ..
                        } if packet_index(src_true) == Some(index)
                            && packet_index(src_false) != Some(index) =>
                        {
                            Some((cond, false))
                        }
                        _ => None,
                    };
                }
            }
        }
        for index in 0..REGISTER_COUNT {
            if writes[index] && self.new[index].is_some() && conditions[index].is_none() {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: if index < 32 {
                        "multiple nonconditional Hexagon GPR writes"
                    } else {
                        "multiple nonconditional Hexagon control-register writes"
                    }
                    .into(),
                });
            }
        }
        let mut result = Vec::with_capacity(operations.len() + 16);
        for index in 0..REGISTER_COUNT {
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
        for index in 0..REGISTER_COUNT {
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
