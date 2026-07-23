//! Scalar legacy MOVRS lifting.

use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    fn movrs_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    pub(crate) fn lift_movrs_0f38(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // MOVRS is NOREP, LOCK is not accepted, and its EGPR-capable form is
        // the dedicated APX-EVEX encoding rather than a legacy REX2 form.
        if prefix.lock || prefix.rep_prefix.is_some() || prefix.rex2.is_some() {
            return Ok(Self::movrs_invalid_opcode(prefix.cursor));
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Ok(Self::movrs_invalid_opcode(
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let is_byte = opcode == 0x8A;
        let op_size = if is_byte { 1 } else { prefix.op_size() };
        let mem_width = self.size_to_memwidth(op_size);
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(
            modrm.addr.as_ref().expect("MOVRS memory address"),
            next_pc,
            ctx,
        );

        let destination = if is_byte && self.high_byte_base(modrm.reg, prefix).is_some() {
            ctx.alloc_vreg()
        } else {
            self.gpr(modrm.reg)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst: destination,
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ));
        if is_byte && self.high_byte_base(modrm.reg, prefix).is_some() {
            self.write_byte_reg(modrm.reg, prefix, destination, pc, ctx, &mut ops);
        }

        for (index, op) in ops.iter_mut().enumerate() {
            op.id = OpId(index as u16);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
