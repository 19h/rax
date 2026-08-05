//! EVEX packed sign/zero-extension memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Helper-backed native replay selected for one packed widening move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedExtendMemoryReplay {
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed sign/zero-extension memory encoding and replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedExtendMemoryEncoding {
    pub(crate) source_elem: VecElementType,
    pub(crate) destination_elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) source_width: VecWidth,
    pub(crate) lanes: u8,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) signed: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) replay: X86EvexPackedExtendMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) instruction_needs_avx512bw: bool,
}

impl X86EvexPackedExtendMemoryEncoding {
    /// The architectural source tuple may occupy only 2, 4, or 8 bytes, but
    /// x86 has no vector register narrower than XMM.
    pub(crate) fn transfer_width(self) -> VecWidth {
        match self.source_width {
            VecWidth::V64 => VecWidth::V128,
            width => width,
        }
    }

    pub(crate) fn memory_size(self) -> u32 {
        u32::from(self.lanes) * self.source_elem.bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedExtendFields {
    source_elem: VecElementType,
    destination_elem: VecElementType,
    width: VecWidth,
    source_width: VecWidth,
    lanes: u8,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    signed: bool,
    opcode: u8,
    w: bool,
    needs_avx512vl: bool,
    instruction_needs_avx512bw: bool,
}

fn packed_extend_shape(opcode: u8) -> Option<(VecElementType, VecElementType, bool)> {
    let signed = opcode < 0x30;
    let (source_elem, destination_elem) = match opcode & 0x0F {
        0x00 => (VecElementType::I8, VecElementType::I16),
        0x01 => (VecElementType::I8, VecElementType::I32),
        0x02 => (VecElementType::I8, VecElementType::I64),
        0x03 => (VecElementType::I16, VecElementType::I32),
        0x04 => (VecElementType::I16, VecElementType::I64),
        0x05 => (VecElementType::I32, VecElementType::I64),
        _ => return None,
    };
    matches!(opcode, 0x20..=0x25 | 0x30..=0x35).then_some((source_elem, destination_elem, signed))
}

fn exact_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=8 => VecWidth::V64,
        9..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
}

fn packed_extend_fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
) -> Option<PackedExtendFields> {
    let w = p1 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    let mask = p2 & 7;
    let zeroing = p2 & 0x80 != 0;
    let (source_elem, destination_elem, signed) = packed_extend_shape(opcode)?;
    if p0 & 7 != 2
        || p1 & 3 != 1
        || p1 & 0x78 != 0x78
        || p2 & 8 == 0
        || p2 & 0x10 != 0
        || ll == 3
        || (zeroing && mask == 0)
        || (matches!(opcode, 0x25 | 0x35) && w)
    {
        return None;
    }

    let width = match ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => unreachable!("reserved LL rejected"),
    };
    let lanes = width.lanes(destination_elem) as u8;
    let source_width = exact_width(u32::from(lanes) * source_elem.bytes());
    Some(PackedExtendFields {
        source_elem,
        destination_elem,
        width,
        source_width,
        lanes,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        writemask: (mask != 0).then_some(mask),
        zeroing,
        signed,
        opcode,
        w,
        needs_avx512vl: width != VecWidth::V512,
        instruction_needs_avx512bw: matches!(opcode, 0x20 | 0x30),
    })
}

fn register_packed_extend_fields(bytes: &[u8]) -> Option<PackedExtendFields> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    if p0 & 0x0F != 2 || p1 & 4 == 0 || modrm >> 6 != 3 {
        return None;
    }
    packed_extend_fields(*p0, *p1, *p2, *opcode, *modrm)
}

fn stack_packed_extend_fields(bytes: &[u8]) -> Option<PackedExtendFields> {
    let [0x62, p0, p1, p2, opcode, modrm, 0x24] = bytes else {
        return None;
    };
    if modrm & 0xC7 != 0x04 || memory_operand_end(bytes, 5)? != bytes.len() {
        return None;
    }
    packed_extend_fields(*p0, *p1, *p2, *opcode, *modrm)
}

impl X86InstructionBytes {
    /// Validate one of the twelve EVEX `VPMOVSX*`/`VPMOVZX*` memory forms and
    /// synthesize an exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 specifies half-, quarter-, and eighth-memory
    /// tuples with per-destination-lane writemask fault suppression. `vvvv/V'`
    /// and EVEX.b are reserved. Segment/address-size prefixes and APX B4/X4
    /// address channels remain confined to helper address evaluation.
    pub(crate) fn evex_packed_extend_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedExtendMemoryEncoding> {
        let bytes = self.as_slice();
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
        if modrm >> 6 == 3 || memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }
        let fields = packed_extend_fields(p0, p1, p2, opcode, modrm)?;

        let replay = if fields.writemask.is_some() {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve destination R/R' and map, select ordinary RSP,
                // and clear APX B4/X4 address channels.
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])?;
            if stack_packed_extend_fields(stack_instruction.as_slice()) != Some(fields) {
                return None;
            }
            X86EvexPackedExtendMemoryReplay::MaskedVector { stack_instruction }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != fields.destination)
                .expect("one destination leaves at least fifteen low scratch registers");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve destination R/R' and map, select scratch bits 4/3,
                // and remove APX B4 from the helper-owned address.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])?;
            if register_packed_extend_fields(register_instruction.as_slice()) != Some(fields) {
                return None;
            }
            X86EvexPackedExtendMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedExtendMemoryEncoding {
            source_elem: fields.source_elem,
            destination_elem: fields.destination_elem,
            width: fields.width,
            source_width: fields.source_width,
            lanes: fields.lanes,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            signed: fields.signed,
            opcode: fields.opcode,
            w: fields.w,
            replay,
            needs_avx512vl: fields.needs_avx512vl,
            instruction_needs_avx512bw: fields.instruction_needs_avx512bw,
        })
    }
}
