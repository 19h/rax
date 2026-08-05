//! EVEX scalar integer-to-floating-point memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, OpWidth, VecElementType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexScalarIntToFpMemoryFields {
    destination: u8,
    merge: u8,
    elem: VecElementType,
    int_width: OpWidth,
    signed: bool,
    map: u8,
    pp: u8,
    w: bool,
    ll: u8,
    opcode: u8,
    memory_width: MemWidth,
    needs_avx512fp16: bool,
}

/// Exact EVEX `VCVT{,U}SI2{SS,SD,SH}` scalar memory encoding and its
/// byte-validated RAX-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarIntToFpMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) merge: u8,
    pub(crate) elem: VecElementType,
    pub(crate) int_width: OpWidth,
    pub(crate) signed: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) ll: u8,
    pub(crate) opcode: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn evex_scalar_int_to_fp_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, EvexScalarIntToFpMemoryFields)> {
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
    let (elem, needs_avx512fp16) = match (map, pp) {
        (1, 2) => (VecElementType::F32, false),
        (1, 3) => (VecElementType::F64, false),
        (5, 2) => (VecElementType::F16, true),
        _ => return None,
    };
    if !matches!(opcode, 0x2A | 0x7B) {
        return None;
    }

    let ll = (p2 >> 5) & 3;
    if p2 & 0x97 != 0
        || ll == 3
        || modrm >> 6 == 3
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let w = p1 & 0x80 != 0;
    let destination =
        ((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4);
    let merge = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    Some((
        p0,
        p1,
        p2,
        modrm,
        EvexScalarIntToFpMemoryFields {
            destination,
            merge,
            elem,
            int_width: if w { OpWidth::W64 } else { OpWidth::W32 },
            signed: opcode == 0x2A,
            map,
            pp,
            w,
            ll,
            opcode,
            memory_width: if w { MemWidth::B8 } else { MemWidth::B4 },
            needs_avx512fp16,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one complete EVEX scalar integer-to-floating-point memory
    /// source and synthesize an exact RAX/EAX register replay.
    ///
    /// Intel SDM revision 092 assigns these encodings Tuple1 Scalar memory
    /// operands and Type E3NF/E10NF exceptions. Memory forms reserve `EVEX.b`
    /// and all writemask controls, use dynamic MXCSR rounding, and accept the
    /// three defined LLIG images. W selects a 4- or 8-byte source. Segment,
    /// address-size, and APX B4/X4 controls remain confined to helper address
    /// evaluation; replay always reads the helper value from RAX/EAX.
    pub(crate) fn evex_scalar_int_to_fp_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarIntToFpMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = evex_scalar_int_to_fp_memory_fields(self.as_slice())?;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map, select ordinary unextended RAX, and
            // remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve the defined LLIG and V' merge-register image.
            p2,
            fields.opcode,
            0xC0 | (modrm & 0x38),
        ])?;
        if register_instruction.evex_register_scalar_int_to_fp_requires_fp16()
            != Some(fields.needs_avx512fp16)
        {
            return None;
        }

        Some(X86EvexScalarIntToFpMemoryEncoding {
            destination: fields.destination,
            merge: fields.merge,
            elem: fields.elem,
            int_width: fields.int_width,
            signed: fields.signed,
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
