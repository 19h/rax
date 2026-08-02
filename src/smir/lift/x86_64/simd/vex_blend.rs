//! VEX immediate- and register-mask blend lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{OpId, VecElementType, VecWidth};
use crate::smir::lift::x86_64::{
    ModRm, VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    fn vex_blend_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    fn vex_blend_modrm(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
    ) -> Result<ModRm, LiftError> {
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        decode_modrm(&bytes[cursor.min(bytes.len())..], &modrm_prefix, pc).map_err(|error| {
            match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: cursor + have,
                    need: cursor + need,
                },
                error => error,
            }
        })
    }

    pub(crate) fn lift_vex_immediate_blend(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F3A
            || !matches!(opcode, 0x02 | 0x0C..=0x0E)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if opcode == 0x02
            && (prefix.pp != X86SsePrefix::OpSize
                || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
                || prefix.w)
        {
            return Ok(Self::vex_blend_invalid_opcode(prefix.bytes + 1));
        }
        if prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, repeat_128) = match opcode {
            0x02 | 0x0C => (VecElementType::I32, false),
            0x0D => (VecElementType::I64, false),
            0x0E => (VecElementType::I16, true),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let cursor = prefix.bytes + 1;
        let modrm = self.vex_blend_modrm(prefix, bytes, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_immediate_blend(
            self.vec_reg(modrm.reg, prefix.width),
            self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            elem,
            prefix.width,
            imm,
            repeat_128,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vex_variable_blend(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x4A => VecElementType::I32,
            0x4B => VecElementType::I64,
            0x4C => VecElementType::I8,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let cursor = prefix.bytes + 1;
        let modrm = self.vex_blend_modrm(prefix, bytes, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let mask_index = bytes[imm_offset] >> 4;
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_variable_blend(
            self.vec_reg(modrm.reg, prefix.width),
            self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            self.vec_reg(mask_index, prefix.width),
            elem,
            prefix.width,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
