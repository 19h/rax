//! VEX-encoded 128-bit chunk insert/extract lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{OpId, SignExtend, VecElementType, VecWidth};
use crate::smir::lift::x86_64::{
    ModRm, VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    fn vex_chunk_invalid_opcode(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    fn vex_chunk_modrm(
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

    pub(crate) fn lift_vex_chunk_extract_insert(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F3A
            || !matches!(opcode, 0x18 | 0x19 | 0x38 | 0x39)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let extract = matches!(opcode, 0x19 | 0x39);
        // Intel SDM Vol. 2C specifies VEX.256.66.0F3A.W0 for every member;
        // VEXTRACT additionally reserves VEX.vvvv=1111b (logical zero after
        // decoding). These fields are complete at the opcode frontier, so #UD
        // must not depend on fetching a ModR/M byte, address, or immediate.
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 1
            || prefix.width != VecWidth::V256
            || prefix.w
            || (extract && prefix.vvvv != 0)
        {
            return Ok(Self::vex_chunk_invalid_opcode(prefix.bytes + 1));
        }

        let cursor = prefix.bytes + 1;
        let modrm = self.vex_chunk_modrm(prefix, bytes, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let first_lane = (bytes[imm_offset] & 1) * 2;
        let mut ops = Vec::new();

        if extract {
            let source = self.vec_reg(modrm.reg, VecWidth::V256);
            let raw =
                self.append_zero_vector(VecWidth::V128, VecElementType::I64, pc, ctx, &mut ops);
            for lane in 0..2 {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: first_lane + lane,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: raw,
                        vec: raw,
                        scalar,
                        lane,
                        elem: VecElementType::I64,
                    },
                ));
            }

            if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VStore {
                        src: raw,
                        addr,
                        width: VecWidth::V128,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst: self.vec_reg(modrm.rm, VecWidth::V128),
                        src: raw,
                        width: VecWidth::V128,
                    },
                ));
            }
        } else {
            let source2 = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: VecWidth::V128,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            } else {
                self.vec_reg(modrm.rm, VecWidth::V128)
            };
            let source1 = self.vec_reg(prefix.vvvv, VecWidth::V256);
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VAnd {
                    dst: raw,
                    src1: source1,
                    src2: source1,
                    width: VecWidth::V256,
                },
            ));
            for lane in 0..2 {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source2,
                        lane,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: raw,
                        vec: raw,
                        scalar,
                        lane: first_lane + lane,
                        elem: VecElementType::I64,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: self.vec_reg(modrm.reg, VecWidth::V256),
                    src: raw,
                    width: VecWidth::V256,
                },
            ));
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
