//! Legacy opcode `FF` Group 5 lifting.

use crate::smir::ir::ops::{X86FarCallOp, X86FarJumpOp};
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    /// Lift Group 5 instructions (`FF /0` through `FF /7`).
    pub(crate) fn lift_group5(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let group = (modrm.byte >> 3) & 0x07;

        if prefix.lock {
            if !modrm.is_memory || (group != 0 && group != 1) {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let one = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: one,
                    src: SrcOperand::Imm(1),
                    width,
                },
            ));
            let old = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::AtomicRmw {
                    dst: old,
                    addr,
                    src: one,
                    op: if group == 0 {
                        AtomicOp::Add
                    } else {
                        AtomicOp::Sub
                    },
                    width: mem_width,
                    order: MemoryOrder::SeqCst,
                },
            ));
            let flag_result = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if group == 0 {
                    OpKind::Inc {
                        dst: flag_result,
                        src: old,
                        width,
                        flags: FlagUpdate::All,
                    }
                } else {
                    OpKind::Dec {
                        dst: flag_result,
                        src: old,
                        width,
                        flags: FlagUpdate::All,
                    }
                },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        // Far CALL/JMP are memory-only, and /7 is reserved. These encoding
        // checks precede every operand read: a register form or /7 raises #UD
        // without observing a would-be memory operand.
        if group == 7 || matches!(group, 3 | 5) && !modrm.is_memory {
            return Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            });
        }

        if group == 5 {
            let x86_addr = modrm
                .addr
                .as_ref()
                .expect("validated far-JMP memory operand changed");
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let target = VReg::Arch(ArchReg::X86(X86Reg::Rip));
            let stack_segment = match prefix.segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86FarJump(X86FarJumpOp {
                    addr,
                    target,
                    offset_width: prefix.op_width(),
                    requires_apx: prefix.rex2.is_some(),
                    stack_segment,
                    next_pc,
                }),
            ));
            return Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::IndirectBranch { target },
                branch_targets: vec![],
            });
        }

        if group == 3 {
            let x86_addr = modrm
                .addr
                .as_ref()
                .expect("validated far-CALL memory operand changed");
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let target = VReg::Arch(ArchReg::X86(X86Reg::Rip));
            let stack_segment = match prefix.segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86FarCall(X86FarCallOp {
                    addr,
                    target,
                    offset_width: prefix.op_width(),
                    requires_apx: prefix.rex2.is_some(),
                    stack_segment,
                    next_pc,
                }),
            ));
            return Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::IndirectBranch { target },
                branch_targets: vec![],
            });
        }

        if modrm.is_memory && (group == 2 || group == 4) {
            let x86_addr = modrm.addr.as_ref().unwrap();
            if group == 2 && x86_addr.address_width == OpWidth::W32 {
                let addr = self.x86_addr32_state_address(x86_addr, next_pc);
                return Ok(LiftResult {
                    ops,
                    bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                    control_flow: ControlFlow::Call {
                        target: CallTarget::X86IndirectMemAddr32(addr),
                    },
                    branch_targets: vec![],
                });
            }

            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let control_flow = if group == 2 {
                ControlFlow::Call {
                    target: CallTarget::IndirectMem(addr),
                }
            } else {
                ControlFlow::IndirectBranchMem { addr }
            };

            return Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow,
                branch_targets: vec![],
            });
        }

        let (operand, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            (self.gpr(modrm.rm), None)
        };

        match group {
            0 => {
                let result = if addr.is_some() {
                    ctx.alloc_vreg()
                } else {
                    operand
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Inc {
                        dst: result,
                        src: operand,
                        width,
                        flags: if addr.is_some() {
                            FlagUpdate::None
                        } else {
                            FlagUpdate::All
                        },
                    },
                ));
                if let Some(addr) = addr {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: result,
                            addr,
                            width: mem_width,
                        },
                    ));
                    let flag_result = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Inc {
                            dst: flag_result,
                            src: operand,
                            width,
                            flags: FlagUpdate::All,
                        },
                    ));
                }
                Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            1 => {
                let result = if addr.is_some() {
                    ctx.alloc_vreg()
                } else {
                    operand
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Dec {
                        dst: result,
                        src: operand,
                        width,
                        flags: if addr.is_some() {
                            FlagUpdate::None
                        } else {
                            FlagUpdate::All
                        },
                    },
                ));
                if let Some(addr) = addr {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: result,
                            addr,
                            width: mem_width,
                        },
                    ));
                    let flag_result = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Dec {
                            dst: flag_result,
                            src: operand,
                            width,
                            flags: FlagUpdate::All,
                        },
                    ));
                }
                Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            2 => Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::Call {
                    target: CallTarget::Indirect(operand),
                },
                branch_targets: vec![],
            }),
            4 => Ok(LiftResult {
                ops,
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::IndirectBranch { target: operand },
                branch_targets: vec![],
            }),
            6 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Sub {
                        dst: self.rsp(),
                        src1: self.rsp(),
                        src2: SrcOperand::Imm(8),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: operand,
                        addr: Address::Direct(self.rsp()),
                        width: MemWidth::B8,
                    },
                ));
                Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            _ => unreachable!("far/invalid Group-5 forms returned before scalar operand decode"),
        }
    }
}
