//! Intel APX Group 2 shift and rotate lifting.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApxShiftKind {
    Rol,
    Ror,
    Rcl,
    Rcr,
    Shl,
    Shr,
    Sar,
}

impl ApxShiftKind {
    fn from_group(group: u8) -> Self {
        match group & 0x07 {
            0 => Self::Rol,
            1 => Self::Ror,
            2 => Self::Rcl,
            3 => Self::Rcr,
            4 | 6 => Self::Shl,
            5 => Self::Shr,
            7 => Self::Sar,
            _ => unreachable!("three-bit Group 2 selector is exhaustive"),
        }
    }

    fn reads_carry(self) -> bool {
        matches!(self, Self::Rcl | Self::Rcr)
    }
}

impl X86_64Lifter {
    fn apx_shift_op(
        &self,
        kind: ApxShiftKind,
        dst: VReg,
        src: VReg,
        amount: SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> OpKind {
        let rotate_flags = if flags.updates_any() {
            x86_rotate_flags()
        } else {
            FlagUpdate::None
        };
        match kind {
            ApxShiftKind::Rol => OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            ApxShiftKind::Ror => OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            ApxShiftKind::Rcl => OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            ApxShiftKind::Rcr => OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            ApxShiftKind::Shl => OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            },
            ApxShiftKind::Shr => OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            },
            ApxShiftKind::Sar => OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            },
        }
    }

    pub(crate) fn lift_apx_shift(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        let kind = ApxShiftKind::from_group((modrm_byte >> 3) & 0x07);

        // Intel APX revision 7.0 specifies {NF=0} for RCL and RCR. The Group
        // 2 extension establishes that reserved encoding at ModR/M; no SIB,
        // displacement, immediate, source read, or memory access is relevant.
        if prefix.nf && kind.reads_carry() {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = matches!(opcode, 0xC0 | 0xD0 | 0xD2);
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;

        let (amount, imm_size) = match opcode {
            0xC0 | 0xC1 => {
                if bytes.len() < modrm.bytes_consumed + 1 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: modrm.bytes_consumed + 1,
                    });
                }
                (SrcOperand::Imm(bytes[modrm.bytes_consumed] as i64), 1)
            }
            0xD0 | 0xD1 => (SrcOperand::Imm(1), 0),
            0xD2 | 0xD3 => (SrcOperand::Reg(self.gpr(1)), 0),
            _ => unreachable!("Map 4 dispatch admits only Group 2 opcodes"),
        };

        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
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
        let op_kind = self.apx_shift_op(kind, dst, legacy_dst, amount, width, prefix.flags());
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
