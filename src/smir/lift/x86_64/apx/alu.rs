//! Intel APX integer ALU and Group 1 immediate lifting.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApxAluKind {
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
}

impl ApxAluKind {
    fn from_group(group: u8) -> Option<Self> {
        match group {
            0 => Some(Self::Add),
            1 => Some(Self::Or),
            2 => Some(Self::Adc),
            3 => Some(Self::Sbb),
            4 => Some(Self::And),
            5 => Some(Self::Sub),
            6 => Some(Self::Xor),
            _ => None,
        }
    }

    fn reads_carry(self) -> bool {
        matches!(self, Self::Adc | Self::Sbb)
    }
}

impl X86_64Lifter {
    fn apx_alu_op(
        &self,
        kind: ApxAluKind,
        dst: VReg,
        src1: VReg,
        src2: SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> OpKind {
        match kind {
            ApxAluKind::Add => OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::Or => OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::Adc => OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::Sbb => OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::And => OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::Sub => OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            ApxAluKind::Xor => OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            },
        }
    }

    pub(crate) fn lift_apx_alu(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let low = opcode & 0x07;
        let (is_byte, rm_is_legacy_dst) = match low {
            0 => (true, true),
            1 => (false, true),
            2 => (true, false),
            3 => (false, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let Some(kind) = ApxAluKind::from_group((opcode >> 3) & 0x07) else {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        };

        // Intel APX revision 7.0 specifies {NF=0} for ADC and SBB. Because
        // the opcode selects the carry-reading operation, #UD is established
        // before a ModR/M, SIB, displacement, or memory operand is observed.
        if prefix.nf && kind.reads_carry() {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }

        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let reg = self.gpr(modrm.reg);
        let (rm, rm_addr) = if modrm.is_memory {
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

        let (legacy_dst, src2, legacy_dst_addr) = if rm_is_legacy_dst {
            (rm, reg, rm_addr)
        } else {
            (reg, rm, None)
        };
        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            legacy_dst
        };
        let op_kind = self.apx_alu_op(
            kind,
            dst,
            legacy_dst,
            SrcOperand::Reg(src2),
            width,
            prefix.flags(),
        );
        let hint = X86OpHint::AluEncoding(if rm_is_legacy_dst {
            X86AluEncoding::RmReg
        } else {
            X86AluEncoding::RegRm
        });
        ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, op_kind, hint));

        if !prefix.nd {
            if let Some(addr) = legacy_dst_addr {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: dst,
                        addr,
                        width: mem_width,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_group1_imm(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        let group = (modrm_byte >> 3) & 0x07;

        // The Group 1 extension selects ADC/SBB. Once ModR/M is available,
        // NF makes /2 and /3 terminal #UD encodings; apparent addressing and
        // immediate bytes are not part of the proof and must not be demanded.
        if prefix.nf && matches!(group, 2 | 3) {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = opcode == 0x80;
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;

        let (imm, imm_size) = match opcode {
            0x80 => {
                if bytes.len() < imm_offset + 1 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + 1,
                    });
                }
                (bytes[imm_offset] as i8 as i64, 1)
            }
            0x81 => {
                if bytes.len() < imm_offset + 4 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + 4,
                    });
                }
                (
                    i32::from_le_bytes([
                        bytes[imm_offset],
                        bytes[imm_offset + 1],
                        bytes[imm_offset + 2],
                        bytes[imm_offset + 3],
                    ]) as i64,
                    4,
                )
            }
            0x83 => {
                if bytes.len() < imm_offset + 1 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + 1,
                    });
                }
                (bytes[imm_offset] as i8 as i64, 1)
            }
            _ => unreachable!("Map 4 dispatch admits only Group 1 opcodes"),
        };

        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        if group == 7 {
            if prefix.nd {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "APX CCMP immediate with NDD".to_string(),
                });
            }

            let memory_load = if modrm.is_memory {
                let x86_addr = modrm.addr.as_ref().unwrap();
                let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                ops.extend(pre_ops);

                let tmp = ctx.alloc_vreg();
                Some((tmp, addr))
            } else {
                None
            };
            let src1 = memory_load
                .as_ref()
                .map(|(tmp, _)| *tmp)
                .unwrap_or_else(|| self.gpr(modrm.rm));

            self.push_apx_conditional_flags_with(
                &mut ops,
                pc,
                ctx,
                self.x86_cond(prefix.ccmp_cond()),
                prefix.ccmp_default_flags(),
                |ops, cond_reg| {
                    if let Some((dst, addr)) = memory_load {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::PredLoad {
                                dst,
                                cond: cond_reg,
                                addr,
                                width: mem_width,
                                signed: SignExtend::Zero,
                            },
                        ));
                    }
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Cmp {
                            src1,
                            src2: SrcOperand::Imm(imm),
                            width,
                        },
                    ));
                },
            );

            return Ok(LiftResult::fallthrough(
                ops,
                prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
            ));
        }

        let Some(kind) = ApxAluKind::from_group(group) else {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        };
        let (legacy_dst, legacy_dst_addr) = if modrm.is_memory {
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

        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            legacy_dst
        };
        let op_kind = self.apx_alu_op(
            kind,
            dst,
            legacy_dst,
            SrcOperand::Imm(imm),
            width,
            prefix.flags(),
        );
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        if !prefix.nd {
            if let Some(addr) = legacy_dst_addr {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: dst,
                        addr,
                        width: mem_width,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
        ))
    }
}
