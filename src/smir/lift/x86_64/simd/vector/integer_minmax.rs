//! Packed signed/unsigned integer minimum and maximum lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn packed_minmax_shape(opcode: u8, qword: bool) -> (VecElementType, bool, bool) {
        let elem = match opcode {
            0xDA | 0xDE => VecElementType::I8,
            0xEA | 0xEE => VecElementType::I16,
            0x38 | 0x3C => VecElementType::I8,
            0x3A | 0x3E => VecElementType::I16,
            0x39 | 0x3B | 0x3D | 0x3F if qword => VecElementType::I64,
            0x39 | 0x3B | 0x3D | 0x3F => VecElementType::I32,
            _ => unreachable!(),
        };
        let min = matches!(opcode, 0x38..=0x3B | 0xDA | 0xEA);
        let signed = matches!(opcode, 0x38 | 0x39 | 0x3C | 0x3D | 0xEA | 0xEE);
        (elem, min, signed)
    }

    /// Select the elementwise signed/unsigned packed integer minimum or maximum.
    /// `VCmp` produces an all-ones lane mask, which makes the subsequent
    /// bit-select exact for every integer element width, including EVEX qwords.
    pub(crate) fn append_packed_minmax(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        min: bool,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let cond = match (min, signed) {
            (true, true) => VecCmpCond::Lt,
            (true, false) => VecCmpCond::Ltu,
            (false, true) => VecCmpCond::Gt,
            (false, false) => VecCmpCond::Gtu,
        };
        let select_src1 = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: select_src1,
                src1,
                src2,
                cond,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst,
                mask: select_src1,
                src_true: src1,
                src_false: src2,
                width,
            },
        ));
    }

    pub(crate) fn lift_vec_packed_minmax(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let qword = prefix.encoding == VecEncodingKind::Evex
            && prefix.w
            && matches!(opcode, 0x39 | 0x3B | 0x3D | 0x3F);
        let (elem, min, signed) = Self::packed_minmax_shape(opcode, qword);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                if broadcast {
                    self.append_masked_broadcast_memory_source(
                        addr,
                        elem,
                        prefix.width,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else {
                    self.append_evex_masked_vector_source(
                        addr,
                        elem,
                        prefix.width,
                        false,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                }
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1,
                    src2,
                    elem,
                    lanes: prefix.width.lanes(elem) as u8,
                    op: if min { VLaneOp::Min } else { VLaneOp::Max },
                    signed,
                    set_ovf: false,
                },
                self.vec_hint(prefix, opcode),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_packed_minmax(
                raw,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            self.append_packed_minmax(
                dst,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
        }
        let result = LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed);
        Ok(self.retain_evex_memory_apx_requirement(&modrm, pc, result))
    }
}
