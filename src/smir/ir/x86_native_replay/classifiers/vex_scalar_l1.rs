//! Deterministic canonicalization of generation-dependent scalar VEX.L=1.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one complete scalar VEX instruction for which Intel documents
    /// `VEX.L=1` as generation-dependent unpredictable, and return the exact
    /// deterministic `VEX.L=0` host instruction selected by RAX.
    ///
    /// Only the L bit is changed. The candidate is then revalidated through
    /// the existing strict register- or memory-form classifier for its family,
    /// so malformed operands, reserved fields, immediate values, prefixes,
    /// lengths, and non-scalar VEX.256 instructions remain rejected.
    ///
    /// The covered families are scalar ADD/MUL/SUB/MIN/DIV/MAX, CMP,
    /// COMI/UCOMI, floating-point precision conversion, signed integer
    /// conversion, SQRT, and VMOVSS. Runtime and auxiliary space are O(1).
    pub(crate) fn vex_scalar_l1_canonical_l0(&self) -> Option<Self> {
        let bytes = self.as_slice();
        let vex_offset = bytes
            .iter()
            .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
            .count();
        let (p1_offset, opcode_offset) = match *bytes.get(vex_offset)? {
            0xC5 => (vex_offset + 1, vex_offset + 2),
            0xC4 if bytes.get(vex_offset + 1)? & 0x1F == 1 => (vex_offset + 2, vex_offset + 3),
            _ => return None,
        };
        let p1 = *bytes.get(p1_offset)?;
        let opcode = *bytes.get(opcode_offset)?;
        if p1 & 0x04 == 0 {
            return None;
        }

        let pp = p1 & 0x03;
        let scalar_family = match (pp, opcode) {
            (
                2 | 3,
                0x2A | 0x2C | 0x2D | 0x51 | 0x58 | 0x59 | 0x5A | 0x5C | 0x5D | 0x5E | 0x5F | 0xC2,
            ) => true,
            (0 | 1, 0x2E | 0x2F) => true,
            (2, 0x10 | 0x11) => true,
            _ => false,
        };
        if !scalar_family {
            return None;
        }

        let mut canonical = *self;
        canonical.bytes[p1_offset] &= !0x04;
        let register_valid = canonical
            .legacy_vex_register_fp_arithmetic_needs_avx()
            .is_some()
            || canonical
                .legacy_vex_register_fp_compare_needs_avx()
                .is_some()
            || canonical.is_vex_register_fp_flag_compare()
            || canonical
                .vex_scalar_fp_convert_destination_index()
                .is_some()
            || canonical.vex_scalar_fp_to_int_destination_index().is_some()
            || canonical.vex_scalar_int_to_fp_destination_index().is_some()
            || canonical.legacy_vex_register_fp_sqrt_needs_avx().is_some()
            || canonical
                .legacy_vex_register_scalar_move_needs_avx()
                .is_some();
        let memory_valid = canonical.vex_scalar_memory_fp_arithmetic_fields().is_some()
            || canonical.vex_memory_scalar_fp_compare_fields().is_some()
            || canonical.vex_memory_fp_flag_compare_fields().is_some()
            || canonical.vex_scalar_convert_memory_encoding().is_some()
            || canonical
                .vex_memory_fp_sqrt_fields()
                .is_some_and(|(_, source1, _, _, _, _)| source1.is_some())
            || canonical
                .vex_scalar_fp_memory_encoding()
                .is_some_and(|encoding| encoding.pp == 2);

        (register_valid || memory_valid).then_some(canonical)
    }
}
