//! SIMD / NEON vector lowering

use crate::smir::lower::aarch64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    ArmDpRegShiftKind, OpKind, SmirOp, X86AdxKind, X86BlsKind, X86CountKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, AtomicOp, Avx10FP16Op, BlockId, Condition, ExtendOp, FenceKind,
    FpPrecision, FpRoundMode, MemWidth, MemoryOrder, OpWidth, ShiftOp, SignExtend, SrcOperand,
    VLaneOp, VReg, VecElementType, VecPermuteKind, VecReduceOp, VecUnaryOp, VecWidth,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

use super::{CodeBuffer, LowerError, LowerResult, Relocation, SmirLowerer};

impl Aarch64Lowerer {

    /// Spill all 32 host V registers into the state struct's V slots. A C
    /// vector-helper `blr` may clobber any caller-saved V register (V0-V7,
    /// V16-V31) — including the live operands of surrounding vector ops — so the
    /// full file must be preserved across the call. For a LOAD, the helper then
    /// overwrites only the destination slot, so the post-call reload yields the
    /// loaded vector in the dst and every other register restored.
    pub(crate) fn emit_simd_spill_all(&mut self) {
        for n in 0..32u32 {
            let imm12 = (A64_GUEST_V_OFFSET + n * 16) / 16;
            // str q_n, [x28, #V_OFFSET + n*16]
            self.emit(0x3d80_0000 | (imm12 << 10) | ((A64_STATE_REG as u32) << 5) | n);
        }
    }


    /// Reload all 32 host V registers from the state struct's V slots.
    pub(crate) fn emit_simd_reload_all(&mut self) {
        for n in 0..32u32 {
            let imm12 = (A64_GUEST_V_OFFSET + n * 16) / 16;
            // ldr q_n, [x28, #V_OFFSET + n*16]
            self.emit(0x3dc0_0000 | (imm12 << 10) | ((A64_STATE_REG as u32) << 5) | n);
        }
    }


    pub(crate) fn emit_simd_ldst_unsigned(&mut self, rt: u8, rn: u8, size: u32, opc: u32, imm12: u32) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (1 << 26)
                | (0b01 << 24)
                | (opc << 22)
                | (imm12 << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn emit_simd_ldst_simm(&mut self, rt: u8, rn: u8, size: u32, opc: u32, imm9: i64, mode: u32) {
        self.emit(
            (size << 30)
                | (0b111 << 27)
                | (1 << 26)
                | (opc << 22)
                | (((imm9 as u32) & 0x1ff) << 12)
                | (mode << 10)
                | ((rn as u32) << 5)
                | (rt as u32),
        );
    }


    pub(crate) fn emit_simd_ldst_unscaled(&mut self, rt: u8, rn: u8, size: u32, opc: u32, imm9: i64) {
        self.emit_simd_ldst_simm(rt, rn, size, opc, imm9, 0b00);
    }


    pub(crate) fn emit_simd_push_scratch(&mut self, rt: u8) {
        self.emit_simd_ldst_simm(rt, 31, 0b00, 0b10, -16, 0b11);
    }


    pub(crate) fn emit_simd_pop_scratch(&mut self, rt: u8) {
        self.emit_simd_ldst_simm(rt, 31, 0b00, 0b11, 16, 0b01);
    }


    pub(crate) fn emit_simd_three_same(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        q: u32,
        u: u32,
        size: u32,
        opcode: u32,
    ) {
        self.emit(
            0x0e20_0400
                | (q << 30)
                | (u << 29)
                | (size << 22)
                | ((rm as u32) << 16)
                | (opcode << 11)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_fp16_three_same(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        q: u32,
        u: u32,
        a: u32,
        opcode: u32,
    ) {
        self.emit(
            (0b01110 << 24)
                | (q << 30)
                | (u << 29)
                | (a << 23)
                | (1 << 22)
                | ((rm as u32) << 16)
                | (opcode << 11)
                | (1 << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_bfcvtn(&mut self, rd: u8, rn: u8, q: u32) {
        self.emit(0x0ea1_6800 | (q << 30) | ((rn as u32) << 5) | (rd as u32));
    }


    pub(crate) fn emit_simd_bfdot(&mut self, rd: u8, rn: u8, rm: u8, q: u32) {
        self.emit(
            (q << 30)
                | (1 << 29)
                | (0b01110 << 24)
                | (0b01 << 22)
                | ((rm as u32) << 16)
                | (0b111111 << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_i8_dot_kind(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        q: u32,
        src1_signed: bool,
        src2_signed: bool,
    ) -> Result<(), LowerError> {
        let (u, opcode) = match (src1_signed, src2_signed) {
            (true, true) => (0, 0b100101),
            (false, false) => (1, 0b100101),
            (false, true) => (0, 0b100111),
            (true, false) => {
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native vector SUDOT register form".to_string(),
                });
            }
        };
        self.emit(
            (0b01110 << 24)
                | (q << 30)
                | (u << 29)
                | (0b10 << 22)
                | ((rm as u32) << 16)
                | (opcode << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_simd_i8_dot(&mut self, rd: u8, rn: u8, rm: u8, q: u32, src1_unsigned: bool) {
        self.emit_simd_i8_dot_kind(rd, rn, rm, q, !src1_unsigned, true)
            .expect("legacy vector dot supports SDOT/USDOT");
    }


    pub(crate) fn emit_simd_two_reg_misc(&mut self, rd: u8, rn: u8, q: u32, u: u32, size: u32, opcode: u32) {
        self.emit(
            0x0e20_0800
                | (q << 30)
                | (u << 29)
                | (size << 22)
                | (opcode << 12)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_dup_general(&mut self, rd: u8, rn: u8, q: u32, size: u32) {
        let imm5 = 1_u32 << size;
        self.emit(
            0x0e00_0000
                | (q << 30)
                | (imm5 << 16)
                | (0b0001 << 11)
                | (1 << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_ins_general(&mut self, rd: u8, rn: u8, imm5: u32) {
        self.emit(0x4e00_1c00 | (imm5 << 16) | ((rn as u32) << 5) | (rd as u32));
    }


    pub(crate) fn emit_simd_umov(&mut self, rd: u8, rn: u8, imm5: u32, to_x: bool) {
        let base = if to_x { 0x4e00_3c00 } else { 0x0e00_3c00 };
        self.emit(base | (imm5 << 16) | ((rn as u32) << 5) | (rd as u32));
    }


    pub(crate) fn emit_simd_smov(&mut self, rd: u8, rn: u8, imm5: u32) {
        self.emit(0x4e00_2c00 | (imm5 << 16) | ((rn as u32) << 5) | (rd as u32));
    }


    pub(crate) fn emit_simd_shift_imm(
        &mut self,
        rd: u8,
        rn: u8,
        q: u32,
        u: u32,
        immh: u32,
        immb: u32,
        opcode: u32,
    ) {
        self.emit(
            0x0f00_0400
                | (q << 30)
                | (u << 29)
                | (immh << 19)
                | (immb << 16)
                | (opcode << 11)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn emit_simd_logical(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        width: VecWidth,
        op: SimdLogicOp,
    ) -> Result<(), LowerError> {
        let q = Self::simd_vec_q(width)?;
        let (u, size) = match op {
            SimdLogicOp::And => (0, 0b00),
            SimdLogicOp::AndNot => (0, 0b01),
            SimdLogicOp::Or => (0, 0b10),
            SimdLogicOp::OrNot => (0, 0b11),
            SimdLogicOp::Xor => (1, 0b00),
        };
        self.emit_simd_three_same(rd, rn, rm, q, u, size, 0b00011);
        Ok(())
    }


    pub(crate) fn emit_simd_tbl(&mut self, rd: u8, rn: u8, rm: u8, q: u32, len: u32, op: u32) {
        self.emit(
            (q << 30)
                | (0b01110 << 24)
                | ((rm as u32) << 16)
                | (len << 13)
                | (op << 12)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn simd_vec_q(width: VecWidth) -> Result<u32, LowerError> {
        match width {
            VecWidth::V64 => Ok(0),
            VecWidth::V128 => Ok(1),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native vector width {other:?}"),
            }),
        }
    }


    pub(crate) fn simd_lane_width(elem: VecElementType, lanes: u8) -> Result<VecWidth, LowerError> {
        match elem.bytes() * u32::from(lanes) {
            8 => Ok(VecWidth::V64),
            16 => Ok(VecWidth::V128),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native vector lane byte width {other}"),
            }),
        }
    }


    pub(crate) fn simd_mem_fields(width: VecWidth, load: bool) -> Result<(u32, u32, u32), LowerError> {
        match width {
            VecWidth::V64 => Ok((0b11, load as u32, 3)),
            VecWidth::V128 => Ok((0b00, 0b10 | load as u32, 4)),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native vector memory width {other:?}"),
            }),
        }
    }


    pub(crate) fn simd_integer_shape(elem: VecElementType, lanes: u8) -> Result<(u32, u32), LowerError> {
        let size = match elem {
            VecElementType::I8 => 0,
            VecElementType::I16 => 1,
            VecElementType::I32 => 2,
            VecElementType::I64 => 3,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native integer vector element {other:?}"),
                });
            }
        };

        let bytes = elem.bytes() * u32::from(lanes);
        let q = match bytes {
            8 => 0,
            16 => 1,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native integer vector byte width {other}"),
                });
            }
        };
        if size == 3 && q == 0 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native integer vector 1D arrangement".to_string(),
            });
        }
        Ok((q, size))
    }


    pub(crate) fn simd_float_shape(elem: VecElementType, lanes: u8) -> Result<(u32, u32), LowerError> {
        let size = match elem {
            VecElementType::F32 => 0,
            VecElementType::F64 => 1,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native FP vector element {other:?}"),
                });
            }
        };

        let bytes = elem.bytes() * u32::from(lanes);
        let q = match bytes {
            8 => 0,
            16 => 1,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native FP vector byte width {other}"),
                });
            }
        };
        if elem == VecElementType::F64 && q == 0 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native FP vector 1D arrangement".to_string(),
            });
        }
        Ok((q, size))
    }


    pub(crate) fn simd_fp16_shape(width: VecWidth) -> Result<u32, LowerError> {
        match width {
            VecWidth::V64 => Ok(0),
            VecWidth::V128 => Ok(1),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native FP16 vector width {other:?}"),
            }),
        }
    }


    pub(crate) fn simd_broadcast_shape(elem: VecElementType, lanes: u8) -> Result<(u32, u32), LowerError> {
        let size = match elem {
            VecElementType::I8 => 0,
            VecElementType::I16 => 1,
            VecElementType::I32 | VecElementType::F32 => 2,
            VecElementType::I64 | VecElementType::F64 => 3,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native broadcast element {other:?}"),
                });
            }
        };

        let bytes = elem.bytes() * u32::from(lanes);
        let q = match bytes {
            8 => 0,
            16 => 1,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native broadcast byte width {other}"),
                });
            }
        };
        if size == 3 && q == 0 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native broadcast 1D arrangement".to_string(),
            });
        }
        Ok((q, size))
    }


    pub(crate) fn simd_lane_imm5(elem: VecElementType, lane: u8) -> Result<(u32, u32), LowerError> {
        let size = match elem {
            VecElementType::I8 => 0,
            VecElementType::I16 => 1,
            VecElementType::I32 => 2,
            VecElementType::I64 => 3,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector lane element {other:?}"),
                });
            }
        };
        let max_lanes = 16_u8 >> size;
        if lane >= max_lanes {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native vector lane".into(),
                operand: format!("lane={lane}, elem={elem:?}"),
            });
        }
        Ok((size, (u32::from(lane) << (size + 1)) | (1 << size)))
    }


    pub(crate) fn lower_simd_mem_access(
        &mut self,
        rt: u8,
        addr: &Address,
        width: VecWidth,
        load: bool,
    ) -> Result<(), LowerError> {
        let (size, opc, scale_shift) = Self::simd_mem_fields(width, load)?;
        if let Address::BaseIndexScale {
            base,
            index,
            scale,
            disp,
            ..
        } = addr
        {
            let (scratches, addr) =
                self.lower_base_index_scale_to_scratch(&[], *base, *index, *scale, *disp)?;
            self.emit_simd_ldst_unsigned(rt, addr, size, opc, 0);
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        let (base_vreg, base, offset) = match addr {
            Address::Direct(base) => (*base, Self::base_gpr(*base)?, 0),
            Address::BaseOffset { base, offset, .. } => (*base, Self::base_gpr(*base)?, *offset),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector memory address {other:?}"),
                });
            }
        };

        let scale = 1_i64 << scale_shift;
        if offset >= 0 && offset % scale == 0 {
            let imm12 = offset / scale;
            if imm12 <= 0xfff {
                self.emit_simd_ldst_unsigned(rt, base, size, opc, imm12 as u32);
                return Ok(());
            }
        }

        if (-256..=255).contains(&offset) {
            self.emit_simd_ldst_unscaled(rt, base, size, opc, offset);
            return Ok(());
        }

        let (scratches, addr) = self.lower_base_offset_to_scratch(&[], base_vreg, offset)?;
        self.emit_simd_ldst_unsigned(rt, addr, size, opc, 0);
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_vlogic(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        op: SimdLogicOp,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        self.emit_simd_logical(rd, rn, rm, width, op)
    }


    pub(crate) fn lower_vshift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: SrcOperand,
        shift: ShiftOp,
        elem: VecElementType,
        lanes: u8,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        let imm = match amount {
            SrcOperand::Imm(value) => value,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector shift amount {other:?}"),
                });
            }
        };
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        let bits = 8_u32 << size;
        let amount = (imm as u32) % bits;
        if amount == 0 {
            let width = if q == 1 {
                VecWidth::V128
            } else {
                VecWidth::V64
            };
            return self.emit_simd_logical(rd, rn, rn, width, SimdLogicOp::Or);
        }

        let (u, opcode, immhimmb) = match shift {
            ShiftOp::Lsl => (0, 0b01010, bits + amount),
            ShiftOp::Lsr => (1, 0b00000, 2 * bits - amount),
            ShiftOp::Asr => (0, 0b00000, 2 * bits - amount),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector shift {other:?}"),
                });
            }
        };
        self.emit_simd_shift_imm(rd, rn, q, u, immhimmb >> 3, immhimmb & 0x7, opcode);
        Ok(())
    }


    pub(crate) fn lower_vshift_acc(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: SrcOperand,
        shift: ShiftOp,
        elem: VecElementType,
        lanes: u8,
    ) -> Result<(), LowerError> {
        let imm = match amount {
            SrcOperand::Imm(value) => value,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector shift-acc amount {other:?}"),
                });
            }
        };
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        if q == 0 && size == 3 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native vector shift-acc I64x1".to_string(),
            });
        }

        let bits = 8_u32 << size;
        let amount = (imm as u32) % bits;
        if amount == 0 {
            return self.lower_varith(dst, dst, src, elem, lanes, SimdArithmeticOp::Add);
        }

        let u = match shift {
            ShiftOp::Asr => 0,
            ShiftOp::Lsr => 1,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native vector shift-acc {other:?}"),
                });
            }
        };
        let immhimmb = 2 * bits - amount;
        self.emit_simd_shift_imm(rd, rn, q, u, immhimmb >> 3, immhimmb & 0x7, 0b00010);
        Ok(())
    }


    /// Emit an "advanced SIMD across lanes" instruction:
    /// `0 Q U 01110 size 11000 opcode 10 Rn Rd`.
    pub(crate) fn emit_simd_across_lanes(&mut self, rd: u8, rn: u8, q: u32, u: u32, size: u32, opcode: u32) {
        self.emit(
            0x0e30_0800
                | (q << 30)
                | (u << 29)
                | (size << 22)
                | (opcode << 12)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    /// Emit an "advanced SIMD permute" instruction:
    /// `0 Q 0 01110 size 0 Rm 0 opcode 10 Rn Rd` (opcode = bits[14:12]).
    pub(crate) fn emit_simd_permute(&mut self, rd: u8, rn: u8, rm: u8, q: u32, size: u32, opcode: u32) {
        self.emit(
            0x0e00_0800
                | (q << 30)
                | (size << 22)
                | ((rm as u32) << 16)
                | (opcode << 12)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
    }


    pub(crate) fn lower_vdotproduct(
        &mut self,
        dst: VReg,
        acc: VReg,
        src1: VReg,
        src2: VReg,
        src_elem: VecElementType,
        acc_elem: VecElementType,
        width: VecWidth,
        src1_unsigned: bool,
        saturate: bool,
    ) -> Result<(), LowerError> {
        if src_elem != VecElementType::I8 || acc_elem != VecElementType::I32 || saturate {
            return Err(LowerError::UnsupportedOp {
                op: format!(
                    "AArch64 native vector dot src={src_elem:?} \
                     acc={acc_elem:?} saturate={saturate}"
                ),
            });
        }

        let q = Self::simd_vec_q(width)?;
        let rd = Self::fp_reg(dst)?;
        let ra = Self::fp_reg(acc)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        if rd != ra {
            if rd == rn || rd == rm {
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native vector dot accumulator copy alias".to_string(),
                });
            }
            self.lower_vmov(dst, acc, width)?;
        }

        self.emit_simd_i8_dot(rd, rn, rm, q, src1_unsigned);
        Ok(())
    }


    pub(crate) fn lower_vdotproduct_ext(
        &mut self,
        dst: VReg,
        acc: VReg,
        src1: VReg,
        src2: VReg,
        src_elem: VecElementType,
        acc_elem: VecElementType,
        width: VecWidth,
        src1_signed: bool,
        src2_signed: bool,
        saturate: bool,
    ) -> Result<(), LowerError> {
        if src_elem != VecElementType::I8 || acc_elem != VecElementType::I32 || saturate {
            return Err(LowerError::UnsupportedOp {
                op: format!(
                    "AArch64 native vector ext dot src={src_elem:?} \
                     acc={acc_elem:?} saturate={saturate}"
                ),
            });
        }

        let q = Self::simd_vec_q(width)?;
        let rd = Self::fp_reg(dst)?;
        let ra = Self::fp_reg(acc)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        if rd != ra {
            if rd == rn || rd == rm {
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native vector ext dot accumulator copy alias".to_string(),
                });
            }
            self.lower_vmov(dst, acc, width)?;
        }

        let (dot_rn, dot_rm, dot_rn_signed, dot_rm_signed) = if src1_signed && !src2_signed {
            (rm, rn, false, true)
        } else {
            (rn, rm, src1_signed, src2_signed)
        };
        self.emit_simd_i8_dot_kind(rd, dot_rn, dot_rm, q, dot_rn_signed, dot_rm_signed)
    }


    pub(crate) fn lower_vdotproduct_bf16(
        &mut self,
        dst: VReg,
        acc: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
    ) -> Result<(), LowerError> {
        let q = Self::simd_vec_q(width)?;
        let rd = Self::fp_reg(dst)?;
        let ra = Self::fp_reg(acc)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        if rd != ra {
            if rd == rn || rd == rm {
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native BF16 dot accumulator copy alias".to_string(),
                });
            }
            self.lower_vmov(dst, acc, width)?;
        }
        self.emit_simd_bfdot(rd, rn, rm, q);
        Ok(())
    }


    pub(crate) fn lower_vcvt_fp32_to_bf16(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: Option<VReg>,
        width: VecWidth,
    ) -> Result<(), LowerError> {
        if width != VecWidth::V128 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native FP32-to-BF16 vector width {width:?}"),
            });
        }

        let rd = Self::fp_reg(dst)?;
        let rn1 = Self::fp_reg(src1)?;
        match src2 {
            Some(src2) => {
                if rd == rn1 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native FP32-to-BF16 src1 alias".to_string(),
                    });
                }
                let rn2 = Self::fp_reg(src2)?;
                self.emit_simd_bfcvtn(rd, rn2, 0);
                self.emit_simd_bfcvtn(rd, rn1, 1);
            }
            None => self.emit_simd_bfcvtn(rd, rn1, 0),
        }
        Ok(())
    }


    pub(crate) fn lower_vcvt_bf16_to_fp32(
        &mut self,
        dst: VReg,
        src: VReg,
        width: VecWidth,
    ) -> Result<(), LowerError> {
        if width != VecWidth::V128 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native BF16-to-FP32 vector width {width:?}"),
            });
        }

        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        self.emit_simd_shift_imm(rd, rn, 0, 1, 0b0010, 0, 0b10100);
        self.emit_simd_shift_imm(rd, rd, 1, 0, 0b0110, 0, 0b01010);
        Ok(())
    }


    pub(crate) fn lower_vcvt_fp_to_int_sat(
        &mut self,
        dst: VReg,
        src: VReg,
        fp_elem: VecElementType,
        int_elem: VecElementType,
        width: VecWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        let q = Self::simd_vec_q(width)?;
        let u = if signed { 0 } else { 1 };

        match (fp_elem, int_elem) {
            (VecElementType::F32, VecElementType::I8) => {
                self.emit_simd_two_reg_misc(rd, rn, q, u, 0b10, 0b11011);
                self.emit_simd_two_reg_misc(rd, rd, 0, u, 0b01, 0b10100);
                self.emit_simd_two_reg_misc(rd, rd, 0, u, 0b00, 0b10100);
                Ok(())
            }
            (VecElementType::F64, VecElementType::I64) => {
                if width != VecWidth::V128 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native saturating FP64-to-int64 width {width:?}"),
                    });
                }
                self.emit_simd_two_reg_misc(rd, rn, q, u, 0b11, 0b11011);
                Ok(())
            }
            _ => Err(LowerError::UnsupportedOp {
                op: format!(
                    "AArch64 native saturating FP-to-int conversion {fp_elem:?} to {int_elem:?}"
                ),
            }),
        }
    }


    pub(crate) fn lower_vlane_three_same(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        signed: bool,
        opcode: u32,
        allow_i64: bool,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        if size == 3 && !allow_i64 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native VLane opcode {opcode:#07b} I64"),
            });
        }
        let u = if signed { 0 } else { 1 };
        self.emit_simd_three_same(rd, rn, rm, q, u, size, opcode);
        Ok(())
    }


    pub(crate) fn lower_vlane(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        op: VLaneOp,
        signed: bool,
        set_ovf: bool,
    ) -> Result<(), LowerError> {
        if set_ovf {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native VLane Hexagon OVF side effect".to_string(),
            });
        }

        match op {
            VLaneOp::Add => self.lower_varith(dst, src1, src2, elem, lanes, SimdArithmeticOp::Add),
            VLaneOp::Sub => self.lower_varith(dst, src1, src2, elem, lanes, SimdArithmeticOp::Sub),
            VLaneOp::Mul => self.lower_varith(dst, src1, src2, elem, lanes, SimdArithmeticOp::Mul),
            VLaneOp::Min => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b01101, false)
            }
            VLaneOp::Max => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b01100, false)
            }
            VLaneOp::And => {
                let width = Self::simd_lane_width(elem, lanes)?;
                self.lower_vlogic(dst, src1, src2, width, SimdLogicOp::And)
            }
            VLaneOp::Or => {
                let width = Self::simd_lane_width(elem, lanes)?;
                self.lower_vlogic(dst, src1, src2, width, SimdLogicOp::Or)
            }
            VLaneOp::Xor => {
                let width = Self::simd_lane_width(elem, lanes)?;
                self.lower_vlogic(dst, src1, src2, width, SimdLogicOp::Xor)
            }
            VLaneOp::AndNot => {
                let width = Self::simd_lane_width(elem, lanes)?;
                self.lower_vlogic(dst, src1, src2, width, SimdLogicOp::AndNot)
            }
            VLaneOp::OrNot => {
                let width = Self::simd_lane_width(elem, lanes)?;
                self.lower_vlogic(dst, src1, src2, width, SimdLogicOp::OrNot)
            }
            VLaneOp::Not => {
                let rd = Self::fp_reg(dst)?;
                let rn = Self::fp_reg(src1)?;
                let q = Self::simd_vec_q(Self::simd_lane_width(elem, lanes)?)?;
                self.emit_simd_two_reg_misc(rd, rn, q, 1, 0, 0b00101);
                Ok(())
            }
            VLaneOp::AddSat => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b00001, true)
            }
            VLaneOp::SubSat => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b00101, true)
            }
            VLaneOp::Avg => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b00000, false)
            }
            VLaneOp::AvgRnd => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b00010, false)
            }
            VLaneOp::Sign => Err(LowerError::UnsupportedOp {
                op: "AArch64 native VLane Sign".to_string(),
            }),
            VLaneOp::AbsDiff => {
                self.lower_vlane_three_same(dst, src1, src2, elem, lanes, signed, 0b01110, false)
            }
        }
    }


    pub(crate) fn lower_vlane_unary_two_reg(
        &mut self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        u: u32,
        opcode: u32,
        allow_i64: bool,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        if size == 3 && !allow_i64 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native VLaneUnary opcode {opcode:#07b} I64"),
            });
        }
        self.emit_simd_two_reg_misc(rd, rn, q, u, size, opcode);
        Ok(())
    }


    pub(crate) fn lower_vlane_unary_clb(
        &mut self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        if size == 3 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native VLaneUnary Clb I64".to_string(),
            });
        }

        let mut lane_imm5 = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let (_, imm5) = Self::simd_lane_imm5(elem, lane)?;
            lane_imm5.push(imm5);
        }

        self.emit_simd_two_reg_misc(rd, rn, q, 0, size, 0b00100);

        let scratches = Self::scratch_regs(&[], 1)?;
        self.emit_scratch_save(&scratches);
        let scratch = scratches[0];
        for imm5 in lane_imm5 {
            self.emit_simd_umov(scratch, rd, imm5, false);
            self.emit_addsub_imm(scratch, scratch, 1, false, false, OpWidth::W32)?;
            self.emit_simd_ins_general(rd, scratch, imm5);
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_vlane_unary(
        &mut self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        op: u8,
        _signed: bool,
    ) -> Result<(), LowerError> {
        match op {
            0 => {
                let rd = Self::fp_reg(dst)?;
                let rn = Self::fp_reg(src)?;
                let q = Self::simd_vec_q(Self::simd_lane_width(elem, lanes)?)?;
                self.emit_simd_two_reg_misc(rd, rn, q, 1, 0, 0b00101);
                Ok(())
            }
            1 => self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 0, 0b01011, true),
            2 => self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 0, 0b00111, true),
            3 => self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 1, 0b00100, false),
            4 => {
                if elem != VecElementType::I8 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native VLaneUnary popcount element {elem:?}"),
                    });
                }
                self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 0, 0b00101, false)
            }
            5 => self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 0, 0b00100, false),
            6 => self.lower_vlane_unary_two_reg(dst, src, elem, lanes, 1, 0b01011, true),
            7 => self.lower_vlane_unary_clb(dst, src, elem, lanes),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native VLaneUnary op {other}"),
            }),
        }
    }


    pub(crate) fn lower_vmultiply_add52(
        &mut self,
        dst: VReg,
        acc: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        high: bool,
    ) -> Result<(), LowerError> {
        if width != VecWidth::V128 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native vector IFMA52 width {width:?}"),
            });
        }

        let rd = Self::fp_reg(dst)?;
        let ra_vec = Self::fp_reg(acc)?;
        let rn_vec = Self::fp_reg(src1)?;
        let rm_vec = Self::fp_reg(src2)?;
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(0x000f_ffff_ffff_ffff, OpWidth::W64)?;
        let scratches = Self::scratch_regs(&[], 5)?;
        self.emit_scratch_save(&scratches);
        let lhs = scratches[0];
        let rhs = scratches[1];
        let part = scratches[2];
        let upper = scratches[3];
        let accum = scratches[4];

        for lane in 0..2 {
            let (_, imm5) = Self::simd_lane_imm5(VecElementType::I64, lane)?;
            self.emit_simd_umov(lhs, rn_vec, imm5, true);
            self.emit_simd_umov(rhs, rm_vec, imm5, true);
            self.emit_simd_umov(accum, ra_vec, imm5, true);
            self.emit_logic_imm(lhs, lhs, 0b00, imm_n, immr, imms, OpWidth::W64)?;
            self.emit_logic_imm(rhs, rhs, 0b00, imm_n, immr, imms, OpWidth::W64)?;
            self.emit_dp3(part, lhs, rhs, 31, 0b000, 0, OpWidth::W64)?;
            if high {
                self.emit_dp3(upper, lhs, rhs, 31, 0b110, 0, OpWidth::W64)?;
                self.emit_bitfield(part, part, 0b10, 52, 63, OpWidth::W64)?;
                self.emit_bitfield(upper, upper, 0b10, 52, 51, OpWidth::W64)?;
                self.emit_logic_reg_n(part, part, upper, 0b01, false, OpWidth::W64)?;
            } else {
                self.emit_logic_imm(part, part, 0b00, imm_n, immr, imms, OpWidth::W64)?;
            }
            self.emit_addsub_reg(part, accum, part, false, false, OpWidth::W64)?;
            self.emit_simd_ins_general(rd, part, imm5);
        }

        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn clmul_source_gpr(src: &SrcOperand) -> Result<Option<u8>, LowerError> {
        match src {
            SrcOperand::Reg(reg) => Ok(Some(Self::gpr_arm_or_x86(*reg)?)),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => Ok(None),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native ClMul source {other:?}"),
            }),
        }
    }


    pub(crate) fn emit_clmul_operand(&mut self, dst: u8, src: &SrcOperand) -> Result<(), LowerError> {
        match src {
            SrcOperand::Reg(reg) => {
                self.emit_mov_reg(dst, Self::gpr_arm_or_x86(*reg)?, OpWidth::W32)
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                self.emit_mov_imm(dst, *imm, OpWidth::W32)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native ClMul source {other:?}"),
            }),
        }
    }


    pub(crate) fn lower_clmul_product(
        &mut self,
        dst: u8,
        lhs: u8,
        rhs: u8,
        bits: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_mov_imm(dst, 0, width)?;
        for bit in 0..u32::from(bits) {
            let skip = self.code.position();
            self.emit_test_branch(rhs, bit, false, 0)?;
            self.emit_logic_shifted(dst, dst, lhs, 0b10, false, 0, bit, width)?;
            self.patch_test_branch_to_current(skip, rhs, bit, false)?;
        }
        Ok(())
    }


    pub(crate) fn emit_finish_clmul_word(&mut self, dst: u8, value: u8, acc: bool) -> Result<(), LowerError> {
        if acc {
            self.emit_logic_shifted(dst, dst, value, 0b10, false, 0, 0, OpWidth::W32)
        } else {
            self.emit_mov_reg(dst, value, OpWidth::W32)
        }
    }


    pub(crate) fn lower_clmul(
        &mut self,
        dst: VReg,
        dst_hi: Option<VReg>,
        src1: &SrcOperand,
        src2: &SrcOperand,
        elem_bits: u8,
        lanes: u8,
        acc: bool,
    ) -> Result<(), LowerError> {
        match (elem_bits, lanes) {
            (32, 1) | (16, 2) => {}
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native ClMul shape {other:?}"),
                });
            }
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let dst_hi = dst_hi.map(Self::dst_gpr_arm_or_x86).transpose()?;
        let src1_reg = Self::clmul_source_gpr(src1)?;
        let src2_reg = Self::clmul_source_gpr(src2)?;
        let mut avoid = vec![dst];
        if let Some(dst_hi) = dst_hi {
            avoid.push(dst_hi);
        }
        if let Some(src1) = src1_reg {
            avoid.push(src1);
        }
        if let Some(src2) = src2_reg {
            avoid.push(src2);
        }

        let scratch_count = if lanes == 1 { 3 } else { 5 };
        let scratches = Self::scratch_regs(&avoid, scratch_count)?;
        self.emit_scratch_save(&scratches);

        if lanes == 1 {
            let product = scratches[0];
            let lhs = scratches[1];
            let rhs = scratches[2];
            self.emit_clmul_operand(lhs, src1)?;
            self.emit_clmul_operand(rhs, src2)?;
            self.lower_clmul_product(product, lhs, rhs, 32, OpWidth::W64)?;
            self.emit_finish_clmul_word(dst, product, acc)?;
            if let Some(dst_hi) = dst_hi {
                self.emit_bitfield(lhs, product, 0b10, 32, 63, OpWidth::W64)?;
                self.emit_finish_clmul_word(dst_hi, lhs, acc)?;
            }
        } else {
            let result_lo = scratches[0];
            let result_hi = scratches[1];
            let lhs = scratches[2];
            let rhs = scratches[3];
            let product = scratches[4];
            self.emit_clmul_operand(lhs, src1)?;
            self.emit_clmul_operand(rhs, src2)?;

            self.emit_bitfield(result_lo, lhs, 0b10, 0, 15, OpWidth::W32)?;
            self.emit_bitfield(result_hi, rhs, 0b10, 0, 15, OpWidth::W32)?;
            self.lower_clmul_product(product, result_lo, result_hi, 16, OpWidth::W32)?;
            self.emit_bitfield(result_lo, product, 0b10, 0, 15, OpWidth::W32)?;
            self.emit_bitfield(result_hi, product, 0b10, 16, 31, OpWidth::W32)?;

            self.emit_bitfield(lhs, lhs, 0b10, 16, 31, OpWidth::W32)?;
            self.emit_bitfield(rhs, rhs, 0b10, 16, 31, OpWidth::W32)?;
            self.lower_clmul_product(product, lhs, rhs, 16, OpWidth::W32)?;
            self.emit_logic_shifted(
                result_lo,
                result_lo,
                product,
                0b01,
                false,
                0,
                16,
                OpWidth::W32,
            )?;
            self.emit_bitfield(lhs, product, 0b10, 16, 31, OpWidth::W32)?;
            self.emit_logic_shifted(result_hi, result_hi, lhs, 0b01, false, 0, 16, OpWidth::W32)?;

            self.emit_finish_clmul_word(dst, result_lo, acc)?;
            if let Some(dst_hi) = dst_hi {
                self.emit_finish_clmul_word(dst_hi, result_hi, acc)?;
            }
        }

        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn emit_simd_scratch_save(&mut self, regs: &[u8]) {
        for &reg in regs {
            self.emit_simd_push_scratch(reg);
        }
    }


    pub(crate) fn emit_simd_scratch_restore(&mut self, regs: &[u8]) {
        for &reg in regs.iter().rev() {
            self.emit_simd_pop_scratch(reg);
        }
    }


    pub(crate) fn try_lower_fused_vector_inverted_logic(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::VXor {
                        dst: inverted,
                        src1: xor_src1,
                        src2: xor_src2,
                        width: xor_width,
                    },
                ..
            },
            next,
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if !matches!(inverted, VReg::Virtual(_)) {
            return Ok(None);
        }

        let inverted_src = if *xor_src1 == VReg::Imm(-1) {
            *xor_src2
        } else if *xor_src2 == VReg::Imm(-1) {
            *xor_src1
        } else {
            return Ok(None);
        };

        let Some((dst, other_src, width, logic_op)) = (match &next.kind {
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            } => Self::vector_inverted_logic_sources(
                *dst,
                *src1,
                *src2,
                *width,
                *inverted,
                SimdLogicOp::AndNot,
            ),
            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            } => Self::vector_inverted_logic_sources(
                *dst,
                *src1,
                *src2,
                *width,
                *inverted,
                SimdLogicOp::OrNot,
            ),
            _ => None,
        }) else {
            return Ok(None);
        };

        if width != *xor_width {
            return Ok(None);
        }

        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(other_src)?;
        let rm = Self::fp_reg(inverted_src)?;
        self.emit_simd_logical(rd, rn, rm, width, logic_op)?;
        Ok(Some(2))
    }
}
