//! EVEX `VDBPSADBW` memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Exact EVEX `VDBPSADBW` Full Mem encoding and its byte-validated
/// register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexDbpsadbwMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX `VDBPSADBW` Full Mem source and select an exact
    /// register-source replay.
    ///
    /// Intel specifies map 0F3A, mandatory 66H, W0, a Full Mem tuple,
    /// word-granular writemasking, and Type E4NF.nb exceptions. EVEX.b is
    /// therefore reserved and the complete 16/32/64-byte vector access is
    /// unconditional. Segment/address-size prefixes and APX B4/X4 address
    /// extensions remain confined to helper address evaluation.
    pub(crate) fn evex_dbpsadbw_memory_encoding(&self) -> Option<X86EvexDbpsadbwMemoryEncoding> {
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
        let operand_end = memory_operand_end(bytes, modrm_index)?;
        let immediate = *bytes.get(operand_end)?;
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        if p0 & 0x07 != 3
            || p1 & 0x80 != 0
            || p1 & 0x03 != 1
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || opcode != 0x42
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || operand_end.checked_add(1)? != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two operands cannot consume every low vector register");
        let needs_avx512vl = width != VecWidth::V512;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Clear APX B4 and retain destination extensions.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W0/vvvv/66 and restore the ordinary EVEX.U bit.
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            immediate,
        ])
        .unwrap();
        if register_instruction.evex_register_bw_immediate_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        Some(X86EvexDbpsadbwMemoryEncoding {
            width,
            destination,
            source1,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            immediate,
            scratch,
            register_instruction,
            needs_avx512vl,
        })
    }
}
