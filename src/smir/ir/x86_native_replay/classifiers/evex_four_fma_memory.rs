//! EVEX 4FMAPS whole-tuple memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FourFmaMemoryFields {
    scalar: bool,
    negate_product: bool,
    destination: u8,
    source_index: u8,
    writemask: Option<u8>,
    zeroing: bool,
    ll: u8,
    opcode: u8,
}

/// Exact EVEX `V4FMADDPS`/`V4FNMADDPS`/`V4FMADDSS`/`V4FNMADDSS`
/// Tuple1_4X memory encoding and its byte-validated host-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFourFmaMemoryEncoding {
    pub(crate) scalar: bool,
    pub(crate) negate_product: bool,
    pub(crate) destination: u8,
    pub(crate) source_index: u8,
    pub(crate) source_base: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) ll: u8,
    pub(crate) opcode: u8,
    pub(crate) stack_instruction: X86InstructionBytes,
}

fn four_fma_memory_fields(bytes: &[u8]) -> Option<(u8, u8, u8, u8, FourFmaMemoryFields)> {
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
    let scalar = matches!(opcode, 0x9B | 0xAB);
    let negate_product = matches!(opcode, 0xAA | 0xAB);
    if !matches!(opcode, 0x9A | 0x9B | 0xAA | 0xAB) {
        return None;
    }
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    if p0 & 0x07 != 2
        || p1 & 0x83 != 0x03
        || p2 & 0x10 != 0
        || ll == 3
        || (!scalar && ll != 2)
        || (zeroing && mask == 0)
        || modrm >> 6 == 3
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let destination =
        (modrm >> 3) & 7 | (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4);
    let source_index = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    Some((
        p0,
        p1,
        p2,
        modrm,
        FourFmaMemoryFields {
            scalar,
            negate_product,
            destination,
            source_index,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            ll,
            opcode,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one complete AVX-512 4FMAPS memory source and synthesize an
    /// exact `[rsp]` replay.
    ///
    /// Intel SDM revision 092 defines a single all-or-none Tuple1_4X access:
    /// packed forms read 16 bytes when any K[15:0] bit is set, while scalar
    /// forms use only K[0]. Four consecutive source registers start at the
    /// encoded source index rounded down to a multiple of four. LLIG scalar
    /// images are retained exactly. Segment/address-size and APX B4/X4 address
    /// controls remain confined to helper address evaluation.
    pub(crate) fn evex_four_fma_memory_encoding(&self) -> Option<X86EvexFourFmaMemoryEncoding> {
        let bytes = self.as_slice();
        let (p0, p1, p2, modrm, fields) = four_fma_memory_fields(bytes)?;
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F38, select ordinary unextended RSP,
            // and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; EVEX.b was rejected.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten) = four_fma_memory_fields(stack_instruction.as_slice())?;
        if rewritten != fields {
            return None;
        }

        Some(X86EvexFourFmaMemoryEncoding {
            scalar: fields.scalar,
            negate_product: fields.negate_product,
            destination: fields.destination,
            source_index: fields.source_index,
            source_base: fields.source_index & !3,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            ll: fields.ll,
            opcode: fields.opcode,
            stack_instruction,
        })
    }
}
