//! AVX_NE_CONVERT VEX replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

/// Exact AVX_NE_CONVERT operation selected by one memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexNeConvertKind {
    BroadcastBf16,
    BroadcastFp16,
    EvenBf16,
    EvenFp16,
    OddBf16,
    OddFp16,
    Fp32ToBf16,
}

impl X86VexNeConvertKind {
    pub(crate) const fn broadcast(self) -> bool {
        matches!(self, Self::BroadcastBf16 | Self::BroadcastFp16)
    }

    pub(crate) const fn fp16(self) -> bool {
        matches!(self, Self::BroadcastFp16 | Self::EvenFp16 | Self::OddFp16)
    }

    pub(crate) const fn odd(self) -> bool {
        matches!(self, Self::OddBf16 | Self::OddFp16)
    }
}

/// Byte-validated AVX_NE_CONVERT memory encoding rewritten to consume the
/// helper-staged operand from `[rsp]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexNeConvertMemoryEncoding {
    pub(crate) kind: X86VexNeConvertKind,
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) scratch: u8,
    pub(crate) memory_size: u32,
    pub(crate) stack_instruction: X86InstructionBytes,
}

fn memory_kind(opcode: u8, pp: u8) -> Option<X86VexNeConvertKind> {
    match (opcode, pp) {
        (0xB1, 2) => Some(X86VexNeConvertKind::BroadcastBf16),
        (0xB1, 1) => Some(X86VexNeConvertKind::BroadcastFp16),
        (0xB0, 2) => Some(X86VexNeConvertKind::EvenBf16),
        (0xB0, 1) => Some(X86VexNeConvertKind::EvenFp16),
        (0xB0, 3) => Some(X86VexNeConvertKind::OddBf16),
        (0xB0, 0) => Some(X86VexNeConvertKind::OddFp16),
        (0x72, 2) => Some(X86VexNeConvertKind::Fp32ToBf16),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate one register-only VEX `VCVTNEPS2BF16` and return
    /// `(destination, source, source width)`.
    ///
    /// Intel SDM Volume 2 assigns the AVX_NE_CONVERT form to
    /// VEX.F3.0F38.W0 72 /r with reserved `VEX.vvvv=1111B` and 128- or
    /// 256-bit input. VEX.X is ignored for a register ModR/M operand.
    pub(crate) fn vex_register_ne_convert_fields(&self) -> Option<(u8, u8, VecWidth)> {
        let [0xC4, p0, p1, 0x72, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0xFB != 0x7A || modrm >> 6 != 3 {
            return None;
        }

        Some((
            (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            if p1 & 0x04 != 0 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
        ))
    }

    /// Architectural XMM destination selected by a register-only
    /// AVX_NE_CONVERT `VCVTNEPS2BF16`.
    pub(crate) fn vex_ne_convert_destination_index(&self) -> Option<u8> {
        self.vex_register_ne_convert_fields().map(|fields| fields.0)
    }

    /// Validate one complete AVX_NE_CONVERT memory source and rewrite only
    /// its guest address to `[rsp]`.
    ///
    /// B1H forms are memory-only 16-bit broadcasts. B0H forms are memory-only
    /// 128-/256-bit even/odd BF16/FP16 conversions. Opcode 72H converts a
    /// 128-/256-bit FP32 source to a zero-padded XMM BF16 result. All require
    /// W=0, reserved `VEX.vvvv=1111B`, and VEX.128 or VEX.256. Optional
    /// segment/address-size prefixes and every original ModR/M/SIB/
    /// displacement byte are consumed only by the helper-computed guest
    /// address and are omitted from the stack rewrite.
    pub(crate) fn vex_ne_convert_memory_encoding(&self) -> Option<X86VexNeConvertMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.source1 != 0 || fields.w {
            return None;
        }
        let kind = memory_kind(fields.opcode, fields.pp)?;
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };

        let bytes = self.as_slice();
        let vex_offset = bytes
            .iter()
            .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
            .count();
        if bytes.get(vex_offset) != Some(&0xC4) {
            return None;
        }
        let p0 = *bytes.get(vex_offset + 1)?;
        let p1 = *bytes.get(vex_offset + 2)?;
        let opcode = *bytes.get(vex_offset + 3)?;
        let modrm = *bytes.get(vex_offset + 4)?;
        let stack_instruction = X86InstructionBytes::new(&[
            0xC4,
            // Preserve destination extension R, select unextended SIB
            // index/base, and retain map 0F38.
            (p0 & 0x80) | 0x62,
            p1,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let rewritten = stack_instruction.vex_memory_fields()?;
        if rewritten.destination != fields.destination
            || rewritten.source1 != 0
            || rewritten.map != 2
            || rewritten.pp != fields.pp
            || rewritten.opcode != fields.opcode
            || rewritten.width_256 != fields.width_256
            || rewritten.w
            || !rewritten.stack_segment
        {
            return None;
        }

        let scratch = (0..16u8).find(|candidate| *candidate != fields.destination)?;
        Some(X86VexNeConvertMemoryEncoding {
            kind,
            width,
            destination: fields.destination,
            scratch,
            memory_size: if kind.broadcast() { 2 } else { width.bytes() },
            stack_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register_encoding(extension_bits: u8, p1: u8, modrm: u8) -> [u8; 5] {
        [0xC4, extension_bits | 2, p1, 0x72, modrm]
    }

    fn complete_memory_encoding(extension_bits: u8, p1: u8, opcode: u8, modrm: u8) -> Vec<u8> {
        assert!(modrm >> 6 != 3);
        let mut bytes = vec![0xC4, extension_bits | 2, p1, opcode, modrm];
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if rm == 4 {
            bytes.push(0x25);
            if mode == 0 {
                bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes());
            }
        } else if mode == 0 && rm == 5 {
            bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes());
        }
        match mode {
            1 => bytes.push(0x20),
            2 => bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes()),
            _ => {}
        }
        bytes
    }

    #[test]
    fn register_classifier_exhaustively_covers_all_65_536_prefix_and_modrm_cells() {
        let mut accepted = 0usize;
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for encoded_vvvv in 0u8..16 {
                for ymm in [false, true] {
                    let p1 = (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 2;
                    for modrm in u8::MIN..=u8::MAX {
                        let bytes = register_encoding(extension_bits, p1, modrm);
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let expected = (encoded_vvvv == 15 && modrm >> 6 == 3).then(|| {
                            (
                                (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
                                (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7),
                                if ymm { VecWidth::V256 } else { VecWidth::V128 },
                            )
                        });
                        assert_eq!(
                            instruction.vex_register_ne_convert_fields(),
                            expected,
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            instruction.vex_ne_convert_destination_index(),
                            expected.map(|fields| fields.0),
                            "{bytes:02X?}"
                        );
                        accepted += usize::from(expected.is_some());
                    }
                }
            }
        }
        assert_eq!(accepted, 1_024);
    }

    #[test]
    fn memory_classifier_exhaustively_covers_all_defined_address_cells() {
        let forms = [
            (0xB1, 2, X86VexNeConvertKind::BroadcastBf16),
            (0xB1, 1, X86VexNeConvertKind::BroadcastFp16),
            (0xB0, 2, X86VexNeConvertKind::EvenBf16),
            (0xB0, 1, X86VexNeConvertKind::EvenFp16),
            (0xB0, 3, X86VexNeConvertKind::OddBf16),
            (0xB0, 0, X86VexNeConvertKind::OddFp16),
            (0x72, 2, X86VexNeConvertKind::Fp32ToBf16),
        ];
        let mut accepted = 0usize;
        for (opcode, pp, kind) in forms {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        let p1 = (encoded_vvvv << 3) | (u8::from(ymm) << 2) | pp;
                        for modrm in u8::MIN..=u8::MAX {
                            if modrm >> 6 == 3 {
                                continue;
                            }
                            let bytes = complete_memory_encoding(extension_bits, p1, opcode, modrm);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let actual = instruction.vex_ne_convert_memory_encoding();
                            if encoded_vvvv == 15 {
                                let actual = actual.unwrap_or_else(|| {
                                    panic!("defined AVX_NE_CONVERT rejected: {bytes:02X?}")
                                });
                                let width = if ymm { VecWidth::V256 } else { VecWidth::V128 };
                                assert_eq!(actual.kind, kind);
                                assert_eq!(actual.width, width);
                                assert_eq!(
                                    actual.destination,
                                    (u8::from(extension_bits & 0x80 == 0) << 3)
                                        | ((modrm >> 3) & 7)
                                );
                                assert_ne!(actual.scratch, actual.destination);
                                assert_eq!(
                                    actual.memory_size,
                                    if kind.broadcast() { 2 } else { width.bytes() }
                                );
                                assert_eq!(
                                    actual.stack_instruction.as_slice(),
                                    &[
                                        0xC4,
                                        (extension_bits & 0x80) | 0x62,
                                        p1,
                                        opcode,
                                        (modrm & 0x38) | 0x04,
                                        0x24,
                                    ]
                                );
                                accepted += 1;
                            } else {
                                assert_eq!(actual, None, "{bytes:02X?}");
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 21_504);
    }

    #[test]
    fn llvm_23_encodings_classify_with_exact_memory_sizes_and_stack_rewrites() {
        for (bytes, kind, width, memory_size, stack) in [
            (
                &[0xC4, 0x62, 0x7A, 0xB1, 0x48, 0x11][..],
                X86VexNeConvertKind::BroadcastBf16,
                VecWidth::V128,
                2,
                &[0xC4, 0x62, 0x7A, 0xB1, 0x0C, 0x24][..],
            ),
            (
                &[0xC4, 0x62, 0x7D, 0xB1, 0x48, 0x11],
                X86VexNeConvertKind::BroadcastFp16,
                VecWidth::V256,
                2,
                &[0xC4, 0x62, 0x7D, 0xB1, 0x0C, 0x24],
            ),
            (
                &[0xC4, 0x62, 0x7A, 0xB0, 0x48, 0x11],
                X86VexNeConvertKind::EvenBf16,
                VecWidth::V128,
                16,
                &[0xC4, 0x62, 0x7A, 0xB0, 0x0C, 0x24],
            ),
            (
                &[0xC4, 0x62, 0x7C, 0xB0, 0x48, 0x11],
                X86VexNeConvertKind::OddFp16,
                VecWidth::V256,
                32,
                &[0xC4, 0x62, 0x7C, 0xB0, 0x0C, 0x24],
            ),
            (
                &[0xC4, 0x62, 0x7E, 0x72, 0x48, 0x11],
                X86VexNeConvertKind::Fp32ToBf16,
                VecWidth::V256,
                32,
                &[0xC4, 0x62, 0x7E, 0x72, 0x0C, 0x24],
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_ne_convert_memory_encoding()
                .unwrap();
            assert_eq!(encoding.kind, kind);
            assert_eq!(encoding.width, width);
            assert_eq!(encoding.destination, 9);
            assert_eq!(encoding.memory_size, memory_size);
            assert_eq!(encoding.stack_instruction.as_slice(), stack);
        }

        assert_eq!(
            X86InstructionBytes::new(&[0xC4, 0x42, 0x7A, 0x72, 0xCA])
                .unwrap()
                .vex_register_ne_convert_fields(),
            Some((9, 10, VecWidth::V128))
        );
        assert_eq!(
            X86InstructionBytes::new(&[0xC4, 0x42, 0x7E, 0x72, 0xCA])
                .unwrap()
                .vex_register_ne_convert_fields(),
            Some((9, 10, VecWidth::V256))
        );
    }

    #[test]
    fn classifiers_reject_reserved_fields_register_substitution_and_bad_boundaries() {
        let valid_register = [0xC4, 0x42, 0x7E, 0x72, 0xCA];
        let valid_memory = [0xC4, 0x62, 0x7E, 0x72, 0x48, 0x11];
        for bytes in [
            vec![0xC4, 0x43, 0x7E, 0x72, 0xCA],
            vec![0xC4, 0x42, 0xFE, 0x72, 0xCA],
            vec![0xC4, 0x42, 0x76, 0x72, 0xCA],
            vec![0xC4, 0x42, 0x7D, 0x72, 0xCA],
            vec![0xC4, 0x42, 0x7E, 0x73, 0xCA],
            vec![0xC4, 0x42, 0x7E, 0x72, 0x0A],
            valid_register[..4].to_vec(),
            [valid_register.as_slice(), &[0]].concat(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_ne_convert_fields(),
                None,
                "{bytes:02X?}"
            );
        }

        for bytes in [
            vec![0xC4, 0x63, 0x7E, 0x72, 0x48, 0x11],
            vec![0xC4, 0x62, 0xFE, 0x72, 0x48, 0x11],
            vec![0xC4, 0x62, 0x76, 0x72, 0x48, 0x11],
            vec![0xC4, 0x62, 0x7D, 0x72, 0x48, 0x11],
            vec![0xC4, 0x62, 0x7E, 0x73, 0x48, 0x11],
            vec![0xC4, 0x62, 0x7E, 0x72, 0xC8],
            valid_memory[..5].to_vec(),
            [valid_memory.as_slice(), &[0]].concat(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_ne_convert_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }

        let prefixed = [
            0x64, 0x67, 0xC4, 0x62, 0x7D, 0xB0, 0x84, 0x25, 0x44, 0x33, 0x22, 0x11,
        ];
        let encoding = X86InstructionBytes::new(&prefixed)
            .unwrap()
            .vex_ne_convert_memory_encoding()
            .unwrap();
        assert_eq!(encoding.kind, X86VexNeConvertKind::EvenFp16);
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            &[0xC4, 0x62, 0x7D, 0xB0, 0x04, 0x24]
        );
    }
}
