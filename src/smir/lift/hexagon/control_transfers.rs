//! Represented user control transfers; PC is packet-relative and read-only.
use super::*;

impl HexagonLifter {
    pub(super) fn lift_control_transfer(
        &self,
        insn: &DecodedInsn,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let mut ops = Vec::new();
        let mut op_id = 0;
        let mut push = |kind| {
            ops.push(SmirOp::new(OpId(op_id), addr, kind));
            op_id += 1;
        };
        match *insn {
            DecodedInsn::TfrCrR { dst, src: 9 } => push(OpKind::Mov {
                dst: self.hex_reg(dst),
                src: SrcOperand::Imm(self.packet_start_pc as u32 as i64),
                width: OpWidth::W32,
            }),
            DecodedInsn::TfrCrR { dst, src: 4 } => {
                let mut packed = None;
                for index in 0..4 {
                    let byte = ctx.alloc_vreg();
                    push(OpKind::And {
                        dst: byte,
                        src1: self.hex_pred(index),
                        src2: SrcOperand::Imm(0xff),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    });
                    let shifted = if index == 0 {
                        byte
                    } else {
                        let shifted = ctx.alloc_vreg();
                        push(OpKind::Shl {
                            dst: shifted,
                            src: byte,
                            amount: SrcOperand::Imm(i64::from(index) * 8),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        });
                        shifted
                    };
                    packed = Some(if let Some(previous) = packed {
                        let combined = ctx.alloc_vreg();
                        push(OpKind::Or {
                            dst: combined,
                            src1: previous,
                            src2: SrcOperand::Reg(shifted),
                            width: OpWidth::W32,
                            flags: FlagUpdate::None,
                        });
                        combined
                    } else {
                        shifted
                    });
                }
                push(OpKind::Mov {
                    dst: self.hex_reg(dst),
                    src: SrcOperand::Reg(packed.unwrap()),
                    width: OpWidth::W32,
                });
            }
            DecodedInsn::TfrCrR { dst, src } => {
                let source = self
                    .hex_creg_value(src)
                    .ok_or_else(|| LiftError::Unsupported {
                        addr,
                        mnemonic: format!("Hexagon control-register read C{src}"),
                    })?;
                push(OpKind::Mov {
                    dst: self.hex_reg(dst),
                    src: SrcOperand::Reg(source),
                    width: OpWidth::W32,
                });
            }
            DecodedInsn::TfrRrCr { dst: 9, .. } => {}
            DecodedInsn::TfrRrCr { dst: 4, src } => {
                for index in 0..4 {
                    let shifted = ctx.alloc_vreg();
                    push(OpKind::Shr {
                        dst: shifted,
                        src: self.hex_reg(src),
                        amount: SrcOperand::Imm(i64::from(index) * 8),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    });
                    push(OpKind::And {
                        dst: self.hex_pred(index),
                        src1: shifted,
                        src2: SrcOperand::Imm(0xff),
                        width: OpWidth::W32,
                        flags: FlagUpdate::None,
                    });
                }
            }
            DecodedInsn::TfrRrCr { dst, src } => {
                let destination =
                    self.hex_creg_value(dst)
                        .ok_or_else(|| LiftError::Unsupported {
                            addr,
                            mnemonic: format!("Hexagon control-register write C{dst}"),
                        })?;
                drop(push);
                self.emit_creg_value_write(
                    &mut ops,
                    &mut op_id,
                    addr,
                    ctx,
                    destination,
                    self.hex_reg(src),
                );
            }
            _ => unreachable!("only scalar user control transfers dispatch here"),
        }
        Ok((ops, ControlFlow::Fallthrough))
    }
}
