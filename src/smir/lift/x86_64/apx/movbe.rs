//! APX-promoted MOVBE lifting.

use super::*;

impl X86_64Lifter {
    pub(crate) fn lift_apx_movbe(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        let register_form = modrm_byte >> 6 == 3;
        if !matches!(prefix.pp, 0 | 1)
            || prefix.nd
            || prefix.nf
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa != 0
            || prefix.vvvv != 0x0F
            || !prefix.v_prime
            || (register_form && !prefix.x4)
        {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let op_size = prefix.op_size(false);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc).map_err(|error| match error {
            LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                addr,
                have: prefix.bytes + 1 + have,
                need: prefix.bytes + 1 + need,
            },
            other => other,
        })?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireApx)];

        match opcode {
            0x60 => {
                let src = if modrm.is_memory {
                    let (addr, pre_ops) = self.x86_addr_to_smir(
                        modrm.addr.as_ref().expect("memory ModR/M address"),
                        next_pc,
                        ctx,
                    );
                    ops.extend(pre_ops);
                    let temporary = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: temporary,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                    temporary
                } else {
                    self.gpr(modrm.rm)
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Bswap {
                        dst: self.gpr(modrm.reg),
                        src,
                        width,
                    },
                ));
            }
            0x61 => {
                let src = self.gpr(modrm.reg);
                if modrm.is_memory {
                    let (addr, pre_ops) = self.x86_addr_to_smir(
                        modrm.addr.as_ref().expect("memory ModR/M address"),
                        next_pc,
                        ctx,
                    );
                    ops.extend(pre_ops);
                    let temporary = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Bswap {
                            dst: temporary,
                            src,
                            width,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: temporary,
                            addr,
                            width: mem_width,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Bswap {
                            dst: self.gpr(modrm.rm),
                            src,
                            width,
                        },
                    ));
                }
            }
            _ => unreachable!("MAP4 MOVBE dispatch admits only opcodes 60/61"),
        }

        for (index, op) in ops.iter_mut().enumerate() {
            op.id = OpId(index as u16);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }
}
