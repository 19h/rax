//! AVX10.1 and AVX10.2 instruction lowering.
//!
//! This module lowers SMIR operations to EVEX-encoded AVX10 machine code.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::*;
use crate::smir::lower::{CodeBuffer, LowerError};

mod saturating_convert;

/// Result type for AVX10 lowering operations
pub type Avx10LowerResult<T> = Result<T, LowerError>;

// ============================================================================
// EVEX Encoder
// ============================================================================

/// EVEX instruction encoder
pub struct EvexEncoder<'a> {
    code: &'a mut CodeBuffer,
}

impl<'a> EvexEncoder<'a> {
    pub fn new(code: &'a mut CodeBuffer) -> Self {
        Self { code }
    }

    /// Encode EVEX prefix
    ///
    /// EVEX format:
    /// P0: 62h
    /// P1: R X B R' 0 m m m
    /// P2: W v v v v 1 p p
    /// P3: z L' L b V' a a a
    pub fn emit_evex(
        &mut self,
        map: u8, // 1=0F, 2=0F38, 3=0F3A, 5=MAP5
        pp: u8,  // 0=none, 1=66, 2=F3, 3=F2
        w: bool,
        vl: VecWidth,
        dst: u8,  // destination register (0-31)
        src1: u8, // vvvv source register (0-31)
        src2: u8, // r/m source register (0-31)
        mask: u8, // opmask k0-k7
        zeroing: bool,
    ) {
        self.emit_evex_with_b(map, pp, w, vl, dst, src1, src2, mask, zeroing, false, None);
    }

    /// Encode an EVEX prefix with an explicit EVEX.b bit and optional L'L
    /// override. This is used for register-only SAE/embedded-rounding forms;
    /// ordinary callers retain b=0 and vector-length-derived L'L through
    /// [`Self::emit_evex`].
    pub fn emit_evex_with_b(
        &mut self,
        map: u8,
        pp: u8,
        w: bool,
        vl: VecWidth,
        dst: u8,
        src1: u8,
        src2: u8,
        mask: u8,
        zeroing: bool,
        b_bit: bool,
        ll_override: Option<u8>,
    ) {
        // Extract register bits
        let r = (dst >> 3) & 1; // bit 3 of dst
        let r_prime = (dst >> 4) & 1; // bit 4 of dst
        let x = (src2 >> 4) & 1; // bit 4 of src2 (index)
        let b = (src2 >> 3) & 1; // bit 3 of src2
        let vvvv = src1 & 0x0F;
        let v_prime = (src1 >> 4) & 1;

        let ll = ll_override.unwrap_or(match vl {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            VecWidth::V64 => 0,
        });
        debug_assert!(ll < 4);

        // Build P0
        self.code.emit_u8(0x62);

        // Build P1: ~R ~X ~B ~R' 0 mmm
        let p1 =
            ((r ^ 1) << 7) | ((x ^ 1) << 6) | ((b ^ 1) << 5) | ((r_prime ^ 1) << 4) | (map & 0x07);
        self.code.emit_u8(p1);

        // Build P2: W ~vvvv 1 pp
        let vvvv_inv = (!vvvv) & 0x0F;
        let p2 = ((w as u8) << 7) | (vvvv_inv << 3) | 0x04 | (pp & 0x03);
        self.code.emit_u8(p2);

        // Build P3: z L'L b ~V' aaa
        let p3 = ((zeroing as u8) << 7)
            | (ll << 5)
            | ((b_bit as u8) << 4)
            | ((v_prime ^ 1) << 3)
            | (mask & 0x07);
        self.code.emit_u8(p3);
    }

    /// Emit opcode byte
    pub fn emit_opcode(&mut self, opcode: u8) {
        self.code.emit_u8(opcode);
    }

    /// Emit immediate byte
    pub fn emit_imm8(&mut self, imm: u8) {
        self.code.emit_u8(imm);
    }

    /// Emit ModR/M byte for register-register operation
    pub fn emit_modrm_rr(&mut self, reg: u8, rm: u8) {
        let modrm = 0xC0 | ((reg & 0x07) << 3) | (rm & 0x07);
        self.code.emit_u8(modrm);
    }

    /// Emit ModR/M and optional SIB for memory operand
    pub fn emit_modrm_mem(&mut self, reg: u8, base: u8, disp: i32) {
        let reg_bits = reg & 0x07;
        let base_bits = base & 0x07;

        // Determine mod bits based on displacement
        let (mod_bits, disp_bytes) = if disp == 0 && base_bits != 5 {
            (0, 0)
        } else if disp >= -128 && disp <= 127 {
            (1, 1)
        } else {
            (2, 4)
        };

        // Check if SIB is needed (RSP/R12 as base)
        if base_bits == 4 {
            self.code.emit_u8((mod_bits << 6) | (reg_bits << 3) | 4);
            self.code.emit_u8(0x24); // SIB: scale=0, index=RSP(4), base=RSP(4)
        } else {
            self.code
                .emit_u8((mod_bits << 6) | (reg_bits << 3) | base_bits);
        }

        // Emit displacement
        match disp_bytes {
            1 => self.code.emit_i8(disp as i8),
            4 => self.code.emit_i32(disp),
            _ => {}
        }
    }
}

// ============================================================================
// AVX10 Lowerer
// ============================================================================

/// AVX10 instruction lowerer
pub struct Avx10Lowerer;

impl Avx10Lowerer {
    pub fn new() -> Self {
        Self
    }

    /// Try to lower an SMIR operation to AVX10 machine code
    /// Returns None if not an AVX10 operation
    pub fn try_lower(&self, op: &OpKind, code: &mut CodeBuffer) -> Option<Avx10LowerResult<()>> {
        match op {
            // AVX10.1 VNNI
            OpKind::VDotProduct {
                dst,
                acc,
                src1,
                src2,
                mask,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing,
                ..
            } => Some(self.lower_vdotproduct(
                code,
                dst,
                acc,
                src1,
                src2,
                mask.as_ref(),
                *src_elem,
                *acc_elem,
                *width,
                *src1_unsigned,
                *saturate,
                *zeroing,
            )),

            // AVX10.1 IFMA
            OpKind::VMultiplyAdd52 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                high,
                zeroing,
            } => Some(self.lower_vpmadd52(
                code,
                dst,
                acc,
                src1,
                src2,
                mask.as_ref(),
                *width,
                *high,
                *zeroing,
            )),

            // AVX10.1 VPOPCNT
            OpKind::VPopcnt {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => Some(self.lower_vpopcnt(code, dst, src, mask.as_ref(), *elem, *width, *zeroing)),

            // AVX-512 conflict detection
            OpKind::VConflict {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                Some(self.lower_vpconflict(code, dst, src, mask.as_ref(), *elem, *width, *zeroing))
            }

            // AVX-512 leading-zero count
            OpKind::VLeadingZeros {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => Some(self.lower_vplzcnt(code, dst, src, mask.as_ref(), *elem, *width, *zeroing)),

            // AVX10.1 VBMI permute
            OpKind::VPermute {
                dst,
                src1,
                src2,
                indices,
                elem,
                width,
                overwrite_table,
            } => Some(self.lower_vpermute(
                code,
                dst,
                src1,
                src2,
                indices,
                *elem,
                *width,
                *overwrite_table,
            )),

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                elem,
                width,
                overwrite_table,
                zeroing,
            } => Some(self.lower_x86_permute_bytes_words(
                code,
                dst,
                table1,
                table2.as_ref(),
                indices,
                mask.as_ref(),
                *elem,
                *width,
                *overwrite_table,
                *zeroing,
            )),

            // AVX10.1 BITALG
            OpKind::VShuffleBitQM {
                dst,
                src,
                indices,
                mask,
                width,
            } => Some(self.lower_vpshufbitqmb(code, dst, src, indices, mask.as_ref(), *width)),

            OpKind::VCompress {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => Some(self.lower_compress_expand(
                code,
                dst,
                src,
                mask.as_ref(),
                *elem,
                *width,
                *zeroing,
                true,
            )),
            OpKind::VExpand {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => Some(self.lower_compress_expand(
                code,
                dst,
                src,
                mask.as_ref(),
                *elem,
                *width,
                *zeroing,
                false,
            )),

            OpKind::X86NarrowInt {
                dst,
                src,
                mask,
                src_elem,
                dst_elem,
                width,
                mode,
                zeroing,
            } => Some(self.lower_x86_narrow_int(
                code,
                dst,
                src,
                mask.as_ref(),
                *src_elem,
                *dst_elem,
                *width,
                *mode,
                *zeroing,
            )),

            OpKind::X86Aes {
                dst,
                src1,
                src2,
                width,
                op,
                imm,
            } => Some(self.lower_x86_aes(code, dst, src1, src2.as_ref(), *width, *op, *imm)),

            OpKind::X86Sha512Msg1 { dst, src } => {
                Some(self.lower_x86_sha512(code, 0xCC, dst, None, src))
            }
            OpKind::X86Sha512Msg2 { dst, src } => {
                Some(self.lower_x86_sha512(code, 0xCD, dst, None, src))
            }
            OpKind::X86Sha512Rounds2 { dst, state, wk } => {
                Some(self.lower_x86_sha512(code, 0xCB, dst, Some(state), wk))
            }

            OpKind::X86Sm3Msg1 { dst, src1, src2 } => {
                Some(self.lower_x86_sm3(code, 2, 0, 0xDA, dst, src1, src2, None))
            }
            OpKind::X86Sm3Msg2 { dst, src1, src2 } => {
                Some(self.lower_x86_sm3(code, 2, 1, 0xDA, dst, src1, src2, None))
            }
            OpKind::X86Sm3Rounds2 {
                dst,
                state,
                words,
                imm,
            } => Some(self.lower_x86_sm3(code, 3, 1, 0xDE, dst, state, words, Some(*imm))),

            OpKind::X86Sm4 {
                dst,
                src1,
                src2,
                width,
                key_schedule,
            } => Some(self.lower_x86_sm4(code, dst, src1, src2, *width, *key_schedule)),

            OpKind::X86PackedShiftImm {
                dst,
                src,
                width,
                elem,
                shift,
                amount,
                byte_lane,
            } => Some(self.lower_x86_packed_shift_imm(
                code, dst, src, *width, *elem, *shift, *amount, *byte_lane,
            )),
            OpKind::X86PackedShift {
                dst,
                src,
                count,
                width,
                elem,
                shift,
            } => Some(self.lower_x86_packed_shift(code, dst, src, count, *width, *elem, *shift)),

            // AVX10.1 BF16
            OpKind::VDotProductBF16 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => Some(self.lower_vdpbf16ps(
                code,
                dst,
                acc,
                src1,
                src2,
                mask.as_ref(),
                *width,
                *zeroing,
            )),

            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => Some(self.lower_vcvtfp32tobf16(
                code,
                dst,
                src1,
                src2.as_ref(),
                mask.as_ref(),
                *width,
                *zeroing,
            )),

            // AVX10.1 FP16
            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                op,
                round,
                width,
                lanes,
                zeroing,
            } => {
                if u32::from(*lanes) != width.lanes(VecElementType::F16) {
                    Some(Err(LowerError::UnsupportedOperation(
                        "partial-lane FP16 arithmetic requires exact source replay".to_string(),
                    )))
                } else if *round != FpRoundMode::Dynamic {
                    Some(Err(LowerError::UnsupportedOperation(
                        "packed FP16 embedded rounding / SAE is not lowered natively".to_string(),
                    )))
                } else {
                    Some(self.lower_vfp16_arith(
                        code,
                        dst,
                        src1,
                        src2,
                        mask.as_ref(),
                        *op,
                        *width,
                        *zeroing,
                    ))
                }
            }

            // AVX10.2 scalar saturation conversions
            OpKind::X86ScalarFpToIntSat {
                dst,
                src,
                elem,
                int_width,
                signed,
                suppress_exceptions,
            } => Some(self.lower_x86_scalar_fp_to_int_sat(
                code,
                dst,
                src,
                *elem,
                *int_width,
                *signed,
                *suppress_exceptions,
            )),

            // AVX10.2 packed saturation conversions
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask,
                fp_elem: fp_format,
                int_elem,
                width,
                signed,
                truncate,
                round,
                zeroing,
                suppress_exceptions,
            } => Some(self.lower_vcvt_fp_to_int_sat(
                code,
                dst,
                src,
                mask.as_ref(),
                *fp_format,
                *int_elem,
                *width,
                *signed,
                *truncate,
                *round,
                *zeroing,
                *suppress_exceptions,
            )),

            // AVX10.2 VMINMAX
            OpKind::VMinMax {
                dst,
                src1,
                src2,
                elem,
                width,
                imm,
            } => Some(self.lower_vminmax(code, dst, src1, src2, *elem, *width, *imm)),

            OpKind::X86PackedShiftVariable {
                dst,
                src,
                count,
                mask,
                width,
                elem,
                shift,
                zeroing,
            } => Some(self.lower_packed_shift_variable(
                code,
                dst,
                src,
                count,
                mask.as_ref(),
                *width,
                *elem,
                *shift,
                *zeroing,
            )),

            OpKind::X86PackedRotate {
                dst,
                src,
                count,
                mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => Some(self.lower_packed_rotate(
                code,
                dst,
                src,
                count.as_ref(),
                mask.as_ref(),
                *amount,
                *width,
                *elem,
                *left,
                *zeroing,
            )),

            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask,
                imm,
                width,
                elem,
                zeroing,
            } => Some(self.lower_ternary_logic(
                code,
                dst,
                src1,
                src2,
                src3,
                mask.as_ref(),
                *imm,
                *width,
                *elem,
                *zeroing,
            )),

            OpKind::X86PackedFunnelShift {
                dst,
                src,
                fill,
                count,
                mask,
                amount,
                width,
                elem,
                left,
                zeroing,
            } => Some(self.lower_packed_funnel_shift(
                code,
                dst,
                src,
                fill,
                count.as_ref(),
                mask.as_ref(),
                *amount,
                *width,
                *elem,
                *left,
                *zeroing,
            )),

            OpKind::X86MultiShiftQB {
                dst,
                control,
                source,
                mask,
                width,
                zeroing,
            } => Some(self.lower_multishift_qb(
                code,
                dst,
                control,
                source,
                mask.as_ref(),
                *width,
                *zeroing,
            )),

            // AVX10.2 VMPSADBW
            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                width,
                imm,
                zeroing,
            } => Some(self.lower_vmpsadbw(
                code,
                dst,
                src1,
                src2,
                mask.as_ref(),
                *width,
                *imm,
                *zeroing,
            )),

            // AVX10.2 Media acceleration
            OpKind::VDotProductExt {
                dst,
                acc,
                src1,
                src2,
                src_elem,
                acc_elem,
                width,
                src1_signed,
                src2_signed,
                saturate,
                ..
            } => Some(self.lower_vdotproduct_ext(
                code,
                dst,
                acc,
                src1,
                src2,
                *src_elem,
                *acc_elem,
                *width,
                *src1_signed,
                *src2_signed,
                *saturate,
            )),

            _ => None,
        }
    }

    // ========================================================================
    // VNNI Instructions
    // ========================================================================

    fn lower_vdotproduct(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        acc: &VReg,
        src1: &VReg,
        src2: &VReg,
        mask: Option<&VReg>,
        src_elem: VecElementType,
        acc_elem: VecElementType,
        width: VecWidth,
        src1_unsigned: bool,
        saturate: bool,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if acc_elem != VecElementType::I32 || dst != acc {
            return Err(LowerError::UnsupportedOperation(format!(
                "VNNI requires an I32 accumulator aliased with dst, got acc={acc_elem:?} dst={dst:?} accumulator={acc:?}"
            )));
        }
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VNNI requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31
            || src1_reg > 31
            || src2_reg > 31
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "VNNI vector register must be 0..31 and explicit opmask must be K1..K7".to_string(),
            ));
        }

        let opcode = match (src_elem, src1_unsigned, saturate) {
            (VecElementType::I8, true, false) => 0x50,   // VPDPBUSD
            (VecElementType::I8, true, true) => 0x51,    // VPDPBUSDS
            (VecElementType::I16, false, false) => 0x52, // VPDPWSSD
            (VecElementType::I16, false, true) => 0x53,  // VPDPWSSDS
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VNNI: invalid element type or signedness".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,     // map 0F38
            1,     // pp = 66
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // IFMA Instructions
    // ========================================================================

    fn lower_vpmadd52(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        acc: &VReg,
        src1: &VReg,
        src2: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
        high: bool,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if dst != acc {
            return Err(LowerError::UnsupportedOperation(
                "VPMADD52 requires acc aliased with dst".to_string(),
            ));
        }
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VPMADD52 requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31
            || src1_reg > 31
            || src2_reg > 31
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "VPMADD52 vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }

        let opcode = if high { 0xB5 } else { 0xB4 };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,    // map 0F38
            1,    // pp = 66
            true, // W = 1
            width, dst_reg, src1_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // VPOPCNT Instructions
    // ========================================================================

    fn lower_vpopcnt(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        elem: VecElementType,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;

        let (opcode, w) = match elem {
            VecElementType::I8 => (0x54, false),
            VecElementType::I16 => (0x54, true),
            VecElementType::I32 => (0x55, false),
            VecElementType::I64 => (0x55, true),
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VPOPCNT: invalid element type".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            1, // pp = 66
            w, width, dst_reg, 0, // no vvvv source
            src_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src_reg);

        Ok(())
    }

    fn lower_vpconflict(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        elem: VecElementType,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VPCONFLICT requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31 || src_reg > 31 || (mask.is_some() && !(1..=7).contains(&mask_reg)) {
            return Err(LowerError::InvalidRegister(
                "VPCONFLICT vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }
        let w = match elem {
            VecElementType::I32 => false,
            VecElementType::I64 => true,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VPCONFLICT requires I32 or I64 elements".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            1, // pp = 66
            w, width, dst_reg, 0, src_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(0xC4);
        enc.emit_modrm_rr(dst_reg, src_reg);
        Ok(())
    }

    fn lower_vplzcnt(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        elem: VecElementType,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VPLZCNT requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31 || src_reg > 31 || (mask.is_some() && !(1..=7).contains(&mask_reg)) {
            return Err(LowerError::InvalidRegister(
                "VPLZCNT vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }
        let w = match elem {
            VecElementType::I32 => false,
            VecElementType::I64 => true,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VPLZCNT requires I32 or I64 elements".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            1, // pp = 66
            w, width, dst_reg, 0, src_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(0x44);
        enc.emit_modrm_rr(dst_reg, src_reg);
        Ok(())
    }

    // ========================================================================
    // VBMI Instructions
    // ========================================================================

    fn lower_vpermute(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &Option<VReg>,
        indices: &VReg,
        _elem: VecElementType,
        width: VecWidth,
        overwrite_table: bool,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let indices_reg = self.vreg_to_zmm(indices)?;

        let opcode = match (src2.is_some(), overwrite_table) {
            (false, _) => 0x8D,    // VPERMB
            (true, false) => 0x75, // VPERMI2B
            (true, true) => 0x7D,  // VPERMT2B
        };

        let src2_reg = if let Some(s2) = src2 {
            self.vreg_to_zmm(s2)?
        } else {
            indices_reg
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,     // map 0F38
            1,     // pp = 66
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, 0, false,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    fn lower_x86_permute_bytes_words(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        table1: &VReg,
        table2: Option<&VReg>,
        indices: &VReg,
        mask: Option<&VReg>,
        elem: VecElementType,
        width: VecWidth,
        overwrite_table: bool,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VPERM B/W requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let table1_reg = self.vreg_to_zmm(table1)?;
        let indices_reg = self.vreg_to_zmm(indices)?;
        let table2_reg = table2.map(|reg| self.vreg_to_zmm(reg)).transpose()?;
        let mask_reg = mask.map_or(Ok(0), |reg| self.vreg_to_k(reg))?;
        if [
            Some(dst_reg),
            Some(table1_reg),
            Some(indices_reg),
            table2_reg,
        ]
        .into_iter()
        .flatten()
        .any(|reg| reg > 31)
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "VPERM B/W vector registers must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }
        let w = match elem {
            VecElementType::I8 => false,
            VecElementType::I16 => true,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VPERM B/W requires I8 or I16 elements".to_string(),
                ));
            }
        };
        let (opcode, vvvv, rm) = match table2_reg {
            None => (0x8D, indices_reg, table1_reg),
            Some(second) if !overwrite_table && dst == indices => (0x75, table1_reg, second),
            Some(second) if overwrite_table && dst == table1 => (0x7D, indices_reg, second),
            Some(_) => {
                return Err(LowerError::UnsupportedOperation(
                    "VPERMI2B/W requires dst == indices and VPERMT2B/W requires dst == table1"
                        .to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            1, // pp = 66
            w, width, dst_reg, vvvv, rm, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, rm);
        Ok(())
    }

    // ========================================================================
    // BITALG Instructions
    // ========================================================================

    fn lower_vpshufbitqmb(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        indices: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_k(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let indices_reg = self.vreg_to_zmm(indices)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,     // map 0F38
            1,     // pp = 66
            false, // W = 0
            width,
            dst_reg,
            src_reg,
            indices_reg,
            mask_reg,
            false,
        );
        enc.emit_opcode(0x8F);
        enc.emit_modrm_rr(dst_reg, indices_reg);

        Ok(())
    }

    fn lower_compress_expand(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        elem: VecElementType,
        width: VecWidth,
        zeroing: bool,
        compress: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "compress/expand requires 128/256/512-bit width and zeroing requires an opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |reg| self.vreg_to_k(reg))?;
        if dst_reg > 31 || src_reg > 31 || (mask.is_some() && !(1..=7).contains(&mask_reg)) {
            return Err(LowerError::InvalidRegister(
                "compress/expand vector registers must be 0..31 and explicit opmask K1..K7"
                    .to_string(),
            ));
        }
        let (opcode, w) = match (compress, elem) {
            (true, VecElementType::I8) => (0x63, false),
            (true, VecElementType::I16) => (0x63, true),
            (true, VecElementType::I32) => (0x8B, false),
            (true, VecElementType::I64) => (0x8B, true),
            (true, VecElementType::F32) => (0x8A, false),
            (true, VecElementType::F64) => (0x8A, true),
            (false, VecElementType::I8) => (0x62, false),
            (false, VecElementType::I16) => (0x62, true),
            (false, VecElementType::I32) => (0x89, false),
            (false, VecElementType::I64) => (0x89, true),
            (false, VecElementType::F32) => (0x88, false),
            (false, VecElementType::F64) => (0x88, true),
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "native compress/expand requires I8/I16/I32/I64/F32/F64 elements".to_string(),
                ));
            }
        };
        let (evex_dst, rm) = if compress {
            (src_reg, dst_reg)
        } else {
            (dst_reg, src_reg)
        };
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(2, 1, w, width, evex_dst, 0, rm, mask_reg, zeroing);
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(evex_dst, rm);
        Ok(())
    }

    fn lower_x86_narrow_int(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        src_elem: VecElementType,
        dst_elem: VecElementType,
        width: VecWidth,
        mode: X86NarrowMode,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "EVEX integer narrowing requires 128/256/512-bit source width and zeroing requires an opmask"
                    .to_string(),
            ));
        }
        let ratio = match (src_elem, dst_elem) {
            (VecElementType::I16, VecElementType::I8) => 0,
            (VecElementType::I32, VecElementType::I8) => 1,
            (VecElementType::I64, VecElementType::I8) => 2,
            (VecElementType::I32, VecElementType::I16) => 3,
            (VecElementType::I64, VecElementType::I16) => 4,
            (VecElementType::I64, VecElementType::I32) => 5,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "EVEX integer narrowing requires I16/I32/I64 to a smaller I8/I16/I32 element"
                        .to_string(),
                ));
            }
        };
        let opcode = match mode {
            X86NarrowMode::UnsignedSaturate => 0x10 | ratio,
            X86NarrowMode::SignedSaturate => 0x20 | ratio,
            X86NarrowMode::Truncate => 0x30 | ratio,
        };
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |reg| self.vreg_to_k(reg))?;
        if dst_reg > 31 || src_reg > 31 || (mask.is_some() && !(1..=7).contains(&mask_reg)) {
            return Err(LowerError::InvalidRegister(
                "EVEX integer narrowing vector registers must be 0..31 and explicit opmask K1..K7"
                    .to_string(),
            ));
        }
        let output_bytes = width.lanes(src_elem) * dst_elem.bytes();
        let expected_dst = if output_bytes <= 16 {
            VecWidth::V128
        } else {
            VecWidth::V256
        };
        let actual_dst = match dst {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))) => VecWidth::V128,
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(_))) => VecWidth::V256,
            VReg::Arch(ArchReg::X86(X86Reg::Zmm(_))) => VecWidth::V512,
            _ => unreachable!("vreg_to_zmm accepted non-vector destination"),
        };
        if actual_dst != expected_dst {
            return Err(LowerError::UnsupportedOperation(format!(
                "EVEX integer narrowing requires {expected_dst:?} destination for {width:?} {src_elem:?}->{dst_elem:?}"
            )));
        }
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(2, 2, false, width, src_reg, 0, dst_reg, mask_reg, zeroing);
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(src_reg, dst_reg);
        Ok(())
    }

    fn lower_x86_aes(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: Option<&VReg>,
        width: VecWidth,
        op: X86AesOp,
        imm: u8,
    ) -> Avx10LowerResult<()> {
        let vector_reg = |reg: &VReg| match (reg, width) {
            (VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))), VecWidth::V128)
            | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))), VecWidth::V256)
            | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(n))), VecWidth::V512)
                if *n <= 31 =>
            {
                Ok(*n)
            }
            _ => Err(LowerError::InvalidRegister(format!(
                "X86Aes requires an architectural vector register matching {width:?}: {reg:?}"
            ))),
        };
        let dst_reg = vector_reg(dst)?;
        let src1_reg = vector_reg(src1)?;

        match op {
            X86AesOp::Enc | X86AesOp::EncLast | X86AesOp::Dec | X86AesOp::DecLast => {
                if imm != 0 {
                    return Err(LowerError::UnsupportedOperation(
                        "AES round operations require an unused zero immediate field".to_string(),
                    ));
                }
                let src2 = src2.ok_or_else(|| {
                    LowerError::UnsupportedOperation(
                        "AES round operations require a round-key source".to_string(),
                    )
                })?;
                let src2_reg = vector_reg(src2)?;
                let opcode = match op {
                    X86AesOp::Enc => 0xDC,
                    X86AesOp::EncLast => 0xDD,
                    X86AesOp::Dec => 0xDE,
                    X86AesOp::DecLast => 0xDF,
                    _ => unreachable!(),
                };
                if width != VecWidth::V512 && dst_reg <= 15 && src1_reg <= 15 && src2_reg <= 15 {
                    Self::emit_vex_rr(code, 2, 1, width, dst_reg, src1_reg, src2_reg, opcode, None);
                } else {
                    let mut enc = EvexEncoder::new(code);
                    enc.emit_evex(2, 1, false, width, dst_reg, src1_reg, src2_reg, 0, false);
                    enc.emit_opcode(opcode);
                    enc.emit_modrm_rr(dst_reg, src2_reg);
                }
            }
            X86AesOp::InvMixColumns | X86AesOp::KeygenAssist => {
                if width != VecWidth::V128 || dst_reg > 15 || src1_reg > 15 || src2.is_some() {
                    return Err(LowerError::UnsupportedOperation(
                        "VAESIMC and VAESKEYGENASSIST require XMM0..XMM15, 128-bit width, and no second source"
                            .to_string(),
                    ));
                }
                if op == X86AesOp::InvMixColumns && imm != 0 {
                    return Err(LowerError::UnsupportedOperation(
                        "VAESIMC requires an unused zero immediate field".to_string(),
                    ));
                }
                let (map, opcode, immediate) = match op {
                    X86AesOp::InvMixColumns => (2, 0xDB, None),
                    X86AesOp::KeygenAssist => (3, 0xDF, Some(imm)),
                    _ => unreachable!(),
                };
                Self::emit_vex_rr(code, map, 1, width, dst_reg, 0, src1_reg, opcode, immediate);
            }
        }
        Ok(())
    }

    fn lower_x86_sha512(
        &self,
        code: &mut CodeBuffer,
        opcode: u8,
        dst: &VReg,
        state: Option<&VReg>,
        source: &VReg,
    ) -> Avx10LowerResult<()> {
        let ymm = |reg: &VReg| match reg {
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))) if *n <= 15 => Ok(*n),
            _ => Err(LowerError::InvalidRegister(format!(
                "SHA-512 requires an architectural YMM0..YMM15 operand: {reg:?}"
            ))),
        };
        let xmm = |reg: &VReg| match reg {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))) if *n <= 15 => Ok(*n),
            _ => Err(LowerError::InvalidRegister(format!(
                "SHA-512 requires an architectural XMM0..XMM15 operand: {reg:?}"
            ))),
        };
        let dst_reg = ymm(dst)?;
        let (state_reg, source_reg) = match opcode {
            0xCC => {
                if state.is_some() {
                    return Err(LowerError::UnsupportedOperation(
                        "VSHA512MSG1 has no VEX.vvvv source".to_string(),
                    ));
                }
                (0, xmm(source)?)
            }
            0xCD => {
                if state.is_some() {
                    return Err(LowerError::UnsupportedOperation(
                        "VSHA512MSG2 has no VEX.vvvv source".to_string(),
                    ));
                }
                (0, ymm(source)?)
            }
            0xCB => {
                let state = state.ok_or_else(|| {
                    LowerError::UnsupportedOperation(
                        "VSHA512RNDS2 requires a YMM state source".to_string(),
                    )
                })?;
                (ymm(state)?, xmm(source)?)
            }
            _ => {
                return Err(LowerError::UnsupportedOperation(format!(
                    "unknown SHA-512 opcode {opcode:#04x}"
                )));
            }
        };
        Self::emit_vex_rr(
            code,
            2,
            3,
            VecWidth::V256,
            dst_reg,
            state_reg,
            source_reg,
            opcode,
            None,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_x86_sm3(
        &self,
        code: &mut CodeBuffer,
        map: u8,
        pp: u8,
        opcode: u8,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        immediate: Option<u8>,
    ) -> Avx10LowerResult<()> {
        let xmm = |reg: &VReg| match reg {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))) if *n <= 15 => Ok(*n),
            _ => Err(LowerError::InvalidRegister(format!(
                "SM3 requires an architectural XMM0..XMM15 operand: {reg:?}"
            ))),
        };
        let dst_reg = xmm(dst)?;
        let src1_reg = xmm(src1)?;
        let src2_reg = xmm(src2)?;
        Self::emit_vex_rr(
            code,
            map,
            pp,
            VecWidth::V128,
            dst_reg,
            src1_reg,
            src2_reg,
            opcode,
            immediate,
        );
        Ok(())
    }

    fn lower_x86_sm4(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        width: VecWidth,
        key_schedule: bool,
    ) -> Avx10LowerResult<()> {
        let vector = |reg: &VReg| match (reg, width) {
            (VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))), VecWidth::V128)
            | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))), VecWidth::V256)
                if *n <= 15 =>
            {
                Ok(*n)
            }
            _ => Err(LowerError::InvalidRegister(format!(
                "SM4 requires XMM0..XMM15 or YMM0..YMM15 matching {width:?}: {reg:?}"
            ))),
        };
        let dst_reg = vector(dst)?;
        let src1_reg = vector(src1)?;
        let src2_reg = vector(src2)?;
        Self::emit_vex_rr(
            code,
            2,
            if key_schedule { 2 } else { 3 },
            width,
            dst_reg,
            src1_reg,
            src2_reg,
            0xDA,
            None,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_x86_packed_shift_imm(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        width: VecWidth,
        elem: VecElementType,
        shift: ShiftOp,
        amount: u8,
        byte_lane: bool,
    ) -> Avx10LowerResult<()> {
        let (opcode, group, w, evex_only) = if byte_lane {
            match (elem, shift) {
                (VecElementType::I8, ShiftOp::Lsr) => (0x73, 3, false, false),
                (VecElementType::I8, ShiftOp::Lsl) => (0x73, 7, false, false),
                _ => {
                    return Err(LowerError::UnsupportedOperation(
                        "packed byte-lane immediate shifts require I8 LSL or LSR".to_string(),
                    ));
                }
            }
        } else {
            match (elem, shift) {
                (VecElementType::I16, ShiftOp::Lsr) => (0x71, 2, false, false),
                (VecElementType::I16, ShiftOp::Asr) => (0x71, 4, false, false),
                (VecElementType::I16, ShiftOp::Lsl) => (0x71, 6, false, false),
                (VecElementType::I32, ShiftOp::Lsr) => (0x72, 2, false, false),
                (VecElementType::I32, ShiftOp::Asr) => (0x72, 4, false, false),
                (VecElementType::I32, ShiftOp::Lsl) => (0x72, 6, false, false),
                (VecElementType::I64, ShiftOp::Lsr) => (0x73, 2, true, false),
                (VecElementType::I64, ShiftOp::Asr) => (0x72, 4, true, true),
                (VecElementType::I64, ShiftOp::Lsl) => (0x73, 6, true, false),
                _ => {
                    return Err(LowerError::UnsupportedOperation(
                        "packed immediate shifts require I16/I32/I64 LSL/LSR or I16/I32/I64 ASR"
                            .to_string(),
                    ));
                }
            }
        };
        let vector = |reg: &VReg| match (reg, width) {
            (VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))), VecWidth::V128)
            | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))), VecWidth::V256)
            | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(n))), VecWidth::V512)
                if *n <= 31 =>
            {
                Ok(*n)
            }
            _ => Err(LowerError::InvalidRegister(format!(
                "packed immediate shift requires a vector register matching {width:?}: {reg:?}"
            ))),
        };
        let dst_reg = vector(dst)?;
        let src_reg = vector(src)?;
        let evex = width == VecWidth::V512 || dst_reg > 15 || src_reg > 15 || evex_only;
        if evex {
            let mut enc = EvexEncoder::new(code);
            enc.emit_evex(1, 1, w, width, group, dst_reg, src_reg, 0, false);
            enc.emit_opcode(opcode);
            enc.emit_modrm_rr(group, src_reg);
            enc.emit_imm8(amount);
        } else {
            Self::emit_vex_rr(
                code,
                1,
                1,
                width,
                group,
                dst_reg,
                src_reg,
                opcode,
                Some(amount),
            );
        }
        Ok(())
    }

    fn lower_x86_packed_shift(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        count: &VReg,
        width: VecWidth,
        elem: VecElementType,
        shift: ShiftOp,
    ) -> Avx10LowerResult<()> {
        let (opcode, w, evex_only) = match (elem, shift) {
            (VecElementType::I16, ShiftOp::Lsr) => (0xD1, false, false),
            (VecElementType::I16, ShiftOp::Asr) => (0xE1, false, false),
            (VecElementType::I16, ShiftOp::Lsl) => (0xF1, false, false),
            (VecElementType::I32, ShiftOp::Lsr) => (0xD2, false, false),
            (VecElementType::I32, ShiftOp::Asr) => (0xE2, false, false),
            (VecElementType::I32, ShiftOp::Lsl) => (0xF2, false, false),
            (VecElementType::I64, ShiftOp::Lsr) => (0xD3, true, false),
            (VecElementType::I64, ShiftOp::Asr) => (0xE2, true, true),
            (VecElementType::I64, ShiftOp::Lsl) => (0xF3, true, false),
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "packed shared-count shifts require I16/I32/I64 LSL/LSR/ASR".to_string(),
                ));
            }
        };
        let vector = |reg: &VReg| match (reg, width) {
            (VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))), VecWidth::V128)
            | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))), VecWidth::V256)
            | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(n))), VecWidth::V512)
                if *n <= 31 =>
            {
                Ok(*n)
            }
            _ => Err(LowerError::InvalidRegister(format!(
                "packed shared-count shift vector must match {width:?}: {reg:?}"
            ))),
        };
        let count_reg = match count {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))) if *n <= 31 => *n,
            _ => {
                return Err(LowerError::InvalidRegister(format!(
                    "packed shared-count shift requires XMM0..XMM31 count: {count:?}"
                )));
            }
        };
        let dst_reg = vector(dst)?;
        let src_reg = vector(src)?;
        let evex =
            width == VecWidth::V512 || dst_reg > 15 || src_reg > 15 || count_reg > 15 || evex_only;
        if evex {
            let mut enc = EvexEncoder::new(code);
            enc.emit_evex(1, 1, w, width, dst_reg, src_reg, count_reg, 0, false);
            enc.emit_opcode(opcode);
            enc.emit_modrm_rr(dst_reg, count_reg);
        } else {
            Self::emit_vex_rr(code, 1, 1, width, dst_reg, src_reg, count_reg, opcode, None);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_vex_rr(
        code: &mut CodeBuffer,
        map: u8,
        pp: u8,
        width: VecWidth,
        dst: u8,
        src1: u8,
        src2: u8,
        opcode: u8,
        immediate: Option<u8>,
    ) {
        debug_assert!(matches!(width, VecWidth::V128 | VecWidth::V256));
        debug_assert!(dst <= 15 && src1 <= 15 && src2 <= 15);
        let r_inv = ((dst >> 3) & 1) ^ 1;
        let b_inv = ((src2 >> 3) & 1) ^ 1;
        let l = u8::from(width == VecWidth::V256);
        if map == 1 && src2 < 8 {
            code.emit_u8(0xC5);
            code.emit_u8((r_inv << 7) | ((!src1 & 0x0F) << 3) | (l << 2) | (pp & 3));
        } else {
            code.emit_u8(0xC4);
            code.emit_u8((r_inv << 7) | (1 << 6) | (b_inv << 5) | map);
            code.emit_u8(((!src1 & 0x0F) << 3) | (l << 2) | (pp & 3));
        }
        code.emit_u8(opcode);
        code.emit_u8(0xC0 | ((dst & 7) << 3) | (src2 & 7));
        if let Some(immediate) = immediate {
            code.emit_u8(immediate);
        }
    }

    // ========================================================================
    // BF16 Instructions
    // ========================================================================

    fn lower_vdpbf16ps(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        acc: &VReg,
        src1: &VReg,
        src2: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if dst != acc {
            return Err(LowerError::UnsupportedOperation(
                "VDPBF16PS requires acc aliased with dst".to_string(),
            ));
        }
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VDPBF16PS requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31
            || src1_reg > 31
            || src2_reg > 31
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "VDPBF16PS vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,     // map 0F38
            2,     // pp = F3
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(0x52);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    fn lower_vcvtfp32tobf16(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: Option<&VReg>,
        mask: Option<&VReg>,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "VCVTNEPS2BF16/VCVTNE2PS2BF16 requires 128/256/512-bit input width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;

        let (pp, vvvv_reg, src2_reg) = if let Some(s2) = src2 {
            // VCVTNE2PS2BF16
            (3, src1_reg, self.vreg_to_zmm(s2)?) // F2
        } else {
            // VCVTNEPS2BF16 has reserved EVEX.vvvv = 0 after decoding.
            (2, 0, src1_reg) // F3
        };
        if dst_reg > 31
            || src1_reg > 31
            || src2_reg > 31
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "VCVTNEPS2BF16/VCVTNE2PS2BF16 vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            pp, false, // W = 0
            width, dst_reg, vvvv_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(0x72);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // FP16 Instructions
    // ========================================================================

    fn lower_vfp16_arith(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        mask: Option<&VReg>,
        op: Avx10FP16Op,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "packed FP16 arithmetic requires 128/256/512-bit width and zeroing requires a nonzero opmask"
                    .to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if dst_reg > 31
            || src1_reg > 31
            || src2_reg > 31
            || (mask.is_some() && !(1..=7).contains(&mask_reg))
        {
            return Err(LowerError::InvalidRegister(
                "packed FP16 vector register must be 0..31 and explicit opmask must be K1..K7"
                    .to_string(),
            ));
        }

        let opcode = match op {
            Avx10FP16Op::Add => 0x58,
            Avx10FP16Op::Mul => 0x59,
            Avx10FP16Op::Sub => 0x5C,
            Avx10FP16Op::Min => 0x5D,
            Avx10FP16Op::Div => 0x5E,
            Avx10FP16Op::Max => 0x5F,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "FP16: unsupported op".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            5,     // MAP5
            0,     // pp = none
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // AVX10.2 VMINMAX
    // ========================================================================

    fn lower_vminmax(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        elem: VecElementType,
        width: VecWidth,
        imm: u8,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;

        let (pp, w) = match elem {
            VecElementType::F32 => (0, false), // VMINMAXPS
            VecElementType::F64 => (1, true),  // VMINMAXPD
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "VMINMAX: invalid element type".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            3, // map 0F3A
            pp, w, width, dst_reg, src1_reg, src2_reg, 0, false,
        );
        enc.emit_opcode(0x52);
        enc.emit_modrm_rr(dst_reg, src2_reg);
        enc.emit_imm8(imm);

        Ok(())
    }

    fn lower_packed_shift_variable(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        count: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
        elem: VecElementType,
        shift: ShiftOp,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let count_reg = self.vreg_to_zmm(count)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        let (opcode, w) = match (elem, shift) {
            (VecElementType::I16, ShiftOp::Lsr) => (0x10, true),
            (VecElementType::I16, ShiftOp::Asr) => (0x11, true),
            (VecElementType::I16, ShiftOp::Lsl) => (0x12, true),
            (VecElementType::I32 | VecElementType::I64, ShiftOp::Lsr) => {
                (0x45, elem == VecElementType::I64)
            }
            (VecElementType::I32 | VecElementType::I64, ShiftOp::Asr) => {
                (0x46, elem == VecElementType::I64)
            }
            (VecElementType::I32 | VecElementType::I64, ShiftOp::Lsl) => {
                (0x47, elem == VecElementType::I64)
            }
            _ => {
                return Err(LowerError::UnsupportedOperation(format!(
                    "packed variable shift {elem:?} {shift:?}"
                )));
            }
        };
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, 1, w, width, dst_reg, src_reg, count_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, count_reg);
        Ok(())
    }

    fn lower_packed_rotate(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        count: Option<&VReg>,
        mask: Option<&VReg>,
        amount: u8,
        width: VecWidth,
        elem: VecElementType,
        left: bool,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if !matches!(elem, VecElementType::I32 | VecElementType::I64) {
            return Err(LowerError::UnsupportedOperation(format!(
                "packed rotate {elem:?}"
            )));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        let mut enc = EvexEncoder::new(code);
        if let Some(count) = count {
            let count_reg = self.vreg_to_zmm(count)?;
            enc.emit_evex(
                2,
                1,
                elem == VecElementType::I64,
                width,
                dst_reg,
                src_reg,
                count_reg,
                mask_reg,
                zeroing,
            );
            enc.emit_opcode(if left { 0x15 } else { 0x14 });
            enc.emit_modrm_rr(dst_reg, count_reg);
        } else {
            // VPROL[DQ]/VPROR[DQ] immediate use 0F.72 /1 and /0. The
            // destination is encoded in EVEX.vvvv/V', while ModRM.reg is the
            // opcode extension and ModRM.r/m is the source.
            enc.emit_evex(
                1,
                1,
                elem == VecElementType::I64,
                width,
                0,
                dst_reg,
                src_reg,
                mask_reg,
                zeroing,
            );
            enc.emit_opcode(0x72);
            enc.emit_modrm_rr(u8::from(left), src_reg);
            enc.emit_imm8(amount);
        }
        Ok(())
    }

    fn lower_ternary_logic(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        src3: &VReg,
        mask: Option<&VReg>,
        imm: u8,
        width: VecWidth,
        elem: VecElementType,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if dst != src1 {
            return Err(LowerError::UnsupportedOperation(
                "VPTERNLOG requires src1 aliased with dst".to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let src3_reg = self.vreg_to_zmm(src3)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if !matches!(elem, VecElementType::I32 | VecElementType::I64) {
            return Err(LowerError::UnsupportedOperation(format!(
                "VPTERNLOG element {elem:?}"
            )));
        }
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            3,
            1,
            elem == VecElementType::I64,
            width,
            dst_reg,
            src2_reg,
            src3_reg,
            mask_reg,
            zeroing,
        );
        enc.emit_opcode(0x25);
        enc.emit_modrm_rr(dst_reg, src3_reg);
        enc.emit_imm8(imm);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_packed_funnel_shift(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        fill: &VReg,
        count: Option<&VReg>,
        mask: Option<&VReg>,
        amount: u8,
        width: VecWidth,
        elem: VecElementType,
        left: bool,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        let (opcode, w) = match (left, elem) {
            (true, VecElementType::I16) => (0x70, true),
            (true, VecElementType::I32) => (0x71, false),
            (true, VecElementType::I64) => (0x71, true),
            (false, VecElementType::I16) => (0x72, true),
            (false, VecElementType::I32) => (0x73, false),
            (false, VecElementType::I64) => (0x73, true),
            _ => {
                return Err(LowerError::UnsupportedOperation(format!(
                    "packed funnel shift {elem:?}"
                )));
            }
        };
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let fill_reg = self.vreg_to_zmm(fill)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        let (map, vvvv, rm, immediate) = if let Some(count) = count {
            if dst != src {
                return Err(LowerError::UnsupportedOperation(
                    "variable funnel shift requires src aliased with dst".to_string(),
                ));
            }
            (2, fill_reg, self.vreg_to_zmm(count)?, None)
        } else {
            (3, src_reg, fill_reg, Some(amount))
        };
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(map, 1, w, width, dst_reg, vvvv, rm, mask_reg, zeroing);
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, rm);
        if let Some(imm) = immediate {
            enc.emit_imm8(imm);
        }
        Ok(())
    }

    fn lower_multishift_qb(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        control: &VReg,
        source: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let control_reg = self.vreg_to_zmm(control)?;
        let source_reg = self.vreg_to_zmm(source)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2,
            1,
            true,
            width,
            dst_reg,
            control_reg,
            source_reg,
            mask_reg,
            zeroing,
        );
        enc.emit_opcode(0x83);
        enc.emit_modrm_rr(dst_reg, source_reg);
        Ok(())
    }

    // ========================================================================
    // AVX10.2 VMPSADBW
    // ========================================================================

    fn lower_vmpsadbw(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src1: &VReg,
        src2: &VReg,
        mask: Option<&VReg>,
        width: VecWidth,
        imm: u8,
        zeroing: bool,
    ) -> Avx10LowerResult<()> {
        if width == VecWidth::V64 || (zeroing && mask.is_none()) {
            return Err(LowerError::UnsupportedOperation(
                "AVX10.2 VMPSADBW requires VL=128/256/512 and zeroing requires K1..K7".to_string(),
            ));
        }
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return Err(LowerError::InvalidRegister(
                "AVX10.2 VMPSADBW vector registers must match VL and be numbered 0..31".to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;
        let mask_reg = mask.map_or(Ok(0), |mask| self.vreg_to_k(mask))?;
        if mask.is_some() && !(1..=7).contains(&mask_reg) {
            return Err(LowerError::InvalidRegister(
                "AVX10.2 VMPSADBW explicit opmask must be K1..K7".to_string(),
            ));
        }

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            3,     // map 0F3A
            2,     // pp = F3
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, mask_reg, zeroing,
        );
        enc.emit_opcode(0x42);
        enc.emit_modrm_rr(dst_reg, src2_reg);
        enc.emit_imm8(imm);

        Ok(())
    }

    // ========================================================================
    // AVX10.2 Media Acceleration
    // ========================================================================

    fn lower_vdotproduct_ext(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        acc: &VReg,
        src1: &VReg,
        src2: &VReg,
        src_elem: VecElementType,
        acc_elem: VecElementType,
        width: VecWidth,
        src1_signed: bool,
        src2_signed: bool,
        saturate: bool,
    ) -> Avx10LowerResult<()> {
        if dst != acc || acc_elem != VecElementType::I32 {
            return Err(LowerError::UnsupportedOperation(
                "extended VNNI requires an I32 accumulator aliased with dst".to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;

        // Determine pp and W based on signedness
        let (pp, w) = match (src_elem, src1_signed, src2_signed) {
            // Byte variants
            (VecElementType::I8, true, true) => (2, false), // VPDPBSSD F3.W0
            (VecElementType::I8, true, false) => (2, true), // VPDPBSUD F3.W1
            (VecElementType::I8, false, false) => (0, true), // VPDPBUUD NP.W1
            // Word variants
            (VecElementType::I16, true, false) => (2, false), // VPDPWSUD F3.W0
            (VecElementType::I16, false, true) => (1, false), // VPDPWUSD 66.W0
            (VecElementType::I16, false, false) => (0, false), // VPDPWUUD NP.W0
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Media accel: invalid types".to_string(),
                ));
            }
        };

        let opcode = match src_elem {
            VecElementType::I8 => {
                if saturate {
                    0x51
                } else {
                    0x50
                }
            }
            VecElementType::I16 => {
                if saturate {
                    0xD3
                } else {
                    0xD2
                }
            }
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Media accel: invalid element".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            pp, w, width, dst_reg, src1_reg, src2_reg, 0, false,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn vreg_to_zmm(&self, vreg: &VReg) -> Avx10LowerResult<u8> {
        match vreg {
            VReg::Arch(ArchReg::X86(X86Reg::Zmm(n))) => Ok(*n),
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(n))) => Ok(*n),
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(n))) => Ok(*n),
            _ => Err(LowerError::InvalidRegister(format!("{:?}", vreg))),
        }
    }

    fn vreg_to_k(&self, vreg: &VReg) -> Avx10LowerResult<u8> {
        match vreg {
            VReg::Arch(ArchReg::X86(X86Reg::K(n))) => Ok(*n),
            _ => Err(LowerError::InvalidRegister(format!("{:?}", vreg))),
        }
    }

    fn vreg_to_gpr(&self, vreg: &VReg) -> Avx10LowerResult<u8> {
        match vreg {
            VReg::Arch(ArchReg::X86(reg)) => reg
                .gpr_index()
                .filter(|index| *index < 32)
                .ok_or_else(|| LowerError::InvalidRegister(format!("{vreg:?}"))),
            _ => Err(LowerError::InvalidRegister(format!("{vreg:?}"))),
        }
    }
}

impl Default for Avx10Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evex_encode_vpdpbusd() {
        let mut code = CodeBuffer::new();

        // VPDPBUSD zmm1, zmm2, zmm3 should be: 62 F2 6D 48 50 CB
        {
            let mut enc = EvexEncoder::new(&mut code);
            enc.emit_evex(
                2,     // map 0F38
                1,     // pp = 66
                false, // W = 0
                VecWidth::V512,
                1, // zmm1
                2, // zmm2
                3, // zmm3
                0,
                false,
            );
            enc.emit_opcode(0x50);
            enc.emit_modrm_rr(1, 3);
        }

        let bytes = code.as_slice();
        assert_eq!(bytes.len(), 6);
        assert_eq!(bytes[0], 0x62); // EVEX prefix
        assert_eq!(bytes[4], 0x50); // opcode
        assert_eq!(bytes[5], 0xCB); // ModR/M: 11 001 011
    }

    #[test]
    fn test_lower_vdotproduct() {
        let lowerer = Avx10Lowerer::new();
        let mut code = CodeBuffer::new();

        let op = OpKind::VDotProduct {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        };

        let result = lowerer.try_lower(&op, &mut code);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert!(code.len() > 0);
    }

    #[test]
    fn vdotproduct_lowering_rejects_non_vnni_accumulator_shapes() {
        let lowerer = Avx10Lowerer::new();
        for (acc, acc_elem) in [
            (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(4))),
                VecElementType::I32,
            ),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                VecElementType::I16,
            ),
        ] {
            let mut code = CodeBuffer::new();
            let op = OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc,
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem,
                width: VecWidth::V512,
                src1_unsigned: true,
                saturate: true,
                zeroing: false,
            };
            let result = lowerer.try_lower(&op, &mut code).unwrap();
            assert!(matches!(result, Err(LowerError::UnsupportedOperation(_))));
            assert_eq!(code.len(), 0);
        }

        let mut code = CodeBuffer::new();
        let invalid_signedness = OpKind::VDotProduct {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        };
        let result = lowerer.try_lower(&invalid_signedness, &mut code).unwrap();
        assert!(matches!(result, Err(LowerError::UnsupportedOperation(_))));
        assert_eq!(code.len(), 0);

        for invalid_mask_shape in [
            OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V512,
                src1_unsigned: true,
                saturate: false,
                zeroing: true,
            },
            OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V512,
                src1_unsigned: true,
                saturate: false,
                zeroing: false,
            },
            OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V64,
                src1_unsigned: true,
                saturate: false,
                zeroing: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid_mask_shape, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid_mask_shape:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn destructive_avx_accumulators_must_alias_destinations() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        let zmm4 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(4)));
        for op in [
            OpKind::VMultiplyAdd52 {
                dst: zmm1,
                acc: zmm4,
                src1: zmm2,
                src2: zmm3,
                mask: None,
                width: VecWidth::V512,
                high: false,
                zeroing: false,
            },
            OpKind::VDotProductBF16 {
                dst: zmm1,
                acc: zmm4,
                src1: zmm2,
                src2: zmm3,
                mask: None,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VDotProductExt {
                dst: zmm1,
                acc: zmm4,
                src1: zmm2,
                src2: zmm3,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V512,
                src1_signed: true,
                src2_signed: true,
                saturate: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let error = lowerer
                .try_lower(&op, &mut code)
                .expect("recognized destructive AVX op")
                .expect_err("non-aliased accumulator must be rejected");
            assert!(matches!(error, LowerError::UnsupportedOperation(_)));
            assert_eq!(code.len(), 0, "rejection emitted partial code for {op:?}");
        }
    }

    #[test]
    fn vpmadd52_lowering_rejects_malformed_mask_and_width_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        for invalid in [
            OpKind::VMultiplyAdd52 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: None,
                width: VecWidth::V512,
                high: false,
                zeroing: true,
            },
            OpKind::VMultiplyAdd52 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                width: VecWidth::V512,
                high: false,
                zeroing: false,
            },
            OpKind::VMultiplyAdd52 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask: None,
                width: VecWidth::V64,
                high: true,
                zeroing: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0, "rejection emitted partial code");
        }
    }

    #[test]
    fn vdpbf16ps_lowering_rejects_malformed_mask_and_width_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        for (mask, width, zeroing) in [
            (None, VecWidth::V512, true),
            (
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                VecWidth::V512,
                false,
            ),
            (None, VecWidth::V64, false),
        ] {
            let invalid = OpKind::VDotProductBF16 {
                dst: zmm1,
                acc: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask,
                width,
                zeroing,
            };
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn vcvtfp32tobf16_lowering_rejects_malformed_mask_and_width_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        for (mask, width, zeroing) in [
            (None, VecWidth::V512, true),
            (
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                VecWidth::V512,
                false,
            ),
            (None, VecWidth::V64, false),
        ] {
            let invalid = OpKind::VCvtFP32ToBF16 {
                dst: zmm1,
                src1: zmm2,
                src2: None,
                mask,
                width,
                zeroing,
            };
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn vfp16_lowering_rejects_malformed_mask_width_and_operation_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        for (mask, op, width, zeroing) in [
            (None, Avx10FP16Op::Add, VecWidth::V512, true),
            (
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                Avx10FP16Op::Mul,
                VecWidth::V512,
                false,
            ),
            (None, Avx10FP16Op::Div, VecWidth::V64, false),
            (None, Avx10FP16Op::Sqrt, VecWidth::V512, false),
        ] {
            let invalid = OpKind::VFP16Arith {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                mask,
                op,
                round: FpRoundMode::Dynamic,
                width,
                lanes: width.lanes(VecElementType::F16) as u8,
                zeroing,
            };
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }

        let scalar = OpKind::VFP16Arith {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            mask: None,
            op: Avx10FP16Op::Div,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 1,
            zeroing: false,
        };
        let mut code = CodeBuffer::new();
        assert!(lowerer.try_lower(&scalar, &mut code).unwrap().is_err());
        assert_eq!(code.len(), 0);
    }

    #[test]
    fn vpconflict_lowering_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        for (mask, elem, width, zeroing) in [
            (None, VecElementType::I32, VecWidth::V512, true),
            (
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                VecElementType::I32,
                VecWidth::V512,
                false,
            ),
            (None, VecElementType::I16, VecWidth::V512, false),
            (None, VecElementType::I64, VecWidth::V64, false),
        ] {
            let invalid = OpKind::VConflict {
                dst: zmm1,
                src: zmm2,
                mask,
                elem,
                width,
                zeroing,
            };
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn vplzcnt_lowering_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        for (mask, elem, width, zeroing) in [
            (None, VecElementType::I32, VecWidth::V512, true),
            (
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                VecElementType::I32,
                VecWidth::V512,
                false,
            ),
            (None, VecElementType::I16, VecWidth::V512, false),
            (None, VecElementType::I64, VecWidth::V64, false),
        ] {
            let invalid = OpKind::VLeadingZeros {
                dst: zmm1,
                src: zmm2,
                mask,
                elem,
                width,
                zeroing,
            };
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }

        let invalid_register = OpKind::VLeadingZeros {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(32))),
            src: zmm2,
            mask: None,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: false,
        };
        let mut code = CodeBuffer::new();
        let result = lowerer.try_lower(&invalid_register, &mut code).unwrap();
        assert!(result.is_err(), "accepted malformed {invalid_register:?}");
        assert_eq!(code.len(), 0);
    }

    #[test]
    fn x86_permute_bytes_words_lowering_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        for invalid in [
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: true,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: None,
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V64,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: Some(zmm3),
                indices: zmm2,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: false,
                zeroing: false,
            },
            OpKind::X86PermuteBytesWords {
                dst: zmm1,
                table1: zmm2,
                table2: Some(zmm3),
                indices: zmm3,
                mask: None,
                elem: VecElementType::I8,
                width: VecWidth::V512,
                overwrite_table: true,
                zeroing: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn compress_expand_lowering_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        for invalid in [
            OpKind::VCompress {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            },
            OpKind::VExpand {
                dst: zmm1,
                src: zmm2,
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                elem: VecElementType::I64,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VCompress {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::F16,
                width: VecWidth::V512,
                zeroing: false,
            },
            OpKind::VExpand {
                dst: zmm1,
                src: zmm2,
                mask: None,
                elem: VecElementType::F32,
                width: VecWidth::V64,
                zeroing: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_integer_narrow_lowering_covers_all_modes_ratios_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
        for (mode, high) in [
            (X86NarrowMode::UnsignedSaturate, 0x10u8),
            (X86NarrowMode::SignedSaturate, 0x20),
            (X86NarrowMode::Truncate, 0x30),
        ] {
            for (low, src_elem, dst_elem, wide_dst) in [
                (0u8, VecElementType::I16, VecElementType::I8, true),
                (1, VecElementType::I32, VecElementType::I8, false),
                (2, VecElementType::I64, VecElementType::I8, false),
                (3, VecElementType::I32, VecElementType::I16, true),
                (4, VecElementType::I64, VecElementType::I16, false),
                (5, VecElementType::I64, VecElementType::I32, true),
            ] {
                let dst = VReg::Arch(ArchReg::X86(if wide_dst {
                    X86Reg::Ymm(1)
                } else {
                    X86Reg::Xmm(1)
                }));
                let op = OpKind::X86NarrowInt {
                    dst,
                    src: zmm2,
                    mask: Some(k4),
                    src_elem,
                    dst_elem,
                    width: VecWidth::V512,
                    mode,
                    zeroing: true,
                };
                let mut code = CodeBuffer::new();
                lowerer.try_lower(&op, &mut code).unwrap().unwrap();
                assert_eq!(code.as_slice(), &[0x62, 0xF2, 0x7E, 0xCC, high | low, 0xD1]);
            }
        }

        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        for invalid in [
            OpKind::X86NarrowInt {
                dst: xmm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I32,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: X86NarrowMode::Truncate,
                zeroing: true,
            },
            OpKind::X86NarrowInt {
                dst: xmm1,
                src: zmm2,
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(0)))),
                src_elem: VecElementType::I32,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: xmm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I32,
                width: VecWidth::V512,
                mode: X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: xmm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I64,
                dst_elem: VecElementType::I32,
                width: VecWidth::V64,
                mode: X86NarrowMode::Truncate,
                zeroing: false,
            },
            OpKind::X86NarrowInt {
                dst: xmm1,
                src: zmm2,
                mask: None,
                src_elem: VecElementType::I16,
                dst_elem: VecElementType::I8,
                width: VecWidth::V512,
                mode: X86NarrowMode::Truncate,
                zeroing: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_aes_lowering_covers_round_unary_keygen_vex_evex_and_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        let ymm = |n| VReg::Arch(ArchReg::X86(X86Reg::Ymm(n)));
        let zmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Zmm(n)));
        for (op, expected) in [
            (
                OpKind::X86Aes {
                    dst: zmm(16),
                    src1: zmm(17),
                    src2: Some(zmm(18)),
                    width: VecWidth::V512,
                    op: X86AesOp::Enc,
                    imm: 0,
                },
                &[0x62, 0xA2, 0x75, 0x40, 0xDC, 0xC2][..],
            ),
            (
                OpKind::X86Aes {
                    dst: zmm(1),
                    src1: zmm(2),
                    src2: Some(zmm(3)),
                    width: VecWidth::V512,
                    op: X86AesOp::EncLast,
                    imm: 0,
                },
                &[0x62, 0xF2, 0x6D, 0x48, 0xDD, 0xCB][..],
            ),
            (
                OpKind::X86Aes {
                    dst: ymm(4),
                    src1: ymm(5),
                    src2: Some(ymm(6)),
                    width: VecWidth::V256,
                    op: X86AesOp::Dec,
                    imm: 0,
                },
                &[0xC4, 0xE2, 0x55, 0xDE, 0xE6][..],
            ),
            (
                OpKind::X86Aes {
                    dst: xmm(7),
                    src1: xmm(8),
                    src2: Some(xmm(9)),
                    width: VecWidth::V128,
                    op: X86AesOp::DecLast,
                    imm: 0,
                },
                &[0xC4, 0xC2, 0x39, 0xDF, 0xF9][..],
            ),
            (
                OpKind::X86Aes {
                    dst: xmm(16),
                    src1: xmm(17),
                    src2: Some(xmm(18)),
                    width: VecWidth::V128,
                    op: X86AesOp::Enc,
                    imm: 0,
                },
                &[0x62, 0xA2, 0x75, 0x00, 0xDC, 0xC2][..],
            ),
            (
                OpKind::X86Aes {
                    dst: xmm(9),
                    src1: xmm(8),
                    src2: None,
                    width: VecWidth::V128,
                    op: X86AesOp::InvMixColumns,
                    imm: 0,
                },
                &[0xC4, 0x42, 0x79, 0xDB, 0xC8][..],
            ),
            (
                OpKind::X86Aes {
                    dst: xmm(11),
                    src1: xmm(10),
                    src2: None,
                    width: VecWidth::V128,
                    op: X86AesOp::KeygenAssist,
                    imm: 0x5A,
                },
                &[0xC4, 0x43, 0x79, 0xDF, 0xDA, 0x5A][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }

        for invalid in [
            OpKind::X86Aes {
                dst: zmm(1),
                src1: zmm(2),
                src2: None,
                width: VecWidth::V512,
                op: X86AesOp::Enc,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: zmm(1),
                src1: zmm(2),
                src2: Some(zmm(3)),
                width: VecWidth::V512,
                op: X86AesOp::Dec,
                imm: 1,
            },
            OpKind::X86Aes {
                dst: xmm(1),
                src1: xmm(2),
                src2: Some(xmm(3)),
                width: VecWidth::V128,
                op: X86AesOp::InvMixColumns,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: xmm(16),
                src1: xmm(2),
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: ymm(1),
                src1: ymm(2),
                src2: None,
                width: VecWidth::V256,
                op: X86AesOp::InvMixColumns,
                imm: 0,
            },
            OpKind::X86Aes {
                dst: xmm(1),
                src1: xmm(2),
                src2: Some(xmm(3)),
                width: VecWidth::V64,
                op: X86AesOp::EncLast,
                imm: 0,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_sha512_lowering_covers_all_mixed_width_forms_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        let ymm = |n| VReg::Arch(ArchReg::X86(X86Reg::Ymm(n)));
        for (op, expected) in [
            (
                OpKind::X86Sha512Msg1 {
                    dst: ymm(9),
                    src: xmm(10),
                },
                &[0xC4, 0x42, 0x7F, 0xCC, 0xCA][..],
            ),
            (
                OpKind::X86Sha512Msg2 {
                    dst: ymm(3),
                    src: ymm(4),
                },
                &[0xC4, 0xE2, 0x7F, 0xCD, 0xDC][..],
            ),
            (
                OpKind::X86Sha512Rounds2 {
                    dst: ymm(9),
                    state: ymm(11),
                    wk: xmm(10),
                },
                &[0xC4, 0x42, 0x27, 0xCB, 0xCA][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }

        for invalid in [
            OpKind::X86Sha512Msg1 {
                dst: xmm(1),
                src: xmm(2),
            },
            OpKind::X86Sha512Msg1 {
                dst: ymm(1),
                src: ymm(2),
            },
            OpKind::X86Sha512Msg2 {
                dst: ymm(1),
                src: xmm(2),
            },
            OpKind::X86Sha512Rounds2 {
                dst: ymm(1),
                state: xmm(2),
                wk: xmm(3),
            },
            OpKind::X86Sha512Rounds2 {
                dst: ymm(1),
                state: ymm(2),
                wk: ymm(3),
            },
            OpKind::X86Sha512Msg2 {
                dst: ymm(16),
                src: ymm(2),
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_sm3_lowering_covers_message_round_forms_and_rejects_malformed_registers() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        for (op, expected) in [
            (
                OpKind::X86Sm3Msg1 {
                    dst: xmm(9),
                    src1: xmm(11),
                    src2: xmm(10),
                },
                &[0xC4, 0x42, 0x20, 0xDA, 0xCA][..],
            ),
            (
                OpKind::X86Sm3Msg2 {
                    dst: xmm(9),
                    src1: xmm(11),
                    src2: xmm(10),
                },
                &[0xC4, 0x42, 0x21, 0xDA, 0xCA][..],
            ),
            (
                OpKind::X86Sm3Rounds2 {
                    dst: xmm(9),
                    state: xmm(11),
                    words: xmm(10),
                    imm: 0x3E,
                },
                &[0xC4, 0x43, 0x21, 0xDE, 0xCA, 0x3E][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }

        for invalid in [
            OpKind::X86Sm3Msg1 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                src1: xmm(2),
                src2: xmm(3),
            },
            OpKind::X86Sm3Msg2 {
                dst: xmm(1),
                src1: VReg::Virtual(VirtualId(7)),
                src2: xmm(3),
            },
            OpKind::X86Sm3Rounds2 {
                dst: xmm(1),
                state: xmm(2),
                words: xmm(16),
                imm: 0xFF,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_sm4_lowering_covers_operations_widths_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        let ymm = |n| VReg::Arch(ArchReg::X86(X86Reg::Ymm(n)));
        for (op, expected) in [
            (
                OpKind::X86Sm4 {
                    dst: xmm(1),
                    src1: xmm(2),
                    src2: xmm(3),
                    width: VecWidth::V128,
                    key_schedule: true,
                },
                &[0xC4, 0xE2, 0x6A, 0xDA, 0xCB][..],
            ),
            (
                OpKind::X86Sm4 {
                    dst: ymm(4),
                    src1: ymm(5),
                    src2: ymm(6),
                    width: VecWidth::V256,
                    key_schedule: true,
                },
                &[0xC4, 0xE2, 0x56, 0xDA, 0xE6][..],
            ),
            (
                OpKind::X86Sm4 {
                    dst: xmm(7),
                    src1: xmm(8),
                    src2: xmm(9),
                    width: VecWidth::V128,
                    key_schedule: false,
                },
                &[0xC4, 0xC2, 0x3B, 0xDA, 0xF9][..],
            ),
            (
                OpKind::X86Sm4 {
                    dst: ymm(10),
                    src1: ymm(11),
                    src2: ymm(12),
                    width: VecWidth::V256,
                    key_schedule: false,
                },
                &[0xC4, 0x42, 0x27, 0xDA, 0xD4][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }
        for invalid in [
            OpKind::X86Sm4 {
                dst: xmm(1),
                src1: ymm(2),
                src2: xmm(3),
                width: VecWidth::V128,
                key_schedule: false,
            },
            OpKind::X86Sm4 {
                dst: ymm(1),
                src1: ymm(2),
                src2: ymm(3),
                width: VecWidth::V512,
                key_schedule: true,
            },
            OpKind::X86Sm4 {
                dst: xmm(16),
                src1: xmm(2),
                src2: xmm(3),
                width: VecWidth::V128,
                key_schedule: true,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_packed_shift_imm_lowering_covers_all_groups_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let xmm2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
        for (elem, shift, amount, byte_lane, expected) in [
            (
                VecElementType::I16,
                ShiftOp::Lsr,
                1,
                false,
                &[0xC5, 0xF1, 0x71, 0xD2, 0x01][..],
            ),
            (
                VecElementType::I16,
                ShiftOp::Asr,
                2,
                false,
                &[0xC5, 0xF1, 0x71, 0xE2, 0x02][..],
            ),
            (
                VecElementType::I16,
                ShiftOp::Lsl,
                3,
                false,
                &[0xC5, 0xF1, 0x71, 0xF2, 0x03][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Lsr,
                4,
                false,
                &[0xC5, 0xF1, 0x72, 0xD2, 0x04][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Asr,
                5,
                false,
                &[0xC5, 0xF1, 0x72, 0xE2, 0x05][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Lsl,
                6,
                false,
                &[0xC5, 0xF1, 0x72, 0xF2, 0x06][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Lsr,
                7,
                false,
                &[0xC5, 0xF1, 0x73, 0xD2, 0x07][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Asr,
                8,
                false,
                &[0x62, 0xF1, 0xF5, 0x08, 0x72, 0xE2, 0x08][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Lsl,
                9,
                false,
                &[0xC5, 0xF1, 0x73, 0xF2, 0x09][..],
            ),
            (
                VecElementType::I8,
                ShiftOp::Lsr,
                10,
                true,
                &[0xC5, 0xF1, 0x73, 0xDA, 0x0A][..],
            ),
            (
                VecElementType::I8,
                ShiftOp::Lsl,
                11,
                true,
                &[0xC5, 0xF1, 0x73, 0xFA, 0x0B][..],
            ),
        ] {
            let op = OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
                elem,
                shift,
                amount,
                byte_lane,
            };
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }
        let high = OpKind::X86PackedShiftImm {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            width: VecWidth::V512,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsl,
            amount: 5,
            byte_lane: false,
        };
        let mut code = CodeBuffer::new();
        lowerer.try_lower(&high, &mut code).unwrap().unwrap();
        assert_eq!(code.as_slice(), &[0x62, 0xB1, 0x75, 0x40, 0x72, 0xF2, 0x05]);

        for invalid in [
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V64,
                elem: VecElementType::I16,
                shift: ShiftOp::Lsr,
                amount: 1,
                byte_lane: false,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
                elem: VecElementType::F32,
                shift: ShiftOp::Lsl,
                amount: 1,
                byte_lane: false,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
                elem: VecElementType::I16,
                shift: ShiftOp::Asr,
                amount: 1,
                byte_lane: true,
            },
            OpKind::X86PackedShiftImm {
                dst: xmm1,
                src: VReg::Virtual(VirtualId(9)),
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsr,
                amount: 1,
                byte_lane: false,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn x86_packed_shared_count_lowering_covers_all_opcodes_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let xmm2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
        let xmm3 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));
        for (elem, shift, expected) in [
            (
                VecElementType::I16,
                ShiftOp::Lsr,
                &[0xC5, 0xE9, 0xD1, 0xCB][..],
            ),
            (
                VecElementType::I16,
                ShiftOp::Asr,
                &[0xC5, 0xE9, 0xE1, 0xCB][..],
            ),
            (
                VecElementType::I16,
                ShiftOp::Lsl,
                &[0xC5, 0xE9, 0xF1, 0xCB][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Lsr,
                &[0xC5, 0xE9, 0xD2, 0xCB][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Asr,
                &[0xC5, 0xE9, 0xE2, 0xCB][..],
            ),
            (
                VecElementType::I32,
                ShiftOp::Lsl,
                &[0xC5, 0xE9, 0xF2, 0xCB][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Lsr,
                &[0xC5, 0xE9, 0xD3, 0xCB][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Asr,
                &[0x62, 0xF1, 0xED, 0x08, 0xE2, 0xCB][..],
            ),
            (
                VecElementType::I64,
                ShiftOp::Lsl,
                &[0xC5, 0xE9, 0xF3, 0xCB][..],
            ),
        ] {
            let op = OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V128,
                elem,
                shift,
            };
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }

        for (op, expected) in [
            (
                OpKind::X86PackedShift {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(4))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(5))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(6))),
                    width: VecWidth::V256,
                    elem: VecElementType::I32,
                    shift: ShiftOp::Asr,
                },
                &[0xC5, 0xD5, 0xE2, 0xE6][..],
            ),
            (
                OpKind::X86PackedShift {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                    width: VecWidth::V512,
                    elem: VecElementType::I64,
                    shift: ShiftOp::Lsl,
                },
                &[0x62, 0xA1, 0xED, 0x40, 0xF3, 0xCB][..],
            ),
            (
                OpKind::X86PackedShift {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    width: VecWidth::V128,
                    elem: VecElementType::I64,
                    shift: ShiftOp::Asr,
                },
                &[0x62, 0xA1, 0xF5, 0x00, 0xE2, 0xC2][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer.try_lower(&op, &mut code).unwrap().unwrap();
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }

        for invalid in [
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V64,
                elem: VecElementType::I16,
                shift: ShiftOp::Lsr,
            },
            OpKind::X86PackedShift {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                src: xmm2,
                count: xmm3,
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsl,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Asr,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: VReg::Virtual(VirtualId(9)),
                width: VecWidth::V128,
                elem: VecElementType::I64,
                shift: ShiftOp::Lsr,
            },
            OpKind::X86PackedShift {
                dst: xmm1,
                src: xmm2,
                count: xmm3,
                width: VecWidth::V128,
                elem: VecElementType::F32,
                shift: ShiftOp::Lsl,
            },
        ] {
            let mut code = CodeBuffer::new();
            let result = lowerer.try_lower(&invalid, &mut code).unwrap();
            assert!(result.is_err(), "accepted malformed {invalid:?}");
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn lowers_x86_evex_bitmanip_ir_to_canonical_encodings() {
        let lowerer = Avx10Lowerer::new();
        let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
        let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
        let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
        let zmm16 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(16)));
        let zmm17 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(17)));
        let zmm18 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(18)));
        let ymm4 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(4)));
        let ymm5 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(5)));
        let ymm6 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(6)));
        let xmm7 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
        let xmm8 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(8)));
        let xmm9 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(9)));
        let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
        let k3 = VReg::Arch(ArchReg::X86(X86Reg::K(3)));
        let k4 = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
        let k5 = VReg::Arch(ArchReg::X86(X86Reg::K(5)));
        let k7 = VReg::Arch(ArchReg::X86(X86Reg::K(7)));
        let cases = [
            (
                OpKind::VFP16Arith {
                    dst: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    mask: Some(k4),
                    op: Avx10FP16Op::Add,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V512,
                    lanes: 32,
                    zeroing: true,
                },
                &[0x62, 0xF5, 0x6C, 0xCC, 0x58, 0xCB][..],
            ),
            (
                OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(16))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(17))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(18))),
                    mask: Some(k7),
                    op: Avx10FP16Op::Mul,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V256,
                    lanes: 16,
                    zeroing: false,
                },
                &[0x62, 0xA5, 0x74, 0x27, 0x59, 0xC2][..],
            ),
            (
                OpKind::VFP16Arith {
                    dst: xmm7,
                    src1: xmm8,
                    src2: xmm9,
                    mask: Some(k3),
                    op: Avx10FP16Op::Sub,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V128,
                    lanes: 8,
                    zeroing: true,
                },
                &[0x62, 0xD5, 0x3C, 0x8B, 0x5C, 0xF9][..],
            ),
            (
                OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(4))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(5))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(6))),
                    mask: None,
                    op: Avx10FP16Op::Div,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V512,
                    lanes: 32,
                    zeroing: false,
                },
                &[0x62, 0xF5, 0x54, 0x48, 0x5E, 0xE6][..],
            ),
            (
                OpKind::VFP16Arith {
                    dst: xmm7,
                    src1: xmm8,
                    src2: xmm9,
                    mask: Some(k3),
                    op: Avx10FP16Op::Min,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V128,
                    lanes: 8,
                    zeroing: true,
                },
                &[0x62, 0xD5, 0x3C, 0x8B, 0x5D, 0xF9][..],
            ),
            (
                OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(16))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(17))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(18))),
                    mask: Some(k7),
                    op: Avx10FP16Op::Max,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V256,
                    lanes: 16,
                    zeroing: false,
                },
                &[0x62, 0xA5, 0x74, 0x27, 0x5F, 0xC2][..],
            ),
            (
                OpKind::VCvtFP32ToBF16 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src1: zmm2,
                    src2: None,
                    mask: Some(k4),
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7E, 0xCC, 0x72, 0xCA][..],
            ),
            (
                OpKind::VCvtFP32ToBF16 {
                    dst: zmm1,
                    src1: zmm2,
                    src2: Some(zmm3),
                    mask: Some(k5),
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x6F, 0x4D, 0x72, 0xCB][..],
            ),
            (
                OpKind::VCvtFP32ToBF16 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(6))),
                    src1: ymm4,
                    src2: None,
                    mask: Some(k2),
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7E, 0x2A, 0x72, 0xF4][..],
            ),
            (
                OpKind::VCvtFP32ToBF16 {
                    dst: xmm7,
                    src1: xmm8,
                    src2: Some(xmm9),
                    mask: Some(k3),
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0x3F, 0x8B, 0x72, 0xF9][..],
            ),
            (
                OpKind::VConflict {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I32,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0xC4, 0xCA][..],
            ),
            (
                OpKind::VConflict {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I64,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0xC4, 0xCA][..],
            ),
            (
                OpKind::VConflict {
                    dst: ymm4,
                    src: ymm6,
                    mask: Some(k2),
                    elem: VecElementType::I32,
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7D, 0x2A, 0xC4, 0xE6][..],
            ),
            (
                OpKind::VConflict {
                    dst: xmm7,
                    src: xmm9,
                    mask: Some(k3),
                    elem: VecElementType::I64,
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0xFD, 0x8B, 0xC4, 0xF9][..],
            ),
            (
                OpKind::VLeadingZeros {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I32,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0x44, 0xCA][..],
            ),
            (
                OpKind::VLeadingZeros {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I64,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0x44, 0xCA][..],
            ),
            (
                OpKind::VLeadingZeros {
                    dst: ymm4,
                    src: ymm6,
                    mask: Some(k2),
                    elem: VecElementType::I32,
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7D, 0x2A, 0x44, 0xE6][..],
            ),
            (
                OpKind::VLeadingZeros {
                    dst: xmm7,
                    src: xmm9,
                    mask: Some(k3),
                    elem: VecElementType::I64,
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0xFD, 0x8B, 0x44, 0xF9][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: zmm1,
                    table1: zmm3,
                    table2: None,
                    indices: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I8,
                    width: VecWidth::V512,
                    overwrite_table: false,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x8D, 0xCB][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: zmm16,
                    table1: zmm18,
                    table2: None,
                    indices: zmm17,
                    mask: Some(k7),
                    elem: VecElementType::I16,
                    width: VecWidth::V512,
                    overwrite_table: false,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xF5, 0x47, 0x8D, 0xC2][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: zmm1,
                    table1: zmm2,
                    table2: Some(zmm3),
                    indices: zmm1,
                    mask: Some(k4),
                    elem: VecElementType::I8,
                    width: VecWidth::V512,
                    overwrite_table: false,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x75, 0xCB][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: ymm4,
                    table1: ymm5,
                    table2: Some(ymm6),
                    indices: ymm4,
                    mask: Some(k2),
                    elem: VecElementType::I16,
                    width: VecWidth::V256,
                    overwrite_table: false,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0xD5, 0x2A, 0x75, 0xE6][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: zmm1,
                    table1: zmm1,
                    table2: Some(zmm3),
                    indices: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I8,
                    width: VecWidth::V512,
                    overwrite_table: true,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x7D, 0xCB][..],
            ),
            (
                OpKind::X86PermuteBytesWords {
                    dst: xmm7,
                    table1: xmm7,
                    table2: Some(xmm9),
                    indices: xmm8,
                    mask: Some(k3),
                    elem: VecElementType::I16,
                    width: VecWidth::V128,
                    overwrite_table: true,
                    zeroing: false,
                },
                &[0x62, 0xD2, 0xBD, 0x0B, 0x7D, 0xF9][..],
            ),
            (
                OpKind::VCompress {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I32,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0x8B, 0xD1][..],
            ),
            (
                OpKind::VCompress {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I64,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0x8B, 0xD1][..],
            ),
            (
                OpKind::VCompress {
                    dst: ymm4,
                    src: ymm6,
                    mask: Some(k2),
                    elem: VecElementType::F32,
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7D, 0x2A, 0x8A, 0xF4][..],
            ),
            (
                OpKind::VCompress {
                    dst: xmm7,
                    src: xmm9,
                    mask: Some(k3),
                    elem: VecElementType::F64,
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0x72, 0xFD, 0x8B, 0x8A, 0xCF][..],
            ),
            (
                OpKind::VExpand {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I32,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0x89, 0xCA][..],
            ),
            (
                OpKind::VExpand {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I64,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0x89, 0xCA][..],
            ),
            (
                OpKind::VExpand {
                    dst: ymm4,
                    src: ymm6,
                    mask: Some(k2),
                    elem: VecElementType::F32,
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7D, 0x2A, 0x88, 0xE6][..],
            ),
            (
                OpKind::VExpand {
                    dst: xmm7,
                    src: xmm9,
                    mask: Some(k3),
                    elem: VecElementType::F64,
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0xFD, 0x8B, 0x88, 0xF9][..],
            ),
            (
                OpKind::VCompress {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I8,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0x63, 0xD1][..],
            ),
            (
                OpKind::VCompress {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I16,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0x63, 0xD1][..],
            ),
            (
                OpKind::VExpand {
                    dst: ymm4,
                    src: ymm6,
                    mask: Some(k2),
                    elem: VecElementType::I8,
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x7D, 0x2A, 0x62, 0xE6][..],
            ),
            (
                OpKind::VExpand {
                    dst: xmm7,
                    src: xmm9,
                    mask: Some(k3),
                    elem: VecElementType::I16,
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0xFD, 0x8B, 0x62, 0xF9][..],
            ),
            (
                OpKind::VDotProductBF16 {
                    dst: zmm1,
                    acc: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    mask: Some(k4),
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6E, 0xCC, 0x52, 0xCB][..],
            ),
            (
                OpKind::VDotProductBF16 {
                    dst: zmm16,
                    acc: zmm16,
                    src1: zmm17,
                    src2: zmm18,
                    mask: Some(k7),
                    width: VecWidth::V256,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0x76, 0x27, 0x52, 0xC2][..],
            ),
            (
                OpKind::VDotProductBF16 {
                    dst: xmm7,
                    acc: xmm7,
                    src1: xmm8,
                    src2: xmm9,
                    mask: Some(k3),
                    width: VecWidth::V128,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0x3E, 0x8B, 0x52, 0xF9][..],
            ),
            (
                OpKind::VMultiplyAdd52 {
                    dst: zmm1,
                    acc: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    mask: Some(k4),
                    width: VecWidth::V512,
                    high: false,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0xED, 0xCC, 0xB4, 0xCB][..],
            ),
            (
                OpKind::VMultiplyAdd52 {
                    dst: zmm16,
                    acc: zmm16,
                    src1: zmm17,
                    src2: zmm18,
                    mask: Some(k7),
                    width: VecWidth::V512,
                    high: true,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xF5, 0x47, 0xB5, 0xC2][..],
            ),
            (
                OpKind::VMultiplyAdd52 {
                    dst: ymm4,
                    acc: ymm4,
                    src1: ymm5,
                    src2: ymm6,
                    mask: Some(k2),
                    width: VecWidth::V256,
                    high: false,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0xD5, 0x2A, 0xB4, 0xE6][..],
            ),
            (
                OpKind::VMultiplyAdd52 {
                    dst: xmm7,
                    acc: xmm7,
                    src1: xmm8,
                    src2: xmm9,
                    mask: Some(k3),
                    width: VecWidth::V128,
                    high: true,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0xBD, 0x8B, 0xB5, 0xF9][..],
            ),
            (
                OpKind::VDotProduct {
                    dst: zmm1,
                    acc: zmm1,
                    src1: zmm2,
                    src2: zmm3,
                    mask: Some(k4),
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V512,
                    src1_unsigned: true,
                    saturate: false,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x50, 0xCB][..],
            ),
            (
                OpKind::VDotProduct {
                    dst: ymm4,
                    acc: ymm4,
                    src1: ymm5,
                    src2: ymm6,
                    mask: Some(k2),
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V256,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x55, 0x2A, 0x51, 0xE6][..],
            ),
            (
                OpKind::VDotProduct {
                    dst: xmm7,
                    acc: xmm7,
                    src1: xmm8,
                    src2: xmm9,
                    mask: Some(k3),
                    src_elem: VecElementType::I16,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V128,
                    src1_unsigned: false,
                    saturate: false,
                    zeroing: true,
                },
                &[0x62, 0xD2, 0x3D, 0x8B, 0x52, 0xF9][..],
            ),
            (
                OpKind::VDotProduct {
                    dst: zmm16,
                    acc: zmm16,
                    src1: zmm17,
                    src2: zmm18,
                    mask: Some(k7),
                    src_elem: VecElementType::I16,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V512,
                    src1_unsigned: false,
                    saturate: true,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0x75, 0x47, 0x53, 0xC2][..],
            ),
            (
                OpKind::VShuffleBitQM {
                    dst: k5,
                    src: zmm3,
                    indices: zmm2,
                    mask: None,
                    width: VecWidth::V512,
                },
                &[0x62, 0xF2, 0x65, 0x48, 0x8F, 0xEA][..],
            ),
            (
                OpKind::VShuffleBitQM {
                    dst: k5,
                    src: zmm3,
                    indices: zmm2,
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    width: VecWidth::V512,
                },
                &[0x62, 0xF2, 0x65, 0x49, 0x8F, 0xEA][..],
            ),
            (
                OpKind::VPopcnt {
                    dst: zmm1,
                    src: zmm2,
                    mask: Some(k4),
                    elem: VecElementType::I8,
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x7D, 0xCC, 0x54, 0xCA][..],
            ),
            (
                OpKind::VPopcnt {
                    dst: zmm17,
                    src: zmm18,
                    mask: Some(k7),
                    elem: VecElementType::I64,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xA2, 0xFD, 0x4F, 0x55, 0xCA][..],
            ),
            (
                OpKind::X86PackedShiftVariable {
                    dst: zmm1,
                    src: zmm2,
                    count: zmm3,
                    mask: None,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    shift: ShiftOp::Lsl,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x6D, 0x48, 0x47, 0xCB][..],
            ),
            (
                OpKind::X86PackedShiftVariable {
                    dst: zmm1,
                    src: zmm2,
                    count: zmm3,
                    mask: Some(k4),
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    shift: ShiftOp::Lsl,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x47, 0xCB][..],
            ),
            (
                OpKind::X86PackedRotate {
                    dst: zmm1,
                    src: zmm2,
                    count: Some(zmm3),
                    mask: None,
                    amount: 0,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x6D, 0x48, 0x15, 0xCB][..],
            ),
            (
                OpKind::X86PackedRotate {
                    dst: zmm1,
                    src: zmm2,
                    count: Some(zmm3),
                    mask: Some(k4),
                    amount: 0,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x15, 0xCB][..],
            ),
            (
                OpKind::X86PackedRotate {
                    dst: zmm17,
                    src: zmm18,
                    count: None,
                    mask: None,
                    amount: 7,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: false,
                },
                &[0x62, 0xB1, 0x75, 0x40, 0x72, 0xCA, 0x07][..],
            ),
            (
                OpKind::X86PackedRotate {
                    dst: zmm1,
                    src: zmm2,
                    count: None,
                    mask: Some(k4),
                    amount: 7,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: true,
                },
                &[0x62, 0xF1, 0x75, 0xCC, 0x72, 0xCA, 0x07][..],
            ),
            (
                OpKind::X86PackedRotate {
                    dst: zmm17,
                    src: zmm18,
                    count: None,
                    mask: None,
                    amount: 63,
                    width: VecWidth::V512,
                    elem: VecElementType::I64,
                    left: false,
                    zeroing: false,
                },
                &[0x62, 0xB1, 0xF5, 0x40, 0x72, 0xC2, 0x3F][..],
            ),
            (
                OpKind::X86TernaryLogic {
                    dst: zmm1,
                    src1: zmm1,
                    src2: zmm2,
                    src3: zmm3,
                    mask: None,
                    imm: 0x96,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    zeroing: false,
                },
                &[0x62, 0xF3, 0x6D, 0x48, 0x25, 0xCB, 0x96][..],
            ),
            (
                OpKind::X86TernaryLogic {
                    dst: zmm1,
                    src1: zmm1,
                    src2: zmm2,
                    src3: zmm3,
                    mask: Some(k4),
                    imm: 0x96,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    zeroing: true,
                },
                &[0x62, 0xF3, 0x6D, 0xCC, 0x25, 0xCB, 0x96][..],
            ),
            (
                OpKind::X86TernaryLogic {
                    dst: zmm16,
                    src1: zmm16,
                    src2: zmm17,
                    src3: zmm18,
                    mask: Some(k7),
                    imm: 0xE4,
                    width: VecWidth::V256,
                    elem: VecElementType::I64,
                    zeroing: false,
                },
                &[0x62, 0xA3, 0xF5, 0x27, 0x25, 0xC2, 0xE4][..],
            ),
            (
                OpKind::X86PackedFunnelShift {
                    dst: zmm1,
                    src: zmm2,
                    fill: zmm3,
                    count: None,
                    mask: None,
                    amount: 7,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: false,
                },
                &[0x62, 0xF3, 0x6D, 0x48, 0x71, 0xCB, 0x07][..],
            ),
            (
                OpKind::X86PackedFunnelShift {
                    dst: zmm1,
                    src: zmm2,
                    fill: zmm3,
                    count: None,
                    mask: Some(k4),
                    amount: 7,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: true,
                },
                &[0x62, 0xF3, 0x6D, 0xCC, 0x71, 0xCB, 0x07][..],
            ),
            (
                OpKind::X86PackedFunnelShift {
                    dst: zmm1,
                    src: zmm1,
                    fill: zmm2,
                    count: Some(zmm3),
                    mask: None,
                    amount: 0,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0x6D, 0x48, 0x71, 0xCB][..],
            ),
            (
                OpKind::X86PackedFunnelShift {
                    dst: zmm1,
                    src: zmm1,
                    fill: zmm2,
                    count: Some(zmm3),
                    mask: Some(k4),
                    amount: 0,
                    width: VecWidth::V512,
                    elem: VecElementType::I32,
                    left: true,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0x6D, 0xCC, 0x71, 0xCB][..],
            ),
            (
                OpKind::X86MultiShiftQB {
                    dst: zmm1,
                    control: zmm2,
                    source: zmm3,
                    mask: None,
                    width: VecWidth::V512,
                    zeroing: false,
                },
                &[0x62, 0xF2, 0xED, 0x48, 0x83, 0xCB][..],
            ),
            (
                OpKind::X86MultiShiftQB {
                    dst: zmm1,
                    control: zmm2,
                    source: zmm3,
                    mask: Some(k4),
                    width: VecWidth::V512,
                    zeroing: true,
                },
                &[0x62, 0xF2, 0xED, 0xCC, 0x83, 0xCB][..],
            ),
        ];
        for (op, expected) in cases {
            let mut code = CodeBuffer::new();
            lowerer
                .try_lower(&op, &mut code)
                .expect("recognized EVEX bit-manipulation op")
                .expect("lower EVEX bit-manipulation op");
            assert_eq!(code.as_slice(), expected, "{op:?}");
        }
    }

    #[test]
    fn lower_avx10_2_vmpsadbw_emits_f3_w0_masks_and_rejects_malformed_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        let ymm = |n| VReg::Arch(ArchReg::X86(X86Reg::Ymm(n)));
        let zmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Zmm(n)));
        let k = |n| VReg::Arch(ArchReg::X86(X86Reg::K(n)));
        let op = |dst, src1, src2, mask, width, imm, zeroing| OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            mask,
            width,
            imm,
            zeroing,
        };

        for (kind, expected) in [
            (
                op(
                    zmm(16),
                    zmm(17),
                    zmm(18),
                    Some(k(3)),
                    VecWidth::V512,
                    0x3F,
                    true,
                ),
                &[0x62, 0xA3, 0x76, 0xC3, 0x42, 0xC2, 0x3F][..],
            ),
            (
                op(
                    xmm(9),
                    xmm(10),
                    xmm(11),
                    Some(k(2)),
                    VecWidth::V128,
                    0xE7,
                    false,
                ),
                &[0x62, 0x53, 0x2E, 0x0A, 0x42, 0xCB, 0xE7][..],
            ),
            (
                op(ymm(4), ymm(5), ymm(6), None, VecWidth::V256, 0x38, false),
                &[0x62, 0xF3, 0x56, 0x28, 0x42, 0xE6, 0x38][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer
                .try_lower(&kind, &mut code)
                .expect("VMPSADBW must be recognized")
                .unwrap();
            assert_eq!(code.as_slice(), expected, "{kind:?}");
        }

        for malformed in [
            op(xmm(1), xmm(2), xmm(3), None, VecWidth::V64, 0, false),
            op(xmm(1), xmm(2), xmm(3), None, VecWidth::V128, 0, true),
            op(ymm(1), ymm(2), ymm(3), None, VecWidth::V128, 0, false),
            op(xmm(32), xmm(2), xmm(3), None, VecWidth::V128, 0, false),
            op(xmm(1), xmm(2), xmm(3), Some(k(0)), VecWidth::V128, 0, false),
            op(
                xmm(1),
                xmm(2),
                xmm(3),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                VecWidth::V128,
                0,
                false,
            ),
        ] {
            let mut code = CodeBuffer::new();
            assert!(matches!(
                lowerer.try_lower(&malformed, &mut code).unwrap(),
                Err(LowerError::InvalidRegister(_) | LowerError::UnsupportedOperation(_))
            ));
            assert!(code.as_slice().is_empty());
        }
    }
}
