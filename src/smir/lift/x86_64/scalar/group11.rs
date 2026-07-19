//! Legacy opcode `C6`/`C7` Group 11 and RTM fixed-encoding lifting.

use crate::smir::lift::x86_64::*;

#[inline]
fn is_canonical_48(addr: u64) -> bool {
    ((addr as i64) << 16 >> 16) as u64 == addr
}

impl X86_64Lifter {
    /// Lift MOV r/m, imm (C6/C7), XABORT (C6 F8 ib), and
    /// XBEGIN (C7 F8 iw/id).
    pub(crate) fn lift_mov_rm_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = match opcode {
            0xC6 => true,
            0xC7 => false,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.byte == 0xF8 {
            if prefix.lock {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
                });
            }

            // XABORT and XBEGIN use fixed raw ModR/M F8. REX/REX2 register
            // extension fields and all non-LOCK legacy prefixes are ignored.
            let imm_size = if is_8bit {
                1
            } else if op_size == 2 {
                2
            } else {
                4
            };
            let imm_offset = modrm.bytes_consumed;
            let insn_len = prefix.cursor + imm_offset + imm_size;
            if bytes.len() < imm_offset + imm_size {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: prefix.cursor + bytes.len(),
                    need: insn_len,
                });
            }

            if is_8bit {
                // Outside transactional execution XABORT is a no-op, but its
                // immediate remains part of the architectural instruction.
                return Ok(LiftResult::fallthrough(vec![], insn_len));
            }

            let offset = if imm_size == 2 {
                i16::from_le_bytes([bytes[imm_offset], bytes[imm_offset + 1]]) as i64
            } else {
                i32::from_le_bytes([
                    bytes[imm_offset],
                    bytes[imm_offset + 1],
                    bytes[imm_offset + 2],
                    bytes[imm_offset + 3],
                ]) as i64
            };
            let next_pc = pc.wrapping_add(insn_len as u64);
            let fallback = next_pc.wrapping_add_signed(offset);
            if !is_canonical_48(fallback) {
                return Ok(LiftResult {
                    ops: Vec::new(),
                    bytes_consumed: insn_len,
                    control_flow: ControlFlow::Trap {
                        kind: TrapKind::GeneralProtection,
                    },
                    branch_targets: Vec::new(),
                });
            }

            let ops = vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: self.gpr(0),
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W32,
                },
            )];
            return Ok(LiftResult::branch(ops, insn_len, fallback));
        }

        let group = (modrm.byte >> 3) & 0x07;
        if group != 0 {
            if self.strict {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("mov group {}", group),
                });
            }
            return Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let imm_offset = modrm.bytes_consumed;
        let imm_size = if is_8bit {
            1
        } else if op_size == 2 {
            2
        } else {
            4
        };
        if bytes.len() < imm_offset + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + imm_size,
            });
        }

        let imm = match imm_size {
            1 => bytes[imm_offset] as i8 as i64,
            2 => i16::from_le_bytes([bytes[imm_offset], bytes[imm_offset + 1]]) as i64,
            _ => i32::from_le_bytes([
                bytes[imm_offset],
                bytes[imm_offset + 1],
                bytes[imm_offset + 2],
                bytes[imm_offset + 3],
            ]) as i64,
        };

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        let hint = X86OpHint::MovImmModRm;

        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: VReg::Imm(imm),
                    addr,
                    width: mem_width,
                },
                hint,
            ));
        } else if let Some(base) = is_8bit
            .then(|| self.high_byte_base(modrm.rm, prefix))
            .flatten()
        {
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: value,
                    src: SrcOperand::Imm(imm),
                    width,
                },
                hint,
            ));
            self.merge_high_byte(base, value, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Imm(imm),
                    width,
                },
                hint,
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + imm_size,
        ))
    }
}
