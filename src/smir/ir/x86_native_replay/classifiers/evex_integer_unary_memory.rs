//! EVEX unary packed-integer memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Unary packed-integer operation carried by one exact native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexIntegerUnaryMemoryKind {
    Conflict,
    LeadingZeros,
    Popcnt,
}

/// Native replay strategy for one exact unary packed-integer memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexIntegerUnaryMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// One scalar helper load followed by the original broadcast from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-required-lane helper loads followed by a nonbroadcast `[rsp]`
    /// replay. Clearing EVEX.b preserves independently materialized lane data.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerUnaryMemoryFields {
    kind: X86EvexIntegerUnaryMemoryKind,
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    opcode: u8,
    w: bool,
    broadcast: bool,
}

/// Exact EVEX unary packed-integer memory encoding and its byte-validated
/// helper-backed replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexIntegerUnaryMemoryEncoding {
    pub(crate) kind: X86EvexIntegerUnaryMemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) broadcast: bool,
    pub(crate) replay: X86EvexIntegerUnaryMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512cd: bool,
    pub(crate) needs_avx512bitalg: bool,
    pub(crate) needs_avx512vpopcntdq: bool,
}

fn operation(opcode: u8, w: bool) -> Option<(X86EvexIntegerUnaryMemoryKind, VecElementType, bool)> {
    Some(match (opcode, w) {
        (0xC4, false) => (
            X86EvexIntegerUnaryMemoryKind::Conflict,
            VecElementType::I32,
            true,
        ),
        (0xC4, true) => (
            X86EvexIntegerUnaryMemoryKind::Conflict,
            VecElementType::I64,
            true,
        ),
        (0x44, false) => (
            X86EvexIntegerUnaryMemoryKind::LeadingZeros,
            VecElementType::I32,
            true,
        ),
        (0x44, true) => (
            X86EvexIntegerUnaryMemoryKind::LeadingZeros,
            VecElementType::I64,
            true,
        ),
        (0x54, false) => (
            X86EvexIntegerUnaryMemoryKind::Popcnt,
            VecElementType::I8,
            false,
        ),
        (0x54, true) => (
            X86EvexIntegerUnaryMemoryKind::Popcnt,
            VecElementType::I16,
            false,
        ),
        (0x55, false) => (
            X86EvexIntegerUnaryMemoryKind::Popcnt,
            VecElementType::I32,
            true,
        ),
        (0x55, true) => (
            X86EvexIntegerUnaryMemoryKind::Popcnt,
            VecElementType::I64,
            true,
        ),
        _ => return None,
    })
}

fn integer_unary_memory_fields(bytes: &[u8]) -> Option<(u8, u8, u8, u8, IntegerUnaryMemoryFields)> {
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
    let w = p1 & 0x80 != 0;
    let (kind, elem, broadcast_allowed) = operation(opcode, w)?;
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let broadcast = p2 & 0x10 != 0;
    let ll = (p2 >> 5) & 3;
    if p0 & 0x07 != 2
        || p1 & 0x78 != 0x78
        || p1 & 0x03 != 1
        || p2 & 0x08 == 0
        || ll == 3
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || (broadcast && !broadcast_allowed)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
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
        IntegerUnaryMemoryFields {
            kind,
            width,
            elem,
            destination,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            opcode,
            w,
            broadcast,
        },
    ))
}

fn register_rewrite_matches(
    instruction: X86InstructionBytes,
    expected: IntegerUnaryMemoryFields,
    scratch: u8,
) -> bool {
    let [0x62, p0, p1, p2, opcode, modrm] = instruction.as_slice() else {
        return false;
    };
    let w = p1 & 0x80 != 0;
    let Some((kind, elem, _)) = operation(*opcode, w) else {
        return false;
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
        && p0 & 0x07 == 2
        && p1 & 0x78 == 0x78
        && p1 & 0x03 == 1
        && p2 & 0x08 != 0
        && p2 & 0x10 == 0
        && p2 & 0x87 == (u8::from(expected.zeroing) << 7) | expected.writemask.unwrap_or(0)
        && *opcode == expected.opcode
        && w == expected.w
        && modrm >> 6 == 3
}

impl X86InstructionBytes {
    /// Validate one EVEX `VPCONFLICTD/Q`, `VPLZCNTD/Q`, or
    /// `VPOPCNTB/W/D/Q` memory source and select an exact native replay.
    ///
    /// The SDM assigns these instructions Type E4/E4NF exception behavior.
    /// Full-vector masked forms therefore retain lane-granular helper accesses;
    /// conflict detection additionally requires every lower source lane needed
    /// by an active destination. Segment/address-size prefixes and APX B4/X4
    /// address extensions remain confined to helper address evaluation.
    pub(crate) fn evex_integer_unary_memory_encoding(
        &self,
    ) -> Option<X86EvexIntegerUnaryMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = integer_unary_memory_fields(self.as_slice())?;
        let needs_avx512vl = fields.width != VecWidth::V512;

        let stack_instruction = |clear_broadcast: bool| {
            let rewritten_p2 = if clear_broadcast { p2 & !0x10 } else { p2 };
            let instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map, select ordinary RSP, and clear APX B4.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                rewritten_p2,
                fields.opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])?;
            let (_, _, _, _, rewritten) = integer_unary_memory_fields(instruction.as_slice())?;
            let mut expected = fields;
            expected.broadcast &= !clear_broadcast;
            (rewritten == expected).then_some(instruction)
        };

        let replay = if fields.writemask.is_some() {
            // Preserve each independently materialized lane, including the
            // repeated guest accesses emitted for masked broadcasts.
            X86EvexIntegerUnaryMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(true)?,
            }
        } else if fields.broadcast {
            X86EvexIntegerUnaryMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(false)?,
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != fields.destination)
                .expect("one destination cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register X/B encode source bits 4/3 with inverted polarity.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                fields.opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])?;
            if !register_rewrite_matches(register_instruction, fields, scratch) {
                return None;
            }
            X86EvexIntegerUnaryMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexIntegerUnaryMemoryEncoding {
            kind: fields.kind,
            width: fields.width,
            elem: fields.elem,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            opcode: fields.opcode,
            w: fields.w,
            broadcast: fields.broadcast,
            replay,
            needs_avx512vl,
            needs_avx512cd: matches!(
                fields.kind,
                X86EvexIntegerUnaryMemoryKind::Conflict
                    | X86EvexIntegerUnaryMemoryKind::LeadingZeros
            ),
            needs_avx512bitalg: fields.kind == X86EvexIntegerUnaryMemoryKind::Popcnt
                && matches!(fields.elem, VecElementType::I8 | VecElementType::I16),
            needs_avx512vpopcntdq: fields.kind == X86EvexIntegerUnaryMemoryKind::Popcnt
                && matches!(fields.elem, VecElementType::I32 | VecElementType::I64),
        })
    }
}
