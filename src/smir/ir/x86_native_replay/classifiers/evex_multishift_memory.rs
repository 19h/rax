//! Exact EVEX VPMULTISHIFTQB memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Native replay strategy for one exact EVEX VPMULTISHIFTQB memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexMultiShiftMemoryReplay {
    /// An unconditional complete-vector helper load followed by a
    /// register-source rewrite using one borrowed low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// An unconditional 8-byte helper load followed by the original
    /// m64-broadcast operation rewritten to consume the staged value at RSP.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX VPMULTISHIFTQB memory encoding and its byte-validated replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexMultiShiftMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) control: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexMultiShiftMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MultiShiftFields {
    width: VecWidth,
    destination: u8,
    control: u8,
    writemask: Option<u8>,
    zeroing: bool,
    broadcast: bool,
}

fn fields(p0: u8, p1: u8, p2: u8, opcode: u8, modrm: u8, memory: bool) -> Option<MultiShiftFields> {
    if p0 & 0x07 != 2
        || (!memory && p0 & 0x0F != 2)
        || p1 & 0x83 != 0x81
        || (!memory && p1 & 0x04 == 0)
        || opcode != 0x83
        || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        || (memory == (modrm >> 6 == 3))
        || (!memory && p2 & 0x10 != 0)
    {
        return None;
    }
    let width = match (p2 >> 5) & 3 {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    Some(MultiShiftFields {
        width,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        control: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        writemask: (p2 & 0x07 != 0).then_some(p2 & 0x07),
        zeroing: p2 & 0x80 != 0,
        broadcast: p2 & 0x10 != 0,
    })
}

fn register_fields(bytes: &[u8]) -> Option<(MultiShiftFields, u8)> {
    if bytes.len() != 6 || bytes[0] != 0x62 {
        return None;
    }
    let p0 = bytes[1];
    let classified = fields(p0, bytes[2], bytes[3], bytes[4], bytes[5], false)?;
    let source = (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (bytes[5] & 7);
    Some((classified, source))
}

fn memory_fields(bytes: &[u8]) -> Option<(MultiShiftFields, usize, usize)> {
    let start = vector_legacy_prefix_len(bytes);
    if bytes.get(start) != Some(&0x62) {
        return None;
    }
    let modrm_index = start + 5;
    if memory_operand_end(bytes, modrm_index)? != bytes.len() {
        return None;
    }
    let classified = fields(
        *bytes.get(start + 1)?,
        *bytes.get(start + 2)?,
        *bytes.get(start + 3)?,
        *bytes.get(start + 4)?,
        *bytes.get(modrm_index)?,
        true,
    )?;
    Some((classified, start, modrm_index))
}

impl X86InstructionBytes {
    /// Validate one EVEX VPMULTISHIFTQB full-vector or m64-broadcast memory
    /// source and select an exact helper-backed native replay.
    ///
    /// Intel assigns exception class E4NF: the complete 128/256/512-bit tuple
    /// or one 8-byte broadcast scalar is read unconditionally, irrespective
    /// of the destination writemask. Segment/address-size prefixes and APX
    /// B4/X4 address extensions remain confined to helper address evaluation.
    pub(crate) fn evex_multishift_memory_encoding(
        &self,
    ) -> Option<X86EvexMultiShiftMemoryEncoding> {
        let bytes = self.as_slice();
        let (classified, start, modrm_index) = memory_fields(bytes)?;
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[modrm_index];

        let replay = if classified.broadcast {
            let rewritten = [
                0x62,
                // Preserve R/R' and map 0F38, select unextended RSP, and
                // clear APX B4 because the rewritten address is architectural.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ];
            let stack_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            let (rewritten_fields, _, rewritten_modrm) =
                memory_fields(stack_instruction.as_slice())?;
            if rewritten_fields != classified
                || stack_instruction.as_slice()[rewritten_modrm] & 7 != 4
            {
                return None;
            }
            X86EvexMultiShiftMemoryReplay::Broadcast { stack_instruction }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| {
                    *candidate != classified.destination && *candidate != classified.control
                })
                .expect("two operands cannot consume every low vector register");
            let rewritten = [
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Scratch is low, so X is one; clear APX B4.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ];
            let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            if register_fields(register_instruction.as_slice()) != Some((classified, scratch)) {
                return None;
            }
            X86EvexMultiShiftMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexMultiShiftMemoryEncoding {
            width: classified.width,
            destination: classified.destination,
            control: classified.control,
            writemask: classified.writemask,
            zeroing: classified.zeroing,
            replay,
            memory_size: if classified.broadcast {
                8
            } else {
                classified.width.bytes()
            },
            needs_avx512vl: classified.width != VecWidth::V512,
        })
    }
}
