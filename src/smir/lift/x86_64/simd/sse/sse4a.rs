//! AMD SSE4A EXTRQ/INSERTQ lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86Sse4aBitfieldKind};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, decode_modrm};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    fn sse4a_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    pub(crate) fn lift_sse4a_bitfield(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let kind = match (prefix.operand_size_override, prefix.rep_prefix) {
            (true, None) => X86Sse4aBitfieldKind::Extract,
            (false, Some(0xF2)) => X86Sse4aBitfieldKind::Insert,
            _ => return Ok(Self::sse4a_invalid_opcode(prefix.cursor)),
        };
        if prefix.lock || prefix.rex2.is_some() || !matches!(opcode, 0x78 | 0x79) {
            return Ok(Self::sse4a_invalid_opcode(prefix.cursor));
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory
            || kind == X86Sse4aBitfieldKind::Extract && opcode == 0x78 && (modrm.byte >> 3) & 7 != 0
        {
            return Ok(Self::sse4a_invalid_opcode(
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let immediate = opcode == 0x78;
        let (length, index) = if immediate {
            let offset = modrm.bytes_consumed;
            if bytes.len() <= offset {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: prefix.cursor + bytes.len(),
                    need: prefix.cursor + offset + 1,
                });
            }
            if bytes.len() <= offset + 1 {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: prefix.cursor + bytes.len(),
                    need: prefix.cursor + offset + 2,
                });
            }
            (Some(bytes[offset] & 0x3F), Some(bytes[offset + 1] & 0x3F))
        } else {
            (None, None)
        };

        let dst = match (kind, immediate) {
            (X86Sse4aBitfieldKind::Extract, true) => self.xmm(modrm.rm),
            _ => self.xmm(modrm.reg),
        };
        let source = if kind == X86Sse4aBitfieldKind::Extract && immediate {
            dst
        } else {
            self.xmm(modrm.rm)
        };
        let ops = vec![
            SmirOp::new(OpId(0), pc, OpKind::X86RequireSse4a),
            SmirOp::new(
                OpId(1),
                pc,
                OpKind::X86Sse4aBitfield {
                    dst,
                    source,
                    kind,
                    length,
                    index,
                },
            ),
        ];
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + if immediate { 2 } else { 0 },
        ))
    }
}
