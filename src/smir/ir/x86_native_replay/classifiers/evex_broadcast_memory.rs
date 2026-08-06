//! EVEX memory-broadcast classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexBroadcastMemoryFields {
    width: VecWidth,
    elem: VecElementType,
    source_lanes: u8,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    opcode: u8,
    w: bool,
    needs_avx512bw: bool,
    needs_avx512dq: bool,
}

/// One exact EVEX memory-broadcast encoding and its byte-validated stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexBroadcastMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) source_lanes: u8,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) memory_size: u32,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512dq: bool,
}

fn width_from_ll(ll: u8) -> Option<VecWidth> {
    match ll {
        0 => Some(VecWidth::V128),
        1 => Some(VecWidth::V256),
        2 => Some(VecWidth::V512),
        _ => None,
    }
}

fn operation(opcode: u8, w: bool, width: VecWidth) -> Option<(VecElementType, u8, bool, bool)> {
    let at_least_256 = matches!(width, VecWidth::V256 | VecWidth::V512);
    let exactly_512 = width == VecWidth::V512;
    Some(match (opcode, w) {
        // Floating-point broadcasts.
        (0x18, false) => (VecElementType::F32, 1, false, false),
        (0x19, false) if at_least_256 => (VecElementType::F32, 2, false, true),
        (0x19, true) if at_least_256 => (VecElementType::F64, 1, false, false),
        (0x1A, false) if at_least_256 => (VecElementType::F32, 4, false, false),
        (0x1A, true) if at_least_256 => (VecElementType::F64, 2, false, true),
        (0x1B, false) if exactly_512 => (VecElementType::F32, 8, false, true),
        (0x1B, true) if exactly_512 => (VecElementType::F64, 4, false, false),

        // Integer broadcasts.
        (0x58, false) => (VecElementType::I32, 1, false, false),
        (0x59, false) => (VecElementType::I32, 2, false, true),
        (0x59, true) => (VecElementType::I64, 1, false, false),
        (0x5A, false) if at_least_256 => (VecElementType::I32, 4, false, false),
        (0x5A, true) if at_least_256 => (VecElementType::I64, 2, false, true),
        (0x5B, false) if exactly_512 => (VecElementType::I32, 8, false, true),
        (0x5B, true) if exactly_512 => (VecElementType::I64, 4, false, false),
        (0x78, false) => (VecElementType::I8, 1, true, false),
        (0x79, false) => (VecElementType::I16, 1, true, false),
        _ => return None,
    })
}

fn memory_fields(bytes: &[u8]) -> Option<(u8, u8, u8, u8, EvexBroadcastMemoryFields)> {
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
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let w = p1 & 0x80 != 0;
    let width = width_from_ll((p2 >> 5) & 3)?;
    let (elem, source_lanes, needs_avx512bw, needs_avx512dq) = operation(opcode, w, width)?;

    // EVEX map 0F38, 66H, reserved vvvv/V'=11111b, b=0, and a memory
    // ModR/M source are invariant across the complete family.  For memory
    // operands, payload-byte-1 bit 2 is the inverted APX X4 address extension,
    // not a fixed EVEX.U bit, so both values are valid here.
    if p0 & 0x07 != 2
        || p1 & 0x03 != 1
        || p1 & 0x78 != 0x78
        || p2 & 0x08 == 0
        || p2 & 0x10 != 0
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    Some((
        p0,
        p1,
        p2,
        modrm,
        EvexBroadcastMemoryFields {
            width,
            elem,
            source_lanes,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            writemask: (mask != 0).then_some(mask),
            zeroing,
            opcode,
            w,
            needs_avx512bw,
            needs_avx512dq,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one complete EVEX memory-broadcast instruction and rewrite
    /// only its memory operand to `[rsp]`.
    ///
    /// Intel SDM revision 092 defines Tuple1/Tuple2/Tuple4/Tuple8 source
    /// accesses of 1 through 32 bytes. The complete tuple is fault-suppressed
    /// only when every applicable destination-mask bit is zero. Segment,
    /// address-size, SIB/displacement, and APX B4/X4 fields remain confined to
    /// helper address evaluation; destination, vector length, W, and mask
    /// controls are retained exactly by the stack replay.
    pub(crate) fn evex_broadcast_memory_encoding(&self) -> Option<X86EvexBroadcastMemoryEncoding> {
        let bytes = self.as_slice();
        let (p0, p1, p2, modrm, fields) = memory_fields(bytes)?;
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F38. Select ordinary unextended RSP and
            // clear APX X4/B4 from the helper-owned guest address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/66 and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; EVEX.b was rejected.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, rewritten_p2, _, rewritten) = memory_fields(stack_instruction.as_slice())?;
        if rewritten != fields || rewritten_p2 != p2 {
            return None;
        }

        Some(X86EvexBroadcastMemoryEncoding {
            width: fields.width,
            elem: fields.elem,
            source_lanes: fields.source_lanes,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            opcode: fields.opcode,
            w: fields.w,
            memory_size: u32::from(fields.source_lanes) * fields.elem.bytes(),
            stack_instruction,
            needs_avx512vl: fields.width != VecWidth::V512,
            needs_avx512bw: fields.needs_avx512bw,
            needs_avx512dq: fields.needs_avx512dq,
        })
    }
}
