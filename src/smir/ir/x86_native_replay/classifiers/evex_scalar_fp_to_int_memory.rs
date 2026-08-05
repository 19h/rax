//! EVEX scalar floating-point-to-integer memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{FpRoundMode, MemWidth, OpWidth, VecElementType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexScalarFpToIntMemoryFields {
    destination: u8,
    elem: VecElementType,
    int_width: OpWidth,
    signed: bool,
    truncate: bool,
    map: u8,
    pp: u8,
    w: bool,
    ll: u8,
    opcode: u8,
    memory_width: MemWidth,
    needs_avx512fp16: bool,
}

/// Exact EVEX `VCVT{T}{SS,SD,SH}2{SI,USI}` scalar memory encoding and its
/// byte-validated XMM0-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFpToIntMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) elem: VecElementType,
    pub(crate) int_width: OpWidth,
    pub(crate) signed: bool,
    pub(crate) truncate: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) ll: u8,
    pub(crate) opcode: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

impl X86EvexScalarFpToIntMemoryEncoding {
    pub(crate) fn round(self) -> FpRoundMode {
        if self.truncate {
            FpRoundMode::RoundTowardZero
        } else {
            FpRoundMode::Dynamic
        }
    }
}

fn evex_scalar_fp_to_int_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, EvexScalarFpToIntMemoryFields)> {
    let start = vector_legacy_prefix_len(bytes);
    if bytes.get(start) != Some(&0x62) {
        return None;
    }

    let p0 = *bytes.get(start + 1)?;
    let p1 = *bytes.get(start + 2)?;
    let p2 = *bytes.get(start + 3)?;
    let opcode = *bytes.get(start + 4)?;
    let modrm_index = start + 5;
    let modrm = *bytes.get(modrm_index)?;
    let map = p0 & 0x07;
    let pp = p1 & 0x03;
    let (elem, memory_width, needs_avx512fp16) = match (map, pp) {
        (1, 2) => (VecElementType::F32, MemWidth::B4, false),
        (1, 3) => (VecElementType::F64, MemWidth::B8, false),
        (5, 2) => (VecElementType::F16, MemWidth::B2, true),
        _ => return None,
    };
    if !matches!(opcode, 0x2C | 0x2D | 0x78 | 0x79) {
        return None;
    }

    let ll = (p2 >> 5) & 3;
    if p1 & 0x78 != 0x78
        || p2 & 0x9F != 0x08
        || ll == 3
        || p0 & 0x10 == 0
        || modrm >> 6 == 3
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let w = p1 & 0x80 != 0;
    let destination = ((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3);
    Some((
        p0,
        p1,
        p2,
        modrm,
        EvexScalarFpToIntMemoryFields {
            destination,
            elem,
            int_width: if w { OpWidth::W64 } else { OpWidth::W32 },
            signed: matches!(opcode, 0x2C | 0x2D),
            truncate: matches!(opcode, 0x2C | 0x78),
            map,
            pp,
            w,
            ll,
            opcode,
            memory_width,
            needs_avx512fp16,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one complete EVEX scalar floating-point-to-integer memory
    /// source and synthesize an exact XMM0 register replay.
    ///
    /// Intel SDM revision 092 assigns these encodings Tuple1 Fixed/Scalar
    /// memory operands and Type E3NF exceptions. Memory forms reserve
    /// `EVEX.b`, `vvvv/V'`, and all writemask controls, use MXCSR dynamic
    /// rounding for `VCVT*`, force round-toward-zero for `VCVTT*`, and accept
    /// the three defined LLIG images. Segment, address-size, and APX B4/X4
    /// controls remain confined to precise helper address evaluation.
    pub(crate) fn evex_scalar_fp_to_int_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFpToIntMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = evex_scalar_fp_to_int_memory_fields(self.as_slice())?;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve the GPR R channel and reserved R' image, select
            // ordinary XMM0, and remove APX B4/X4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U after removing
            // the APX X4 address channel.
            p1 | 0x04,
            // Preserve the defined LLIG image and reserved V'/mask controls.
            p2,
            fields.opcode,
            0xC0 | (modrm & 0x38),
        ])?;
        if register_instruction.evex_register_scalar_fp_to_int_requires_fp16()
            != Some(fields.needs_avx512fp16)
        {
            return None;
        }

        Some(X86EvexScalarFpToIntMemoryEncoding {
            destination: fields.destination,
            elem: fields.elem,
            int_width: fields.int_width,
            signed: fields.signed,
            truncate: fields.truncate,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            ll: fields.ll,
            opcode: fields.opcode,
            memory_width: fields.memory_width,
            register_instruction,
            needs_avx512fp16: fields.needs_avx512fp16,
        })
    }
}
