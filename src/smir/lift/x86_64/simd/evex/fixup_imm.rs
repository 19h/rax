//! EVEX VFIXUPIMM floating-point table replacement.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_fixup_imm(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x55;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x54 | 0x55)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };

        // Packed register EVEX.b is SAE and fixes the vector length at 512
        // bits. Packed memory EVEX.b is broadcast. Scalar EVEX.b is SAE for
        // both register and scalar-memory encodings.
        let embedded_sae = prefix.b && (scalar || !modrm.is_memory);
        if !scalar && !embedded_sae && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + imm_offset as u64 + 1;
        let (src2, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let register_width = if scalar { VecWidth::V128 } else { width };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            register_width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            register_width,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
