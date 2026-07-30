//! AMD XOP `VPERMIL2PS` and `VPERMIL2PD` lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::{
    VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    fn append_vpermil2_indices(
        &self,
        selector: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = (16 / elem.bytes()) as u8;
        let shifted = if elem == VecElementType::I64 {
            self.append_vector_shift(selector, 1, ShiftOp::Lsr, elem, lanes, pc, ctx, ops)
        } else {
            selector
        };
        let selector_mask =
            self.append_vector_splat_imm(u64::from(2 * block_lanes - 1), width, elem, pc, ctx, ops);
        let selected = self.append_vector_and(shifted, selector_mask, width, pc, ctx, ops);

        if width == VecWidth::V128 {
            return selected;
        }

        debug_assert_eq!(width, VecWidth::V256);
        let within_mask =
            self.append_vector_splat_imm(u64::from(block_lanes - 1), width, elem, pc, ctx, ops);
        let within = self.append_vector_and(selected, within_mask, width, pc, ctx, ops);
        let source_mask =
            self.append_vector_splat_imm(u64::from(block_lanes), width, elem, pc, ctx, ops);
        let source = self.append_vector_and(selected, source_mask, width, pc, ctx, ops);
        let source = self.append_vector_shift(source, 1, ShiftOp::Lsl, elem, lanes, pc, ctx, ops);

        // VPermute numbers its second table after all lanes of the first table,
        // while VPERMIL2 selectors number sources independently inside each
        // 128-bit block. Add the high-block table offset explicitly.
        let block_offsets = self.append_zero_vector(width, elem, pc, ctx, ops);
        let high_block_offset = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: high_block_offset,
                src: SrcOperand::Imm(i64::from(block_lanes)),
                width: OpWidth::W64,
            },
        ));
        for lane in block_lanes..lanes {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: block_offsets,
                    vec: block_offsets,
                    scalar: high_block_offset,
                    lane,
                    elem,
                },
            ));
        }
        let normalized = self.append_vector_or(within, source, width, pc, ctx, ops);
        self.append_vector_or(normalized, block_offsets, width, pc, ctx, ops)
    }

    fn append_vpermil2_m2z(
        &self,
        permuted: VReg,
        selector: VReg,
        m2z: u8,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        if m2z & 0b10 == 0 {
            return permuted;
        }

        let lanes = width.lanes(elem) as u8;
        let m_bit = self.append_vector_splat_imm(8, width, elem, pc, ctx, ops);
        let selected_m = self.append_vector_and(selector, m_bit, width, pc, ctx, ops);
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let m_mask = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: m_mask,
                src1: selected_m,
                src2: zero,
                cond: VecCmpCond::Ne,
                elem,
                lanes,
            },
        ));

        if m2z == 0b10 {
            self.append_vector_and_not(m_mask, permuted, width, pc, ctx, ops)
        } else {
            self.append_vector_and(m_mask, permuted, width, pc, ctx, ops)
        }
    }

    pub(crate) fn lift_vex_vpermil2(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x48 => VecElementType::I32,
            0x49 => VecElementType::I64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor.min(bytes.len())..], &modrm_prefix, pc).map_err(
            |error| match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: cursor + have,
                    need: cursor + need,
                },
                error => error,
            },
        )?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }

        let next_pc = pc + imm_offset as u64 + 1;
        // Although VPERMIL2 uses a VEX encoding, AMD assigns it to the XOP
        // feature subset. The live guest enablement guard must execute before
        // address generation, alignment validation, or architectural reads.
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireXop)];
        let dst = self.vec_reg(modrm.reg, prefix.width);
        let src1 = self.vec_reg(prefix.vvvv, prefix.width);
        let is4_source = self.vec_reg(bytes[imm_offset] >> 4, prefix.width);
        let rm_source = if modrm.is_memory {
            let x86_addr = modrm
                .addr
                .as_ref()
                .expect("decoded VPERMIL2 memory address");
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            for mut pre_op in pre_ops {
                pre_op.id = OpId(ops.len() as u16);
                ops.push(pre_op);
            }
            let stack_segment = match prefix.segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => x86_addr.base.is_some_and(|base| matches!(base & 7, 4 | 5)),
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignmentAc {
                    addr: addr.clone(),
                    access_size: prefix.width.bytes() as u8,
                    alignment: 16,
                    stack_segment,
                    natural_alignment: false,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };

        // AMD APM Volume 4: VEX.W swaps only the r/m and SRS source roles.
        let (src2, selector) = if prefix.w {
            (is4_source, rm_source)
        } else {
            (rm_source, is4_source)
        };
        let indices = self.append_vpermil2_indices(selector, elem, prefix.width, pc, ctx, &mut ops);
        let permuted = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPermute {
                dst: permuted,
                src1,
                src2: Some(src2),
                indices,
                elem,
                width: prefix.width,
                overwrite_table: false,
            },
        ));
        let result = self.append_vpermil2_m2z(
            permuted,
            selector,
            bytes[imm_offset] & 0b11,
            elem,
            prefix.width,
            pc,
            ctx,
            &mut ops,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: result,
                width: prefix.width,
            },
        ));

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
