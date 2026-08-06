//! Legacy SSE2 transfers between the MMX and XMM register files.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86X87ControlKind};
use crate::smir::ir::types::{OpId, OpWidth};
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    /// Lift `MOVQ2DQ xmm, mm` (`F3 0F D6 /r`) and `MOVDQ2Q mm, xmm`
    /// (`F2 0F D6 /r`). Both encodings are register-only and enter MMX state.
    /// Ordinary REX extends only the encoded XMM operand; MMX indices remain
    /// three bits.
    pub(crate) fn lift_sse_mmx_xmm_transfer(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let Some(rep) = prefix.rep_prefix else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.lock
            || prefix.operand_size_override
            || prefix.rex2.is_some()
            || !matches!(rep, 0xF2 | 0xF3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let (dst, src, mandatory_prefix) = if rep == 0xF3 {
            (
                self.xmm(modrm.reg),
                self.mm(modrm.rm & 0x07),
                X86SsePrefix::Rep,
            )
        } else {
            (
                self.mm(modrm.reg & 0x07),
                self.xmm(modrm.rm),
                X86SsePrefix::Repne,
            )
        };
        let ops = vec![
            SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ),
            SmirOp::with_hint(
                OpId(1),
                pc,
                OpKind::X86MovdQ {
                    dst,
                    src,
                    width: OpWidth::W64,
                    zero_upper: false,
                },
                X86OpHint::SseOp {
                    prefix: mandatory_prefix,
                    opcode: 0xD6,
                },
            ),
        ];

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
