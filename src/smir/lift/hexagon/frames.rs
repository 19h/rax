//! Deallocation reads one old effective address and always updates SP.

use super::*;

impl HexagonLifter {
    pub(super) fn lift_deallocation(
        &mut self,
        instruction: &DecodedInsn,
        pc: GuestAddr,
        context: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let (base, destination, update_lr_fp, predicate, returns) = match instruction {
            DecodedInsn::DeallocFrame {
                base,
                dst,
                update_lr_fp,
            } => (*base, *dst, *update_lr_fp, None, false),
            DecodedInsn::DeallocReturn {
                base,
                dst,
                update_lr_fp,
                pred,
            } => (*base, *dst, *update_lr_fp, *pred, true),
            _ => return Err(LiftError::Internal("non-deallocation instruction".into())),
        };
        let mut operations = Vec::new();
        let mut id = 0;
        let effective = self.emit_address32(
            &mut operations,
            &mut id,
            pc,
            context,
            Address::Direct(self.hex_reg(base)),
        );
        let condition = predicate.map(|predicate| {
            self.emit_pred_truth(
                &mut operations,
                &mut id,
                pc,
                context,
                predicate.pred,
                predicate.sense,
            )
        });
        let value = context.alloc_vreg();
        let load = if let Some(cond) = condition {
            OpKind::PredLoad {
                dst: value,
                cond,
                addr: Address::Direct(effective),
                width: MemWidth::B8,
                signed: SignExtend::Zero,
            }
        } else {
            OpKind::Load {
                dst: value,
                addr: Address::Direct(effective),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            }
        };
        operations.push(SmirOp::new(OpId(id), pc, load));
        id += 1;
        let high = context.alloc_vreg();
        operations.push(SmirOp::new(
            OpId(id),
            pc,
            OpKind::Shr {
                dst: high,
                src: value,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        id += 1;
        let mut write = |register: u8, source: VReg| {
            let destination = self.hex_reg(register);
            let kind = if let Some(cond) = condition {
                OpKind::Select {
                    dst: destination,
                    cond,
                    src_true: source,
                    src_false: destination,
                    width: OpWidth::W32,
                }
            } else {
                OpKind::Mov {
                    dst: destination,
                    src: SrcOperand::Reg(source),
                    width: OpWidth::W32,
                }
            };
            operations.push(SmirOp::new(OpId(id), pc, kind));
            id += 1;
        };
        if let Some(destination) = destination {
            if destination & 1 != 0 || destination >= 31 {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "invalid Hexagon deallocation pair".into(),
                });
            }
            write(destination, value);
            write(destination + 1, high);
        }
        if update_lr_fp {
            write(30, value);
            write(31, high);
        }
        let new_sp = context.alloc_vreg();
        operations.push(SmirOp::new(
            OpId(id),
            pc,
            OpKind::Add {
                dst: new_sp,
                src1: effective,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ));
        id += 1;
        let destination = self.hex_reg(29);
        let sp_write = if let Some(cond) = condition {
            OpKind::Select {
                dst: destination,
                cond,
                src_true: new_sp,
                src_false: destination,
                width: OpWidth::W32,
            }
        } else {
            OpKind::Mov {
                dst: destination,
                src: SrcOperand::Reg(new_sp),
                width: OpWidth::W32,
            }
        };
        operations.push(SmirOp::new(OpId(id), pc, sp_write));
        id += 1;
        let flow = if returns {
            let masked = self.emit_align4(&mut operations, &mut id, pc, context, high);
            let target = if let Some(cond) = condition {
                let target = context.alloc_vreg();
                operations.push(SmirOp::new(
                    OpId(id),
                    pc,
                    OpKind::Select {
                        dst: target,
                        cond,
                        src_true: masked,
                        src_false: VReg::imm(i64::from((pc as u32).wrapping_add(4))),
                        width: OpWidth::W32,
                    },
                ));
                target
            } else {
                masked
            };
            ControlFlow::IndirectBranch { target }
        } else {
            ControlFlow::Fallthrough
        };
        Ok((operations, flow))
    }
}
