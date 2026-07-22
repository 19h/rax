//! Legacy and APX-promoted INVPCID lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86InvpcidOp};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift legacy `66 0F 38 82 /r` INVPCID in 64-bit mode.
    pub(crate) fn lift_invpcid_0f38(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || !prefix.operand_size_override || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        self.lift_invpcid_modrm(modrm, prefix, bytes, pc, ctx, false)
    }

    /// Lift `EVEX.LLZ.F3.MAP4.WIG F2 !(11):rrr:bbb` INVPCID.
    pub(crate) fn lift_apx_invpcid(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != 2
            || prefix.nd
            || prefix.nf
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa != 0
            || prefix.vvvv_reg() != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc).map_err(|error| match error {
            LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                addr,
                have: modrm_prefix.cursor + have,
                need: modrm_prefix.cursor + need,
            },
            other => other,
        })?;
        self.lift_invpcid_modrm(modrm, &modrm_prefix, full_bytes, pc, ctx, true)
    }

    fn lift_invpcid_modrm(
        &self,
        modrm: ModRm,
        prefix: &X86Prefix,
        invalid_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        requires_apx: bool,
    ) -> Result<LiftResult, LiftError> {
        let Some(x86_addr) = modrm.addr.as_ref() else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: if requires_apx {
                    invalid_bytes.to_vec()
                } else {
                    invalid_bytes[..modrm.bytes_consumed.min(invalid_bytes.len())].to_vec()
                },
            });
        };

        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
        debug_assert!(pre_ops.is_empty());
        let stack_segment = match prefix.segment_override {
            Some(0x36) => true,
            Some(_) => false,
            None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86Invpcid(X86InvpcidOp {
                    invpcid_type: self.gpr(modrm.reg),
                    addr,
                    requires_apx,
                    stack_segment,
                    next_pc,
                }),
            )],
            bytes_consumed,
        ))
    }
}
