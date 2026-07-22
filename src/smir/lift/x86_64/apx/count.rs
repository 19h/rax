//! APX-promoted POPCNT, TZCNT, and LZCNT lifting.

use super::*;

impl X86_64Lifter {
    pub(crate) fn lift_apx_count(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;

        // These are two-operand APX-EVEX-INT instructions: ND is fixed zero and
        // VVVVV encodes architectural register zero. NF is optional. P2 bit 2
        // is NF, leaving only P2 bits 1:0 reserved in the decoded `aaa` field.
        // U/X4 must be encoded one for register sources and remains an address
        // extension for memory sources.
        if !matches!(prefix.pp, 0 | 1)
            || prefix.nd
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa & 0x03 != 0
            || prefix.vvvv != 0x0F
            || !prefix.v_prime
            || (modrm_byte >> 6 == 3 && !prefix.x4)
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

        let source = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };
        let destination = self.gpr(modrm.reg);
        let kind = match opcode {
            0x88 => X86CountKind::Popcnt,
            0xF4 => X86CountKind::Tzcnt,
            0xF5 => X86CountKind::Lzcnt,
            _ => unreachable!("MAP4 count dispatch admits only 88/F4/F5"),
        };
        let flags = if prefix.nf {
            FlagUpdate::None
        } else if kind == X86CountKind::Popcnt {
            FlagUpdate::All
        } else {
            FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF))
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Count {
                dst: destination,
                src: source,
                width,
                kind,
                flags,
            },
        ));
        for (index, op) in ops.iter_mut().enumerate() {
            op.id = OpId(index as u16);
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }
}
