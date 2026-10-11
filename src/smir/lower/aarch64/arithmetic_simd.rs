//! Integer and floating-point vector arithmetic, unary operations, and reductions.

use crate::smir::ir::types::{VReg, VecElementType, VecReduceOp, VecUnaryOp};
use crate::smir::lower::LowerError;

use super::{Aarch64Lowerer, SimdArithmeticOp};

impl Aarch64Lowerer {
    pub(crate) fn lower_varith(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        op: SimdArithmeticOp,
    ) -> Result<(), LowerError> {
        if matches!(elem, VecElementType::F32 | VecElementType::F64) {
            return self.lower_vfloat_arith(dst, src1, src2, elem, lanes, op);
        }

        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        if elem == VecElementType::I64 && lanes == 1 {
            let u = match op {
                SimdArithmeticOp::Add => 0,
                SimdArithmeticOp::Sub => 1,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 integer scalar arithmetic {other:?}"),
                    });
                }
            };
            let shape = Self::simd_scalar_or_vector_integer_shape(elem, lanes)?;
            self.emit_integer_simd_three_same(rd, rn, rm, &shape, u, 0b10000);
            return Ok(());
        }
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        let (u, opcode) = match op {
            SimdArithmeticOp::Add => (0, 0b10000),
            SimdArithmeticOp::Sub => (1, 0b10000),
            SimdArithmeticOp::Mul => {
                if size == 3 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native integer vector multiply I64".to_string(),
                    });
                }
                (0, 0b10011)
            }
            SimdArithmeticOp::Max => {
                if size == 3 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native integer vector max I64".to_string(),
                    });
                }
                (1, 0b01100)
            }
            SimdArithmeticOp::Min { signed } => {
                if size == 3 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native integer vector min I64".to_string(),
                    });
                }
                (if signed { 0 } else { 1 }, 0b01101)
            }
            SimdArithmeticOp::Div => {
                // No integer vector divide in NEON; FP divide is routed to
                // lower_vfloat_arith above. Reaching here means an integer
                // element type, which is unsupported.
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native integer vector divide".to_string(),
                });
            }
        };
        self.emit_simd_three_same(rd, rn, rm, q, u, size, opcode);
        Ok(())
    }

    pub(crate) fn lower_vfloat_arith(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        op: SimdArithmeticOp,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        let (q, elem_size) = Self::simd_float_shape(elem, lanes)?;
        let (u, size, opcode) = match op {
            SimdArithmeticOp::Add => (0, elem_size, 0b11010),
            SimdArithmeticOp::Sub => (0, elem_size | 0b10, 0b11010),
            SimdArithmeticOp::Mul => (1, elem_size, 0b11011),
            SimdArithmeticOp::Div => (1, elem_size, 0b11111),
            // The lifter maps architectural vector FMAX/FMIN (NaN-PROPAGATING)
            // to VMax/VMin, and FMAXNM/FMINNM (numeric, NaN-quiet) to the
            // separate VFMinMaxNm op. So VMax/VMin must emit the propagating
            // opcode 0b11110 (FMAX/FMIN), NOT the numeric 0b11000 (FMAXNM/FMINNM
            // — which VFMinMaxNm already uses). The interpreter's VMax/VMin are
            // fixed to propagate NaN to match. (See #159; the earlier #56
            // change to 0b11000 collapsed the FMAX-vs-FMAXNM distinction.)
            SimdArithmeticOp::Max => (0, elem_size, 0b11110),
            SimdArithmeticOp::Min { .. } => (0, elem_size | 0b10, 0b11110),
        };
        self.emit_simd_three_same(rd, rn, rm, q, u, size, opcode);
        Ok(())
    }

    /// Lower FP vector unary operations and integer scalar/vector ABS or NEG.
    pub(crate) fn lower_vunary(
        &mut self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        op: VecUnaryOp,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        if elem == VecElementType::I64 && lanes == 1 {
            let u = match op {
                VecUnaryOp::Abs => 0,
                VecUnaryOp::Neg => 1,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 integer scalar unary {other:?}"),
                    });
                }
            };
            let shape = Self::simd_scalar_or_vector_integer_shape(elem, lanes)?;
            self.emit_integer_simd_two_misc(rd, rn, &shape, u, 0b01011);
            return Ok(());
        }
        let (q, u, size, opcode) = match op {
            // FP forms: a = size<1> is always 1, sz = size<0> selects S/D.
            VecUnaryOp::FAbs => {
                let (q, sz) = Self::simd_float_shape(elem, lanes)?;
                (q, 0, 0b10 | sz, 0b01111)
            }
            VecUnaryOp::FNeg => {
                let (q, sz) = Self::simd_float_shape(elem, lanes)?;
                (q, 1, 0b10 | sz, 0b01111)
            }
            VecUnaryOp::FSqrt => {
                let (q, sz) = Self::simd_float_shape(elem, lanes)?;
                (q, 1, 0b10 | sz, 0b11111)
            }
            VecUnaryOp::FRecipEstimate | VecUnaryOp::FRsqrtEstimate => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("x86 vector estimate {op:?}"),
                });
            }
            // Integer forms: size = element width, opcode 01011 (NEG: U=1, ABS: U=0).
            VecUnaryOp::Neg => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                (q, 1, size, 0b01011)
            }
            VecUnaryOp::Abs => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                (q, 0, size, 0b01011)
            }
            // CLZ/CLS: opcode 00100, size = element width (8/16/32 only — there
            // is no 64-bit CLZ/CLS). CLZ: U=1, CLS: U=0.
            VecUnaryOp::Clz | VecUnaryOp::Cls => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                if size == 3 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native vector {op:?} I64"),
                    });
                }
                let u = if matches!(op, VecUnaryOp::Clz) { 1 } else { 0 };
                (q, u, size, 0b00100)
            }
            // Per-byte forms share opcode 00101 with fixed size fields: CNT
            // (U=0,size=00), NOT (U=1,size=00), RBIT (U=1,size=01). The element
            // is always I8, so simd_integer_shape only supplies Q.
            VecUnaryOp::Cnt => {
                let (q, _) = Self::simd_integer_shape(elem, lanes)?;
                (q, 0, 0b00, 0b00101)
            }
            VecUnaryOp::Not => {
                let (q, _) = Self::simd_integer_shape(elem, lanes)?;
                (q, 1, 0b00, 0b00101)
            }
            VecUnaryOp::Rbit => {
                let (q, _) = Self::simd_integer_shape(elem, lanes)?;
                (q, 1, 0b01, 0b00101)
            }
            // REV: reverse `elem`-sized elements within each container. size =
            // the reversed-element width; opcode/U select the container:
            // REV64 U=0/op00000, REV16 U=0/op00001, REV32 U=1/op00000.
            VecUnaryOp::Rev64 => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                (q, 0, size, 0b00000)
            }
            VecUnaryOp::Rev16 => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                (q, 0, size, 0b00001)
            }
            VecUnaryOp::Rev32 => {
                let (q, size) = Self::simd_integer_shape(elem, lanes)?;
                (q, 1, size, 0b00000)
            }
        };
        self.emit_simd_two_reg_misc(rd, rn, q, u, size, opcode);
        Ok(())
    }

    /// Lower a vector across-lanes integer reduction (ADDV/SMAXV/UMAXV/SMINV/
    /// UMINV) to the native AArch64 "advanced SIMD across lanes" form. The
    /// result is a scalar in lane 0 of the destination.
    pub(crate) fn lower_vreduce(
        &mut self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        op: VecReduceOp,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        if elem == VecElementType::I64 && lanes == 2 && op == VecReduceOp::Add {
            // ADDP Dd,Vn.2D reduces exactly two 64-bit elements modulo 2^64.
            self.emit(0x5ef1_b800 | (u32::from(rn) << 5) | u32::from(rd));
            return Ok(());
        }
        // FP reductions: across-lanes, U=1 (the f32 .4S form), opcode 01100 (NM)
        // / 01111, with a = size<1> selecting max (0) vs min (1). For f32 the
        // sz bit (size<0>) is 0.
        if let Some((opcode, min)) = match op {
            VecReduceOp::FMax => Some((0b01111, false)),
            VecReduceOp::FMin => Some((0b01111, true)),
            VecReduceOp::FMaxNm => Some((0b01100, false)),
            VecReduceOp::FMinNm => Some((0b01100, true)),
            _ => None,
        } {
            let (q, _sz) = Self::simd_float_shape(elem, lanes)?;
            let size = if min { 0b10 } else { 0b00 };
            self.emit_simd_across_lanes(rd, rn, q, 1, size, opcode);
            return Ok(());
        }
        let (q, size) = Self::simd_integer_shape(elem, lanes)?;
        // Across-lanes integer reductions allow 8B, 16B, 4H, 8H, or 4S.
        // The scalar ADDP case above handles the separate 2D addition form.
        let reserved_source_shape = size == 3 || (size == 2 && q == 0);
        if reserved_source_shape {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 vector reduction {op:?} {elem:?}x{lanes}"),
            });
        }
        let (u, opcode) = match op {
            VecReduceOp::Add => (0, 0b11011),
            VecReduceOp::SMax => (0, 0b01010),
            VecReduceOp::UMax => (1, 0b01010),
            VecReduceOp::SMin => (0, 0b11010),
            VecReduceOp::UMin => (1, 0b11010),
            // SADDLV/UADDLV: widening add (the native op produces a 2x-width
            // scalar). Source element is B/H/S (never 64-bit), so the size==3
            // guard above never trips here.
            VecReduceOp::SAddLong => (0, 0b00011),
            VecReduceOp::UAddLong => (1, 0b00011),
            // FP forms handled above.
            _ => unreachable!(),
        };
        self.emit_simd_across_lanes(rd, rn, q, u, size, opcode);
        Ok(())
    }
}
