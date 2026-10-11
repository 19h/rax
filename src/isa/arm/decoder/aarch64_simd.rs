//! A64 AdvSIMD decoding.

use super::aarch64::Aarch64Decoder;
use super::{DecodeError, DecodedInsn, Mnemonic, operand::*};
use crate::isa::arm::ExecutionState;

impl Aarch64Decoder {
    pub(super) fn decode_simd_fp(raw: u32) -> Result<DecodedInsn, DecodeError> {
        if let Some(instruction) = Self::decode_simd_saturating(raw) {
            return Ok(instruction);
        }
        if let Some(instruction) = Self::decode_simd_resize(raw) {
            return Ok(instruction);
        }
        if let Some(instruction) = Self::decode_simd_integer_scalar_or_zero(raw) {
            return Ok(instruction);
        }
        // Bits [30] and [29] are Q and U for vector AdvSIMD forms.
        let q = (raw >> 30) & 1;
        let u = (raw >> 29) & 1;

        if raw & 0x9f80_0400 == 0x0f00_0400 && (raw >> 19) & 0xf != 0 {
            return Self::decode_simd_shift_immediate(raw, q, u);
        }

        // SIMD three-same: [31]=0, [30]=Q, [28:24]=01110, [21]=1,
        // [15:11]=opcode.
        // Encoding: 0_Q_U_01110_size_1_Rm_opcode_1_Rn_Rd
        if (raw >> 24) & 0x1F == 0b01110 && (raw >> 21) & 1 == 1 && (raw >> 10) & 1 == 1 {
            return Self::decode_simd_three_same(raw, q, u);
        }

        // SIMD two-reg misc: [31]=0, [30]=Q, [28:24]=01110,
        // [21:17]=10000, [16:12]=opcode.
        if (raw >> 24) & 0x1F == 0b01110 && (raw >> 17) & 0x1F == 0b10000 {
            return Self::decode_simd_two_reg_misc(raw, q, u);
        }

        // SIMD across lanes: [31]=0, [30]=Q, [28:24]=01110, [21:17]=11000.
        if (raw >> 24) & 0x1F == 0b01110 && (raw >> 17) & 0x1F == 0b11000 {
            return Self::decode_simd_across_lanes(raw, q, u);
        }

        // SIMD scalar pairwise: [31:30]=01, [28:24]=11110
        if (raw >> 30) & 0x3 == 0b01 && (raw >> 24) & 0x1F == 0b11110 {
            return Self::decode_simd_scalar_pairwise(raw);
        }

        // SIMD table lookup (TBL/TBX): [28:24]=01110, U=0, [23:22]=00, [21]=0,
        // [11:10]=00. Must precede SIMD copy (also [21]=0) — copy has bit10=1.
        if (raw >> 24) & 0x1F == 0b01110
            && u == 0
            && (raw >> 22) & 0x3 == 0b00
            && (raw >> 21) & 1 == 0
            && (raw >> 10) & 0x3 == 0b00
        {
            return Self::decode_simd_table(raw, q);
        }

        // SIMD permute (ZIP/UZP/TRN): [28:24]=01110, [21]=0, [15]=0, [11:10]=10.
        // Must precede SIMD copy (also [21]=0) — copy has bit10=1.
        if (raw >> 24) & 0x1F == 0b01110
            && (raw >> 21) & 1 == 0
            && (raw >> 15) & 1 == 0
            && (raw >> 10) & 0x3 == 0b10
        {
            return Self::decode_simd_permute(raw, q);
        }

        // SIMD copy (DUP, MOV, INS): [31:30]=0x, [28:24]=01110, [21]=0
        if raw & 0x9fe0_8400 == 0x0e00_0400 {
            return Self::decode_simd_copy(raw, q);
        }

        // Advanced SIMD scalar three-same: [31:30]=01, [28:24]=11110
        // Advanced SIMD scalar two-reg misc: similar encoding
        if (raw >> 30) & 0x3 == 0b01 && (raw >> 24) & 0x1F == 0b11110 {
            return Self::decode_simd_scalar_three(raw, u);
        }

        // Scalar FP 3-source: [31:30]=00, [28:24]=11111
        if (raw >> 30) & 0x3 == 0b00 && (raw >> 24) & 0x1F == 0b11111 {
            return Self::decode_scalar_fp_3source(raw);
        }

        // Scalar FP data-processing: [31:30]=00, [28:24]=11110. Subclasses
        // under bit 21 use overlapping encodings, so dispatch by their specific
        // opcode subfields instead of treating bit 21 as "2 source".
        if (raw >> 30) & 0x3 == 0b00 && (raw >> 24) & 0x1F == 0b11110 {
            return Self::decode_scalar_fp(raw);
        }

        // SIMD load/store: various encodings
        if (raw >> 24) & 0x1F == 0b01100 || (raw >> 24) & 0x1F == 0b01101 {
            return Self::decode_simd_ldst(raw, q);
        }

        Ok(DecodedInsn::new(
            Mnemonic::UNKNOWN,
            ExecutionState::Aarch64,
            raw,
            4,
        ))
    }

    /// Decode SIMD three-same register instructions.
    fn decode_simd_three_same(raw: u32, q: u32, u: u32) -> Result<DecodedInsn, DecodeError> {
        let size = (raw >> 22) & 0x3;
        let opcode = (raw >> 11) & 0x1F;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let fp_reg = |num| {
            Operand::FpReg(FpRegister {
                num,
                size: vec_size,
            })
        };

        // FP three-same occupy opcode 0b11xxx. Unlike the integer forms below
        // (keyed on (U, opcode)), the FP forms are discriminated by
        // (U, a, opcode) where a = size<1> (bit 23): FADD/FSUB, FMAX/FMIN,
        // FMAXNM/FMINNM and FMLA/FMLS are ALL U=0 and differ only in `a`. sz
        // (bit 22) selects single/double and is read by the lifter. Keying these
        // on U alone (the prior bug) mislabeled FSUB as FADD, FMIN as FMAX, and
        // the U=1 pairwise/FABD forms as FSUB/FMIN. Forms RAX does not model
        // (FMULX/FRECPS/FRSQRTS/FADDP/FABD/pairwise/compare/FACG*) → UNKNOWN.
        if opcode >> 3 == 0b11 {
            let a = (raw >> 23) & 1;
            let mnemonic = match (u, a, opcode) {
                (0, 0, 0b11000) => Mnemonic::FMAXNM,
                (0, 1, 0b11000) => Mnemonic::FMINNM,
                (0, 0, 0b11001) => Mnemonic::FMLA,
                (0, 1, 0b11001) => Mnemonic::FMLS,
                (0, 0, 0b11010) => Mnemonic::FADD,
                (0, 1, 0b11010) => Mnemonic::FSUB,
                (0, 0, 0b11110) => Mnemonic::FMAX,
                (0, 1, 0b11110) => Mnemonic::FMIN,
                (1, 0, 0b11011) => Mnemonic::FMUL,
                (1, 0, 0b11111) => Mnemonic::FDIV,
                _ => Mnemonic::UNKNOWN,
            };
            return Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
                .with_operand(fp_reg(rd))
                .with_operand(fp_reg(rn))
                .with_operand(fp_reg(rm)));
        }

        let mnemonic = match (u, opcode) {
            // Integer three-same. DDI 0602 AdvSIMD, not scalar ALU slots.
            (_, 0b00000) => Mnemonic::VHADD,
            (_, 0b00010) => Mnemonic::VRHADD,
            (_, 0b00100) => Mnemonic::VHSUB,
            (_, 0b00110) => Mnemonic::VCGT,
            (_, 0b00111) => Mnemonic::VCGE,
            (_, 0b01000) => Mnemonic::VSHL,
            (_, 0b01010) => Mnemonic::VRSHL,
            (_, 0b01100) => Mnemonic::VMAX,
            (_, 0b01101) => Mnemonic::VMIN,
            (_, 0b01110) => Mnemonic::VABD,
            (_, 0b01111) => Mnemonic::VABA,
            (0, 0b10000) => Mnemonic::VADD,
            (1, 0b10000) => Mnemonic::VSUB,
            (0, 0b10001) => Mnemonic::VTST,
            (1, 0b10001) => Mnemonic::VCEQ,
            (0, 0b10010) => Mnemonic::VMLA,
            (1, 0b10010) => Mnemonic::VMLS,
            (0, 0b10011) => Mnemonic::VMUL,
            (_, 0b10100) => Mnemonic::VPMAX,
            (_, 0b10101) => Mnemonic::VPMIN,
            (0, 0b10111) => Mnemonic::VPADD,

            // Logical operations
            (0, 0b00011) if size == 0b00 => Mnemonic::VAND,
            (0, 0b00011) if size == 0b01 => Mnemonic::VBIC,
            (0, 0b00011) if size == 0b10 => Mnemonic::VORR,
            (0, 0b00011) if size == 0b11 => Mnemonic::VORN,

            (1, 0b00011) if size == 0b00 => Mnemonic::VEOR,
            (1, 0b00011) if size == 0b01 => Mnemonic::VBSL,
            (1, 0b00011) if size == 0b10 => Mnemonic::VBIT,
            (1, 0b00011) if size == 0b11 => Mnemonic::VBIF,

            // FP three-same (opcode 0b11xxx) handled by the (U, a, opcode)
            // branch above.
            _ => Mnemonic::UNKNOWN,
        };

        let is_integer = matches!(
            mnemonic,
            Mnemonic::VHADD
                | Mnemonic::VRHADD
                | Mnemonic::VHSUB
                | Mnemonic::VCGT
                | Mnemonic::VCGE
                | Mnemonic::VSHL
                | Mnemonic::VRSHL
                | Mnemonic::VMAX
                | Mnemonic::VMIN
                | Mnemonic::VABD
                | Mnemonic::VABA
                | Mnemonic::VADD
                | Mnemonic::VSUB
                | Mnemonic::VTST
                | Mnemonic::VCEQ
                | Mnemonic::VMLA
                | Mnemonic::VMLS
                | Mnemonic::VMUL
                | Mnemonic::VPMAX
                | Mnemonic::VPMIN
                | Mnemonic::VPADD
        );
        let supports_i64 = matches!(
            mnemonic,
            Mnemonic::VCGT
                | Mnemonic::VCGE
                | Mnemonic::VADD
                | Mnemonic::VSUB
                | Mnemonic::VTST
                | Mnemonic::VCEQ
                | Mnemonic::VSHL
                | Mnemonic::VRSHL
                | Mnemonic::VPADD
        );
        let reserved_i64_shape = is_integer && size == 0b11 && (q == 0 || !supports_i64);
        if raw >> 31 != 0 || reserved_i64_shape {
            return Ok(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }
        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn))
            .with_operand(fp_reg(rm)))
    }

    /// Decode SIMD two-register miscellaneous instructions.
    fn decode_simd_two_reg_misc(raw: u32, q: u32, u: u32) -> Result<DecodedInsn, DecodeError> {
        let size = (raw >> 22) & 0x3;
        let opcode = (raw >> 12) & 0x1F;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let fp_reg = |num| {
            Operand::FpReg(FpRegister {
                num,
                size: vec_size,
            })
        };

        // `a` (size<1>, bit 23) selects the FP opcode sub-group, mirroring the
        // three-same FP encoding; for integer forms `size` is the element width.
        // Width transitions and pairwise-long arithmetic are handled by
        // decode_simd_resize before this dispatch. Remaining vector forms
        // (CM#0, FCVT*/FRINT*/SCVTF/
        // UCVTF/FCM#0/FRECPE/FRSQRTE) decode to UNKNOWN and remain direct
        // interpreter frontiers.
        let a = (raw >> 23) & 1;
        let mnemonic = match (u, opcode) {
            // integer two-register misc (size = element width)
            (0, 0b00000) => Mnemonic::REV64,
            (0, 0b00001) => Mnemonic::REV16,
            (0, 0b00100) => Mnemonic::CLS,
            (0, 0b00101) if size == 0b00 => Mnemonic::CNT, // CNT (per-byte popcount)
            (0, 0b01011) => Mnemonic::VABS,                // ABS
            (1, 0b00000) => Mnemonic::REV32,
            (1, 0b00100) => Mnemonic::CLZ,
            (1, 0b00101) if size == 0b00 => Mnemonic::VMVN, // NOT
            (1, 0b00101) if size == 0b01 => Mnemonic::RBIT, // RBIT
            (1, 0b01011) => Mnemonic::VNEG,                 // NEG

            // FP two-register misc (a = size<1>; sz = size<0> single/double)
            (0, 0b01111) if a == 1 => Mnemonic::FABS,
            (1, 0b01111) if a == 1 => Mnemonic::FNEG,
            (1, 0b11111) if a == 1 => Mnemonic::FSQRT,

            _ => Mnemonic::UNKNOWN,
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn)))
    }

    /// Decode SIMD across-lanes instructions (reduction).
    fn decode_simd_across_lanes(raw: u32, q: u32, u: u32) -> Result<DecodedInsn, DecodeError> {
        let size = (raw >> 22) & 0x3;
        let opcode = (raw >> 12) & 0x1F;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let scalar_size = match size {
            0b00 => FpRegSize::B,
            0b01 => FpRegSize::H,
            0b10 => FpRegSize::S,
            0b11 => FpRegSize::D,
            _ => FpRegSize::S,
        };

        // FP across-lanes reductions (opcode 01100 = FMAXNMV/FMINNMV, 01111 =
        // FMAXV/FMINV) discriminate max vs min via a = bit23 (NOT U — the prior
        // table keyed min on U=1, which is wrong). The U bit selects the element
        // type: U=1 → f32 (.4S, scalar S), U=0 → FP16 (.4H/.8H, scalar H).
        if opcode == 0b01100 || opcode == 0b01111 {
            let a = (raw >> 23) & 1;
            let mnemonic = match (a, opcode) {
                (0, 0b01100) => Mnemonic::FMAXNM,
                (1, 0b01100) => Mnemonic::FMINNM,
                (0, 0b01111) => Mnemonic::FMAX,
                (1, 0b01111) => Mnemonic::FMIN,
                _ => Mnemonic::UNKNOWN,
            };
            let fp_dst = if u == 1 { FpRegSize::S } else { FpRegSize::H };
            return Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
                .with_operand(Operand::FpReg(FpRegister {
                    num: rd,
                    size: fp_dst,
                }))
                .with_operand(Operand::FpReg(FpRegister {
                    num: rn,
                    size: vec_size,
                })));
        }

        let mnemonic = match (u, opcode) {
            (0, 0b00011) => Mnemonic::SADDV, // SADDLV
            (0, 0b01010) => Mnemonic::SMAXV, // SMAXV
            (0, 0b11010) => Mnemonic::SMINV, // SMINV
            (0, 0b11011) => Mnemonic::ADDV,  // ADDV

            (1, 0b00011) => Mnemonic::UADDV, // UADDLV
            (1, 0b01010) => Mnemonic::UMAXV, // UMAXV
            (1, 0b11010) => Mnemonic::UMINV, // UMINV

            _ => Mnemonic::UNKNOWN,
        };

        // ADDV has no 2S, 1D, or 2D source arrangement. Scalar ADDP is a
        // separate encoding and must not make these vector forms valid.
        let reserved_addv = mnemonic == Mnemonic::ADDV && (size == 3 || (size == 2 && q == 0));
        if reserved_addv {
            return Ok(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(Operand::FpReg(FpRegister {
                num: rd,
                size: scalar_size,
            }))
            .with_operand(Operand::FpReg(FpRegister {
                num: rn,
                size: vec_size,
            })))
    }

    /// Decode SIMD table lookup (TBL/TBX).
    /// Encoding: `0 Q 0 01110 00 0 Rm 0 len op 00 Rn Rd` (op = bit12: 0=TBL,
    /// 1=TBX; len = bits[14:13] = #table regs - 1). Operands: dst, table base
    /// (Vn — the lifter reads `len` from the raw to know how many regs), index.
    fn decode_simd_table(raw: u32, q: u32) -> Result<DecodedInsn, DecodeError> {
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        let op = (raw >> 12) & 1;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let mnemonic = if op == 1 {
            Mnemonic::TBX
        } else {
            Mnemonic::TBL
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            // dst (Vd.Ta)
            .with_operand(Operand::FpReg(FpRegister {
                num: rd,
                size: vec_size,
            }))
            // table base (Vn.16b — table registers are always full 16-byte)
            .with_operand(Operand::FpReg(FpRegister {
                num: rn,
                size: FpRegSize::Q,
            }))
            // index (Vm.Ta)
            .with_operand(Operand::FpReg(FpRegister {
                num: rm,
                size: vec_size,
            })))
    }

    /// Decode SIMD permute instructions (ZIP1/ZIP2/UZP1/UZP2/TRN1/TRN2).
    /// Encoding: `0 Q 0 01110 size 0 Rm 0 opcode 10 Rn Rd` (opcode = bits[14:12]).
    fn decode_simd_permute(raw: u32, q: u32) -> Result<DecodedInsn, DecodeError> {
        let rm = ((raw >> 16) & 0x1F) as u8;
        let opcode = (raw >> 12) & 0x7;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let fp_reg = |num| {
            Operand::FpReg(FpRegister {
                num,
                size: vec_size,
            })
        };

        let mnemonic = match opcode {
            0b001 => Mnemonic::UZP1,
            0b010 => Mnemonic::TRN1,
            0b011 => Mnemonic::ZIP1,
            0b101 => Mnemonic::UZP2,
            0b110 => Mnemonic::TRN2,
            0b111 => Mnemonic::ZIP2,
            _ => Mnemonic::UNKNOWN,
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn))
            .with_operand(fp_reg(rm)))
    }

    /// Decode SIMD scalar pairwise instructions.
    fn decode_simd_scalar_pairwise(raw: u32) -> Result<DecodedInsn, DecodeError> {
        let u = (raw >> 29) & 1;
        let size = (raw >> 22) & 0x3;
        let opcode = (raw >> 12) & 0x1F;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let fp_size = if size & 1 == 0 {
            FpRegSize::S
        } else {
            FpRegSize::D
        };
        let fp_reg = |num| Operand::FpReg(FpRegister { num, size: fp_size });

        let mnemonic = match (u, opcode) {
            (0, 0b01100) => Mnemonic::FMAXNM, // FMAXNMP
            (0, 0b01101) => Mnemonic::FADD,   // FADDP
            (0, 0b01111) => Mnemonic::FMAX,   // FMAXP
            (1, 0b01100) => Mnemonic::FMINNM, // FMINNMP
            (1, 0b01111) => Mnemonic::FMIN,   // FMINP
            _ => Mnemonic::UNKNOWN,
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn)))
    }

    fn decode_simd_scalar_three(raw: u32, u: u32) -> Result<DecodedInsn, DecodeError> {
        let size = (raw >> 22) & 0x3;
        let opcode = (raw >> 11) & 0x1F;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let fp_size = if size & 1 == 0 {
            FpRegSize::S
        } else {
            FpRegSize::D
        };
        let fp_reg = |num| Operand::FpReg(FpRegister { num, size: fp_size });

        let mnemonic = match (u, opcode) {
            (0, 0b11010) => Mnemonic::FADD,
            (0, 0b11011) => Mnemonic::FMUL,
            (0, 0b11110) => Mnemonic::FMAX,
            (0, 0b11111) => Mnemonic::FMIN,
            (1, 0b11010) => Mnemonic::FSUB,
            (1, 0b11101) => Mnemonic::FMUL,
            (1, 0b11111) => Mnemonic::FDIV,
            _ => Mnemonic::UNKNOWN,
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn))
            .with_operand(fp_reg(rm)))
    }

    /// Copy operands retain register class and lane indices; register 31 is ZR.
    fn decode_simd_copy(raw: u32, q: u32) -> Result<DecodedInsn, DecodeError> {
        let op = (raw >> 29) & 1;
        let imm5 = (raw >> 16) & 0x1F;
        let imm4 = (raw >> 11) & 0xF;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        // The lowest set bit of imm5 selects the element width. If none of
        // bits [3:0] is set, the encoding does not describe a copy operation.
        let size = (imm5 & 0xF).trailing_zeros();
        let elem_size = match size {
            0 => FpRegSize::B,
            1 => FpRegSize::H,
            2 => FpRegSize::S,
            3 => FpRegSize::D,
            _ => {
                return Ok(DecodedInsn::new(
                    Mnemonic::UNDEFINED,
                    ExecutionState::Aarch64,
                    raw,
                    4,
                ));
            }
        };
        let index = imm5 >> (size + 1);
        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let fp_reg = |num, size| Operand::FpReg(FpRegister { num, size });

        if op == 1 {
            // INS (element) always writes a 128-bit vector. Low imm4 bits
            // below the element-size bit are reserved; the remaining bits
            // encode the source lane independently of the destination lane.
            let reserved_index_bits = imm4 & ((1 << size) - 1);
            if q == 0 || reserved_index_bits != 0 {
                return Ok(DecodedInsn::new(
                    Mnemonic::UNDEFINED,
                    ExecutionState::Aarch64,
                    raw,
                    4,
                ));
            }
            return Ok(
                DecodedInsn::new(Mnemonic::VMOV, ExecutionState::Aarch64, raw, 4)
                    .with_operand(fp_reg(rd, FpRegSize::Q))
                    .with_operand(fp_reg(rn, elem_size))
                    .with_operand(Operand::Imm(Immediate::new(i64::from(index))))
                    .with_operand(Operand::Imm(Immediate::new(i64::from(imm4 >> size)))),
            );
        }

        let (mnemonic, dst, src) = match imm4 {
            // DUP (element).
            0b0000 if size != 3 || q == 1 => {
                (Mnemonic::VDUP, fp_reg(rd, vec_size), fp_reg(rn, elem_size))
            }
            // DUP (general).
            0b0001 if size != 3 || q == 1 => (
                Mnemonic::VDUP,
                fp_reg(rd, vec_size),
                Operand::Reg(Register::with_zr(rn, size == 3)),
            ),
            // INS (general).
            0b0011 if q == 1 => (
                Mnemonic::VMOV,
                fp_reg(rd, FpRegSize::Q),
                Operand::Reg(Register::with_zr(rn, size == 3)),
            ),
            // SMOV permits B/H -> W/X and S -> X.
            0b0101 if size < 3 && (size < 2 || q == 1) => (
                Mnemonic::VMOV,
                Operand::Reg(Register::with_zr(rd, q == 1)),
                fp_reg(rn, elem_size),
            ),
            // UMOV permits B/H/S -> W and D -> X.
            0b0111 if (size == 3) == (q == 1) => (
                Mnemonic::VMOV,
                Operand::Reg(Register::with_zr(rd, q == 1)),
                fp_reg(rn, elem_size),
            ),
            _ => {
                return Ok(DecodedInsn::new(
                    Mnemonic::UNDEFINED,
                    ExecutionState::Aarch64,
                    raw,
                    4,
                ));
            }
        };
        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(dst)
            .with_operand(src)
            .with_operand(Operand::Imm(Immediate::new(i64::from(index)))))
    }

    fn decode_simd_shift_immediate(raw: u32, q: u32, u: u32) -> Result<DecodedInsn, DecodeError> {
        let immh = (raw >> 19) & 0xF;
        let size = 31 - immh.leading_zeros();
        let opcode = (raw >> 11) & 0x1F;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        let bits = 8 << size;
        let encoded = (raw >> 16) & 0x7F;
        let mnemonic = match (u, opcode) {
            (_, 0b00000) => Mnemonic::VSHR,
            (_, 0b00010) => Mnemonic::VSRA,
            (_, 0b00100) => Mnemonic::VRSHR,
            (_, 0b00110) => Mnemonic::VRSRA,
            (0, 0b01010) => Mnemonic::VSHL,
            _ => Mnemonic::UNKNOWN,
        };
        if size == 3 && q == 0 {
            return Ok(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }

        let amount = if opcode == 0b01010 {
            encoded - bits
        } else {
            2 * bits - encoded
        };
        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };
        let fp_reg = |num| {
            Operand::FpReg(FpRegister {
                num,
                size: vec_size,
            })
        };
        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn))
            .with_operand(Operand::Imm(Immediate::new(i64::from(amount)))))
    }

    /// Decode scalar FP three-source instructions.
    fn decode_scalar_fp_3source(raw: u32) -> Result<DecodedInsn, DecodeError> {
        let size = (raw >> 22) & 0x3;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let ra = ((raw >> 10) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        let o0 = (raw >> 15) & 1;
        let o1 = (raw >> 21) & 1;

        let Some(fp_size) = Self::scalar_fp_size(size) else {
            return Ok(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        };
        let fp_reg = |num| Operand::FpReg(FpRegister { num, size: fp_size });

        let mnemonic = match (o1, o0) {
            (0, 0) => Mnemonic::FMADD,
            (0, 1) => Mnemonic::FMSUB,
            (1, 0) => Mnemonic::FNMADD,
            (1, 1) => Mnemonic::FNMSUB,
            _ => Mnemonic::UNKNOWN,
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(fp_reg(rd))
            .with_operand(fp_reg(rn))
            .with_operand(fp_reg(rm))
            .with_operand(fp_reg(ra)))
    }

    /// Decode SIMD load/store instructions.
    pub(super) fn decode_simd_ldst(raw: u32, q: u32) -> Result<DecodedInsn, DecodeError> {
        let l = (raw >> 22) & 1; // 1=load, 0=store
        let opcode = (raw >> 12) & 0xF;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt = (raw & 0x1F) as u8;

        let vec_size = if q == 1 { FpRegSize::Q } else { FpRegSize::D };

        let mnemonic = if l == 1 {
            match opcode {
                0b0000 | 0b0010 => Mnemonic::VLD1,
                0b0100 | 0b0110 => Mnemonic::VLD2,
                0b0001 | 0b0011 => Mnemonic::VLD3,
                0b0101 | 0b0111 => Mnemonic::VLD4,
                _ => Mnemonic::VLDR,
            }
        } else {
            match opcode {
                0b0000 | 0b0010 => Mnemonic::VST1,
                0b0100 | 0b0110 => Mnemonic::VST2,
                0b0001 | 0b0011 => Mnemonic::VST3,
                0b0101 | 0b0111 => Mnemonic::VST4,
                _ => Mnemonic::VSTR,
            }
        };

        Ok(DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(Operand::FpReg(FpRegister {
                num: rt,
                size: vec_size,
            }))
            .with_operand(Operand::Mem(MemOperand::base(Register::x(rn)))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_halving_and_bit_test_three_same_decode() {
        let cases = [
            (0x4e22_0420, Mnemonic::VHADD),  // SHADD V0.16B, V1.16B, V2.16B
            (0x4e22_1420, Mnemonic::VRHADD), // SRHADD V0.16B, V1.16B, V2.16B
            (0x4e22_2420, Mnemonic::VHSUB),  // SHSUB V0.16B, V1.16B, V2.16B
            (0x4e22_8c20, Mnemonic::VTST),   // CMTST V0.16B, V1.16B, V2.16B
            (0x6e22_0420, Mnemonic::VHADD),  // UHADD V0.16B, V1.16B, V2.16B
            (0x6e22_1420, Mnemonic::VRHADD), // URHADD V0.16B, V1.16B, V2.16B
            (0x6e22_2420, Mnemonic::VHSUB),  // UHSUB V0.16B, V1.16B, V2.16B
        ];
        for (raw, expected) in cases {
            let insn = Aarch64Decoder::decode(raw).unwrap();
            assert_eq!(insn.mnemonic, expected, "raw={raw:#010x}");
        }
    }

    #[test]
    fn test_saturating_simd_three_same_integer_ops_decode() {
        let cases = [
            0x4e22_0c20, // SQADD V0.16B, V1.16B, V2.16B
            0x6e22_0c20, // UQADD V0.16B, V1.16B, V2.16B
        ];
        for raw in cases {
            let insn = Aarch64Decoder::decode(raw).unwrap();
            assert_eq!(insn.mnemonic, Mnemonic::VQADD, "raw={raw:#010x}");
        }
    }
}
