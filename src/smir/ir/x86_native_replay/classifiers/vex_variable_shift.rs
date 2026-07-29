//! VEX per-element variable-shift memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{ShiftOp, VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete AVX2 VEX memory-source `VPSLLVD/Q`, `VPSRAVD`,
    /// or `VPSRLVD/Q` instruction.
    ///
    /// Returns `(destination, source1, element type, shift, vector width)`.
    /// The complete ModR/M/SIB/displacement shape is validated by the shared
    /// VEX memory parser. Register sources, wrong map/prefix/W combinations,
    /// truncated instructions, and trailing bytes fail closed.
    pub(crate) fn vex_memory_variable_shift_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, ShiftOp, VecWidth)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 {
            return None;
        }
        let (elem, shift) = match (fields.opcode, fields.w) {
            (0x45, false) => (VecElementType::I32, ShiftOp::Lsr),
            (0x45, true) => (VecElementType::I64, ShiftOp::Lsr),
            (0x46, false) => (VecElementType::I32, ShiftOp::Asr),
            (0x47, false) => (VecElementType::I32, ShiftOp::Lsl),
            (0x47, true) => (VecElementType::I64, ShiftOp::Lsl),
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            shift,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [(u8, bool, VecElementType, ShiftOp); 5] = [
        (0x45, false, VecElementType::I32, ShiftOp::Lsr),
        (0x45, true, VecElementType::I64, ShiftOp::Lsr),
        (0x46, false, VecElementType::I32, ShiftOp::Asr),
        (0x47, false, VecElementType::I32, ShiftOp::Lsl),
        (0x47, true, VecElementType::I64, ShiftOp::Lsl),
    ];

    fn complete_memory_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> Vec<u8> {
        let mut bytes = vec![0xC4, p0, p1, opcode, modrm];
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if rm == 4 {
            // Use an ordinary RSP-based SIB. Dedicated tests cover no-base SIB.
            bytes.push(0x24);
        } else if mode == 0 && rm == 5 {
            bytes.extend_from_slice(&[0; 4]);
        }
        bytes.extend(std::iter::repeat_n(
            0,
            match mode {
                1 => 1,
                2 => 4,
                _ => 0,
            },
        ));
        bytes
    }

    #[test]
    fn classifier_exhaustively_covers_327_680_extension_source_width_and_modrm_cells() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for (opcode, w, elem, shift) in KINDS {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                let p0 = extension_bits | 2;
                for encoded_vvvv in 0u8..16 {
                    for width_256 in [false, true] {
                        let p1 = (u8::from(w) << 7)
                            | (encoded_vvvv << 3)
                            | (u8::from(width_256) << 2)
                            | 1;
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes = complete_memory_encoding(p0, p1, opcode, modrm);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let expected = (modrm >> 6 != 3).then_some((
                                (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
                                (!p1 >> 3) & 0x0F,
                                elem,
                                shift,
                                if width_256 {
                                    VecWidth::V256
                                } else {
                                    VecWidth::V128
                                },
                            ));
                            assert_eq!(
                                instruction.vex_memory_variable_shift_fields(),
                                expected,
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 245_760);
        assert_eq!(tested, 327_680);
    }

    #[test]
    fn classifier_exhaustively_rejects_wrong_map_prefix_opcode_and_w() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for map in 0u8..32 {
            for pp in 0u8..4 {
                for opcode in u8::MIN..=u8::MAX {
                    for w in [false, true] {
                        for width_256 in [false, true] {
                            let p0 = 0xE0 | map;
                            let p1 =
                                (u8::from(w) << 7) | (0x0D << 3) | (u8::from(width_256) << 2) | pp;
                            let bytes = complete_memory_encoding(p0, p1, opcode, 0x43);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let semantics =
                                KINDS
                                    .iter()
                                    .find_map(|&(kind_opcode, kind_w, elem, shift)| {
                                        (opcode == kind_opcode && w == kind_w)
                                            .then_some((elem, shift))
                                    });
                            let expected = if map == 2 && pp == 1 {
                                semantics.map(|(elem, shift)| {
                                    (
                                        0,
                                        2,
                                        elem,
                                        shift,
                                        if width_256 {
                                            VecWidth::V256
                                        } else {
                                            VecWidth::V128
                                        },
                                    )
                                })
                            } else {
                                None
                            };
                            assert_eq!(
                                instruction.vex_memory_variable_shift_fields(),
                                expected,
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, KINDS.len() * 2);
        assert_eq!(tested, 131_072);
    }

    #[test]
    fn classifier_accepts_llvm_23_memory_encodings_and_rejects_non_memory_or_bad_length() {
        // Independently assembled by LLVM 23 with +avx2.
        for (bytes, elem, shift, width) in [
            (
                &[0xC4, 0x42, 0x69, 0x47, 0x4B, 0x20][..],
                VecElementType::I32,
                ShiftOp::Lsl,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0xED, 0x47, 0x4B, 0x20][..],
                VecElementType::I64,
                ShiftOp::Lsl,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x6D, 0x46, 0x4B, 0x20][..],
                VecElementType::I32,
                ShiftOp::Asr,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x69, 0x45, 0x4B, 0x20][..],
                VecElementType::I32,
                ShiftOp::Lsr,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0xED, 0x45, 0x4B, 0x20][..],
                VecElementType::I64,
                ShiftOp::Lsr,
                VecWidth::V256,
            ),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert_eq!(
                instruction.vex_memory_variable_shift_fields(),
                Some((9, 2, elem, shift, width)),
                "{bytes:02X?}"
            );
        }

        let prefixed = [0x64, 0x67, 0xC4, 0x42, 0x69, 0x47, 0x8C, 0x25, 0, 0, 0, 0];
        assert_eq!(
            X86InstructionBytes::new(&prefixed)
                .unwrap()
                .vex_memory_variable_shift_fields(),
            Some((9, 2, VecElementType::I32, ShiftOp::Lsl, VecWidth::V128))
        );

        for bytes in [
            vec![0xC4, 0x62, 0x69, 0x47, 0xC8],
            vec![0xC4, 0x42, 0x69, 0x47, 0x4B],
            vec![0xC4, 0x42, 0x69, 0x47, 0x4B, 0x20, 0],
            vec![0xC4, 0x42, 0xE9, 0x46, 0x4B, 0x20],
            vec![0xC5, 0xE9, 0x47, 0x4B, 0x20],
            vec![0x62, 0xF2, 0x6D, 0x08, 0x47, 0x4B, 0x20],
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_variable_shift_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
