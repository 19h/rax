//! Scalar floating-point precision-conversion replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexScalarFpConvertMemoryFields {
    from: VecElementType,
    to: VecElementType,
    destination: u8,
    merge: u8,
    writemask: Option<u8>,
    zeroing: bool,
    map: u8,
    pp: u8,
    w: bool,
    opcode: u8,
    ll: u8,
    memory_width: MemWidth,
    needs_avx512fp16: bool,
}

/// Exact EVEX scalar floating-point precision-conversion memory encoding and
/// its byte-validated host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFpConvertMemoryEncoding {
    pub(crate) from: VecElementType,
    pub(crate) to: VecElementType,
    pub(crate) destination: u8,
    pub(crate) merge: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) ll: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn evex_scalar_fp_convert_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, EvexScalarFpConvertMemoryFields)> {
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
    let w = p1 & 0x80 != 0;
    let (from, to, needs_avx512fp16) = match (map, opcode, pp, w) {
        (1, 0x5A, 3, true) => (VecElementType::F64, VecElementType::F32, false),
        (1, 0x5A, 2, false) => (VecElementType::F32, VecElementType::F64, false),
        (5, 0x5A, 3, true) => (VecElementType::F64, VecElementType::F16, true),
        (5, 0x5A, 2, false) => (VecElementType::F16, VecElementType::F64, true),
        (5, 0x1D, 0, false) => (VecElementType::F32, VecElementType::F16, true),
        (6, 0x13, 0, false) => (VecElementType::F16, VecElementType::F32, true),
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    if p2 & 0x10 != 0
        || ll == 3
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let merge = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    let memory_width = match from {
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated scalar floating-point conversion source"),
    };
    Some((
        p0,
        p1,
        p2,
        modrm,
        EvexScalarFpConvertMemoryFields {
            from,
            to,
            destination,
            merge,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            map,
            pp,
            w,
            opcode,
            ll,
            memory_width,
            needs_avx512fp16,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX `VCVTSS2SD` or `VCVTSD2SS`
    /// instruction and return its architectural destination.
    ///
    /// Both forms use map 0F, opcode 5A, and consume `VEX.vvvv` as the
    /// upper-lane merge source. F3 selects binary32-to-binary64 and F2 selects
    /// binary64-to-binary32. `VEX.W` and register-form `VEX.X` are ignored.
    /// Intel documents `VEX.L=1` as generation-dependent unpredictable, so
    /// only `VEX.L=0` register forms are admitted. Memory and non-exact source
    /// byte strings fail closed.
    pub fn vex_scalar_fp_convert_destination_index(&self) -> Option<u8> {
        let (encoded_r, p1, opcode, modrm) = match self.as_slice() {
            &[0xC5, p1, opcode, modrm] => (p1 & 0x80 != 0, p1, opcode, modrm),
            &[0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (p0 & 0x80 != 0, p1, opcode, modrm),
            _ => return None,
        };
        if p1 & 0x04 != 0 || !matches!(p1 & 0x03, 2 | 3) || opcode != 0x5A || modrm >> 6 != 3 {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(!encoded_r) << 3))
    }

    /// Validate one register-only EVEX scalar floating-point precision
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVTSD2SS`, `VCVTSS2SD`, `VCVTSD2SH`,
    /// `VCVTSH2SD`, `VCVTSS2SH`, and `VCVTSH2SS`. Every family is LLIG.
    /// Register-source `EVEX.b=1` selects embedded rounding plus SAE for the
    /// narrowing forms and SAE for the exact widening forms. The register-
    /// source control makes all four L'L bit images defined; without it, LLIG
    /// accepts the three defined EVEX vector-length encodings. EVEX.vvvv/V'
    /// supplies the upper-lane merge source.
    ///
    /// Memory forms, malformed zeroing with k0, absent EVEX fixed-one, and
    /// every non-family map/opcode/prefix/W combination fail closed.
    pub fn evex_register_scalar_fp_convert_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_fp16 = match (map, opcode, pp, w) {
            // VCVTSD2SS and VCVTSS2SD.
            (1, 0x5A, 3, true) | (1, 0x5A, 2, false) => false,
            // VCVTSD2SH, VCVTSH2SD, VCVTSS2SH, and VCVTSH2SS respectively.
            (5, 0x5A, 3, true)
            | (5, 0x5A, 2, false)
            | (5, 0x1D, 0, false)
            | (6, 0x13, 0, false) => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if (zeroing && mask == 0) || (ll == 3 && !embedded_control) {
            return None;
        }
        Some(needs_fp16)
    }

    /// Validate one EVEX scalar binary16/binary32/binary64 precision
    /// conversion whose final source is memory and synthesize an exact `[rsp]`
    /// replay.
    ///
    /// Intel assigns these forms Tuple1 Scalar memory operands and Type E3
    /// exceptions. Memory always selects dynamic MXCSR behavior, so
    /// `EVEX.b=1` is reserved. Only writemask bit 0 controls the exact 2/4/8-
    /// byte access. The three defined LLIG images are retained byte-for-byte
    /// and do not require AVX-512VL. Segment/address-size prefixes and APX
    /// B4/X4 extensions remain exclusively in helper address evaluation.
    pub(crate) fn evex_scalar_fp_convert_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFpConvertMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = evex_scalar_fp_convert_memory_fields(self.as_slice())?;
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten_fields) =
            evex_scalar_fp_convert_memory_fields(stack_instruction.as_slice())?;
        if rewritten_fields != fields {
            return None;
        }

        Some(X86EvexScalarFpConvertMemoryEncoding {
            from: fields.from,
            to: fields.to,
            destination: fields.destination,
            merge: fields.merge,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            opcode: fields.opcode,
            ll: fields.ll,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512fp16: fields.needs_avx512fp16,
        })
    }
}
