//! EVEX VP2INTERSECTD/Q replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vp2IntersectFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    broadcast: bool,
}

/// Native replay selected for one exact VP2INTERSECTD/Q memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexVp2IntersectMemoryReplay {
    /// Stage one complete Full Mem tuple in a low vector scratch register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// Stage one scalar tuple and execute the original `{1toN}` form on `[rsp]`.
    Broadcast {
        memory_width: MemWidth,
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact VP2INTERSECTD/Q memory encoding and byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexVp2IntersectMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination_base: u8,
    pub(crate) source1: u8,
    pub(crate) replay: X86EvexVp2IntersectMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

fn fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
    memory: bool,
) -> Option<Vp2IntersectFields> {
    let map = if memory { p0 & 0x07 } else { p0 & 0x0F };
    if map != 2
        // ModR/M.reg addresses K0-K7, so both destination extensions are zero.
        || p0 & 0x90 != 0x90
        || p1 & 0x03 != 3
        || (!memory && p1 & 0x04 == 0)
        // z and aaa are reserved; b is the memory-only broadcast control.
        || p2 & 0x87 != 0
        || (!memory && p2 & 0x10 != 0)
        || opcode != 0x68
        || (memory == (modrm >> 6 == 3))
    {
        return None;
    }
    let width = match (p2 >> 5) & 3 {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    Some(Vp2IntersectFields {
        width,
        elem: if p1 & 0x80 == 0 {
            VecElementType::I32
        } else {
            VecElementType::I64
        },
        destination: (modrm >> 3) & 7,
        source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        broadcast: memory && p2 & 0x10 != 0,
    })
}

fn register_fields(bytes: &[u8]) -> Option<(Vp2IntersectFields, u8)> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    let classified = fields(*p0, *p1, *p2, *opcode, *modrm, false)?;
    let source2 = (modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4);
    Some((classified, source2))
}

impl X86InstructionBytes {
    /// Validate one register-only EVEX VP2INTERSECTD/Q instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and AVX512_VP2INTERSECT.
    ///
    /// ModR/M.reg addresses K0-K7 and therefore both EVEX destination-extension
    /// bits are reserved. W selects dword or qword elements. Memory sources,
    /// masking, zeroing, EVEX.b, reserved vector lengths, malformed fixed
    /// fields, and incomplete or trailing bytes fail closed.
    pub fn evex_register_vp2intersect_needs_vl(&self) -> Option<bool> {
        let (classified, _) = register_fields(self.as_slice())?;
        Some(classified.width != VecWidth::V512)
    }

    /// Validate one Type-E4NF VP2INTERSECTD/Q full-vector or scalar-broadcast
    /// memory source and construct an exact helper-backed native replay.
    ///
    /// The memory read is unconditional and completes before either member of
    /// the K-register destination pair is committed. Segment/address-size
    /// prefixes and APX B4/X4 controls remain confined to helper evaluation.
    pub(crate) fn evex_vp2intersect_memory_encoding(
        &self,
    ) -> Option<X86EvexVp2IntersectMemoryEncoding> {
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
        if memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }
        let classified = fields(p0, p1, p2, opcode, modrm, true)?;

        let replay = if classified.broadcast {
            let memory_width = match classified.elem {
                VecElementType::I32 => MemWidth::B4,
                VecElementType::I64 => MemWidth::B8,
                _ => unreachable!("validated VP2INTERSECT element width"),
            };
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve reserved R/R' and map, select ordinary RSP/SIB,
                // and remove the helper-owned APX B4 address extension.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/F2 and remove APX X4 from the stack address.
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])?;
            let stack = stack_instruction.as_slice();
            let stack_fields = fields(stack[1], stack[2], stack[3], stack[4], stack[5], true)?;
            if stack_fields != classified || memory_operand_end(stack, 5)? != stack.len() {
                return None;
            }
            X86EvexVp2IntersectMemoryReplay::Broadcast {
                memory_width,
                stack_instruction,
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != classified.source1)
                .expect("one source cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register X/B encode scratch bits 4/3 with inverted polarity;
                // clear address-only APX B4 and preserve reserved R/R'.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2 & !0x10,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])?;
            let (register, source2) = register_fields(register_instruction.as_slice())?;
            if register != classified || source2 != scratch {
                return None;
            }
            X86EvexVp2IntersectMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexVp2IntersectMemoryReplay::Vector { .. } => classified.width.bytes(),
            X86EvexVp2IntersectMemoryReplay::Broadcast { memory_width, .. } => memory_width.bytes(),
        };
        Some(X86EvexVp2IntersectMemoryEncoding {
            width: classified.width,
            elem: classified.elem,
            destination_base: classified.destination & !1,
            source1: classified.source1,
            replay,
            memory_size,
            needs_avx512vl: classified.width != VecWidth::V512,
        })
    }
}
