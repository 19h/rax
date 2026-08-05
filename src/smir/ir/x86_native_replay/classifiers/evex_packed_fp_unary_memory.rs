//! EVEX packed unary floating-point memory classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Packed unary floating-point operation carried by one exact native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFpUnaryMemoryKind {
    Sqrt,
    GetExponent,
    GetMantissa,
    RoundScale,
    Reduce,
    Recip14,
    Rsqrt14,
    RecipFp16,
    RsqrtFp16,
}

/// Native replay strategy for one exact packed unary floating-point memory
/// encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFpUnaryMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane helper loads accumulated on the stack before replay.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedFpUnaryMemoryFields {
    kind: X86EvexPackedFpUnaryMemoryKind,
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    map: u8,
    pp: u8,
    w: bool,
    opcode: u8,
    broadcast: bool,
    immediate: Option<u8>,
}

/// Exact EVEX packed unary floating-point memory encoding and its
/// byte-validated helper-backed replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFpUnaryMemoryEncoding {
    pub(crate) kind: X86EvexPackedFpUnaryMemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) immediate: Option<u8>,
    pub(crate) replay: X86EvexPackedFpUnaryMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512fp16: bool,
}

fn operation(
    map: u8,
    pp: u8,
    opcode: u8,
    w: bool,
) -> Option<(X86EvexPackedFpUnaryMemoryKind, VecElementType, bool)> {
    Some(match (map, pp, opcode, w) {
        (1, 0, 0x51, false) => (
            X86EvexPackedFpUnaryMemoryKind::Sqrt,
            VecElementType::F32,
            false,
        ),
        (1, 1, 0x51, true) => (
            X86EvexPackedFpUnaryMemoryKind::Sqrt,
            VecElementType::F64,
            false,
        ),
        (5, 0, 0x51, false) => (
            X86EvexPackedFpUnaryMemoryKind::Sqrt,
            VecElementType::F16,
            false,
        ),
        (2, 1, 0x42, false) => (
            X86EvexPackedFpUnaryMemoryKind::GetExponent,
            VecElementType::F32,
            false,
        ),
        (2, 1, 0x42, true) => (
            X86EvexPackedFpUnaryMemoryKind::GetExponent,
            VecElementType::F64,
            false,
        ),
        (6, 1, 0x42, false) => (
            X86EvexPackedFpUnaryMemoryKind::GetExponent,
            VecElementType::F16,
            false,
        ),
        (3, 0, 0x26, false) => (
            X86EvexPackedFpUnaryMemoryKind::GetMantissa,
            VecElementType::F16,
            true,
        ),
        (3, 1, 0x26, false) => (
            X86EvexPackedFpUnaryMemoryKind::GetMantissa,
            VecElementType::F32,
            true,
        ),
        (3, 1, 0x26, true) => (
            X86EvexPackedFpUnaryMemoryKind::GetMantissa,
            VecElementType::F64,
            true,
        ),
        (3, 0, 0x08, false) => (
            X86EvexPackedFpUnaryMemoryKind::RoundScale,
            VecElementType::F16,
            true,
        ),
        (3, 1, 0x08, false) => (
            X86EvexPackedFpUnaryMemoryKind::RoundScale,
            VecElementType::F32,
            true,
        ),
        (3, 1, 0x09, true) => (
            X86EvexPackedFpUnaryMemoryKind::RoundScale,
            VecElementType::F64,
            true,
        ),
        (3, 0, 0x56, false) => (
            X86EvexPackedFpUnaryMemoryKind::Reduce,
            VecElementType::F16,
            true,
        ),
        (3, 1, 0x56, false) => (
            X86EvexPackedFpUnaryMemoryKind::Reduce,
            VecElementType::F32,
            true,
        ),
        (3, 1, 0x56, true) => (
            X86EvexPackedFpUnaryMemoryKind::Reduce,
            VecElementType::F64,
            true,
        ),
        (2, 1, 0x4C, false) => (
            X86EvexPackedFpUnaryMemoryKind::Recip14,
            VecElementType::F32,
            false,
        ),
        (2, 1, 0x4C, true) => (
            X86EvexPackedFpUnaryMemoryKind::Recip14,
            VecElementType::F64,
            false,
        ),
        (2, 1, 0x4E, false) => (
            X86EvexPackedFpUnaryMemoryKind::Rsqrt14,
            VecElementType::F32,
            false,
        ),
        (2, 1, 0x4E, true) => (
            X86EvexPackedFpUnaryMemoryKind::Rsqrt14,
            VecElementType::F64,
            false,
        ),
        (6, 1, 0x4C, false) => (
            X86EvexPackedFpUnaryMemoryKind::RecipFp16,
            VecElementType::F16,
            false,
        ),
        (6, 1, 0x4E, false) => (
            X86EvexPackedFpUnaryMemoryKind::RsqrtFp16,
            VecElementType::F16,
            false,
        ),
        _ => return None,
    })
}

fn packed_fp_unary_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, PackedFpUnaryMemoryFields)> {
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
    let (kind, elem, has_immediate) = operation(map, pp, opcode, w)?;
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    let operand_end = memory_operand_end(bytes, modrm_index)?;
    if p1 & 0x78 != 0x78
        || p2 & 0x08 == 0
        || ll == 3
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || operand_end + usize::from(has_immediate) != bytes.len()
    {
        return None;
    }
    let width = match ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => unreachable!("reserved EVEX vector length rejected"),
    };
    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    Some((
        p0,
        p1,
        p2,
        modrm,
        PackedFpUnaryMemoryFields {
            kind,
            width,
            elem,
            destination,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            map,
            pp,
            w,
            opcode,
            broadcast: p2 & 0x10 != 0,
            immediate: has_immediate.then(|| bytes[operand_end]),
        },
    ))
}

fn register_rewrite_matches(
    instruction: X86InstructionBytes,
    expected: PackedFpUnaryMemoryFields,
    scratch: u8,
) -> bool {
    let bytes = instruction.as_slice();
    let [0x62, p0, p1, p2, opcode, modrm, rest @ ..] = bytes else {
        return false;
    };
    let map = p0 & 0x07;
    let pp = p1 & 0x03;
    let w = p1 & 0x80 != 0;
    let Some((kind, elem, has_immediate)) = operation(map, pp, *opcode, w) else {
        return false;
    };
    let immediate = match (has_immediate, rest) {
        (false, []) => None,
        (true, [immediate]) => Some(*immediate),
        _ => return false,
    };
    let ll = (p2 >> 5) & 3;
    let width = match ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return false,
    };
    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let source = (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (modrm & 7);
    kind == expected.kind
        && elem == expected.elem
        && width == expected.width
        && destination == expected.destination
        && source == scratch
        && p1 & 0x78 == 0x78
        && pp == expected.pp
        && p2 & 0x08 != 0
        && p2 & 0x10 == 0
        && p2 & 0x87 == (u8::from(expected.zeroing) << 7) | expected.writemask.unwrap_or(0)
        && map == expected.map
        && w == expected.w
        && *opcode == expected.opcode
        && immediate == expected.immediate
        && modrm >> 6 == 3
        && (expected.kind != X86EvexPackedFpUnaryMemoryKind::Sqrt
            || instruction.evex_register_fp_sqrt_requirements()
                == Some((
                    expected.width != VecWidth::V512,
                    expected.elem == VecElementType::F16,
                )))
}

impl X86InstructionBytes {
    /// Validate one EVEX packed `VSQRT`, `VGETEXP`, `VGETMANT`, `VRNDSCALE`,
    /// `VREDUCE`, `VRCP14`, `VRSQRT14`, `VRCPPH`, or `VRSQRTPH` memory source
    /// and select an exact native replay.
    ///
    /// Intel SDM Vol. 2 assigns every owned encoding a Full tuple. Inactive
    /// writemask lanes suppress their corresponding 2/4/8-byte accesses;
    /// memory-source `EVEX.b=1` selects a one-element broadcast and never SAE.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_packed_fp_unary_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFpUnaryMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = packed_fp_unary_memory_fields(self.as_slice())?;
        let needs_avx512vl = fields.width != VecWidth::V512;
        let stack_instruction = || {
            let mut bytes = [0u8; 8];
            bytes[..7].copy_from_slice(&[
                0x62,
                // Preserve R/R' and map, select ordinary RSP, and clear APX B4.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                fields.opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ]);
            if let Some(immediate) = fields.immediate {
                bytes[7] = immediate;
            }
            let instruction =
                X86InstructionBytes::new(&bytes[..7 + usize::from(fields.immediate.is_some())])?;
            let (_, _, _, _, rewritten) = packed_fp_unary_memory_fields(instruction.as_slice())?;
            (rewritten == fields).then_some(instruction)
        };

        let replay = if fields.broadcast {
            X86EvexPackedFpUnaryMemoryReplay::Broadcast {
                stack_instruction: stack_instruction()?,
            }
        } else if fields.writemask.is_some() {
            X86EvexPackedFpUnaryMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction()?,
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != fields.destination)
                .expect("one destination cannot consume every low vector register");
            let mut bytes = [0u8; 7];
            bytes[..6].copy_from_slice(&[
                0x62,
                // Register X/B encode source bits 4/3 with inverted polarity.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                fields.opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ]);
            if let Some(immediate) = fields.immediate {
                bytes[6] = immediate;
            }
            let register_instruction =
                X86InstructionBytes::new(&bytes[..6 + usize::from(fields.immediate.is_some())])?;
            if !register_rewrite_matches(register_instruction, fields, scratch) {
                return None;
            }
            X86EvexPackedFpUnaryMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFpUnaryMemoryEncoding {
            kind: fields.kind,
            width: fields.width,
            elem: fields.elem,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            opcode: fields.opcode,
            immediate: fields.immediate,
            replay,
            needs_avx512vl,
            needs_avx512dq: fields.kind == X86EvexPackedFpUnaryMemoryKind::Reduce
                && fields.elem != VecElementType::F16,
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
