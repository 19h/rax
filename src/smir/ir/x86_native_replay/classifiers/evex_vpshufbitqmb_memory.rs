//! EVEX `VPSHUFBITQMB` memory-source replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VpshufbitqmbFields {
    width: VecWidth,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
}

/// Native replay selected for one exact `VPSHUFBITQMB` memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexVpshufbitqmbMemoryReplay {
    /// Stage one complete Full Mem tuple in a low vector scratch register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// Stage only active source bytes in a zeroed stack vector and retain the
    /// architectural writemask on a `[rsp]` replay.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact `VPSHUFBITQMB` memory encoding and byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexVpshufbitqmbMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) replay: X86EvexVpshufbitqmbMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

fn fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
    memory: bool,
) -> Option<VpshufbitqmbFields> {
    let map = if memory { p0 & 0x07 } else { p0 & 0x0F };
    if map != 2
        // ModR/M.reg addresses K0-K7, so both destination extensions are zero.
        || p0 & 0x90 != 0x90
        // W=0 and pp=66. P1.bit2 may carry APX X4 only for a memory address.
        || p1 & 0x83 != 1
        || (!memory && p1 & 0x04 == 0)
        // EVEX.z and EVEX.b are reserved; aaa selects an optional writemask.
        || p2 & 0x90 != 0
        || opcode != 0x8F
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
    let mask = p2 & 7;
    Some(VpshufbitqmbFields {
        width,
        destination: (modrm >> 3) & 7,
        source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        writemask: (mask != 0).then_some(mask),
    })
}

fn register_fields(bytes: &[u8]) -> Option<(VpshufbitqmbFields, u8)> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    let classified = fields(*p0, *p1, *p2, *opcode, *modrm, false)?;
    let source2 = (modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4);
    Some((classified, source2))
}

impl X86InstructionBytes {
    /// Validate one Full-Mem `VPSHUFBITQMB` memory source and construct an
    /// exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 specifies byte-granular writemask fault
    /// suppression. Unmasked tuples therefore use one complete vector helper
    /// load, while masked tuples use ascending one-byte helper loads for only
    /// active output bits. Segment/address-size prefixes and APX B4/X4 address
    /// controls remain confined to helper address evaluation.
    pub(crate) fn evex_vpshufbitqmb_memory_encoding(
        &self,
    ) -> Option<X86EvexVpshufbitqmbMemoryEncoding> {
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

        let replay = if classified.writemask.is_some() {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Select ordinary RSP/SIB and remove helper-owned APX B4.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/66 and remove helper-owned APX X4.
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])?;
            let stack = stack_instruction.as_slice();
            if fields(stack[1], stack[2], stack[3], stack[4], stack[5], true)? != classified
                || memory_operand_end(stack, 5)? != stack.len()
            {
                return None;
            }
            X86EvexVpshufbitqmbMemoryReplay::MaskedVector { stack_instruction }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != classified.source1)
                .expect("one source cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register X/B encode scratch bits 4/3 with inverted polarity;
                // remove address-only APX controls and retain reserved R/R'.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])?;
            let (register, source2) = register_fields(register_instruction.as_slice())?;
            if register != classified || source2 != scratch {
                return None;
            }
            X86EvexVpshufbitqmbMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        Some(X86EvexVpshufbitqmbMemoryEncoding {
            width: classified.width,
            destination: classified.destination,
            source1: classified.source1,
            writemask: classified.writemask,
            replay,
            needs_avx512vl: classified.width != VecWidth::V512,
        })
    }
}
