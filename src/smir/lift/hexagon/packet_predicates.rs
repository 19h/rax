//! Packet-old predicate inputs and auto-AND outputs (V68 sections 3.3.1/6.1.3).

use super::*;
use crate::isa::hexagon::decode::PredCond;

#[derive(Default)]
pub(super) struct PacketPredicates {
    old: Option<[VReg; 4]>,
    new: [Option<VReg>; 4],
    packed_control_write: [bool; 4],
}

fn predicate(register: VReg) -> Option<usize> {
    match register {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::P(index))) if index < 4 => Some(index as usize),
        _ => None,
    }
}

fn architectural(index: usize) -> VReg {
    VReg::Arch(ArchReg::Hexagon(HexagonReg::P(index as u8)))
}

fn new_source(insn: &DecodedInsn) -> Option<usize> {
    let condition: Option<PredCond> = match insn {
        DecodedInsn::JumpCond {
            pred,
            pred_new: true,
            ..
        }
        | DecodedInsn::JumpRegCond {
            pred,
            pred_new: true,
            ..
        } => return Some(*pred as usize),
        DecodedInsn::Load { pred, .. }
        | DecodedInsn::LoadAlign { pred, .. }
        | DecodedInsn::LoadUnpack { pred, .. }
        | DecodedInsn::Store { pred, .. }
        | DecodedInsn::StoreImm { pred, .. }
        | DecodedInsn::StoreNew { pred, .. }
        | DecodedInsn::DeallocReturn { pred, .. }
        | DecodedInsn::Call { pred, .. }
        | DecodedInsn::CallReg { pred, .. }
        | DecodedInsn::VLoad { pred, .. }
        | DecodedInsn::VStore { pred, .. } => *pred,
        DecodedInsn::ClearCond { pred, .. } => Some(*pred),
        _ => None,
    };
    condition
        .filter(|condition| condition.pred_new)
        .map(|condition| condition.pred as usize)
}

fn push(ops: &mut Vec<SmirOp>, address: GuestAddr, kind: OpKind) {
    ops.push(SmirOp::new(OpId(ops.len() as u16), address, kind));
}

fn copy(ops: &mut Vec<SmirOp>, address: GuestAddr, destination: VReg, source: VReg) {
    push(
        ops,
        address,
        OpKind::Mov {
            dst: destination,
            src: SrcOperand::Reg(source),
            width: OpWidth::W32,
        },
    );
}

impl PacketPredicates {
    /// Capture old predicate bytes before their first use. Surround an
    /// instruction with the required old/new views, then retain its aggregate
    /// result in SSA so later old reads cannot destroy the in-flight value.
    /// Work is O(operations + predicate operands), with four persistent slots.
    pub(super) fn rewrite(
        &mut self,
        operations: Vec<SmirOp>,
        insn: &DecodedInsn,
        control_flow: &mut ControlFlow,
        address: GuestAddr,
        context: &mut LiftContext,
    ) -> Result<Vec<SmirOp>, LiftError> {
        if let Some(index) = new_source(insn)
            && self.packed_control_write[index]
        {
            return Err(LiftError::Unsupported {
                addr: address,
                mnemonic: "packed predicate control write is not a .new producer".into(),
            });
        }
        let mut reads = [false; 4];
        let mut writes = [false; 4];
        for op in &operations {
            for source in op.kind.source_vregs() {
                if let Some(index) = predicate(source) {
                    reads[index] = true;
                }
            }
            for destination in op.kind.dests() {
                if let Some(index) = predicate(destination) {
                    writes[index] = true;
                }
            }
        }
        if !(0..4).any(|index| reads[index] || writes[index]) {
            return Ok(operations);
        }
        // Opaque opcode lifts do not expose whether a predicate read is .new.
        // They remain valid without a producer; after a same-packet producer,
        // defer rather than infer an architectural read mode from an opcode name.
        if matches!(insn, DecodedInsn::Unknown(_))
            && (0..4).any(|index| reads[index] && self.new[index].is_some())
        {
            return Err(LiftError::Unsupported {
                addr: address,
                mnemonic: "opaque predicate consumer after a packet producer".into(),
            });
        }
        let mut result = Vec::with_capacity(operations.len() + 16);
        let old = *self.old.get_or_insert_with(|| {
            std::array::from_fn(|index| {
                let value = context.alloc_vreg();
                copy(&mut result, address, value, architectural(index));
                value
            })
        });
        let new_read = new_source(insn);
        for index in 0..4 {
            if reads[index] {
                let source = if new_read == Some(index) {
                    self.new[index].unwrap_or(old[index])
                } else {
                    old[index]
                };
                copy(&mut result, address, architectural(index), source);
            }
        }
        for mut operation in operations {
            operation.id = OpId(result.len() as u16);
            result.push(operation);
        }
        for index in 0..4 {
            if writes[index] {
                self.packed_control_write[index] =
                    matches!(insn, DecodedInsn::TfrRrCr { dst: 4, .. });
                let computed = context.alloc_vreg();
                copy(&mut result, address, computed, architectural(index));
                self.new[index] = Some(if let Some(previous) = self.new[index] {
                    let aggregate = context.alloc_vreg();
                    push(
                        &mut result,
                        address,
                        OpKind::And {
                            dst: aggregate,
                            src1: previous,
                            src2: SrcOperand::Reg(computed),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    aggregate
                } else {
                    computed
                });
            }
            if reads[index] || writes[index] {
                copy(
                    &mut result,
                    address,
                    architectural(index),
                    self.new[index].unwrap_or(old[index]),
                );
            }
        }
        // A compound compare-jump is the packet's aggregate predicate write
        // followed by a .new consumer, not a branch on its comparison alone.
        if let DecodedInsn::CompoundCmpJump {
            write_pred: Some(pred),
            sense,
            ..
        } = insn
        {
            if let ControlFlow::CondBranchReg { cond, .. } = control_flow {
                let truth = context.alloc_vreg();
                push(
                    &mut result,
                    address,
                    OpKind::And {
                        dst: truth,
                        src1: architectural(*pred as usize),
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    },
                );
                *cond = if *sense {
                    truth
                } else {
                    let inverted = context.alloc_vreg();
                    push(
                        &mut result,
                        address,
                        OpKind::Xor {
                            dst: inverted,
                            src1: truth,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        },
                    );
                    inverted
                };
            }
        }
        Ok(result)
    }
}
