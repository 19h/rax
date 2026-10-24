//! Shared A32 conversion fields for decoding and direct execution.
use super::*;
#[derive(Clone, Copy, Debug)]
pub(crate) struct FixedConversionFields {
    pub(crate) mnemonic: Mnemonic,
    pub(crate) fp_bits: u32,
    pub(crate) integer_bits: u32,
    pub(crate) fractional_bits: u32,
    pub(crate) signed: bool,
    pub(crate) to_fixed: bool,
    pub(crate) d: u8,
    pub(crate) m: u8,
    pub(crate) regs: u8,
}
impl FixedConversionFields {
    fn tag(fp_bits: u32, integer_bits: u32, signed: bool, to_fixed: bool) -> Mnemonic {
        match (fp_bits, integer_bits, signed, to_fixed) {
            (16, 16, false, false) => Mnemonic::VCVT_F16_U16_FIXED,
            (16, 16, false, true) => Mnemonic::VCVT_U16_F16_FIXED,
            (16, 16, true, false) => Mnemonic::VCVT_F16_S16_FIXED,
            (16, 16, true, true) => Mnemonic::VCVT_S16_F16_FIXED,
            (16, 32, false, false) => Mnemonic::VCVT_F16_U32_FIXED,
            (16, 32, false, true) => Mnemonic::VCVT_U32_F16_FIXED,
            (16, 32, true, false) => Mnemonic::VCVT_F16_S32_FIXED,
            (16, 32, true, true) => Mnemonic::VCVT_S32_F16_FIXED,
            (32, 16, false, false) => Mnemonic::VCVT_F32_U16_FIXED,
            (32, 16, false, true) => Mnemonic::VCVT_U16_F32_FIXED,
            (32, 16, true, false) => Mnemonic::VCVT_F32_S16_FIXED,
            (32, 16, true, true) => Mnemonic::VCVT_S16_F32_FIXED,
            (32, 32, false, false) => Mnemonic::VCVT_F32_U32_FIXED,
            (32, 32, false, true) => Mnemonic::VCVT_U32_F32_FIXED,
            (32, 32, true, false) => Mnemonic::VCVT_F32_S32_FIXED,
            (32, 32, true, true) => Mnemonic::VCVT_S32_F32_FIXED,
            (64, 16, false, false) => Mnemonic::VCVT_F64_U16_FIXED,
            (64, 16, false, true) => Mnemonic::VCVT_U16_F64_FIXED,
            (64, 16, true, false) => Mnemonic::VCVT_F64_S16_FIXED,
            (64, 16, true, true) => Mnemonic::VCVT_S16_F64_FIXED,
            (64, 32, false, false) => Mnemonic::VCVT_F64_U32_FIXED,
            (64, 32, false, true) => Mnemonic::VCVT_U32_F64_FIXED,
            (64, 32, true, false) => Mnemonic::VCVT_F64_S32_FIXED,
            (64, 32, true, true) => Mnemonic::VCVT_S32_F64_FIXED,
            _ => Mnemonic::UNDEFINED,
        }
    }
}
impl Aarch32Decoder {
    pub(crate) fn scalar_fixed_fields(raw: u32) -> Option<FixedConversionFields> {
        if raw & 0x0fba0c50 != 0x0eba0840 {
            return None;
        }
        let fp_bits = match (raw >> 8) & 3 {
            1 => 16,
            2 => 32,
            3 => 64,
            _ => 0,
        };
        let integer_bits = if raw & (1 << 7) != 0 { 32 } else { 16 };
        let immediate = ((raw & 15) << 1) | ((raw >> 5) & 1);
        let valid = raw >> 28 != 15
            && fp_bits != 0
            && immediate <= integer_bits
            && (fp_bits != 16 || raw >> 28 == 14);
        let signed = raw & (1 << 16) == 0;
        let to_fixed = raw & (1 << 18) != 0;
        let d = if fp_bits == 64 {
            (((raw >> 22) & 1) << 4) | ((raw >> 12) & 15)
        } else {
            (((raw >> 12) & 15) << 1) | ((raw >> 22) & 1)
        } as u8;
        Some(FixedConversionFields {
            mnemonic: if valid {
                FixedConversionFields::tag(fp_bits, integer_bits, signed, to_fixed)
            } else {
                Mnemonic::UNDEFINED
            },
            fp_bits,
            integer_bits,
            fractional_bits: integer_bits.saturating_sub(immediate),
            signed,
            to_fixed,
            d,
            m: d,
            regs: 1,
        })
    }
    pub(crate) fn vector_fixed_fields(raw: u32) -> Option<FixedConversionFields> {
        let immediate = (raw >> 16) & 63;
        if raw >> 25 != 0b1111001
            || raw & (1 << 23) == 0
            || (raw >> 10) & 3 != 3
            || raw & (1 << 7) != 0
            || raw & (1 << 4) == 0
            || immediate < 8
        {
            return None;
        }
        let fp_bits = if raw & (1 << 9) != 0 { 32 } else { 16 };
        let integer_bits = fp_bits;
        let signed = raw & (1 << 24) == 0;
        let to_fixed = raw & (1 << 8) != 0;
        let q = raw & (1 << 6) != 0;
        let d = ((((raw >> 22) & 1) << 4) | ((raw >> 12) & 15)) as u8;
        let m = ((((raw >> 5) & 1) << 4) | (raw & 15)) as u8;
        let valid = immediate >= 64 - fp_bits && (!q || (d | m) & 1 == 0);
        Some(FixedConversionFields {
            mnemonic: if valid {
                FixedConversionFields::tag(fp_bits, integer_bits, signed, to_fixed)
            } else {
                Mnemonic::UNDEFINED
            },
            fp_bits,
            integer_bits,
            fractional_bits: 64 - immediate,
            signed,
            to_fixed,
            d,
            m,
            regs: if q { 2 } else { 1 },
        })
    }
    pub(crate) fn vector_integer_fields(raw: u32) -> Option<FixedConversionFields> {
        if raw >> 24 != 0xf3
            || raw & (1 << 23) == 0
            || raw & (1 << 21) == 0
            || raw & (1 << 20) == 0
            || (raw >> 16) & 3 != 3
            || (raw >> 9) & 7 != 3
            || raw & (1 << 4) != 0
        {
            return None;
        }
        let fp_bits = match (raw >> 18) & 3 {
            1 => 16,
            2 => 32,
            _ => 0,
        };
        let signed = raw & (1 << 7) == 0;
        let to_fixed = raw & (1 << 8) != 0;
        let q = raw & (1 << 6) != 0;
        let d = ((((raw >> 22) & 1) << 4) | ((raw >> 12) & 15)) as u8;
        let m = ((((raw >> 5) & 1) << 4) | (raw & 15)) as u8;
        let valid = fp_bits != 0 && (!q || (d | m) & 1 == 0);
        let mnemonic = if !valid {
            Mnemonic::UNDEFINED
        } else {
            match (fp_bits, signed, to_fixed) {
                (16, true, false) => Mnemonic::VCVT_F16_S16,
                (16, false, false) => Mnemonic::VCVT_F16_U16,
                (16, true, true) => Mnemonic::VCVT_S16_F16,
                (16, false, true) => Mnemonic::VCVT_U16_F16,
                (32, true, false) => Mnemonic::VCVT_F32_S32,
                (32, false, false) => Mnemonic::VCVT_F32_U32,
                (32, true, true) => Mnemonic::VCVT_S32_F32,
                (32, false, true) => Mnemonic::VCVT_U32_F32,
                _ => unreachable!(),
            }
        };
        Some(FixedConversionFields {
            mnemonic,
            fp_bits,
            integer_bits: fp_bits,
            fractional_bits: 0,
            signed,
            to_fixed,
            d,
            m,
            regs: if q { 2 } else { 1 },
        })
    }
}
