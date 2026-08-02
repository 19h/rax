//! EVEX packed shared-count shift memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{ShiftOp, VecElementType, VecWidth};

/// Exact EVEX VPSLL*, VPSRA*, or VPSRL* encoding whose shared 128-bit count
/// operand is memory, together with its byte-validated register replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexSharedCountShiftMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) shift: ShiftOp,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) w: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86EvexSharedCountShiftRegisterFields {
    pub(super) width: VecWidth,
    pub(super) elem: VecElementType,
    pub(super) shift: ShiftOp,
    pub(super) destination: u8,
    pub(super) source: u8,
    pub(super) count: u8,
    pub(super) writemask: Option<u8>,
    pub(super) zeroing: bool,
    pub(super) w: bool,
}

fn shared_count_shift_kind(opcode: u8, w: bool) -> Option<(VecElementType, ShiftOp)> {
    match (opcode, w) {
        // W is ignored for the word forms.
        (0xD1, _) => Some((VecElementType::I16, ShiftOp::Lsr)),
        (0xD2, false) => Some((VecElementType::I32, ShiftOp::Lsr)),
        (0xD3, true) => Some((VecElementType::I64, ShiftOp::Lsr)),
        (0xE1, _) => Some((VecElementType::I16, ShiftOp::Asr)),
        (0xE2, false) => Some((VecElementType::I32, ShiftOp::Asr)),
        (0xE2, true) => Some((VecElementType::I64, ShiftOp::Asr)),
        (0xF1, _) => Some((VecElementType::I16, ShiftOp::Lsl)),
        (0xF2, false) => Some((VecElementType::I32, ShiftOp::Lsl)),
        (0xF3, true) => Some((VecElementType::I64, ShiftOp::Lsl)),
        _ => None,
    }
}

impl X86InstructionBytes {
    pub(super) fn evex_register_shared_count_shift_fields(
        &self,
    ) -> Option<X86EvexSharedCountShiftRegisterFields> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let mask = p2 & 0x07;
        if p0 & 0x0F != 1
            || p1 & 0x07 != 5
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
        {
            return None;
        }
        let w = p1 & 0x80 != 0;
        let (elem, shift) = shared_count_shift_kind(opcode, w)?;
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        Some(X86EvexSharedCountShiftRegisterFields {
            width,
            elem,
            shift,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            count: (modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing: p2 & 0x80 != 0,
            w,
        })
    }

    /// Validate one packed AVX-512 shared-count shift with a 128-bit memory
    /// count and construct an exact register-source native replay.
    ///
    /// All forms use map 0F, mandatory prefix 66H, a fixed Mem128 tuple, and
    /// forbid EVEX.b. W is ignored for word shifts, selects VPSRAD/VPSRAQ for
    /// opcode E2H, and is fixed for the remaining doubleword/quadword forms.
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation.
    pub(crate) fn evex_shared_count_shift_memory_encoding(
        &self,
    ) -> Option<X86EvexSharedCountShiftMemoryEncoding> {
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
        let mask = p2 & 0x07;
        if p0 & 0x07 != 1
            || p1 & 0x03 != 1
            || modrm >> 6 == 3
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
            || operand_end != bytes.len()
        {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let (elem, shift) = shared_count_shift_kind(opcode, w)?;
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let zeroing = p2 & 0x80 != 0;
        let needs_avx512vl = width != VecWidth::V512;
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source)
            .expect("two operands cannot consume every low vector register");
        let rewritten = [
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Clear APX B4 and retain destination extensions.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; EVEX.b was rejected above.
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
        let expected = X86EvexSharedCountShiftRegisterFields {
            width,
            elem,
            shift,
            destination,
            source,
            count: scratch,
            writemask,
            zeroing,
            w,
        };
        if register_instruction.evex_register_shared_count_shift_fields() != Some(expected) {
            return None;
        }

        Some(X86EvexSharedCountShiftMemoryEncoding {
            width,
            elem,
            shift,
            destination,
            source,
            writemask,
            zeroing,
            w,
            scratch,
            register_instruction,
            needs_avx512vl,
        })
    }
}
