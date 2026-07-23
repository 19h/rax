//! APX-promoted scalar MOVRS lifting.

use super::*;

impl X86_64Lifter {
    pub(super) fn apx_movrs_opcode_fields_valid(prefix: ApxEvexPrefix, opcode: u8) -> bool {
        let is_byte = opcode == 0x8A;
        matches!(prefix.pp, 0 | 1)
            && !prefix.nd
            && !prefix.nf
            && !prefix.z
            && prefix.ll == 0
            && prefix.aaa & 0x03 == 0
            && prefix.vvvv == 0x0F
            && prefix.v_prime
            && (!is_byte || (!prefix.w && prefix.pp == 0))
    }

    pub(crate) fn lift_apx_movrs(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !Self::apx_movrs_opcode_fields_valid(prefix, opcode) {
            return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
        }

        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        if modrm_byte >> 6 == 3 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = opcode == 0x8A;
        let op_size = prefix.op_size(is_byte);
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
        let (addr, address_ops) = self.x86_addr_to_smir(
            modrm.addr.as_ref().expect("APX MOVRS memory address"),
            next_pc,
            ctx,
        );
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireApx)];
        ops.extend(address_ops);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst: self.gpr(modrm.reg),
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
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
