//! VEX BMI2 `MULX` lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VecWidth};
use crate::smir::lift::x86_64::{
    VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vex_mulx_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::Repne
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: Some(0xF2),
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
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

        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::MulU {
                dst_lo: self.gpr(prefix.vvvv),
                dst_hi: Some(self.gpr(modrm.reg)),
                src1: self.gpr(2),
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }
}
