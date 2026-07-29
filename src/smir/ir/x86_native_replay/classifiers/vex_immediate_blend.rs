//! AVX/AVX2 VEX immediate-blend replay classifiers.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Complete encoding fields for one VEX immediate blend whose second source is
/// memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexImmediateBlendMemoryFields {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) element: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) immediate: u8,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) repeat_128: bool,
    pub(crate) needs_avx2: bool,
}

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX immediate blend and
    /// report whether the selected vector width requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns `VPBLENDD`, `VBLENDPS`, `VBLENDPD`, and
    /// `VPBLENDW` to map 0F3A with mandatory 66H and opcodes 02H/0CH/0DH/0EH.
    /// `VPBLENDD` requires AVX2 and VEX.W=0. `VBLENDPS` and `VBLENDPD` require
    /// AVX for both widths. `VPBLENDW` requires AVX for 128 bits and AVX2 for
    /// 256 bits. The latter three opcodes are WIG. Memory forms remain excluded
    /// so replay cannot bypass guest translation or precise fault handling.
    pub fn vex_register_immediate_blend_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm, _imm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        match opcode {
            0x02 if p1 & 0x80 == 0 => Some(true),
            0x0C | 0x0D => Some(false),
            0x0E => Some(p1 & 0x04 != 0),
            _ => None,
        }
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX immediate blend. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_immediate_blend_destination_index(&self) -> Option<u8> {
        self.vex_register_immediate_blend_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VEX immediate blend whose second source is memory.
    ///
    /// `VPBLENDD`, `VBLENDPS`, `VBLENDPD`, and `VPBLENDW` use map 0F3A,
    /// mandatory prefix 66H, and an imm8 selector. `VPBLENDD` requires VEX.W=0
    /// and AVX2 at both widths. The floating blends are AVX instructions at both
    /// widths. `VPBLENDW` requires AVX for 128 bits and AVX2 for 256 bits; its
    /// imm8 selector repeats independently in each 128-bit lane. VEX.W is
    /// ignored for the latter three opcodes and retained for exact native
    /// replay. The shared parser validates the complete
    /// ModR/M/SIB/displacement plus imm8 shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_immediate_blend_fields(
        &self,
    ) -> Option<X86VexImmediateBlendMemoryFields> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let (element, repeat_128, needs_avx2) = match fields.opcode {
            0x02 if !fields.w => (VecElementType::I32, false, true),
            0x0C => (VecElementType::I32, false, false),
            0x0D => (VecElementType::I64, false, false),
            0x0E => (VecElementType::I16, true, fields.width_256),
            _ => return None,
        };
        Some(X86VexImmediateBlendMemoryFields {
            destination: fields.destination,
            source1: fields.source1,
            element,
            width,
            immediate,
            opcode: fields.opcode,
            w: fields.w,
            repeat_128,
            needs_avx2,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_encoding(
        destination: u8,
        source1: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn memory_classifier_exhaustively_covers_1_048_576_encoding_cells() {
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for opcode in [0x02, 0x0C, 0x0D, 0x0E] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for w in [false, true] {
                            for immediate in u8::MIN..=u8::MAX {
                                let base = if immediate & 1 == 0 { 3 } else { 11 };
                                let bytes = memory_encoding(
                                    destination,
                                    source1,
                                    base,
                                    opcode,
                                    width,
                                    immediate,
                                    w,
                                );
                                let actual = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_immediate_blend_fields();
                                let expected = match opcode {
                                    0x02 if !w => Some((VecElementType::I32, false, true)),
                                    0x0C => Some((VecElementType::I32, false, false)),
                                    0x0D => Some((VecElementType::I64, false, false)),
                                    0x0E => {
                                        Some((VecElementType::I16, true, width == VecWidth::V256))
                                    }
                                    _ => None,
                                }
                                .map(
                                    |(element, repeat_128, needs_avx2)| {
                                        X86VexImmediateBlendMemoryFields {
                                            destination,
                                            source1,
                                            element,
                                            width,
                                            immediate,
                                            opcode,
                                            w,
                                            repeat_128,
                                            needs_avx2,
                                        }
                                    },
                                );
                                assert_eq!(actual, expected, "{bytes:02X?}");
                                if actual.is_some() {
                                    accepted += 1;
                                } else {
                                    rejected += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 16 * 16 * 2 * 256 * 7);
        assert_eq!(rejected, 16 * 16 * 2 * 256);
        assert_eq!(accepted + rejected, 1_048_576);
    }

    #[test]
    fn memory_classifier_accepts_complete_prefixed_sib_displacement_shape() {
        // FS addr32: VBLENDPD ymm14,ymm10,[r14d+r15d*2+0x44332211],0xA5.
        let bytes = [
            0x64, 0x67, 0xC4, 0x03, 0xAD, 0x0D, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_immediate_blend_fields(),
            Some(X86VexImmediateBlendMemoryFields {
                destination: 14,
                source1: 10,
                element: VecElementType::I64,
                width: VecWidth::V256,
                immediate: 0xA5,
                opcode: 0x0D,
                w: true,
                repeat_128: false,
                needs_avx2: false,
            })
        );
    }

    #[test]
    fn memory_classifier_rejects_every_structural_frontier() {
        let valid = memory_encoding(9, 10, 11, 0x0C, VecWidth::V256, 0xA5, true);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x0B;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut missing_immediate = valid.clone();
        missing_immediate.pop();
        cases.push(missing_immediate);

        let mut truncated_displacement = valid.clone();
        truncated_displacement.remove(5);
        cases.push(truncated_displacement);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);

        cases.push(memory_encoding(9, 10, 11, 0x02, VecWidth::V128, 0xA5, true));

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_immediate_blend_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
