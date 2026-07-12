//! AVX10.1 and AVX10.2 instruction lowering.
//!
//! This module lowers SMIR operations to EVEX-encoded AVX10 machine code.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::*;
use crate::smir::lower::{CodeBuffer, LowerError};

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
    /// P1: R X B R' 0 0 m m
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
        // Extract register bits
        let r = (dst >> 3) & 1; // bit 3 of dst
        let r_prime = (dst >> 4) & 1; // bit 4 of dst
        let x = (src2 >> 4) & 1; // bit 4 of src2 (index)
        let b = (src2 >> 3) & 1; // bit 3 of src2
        let vvvv = src1 & 0x0F;
        let v_prime = (src1 >> 4) & 1;

        let ll = match vl {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            VecWidth::V64 => 0,
        };

        // Build P0
        self.code.emit_u8(0x62);

        // Build P1: ~R ~X ~B ~R' 0 0 m m
        let p1 =
            ((r ^ 1) << 7) | ((x ^ 1) << 6) | ((b ^ 1) << 5) | ((r_prime ^ 1) << 4) | (map & 0x03);
        self.code.emit_u8(p1);

        // Build P2: W ~vvvv 1 pp
        let vvvv_inv = (!vvvv) & 0x0F;
        let p2 = ((w as u8) << 7) | (vvvv_inv << 3) | 0x04 | (pp & 0x03);
        self.code.emit_u8(p2);

        // Build P3: z L'L b ~V' aaa
        let p3 = ((zeroing as u8) << 7)
            | (ll << 5)
            | 0 // b bit (broadcast) - could add later
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

            // AVX10.1 BITALG
            OpKind::VShuffleBitQM {
                dst,
                src,
                indices,
                mask,
                width,
            } => Some(self.lower_vpshufbitqmb(code, dst, src, indices, mask.as_ref(), *width)),

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
                width,
            } => Some(self.lower_vcvtfp32tobf16(code, dst, src1, src2.as_ref(), *width)),

            // AVX10.1 FP16
            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                op,
                width,
            } => Some(self.lower_vfp16_arith(code, dst, src1, src2, *op, *width)),

            // AVX10.2 saturation conversions
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                fp_elem,
                int_elem,
                width,
                signed,
            } => Some(
                self.lower_vcvt_fp_to_int_sat(code, dst, src, *fp_elem, *int_elem, *width, *signed),
            ),

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
                width,
                imm,
            } => Some(self.lower_vmpsadbw(code, dst, src1, src2, *width, *imm)),

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
        width: VecWidth,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;

        let (pp, src2_reg) = if let Some(s2) = src2 {
            // VCVTNE2PS2BF16
            (3, self.vreg_to_zmm(s2)?) // F2
        } else {
            // VCVTNEPS2BF16
            (2, src1_reg) // F3
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            pp, false, // W = 0
            width, dst_reg, src1_reg, src2_reg, 0, false,
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
        op: Avx10FP16Op,
        width: VecWidth,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;

        let opcode = match op {
            Avx10FP16Op::Add => 0x58,
            Avx10FP16Op::Mul => 0x59,
            Avx10FP16Op::Sub => 0x5C,
            Avx10FP16Op::Div => 0x5E,
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
            width, dst_reg, src1_reg, src2_reg, 0, false,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src2_reg);

        Ok(())
    }

    // ========================================================================
    // AVX10.2 Saturation Conversions
    // ========================================================================

    fn lower_vcvt_fp_to_int_sat(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        fp_elem: VecElementType,
        int_elem: VecElementType,
        width: VecWidth,
        signed: bool,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;

        let (opcode, pp, w) = match (fp_elem, int_elem, signed) {
            (VecElementType::F32, VecElementType::I8, true) => (0x68, 0, false), // VCVTTPS2IBS
            (VecElementType::F32, VecElementType::I8, false) => (0x6A, 0, false), // VCVTTPS2IUBS
            (VecElementType::F64, VecElementType::I64, true) => (0x6D, 1, true), // VCVTTPD2QQS
            (VecElementType::F64, VecElementType::I64, false) => (0x6C, 1, true), // VCVTTPD2UQQS
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Saturation conversion: invalid types".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            2, // map 0F38
            pp, w, width, dst_reg, 0, src_reg, 0, false,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src_reg);

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
        width: VecWidth,
        imm: u8,
    ) -> Avx10LowerResult<()> {
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src1_reg = self.vreg_to_zmm(src1)?;
        let src2_reg = self.vreg_to_zmm(src2)?;

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex(
            3,     // map 0F3A
            1,     // pp = 66
            false, // W = 0
            width, dst_reg, src1_reg, src2_reg, 0, false,
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
}
