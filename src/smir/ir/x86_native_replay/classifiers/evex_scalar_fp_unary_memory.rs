//! EVEX scalar floating-point unary memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

/// Scalar unary operation carried by one exact helper-backed replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexScalarFpUnaryMemoryKind {
    GetExponent,
    GetMantissa,
    RoundScale,
    Reduce,
    Recip14,
    Rsqrt14,
    RecipFp16,
    RsqrtFp16,
    Recip28,
    Rsqrt28,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpUnaryMemoryFields {
    kind: X86EvexScalarFpUnaryMemoryKind,
    elem: VecElementType,
    destination: u8,
    merge: u8,
    writemask: Option<u8>,
    zeroing: bool,
    map: u8,
    pp: u8,
    w: bool,
    opcode: u8,
    ll: u8,
    immediate: Option<u8>,
    memory_width: MemWidth,
}

/// Exact scalar special or approximate floating-point memory encoding and its
/// byte-validated host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFpUnaryMemoryEncoding {
    pub(crate) kind: X86EvexScalarFpUnaryMemoryKind,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) merge: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) ll: u8,
    pub(crate) immediate: Option<u8>,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512er: bool,
    pub(crate) needs_avx512fp16: bool,
}

fn scalar_fp_unary_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, ScalarFpUnaryMemoryFields)> {
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
    let operand_end = memory_operand_end(bytes, modrm_index)?;
    let map = p0 & 0x07;
    let pp = p1 & 0x03;
    let w = p1 & 0x80 != 0;
    let (kind, elem, has_immediate) = match (map, opcode, pp, w) {
        (6, 0x43, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::GetExponent,
            VecElementType::F16,
            false,
        ),
        (2, 0x43, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::GetExponent,
            VecElementType::F32,
            false,
        ),
        (2, 0x43, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::GetExponent,
            VecElementType::F64,
            false,
        ),
        (3, 0x27, 0, false) => (
            X86EvexScalarFpUnaryMemoryKind::GetMantissa,
            VecElementType::F16,
            true,
        ),
        (3, 0x27, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::GetMantissa,
            VecElementType::F32,
            true,
        ),
        (3, 0x27, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::GetMantissa,
            VecElementType::F64,
            true,
        ),
        (3, 0x0A, 0, false) => (
            X86EvexScalarFpUnaryMemoryKind::RoundScale,
            VecElementType::F16,
            true,
        ),
        (3, 0x0A, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::RoundScale,
            VecElementType::F32,
            true,
        ),
        (3, 0x0B, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::RoundScale,
            VecElementType::F64,
            true,
        ),
        (3, 0x57, 0, false) => (
            X86EvexScalarFpUnaryMemoryKind::Reduce,
            VecElementType::F16,
            true,
        ),
        (3, 0x57, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::Reduce,
            VecElementType::F32,
            true,
        ),
        (3, 0x57, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::Reduce,
            VecElementType::F64,
            true,
        ),
        (2, 0x4D, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::Recip14,
            VecElementType::F32,
            false,
        ),
        (2, 0x4D, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::Recip14,
            VecElementType::F64,
            false,
        ),
        (2, 0x4F, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::Rsqrt14,
            VecElementType::F32,
            false,
        ),
        (2, 0x4F, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::Rsqrt14,
            VecElementType::F64,
            false,
        ),
        (6, 0x4D, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::RecipFp16,
            VecElementType::F16,
            false,
        ),
        (6, 0x4F, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::RsqrtFp16,
            VecElementType::F16,
            false,
        ),
        (2, 0xCB, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::Recip28,
            VecElementType::F32,
            false,
        ),
        (2, 0xCB, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::Recip28,
            VecElementType::F64,
            false,
        ),
        (2, 0xCD, 1, false) => (
            X86EvexScalarFpUnaryMemoryKind::Rsqrt28,
            VecElementType::F32,
            false,
        ),
        (2, 0xCD, 1, true) => (
            X86EvexScalarFpUnaryMemoryKind::Rsqrt28,
            VecElementType::F64,
            false,
        ),
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    let expected_end = operand_end + usize::from(has_immediate);
    if p2 & 0x10 != 0
        || ll == 3
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || expected_end != bytes.len()
    {
        return None;
    }
    let immediate = has_immediate.then(|| bytes[operand_end]);
    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let merge = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    let memory_width = match elem {
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated scalar unary floating-point element"),
    };
    Some((
        p0,
        p1,
        p2,
        modrm,
        ScalarFpUnaryMemoryFields {
            kind,
            elem,
            destination,
            merge,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            map,
            pp,
            w,
            opcode,
            ll,
            immediate,
            memory_width,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one scalar special or approximate floating-point memory
    /// source and synthesize its exact `[rsp]` replay.
    ///
    /// Intel assigns every owned form a Tuple1 Scalar operand. Only writemask
    /// bit 0 controls the exact 2/4/8-byte access; exception classes remain
    /// instruction-specific and are preserved by replaying the original opcode.
    /// Memory-source `EVEX.b=1` and `L'L=11B` are reserved; the other three
    /// LLIG images are retained exactly and do not require AVX-512VL. SAE for
    /// VRCP28/VRSQRT28 is register-register only under the common EVEX rules.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// exclusively in helper address evaluation. The unconstrained immediate
    /// is preserved for the nine immediate-control forms.
    pub(crate) fn evex_scalar_fp_unary_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFpUnaryMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = scalar_fp_unary_memory_fields(self.as_slice())?;
        let mut rewritten = [0u8; 8];
        rewritten[..7].copy_from_slice(&[
            0x62,
            // Preserve R/R' and the map, select ordinary unextended SIB
            // index/base, and clear APX B4 for the RSP rewrite.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, LLIG, V', and aaa; b was validated clear.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ]);
        if let Some(immediate) = fields.immediate {
            rewritten[7] = immediate;
        }
        let stack_instruction =
            X86InstructionBytes::new(&rewritten[..7 + usize::from(fields.immediate.is_some())])?;
        let (_, _, _, _, rewritten_fields) =
            scalar_fp_unary_memory_fields(stack_instruction.as_slice())?;
        if rewritten_fields != fields {
            return None;
        }

        Some(X86EvexScalarFpUnaryMemoryEncoding {
            kind: fields.kind,
            elem: fields.elem,
            destination: fields.destination,
            merge: fields.merge,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            opcode: fields.opcode,
            ll: fields.ll,
            immediate: fields.immediate,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512dq: fields.kind == X86EvexScalarFpUnaryMemoryKind::Reduce
                && fields.elem != VecElementType::F16,
            needs_avx512er: matches!(
                fields.kind,
                X86EvexScalarFpUnaryMemoryKind::Recip28 | X86EvexScalarFpUnaryMemoryKind::Rsqrt28
            ),
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
