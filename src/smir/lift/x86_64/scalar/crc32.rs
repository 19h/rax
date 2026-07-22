//! CRC32 lifting shared by legacy SSE4.2 and APX MAP4 encodings.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_crc32_0f38(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix != Some(0xF2) || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let data_width = if opcode == 0xF0 {
            OpWidth::W8
        } else if prefix.rex_w() {
            OpWidth::W64
        } else if prefix.operand_size_override {
            OpWidth::W16
        } else {
            OpWidth::W32
        };
        let modrm = Self::decode_crc32_modrm(bytes, prefix, pc)?;
        self.lift_crc32_modrm(modrm, prefix, pc, ctx, data_width, false)
    }

    pub(crate) fn lift_apx_crc32(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let pp_valid = match opcode {
            0xF0 => prefix.pp == 0,
            0xF1 => matches!(prefix.pp, 0 | 1),
            _ => false,
        };
        let register_source = bytes.first().is_some_and(|modrm| modrm >> 6 == 3);
        if !pp_valid
            || prefix.nd
            || prefix.nf
            || prefix.z
            || prefix.ll != 0
            || prefix.aaa != 0
            || prefix.vvvv_reg() != 0
            || (register_source && !prefix.x4)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let data_width = if opcode == 0xF0 {
            OpWidth::W8
        } else {
            self.size_to_width(prefix.op_size(false))
        };
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = Self::decode_crc32_modrm(bytes, &modrm_prefix, pc)?;
        self.lift_crc32_modrm(modrm, &modrm_prefix, pc, ctx, data_width, true)
    }

    fn decode_crc32_modrm(bytes: &[u8], prefix: &X86Prefix, pc: u64) -> Result<ModRm, LiftError> {
        decode_modrm(bytes, prefix, pc).map_err(|error| match error {
            LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                addr,
                have: prefix.cursor + have,
                need: prefix.cursor + need,
            },
            other => other,
        })
    }

    fn lift_crc32_modrm(
        &self,
        modrm: ModRm,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        data_width: OpWidth,
        requires_apx: bool,
    ) -> Result<LiftResult, LiftError> {
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = if requires_apx {
            vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireApx)]
        } else {
            Vec::new()
        };
        let data = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: loaded,
                    addr,
                    width: match data_width {
                        OpWidth::W8 => MemWidth::B1,
                        OpWidth::W16 => MemWidth::B2,
                        OpWidth::W32 => MemWidth::B4,
                        OpWidth::W64 => MemWidth::B8,
                        OpWidth::W128 => unreachable!(),
                    },
                    sign: SignExtend::Zero,
                },
            ));
            loaded
        } else if data_width == OpWidth::W8 {
            self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops)
        } else {
            self.gpr(modrm.rm)
        };
        let dst = self.gpr(modrm.reg);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Crc32C {
                dst,
                crc: dst,
                data,
                data_width,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
