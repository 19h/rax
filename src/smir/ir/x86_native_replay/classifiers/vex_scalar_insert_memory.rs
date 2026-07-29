//! Complete VEX scalar-insert memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{MemWidth, VecElementType};

/// Exact scalar-insert operation selected by VEX map, opcode, and W.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexScalarInsertMemoryKind {
    Vpinsrb,
    Vpinsrw,
    Vpinsrd,
    Vpinsrq,
    Vinsertps,
}

impl X86VexScalarInsertMemoryKind {
    /// Width of the precise scalar memory access.
    pub(crate) const fn memory_width(self) -> MemWidth {
        match self {
            Self::Vpinsrb => MemWidth::B1,
            Self::Vpinsrw => MemWidth::B2,
            Self::Vpinsrd | Self::Vinsertps => MemWidth::B4,
            Self::Vpinsrq => MemWidth::B8,
        }
    }

    /// Integer lane type used by the canonical SMIR decomposition.
    pub(crate) const fn element(self) -> VecElementType {
        match self {
            Self::Vpinsrb => VecElementType::I8,
            Self::Vpinsrw => VecElementType::I16,
            Self::Vpinsrd | Self::Vinsertps => VecElementType::I32,
            Self::Vpinsrq => VecElementType::I64,
        }
    }

    /// Destination lane selected after architectural immediate masking.
    pub(crate) const fn destination_lane(self, immediate: u8) -> u8 {
        match self {
            Self::Vpinsrb => immediate & 0x0F,
            Self::Vpinsrw => immediate & 0x07,
            Self::Vpinsrd => immediate & 0x03,
            Self::Vpinsrq => immediate & 0x01,
            Self::Vinsertps => (immediate >> 4) & 0x03,
        }
    }
}

/// Byte-validated fields for one VEX.128 scalar-insert memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexScalarInsertMemoryFields {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) kind: X86VexScalarInsertMemoryKind,
    pub(crate) immediate: u8,
    /// Exact VEX.W bit. It is architecturally ignored except for opcode 22H,
    /// where it selects VPINSRD (W0) or VPINSRQ (W1).
    pub(crate) w: bool,
}

impl X86InstructionBytes {
    /// Validate one complete VEX.128 `VPINSRB`, `VPINSRW`, `VPINSRD`,
    /// `VPINSRQ`, or `VINSERTPS` instruction whose scalar source is memory.
    ///
    /// Intel SDM Volume 2 assigns mandatory prefix 66H to all five forms.
    /// `VPINSRW` uses map 0F/opcode C4H; the other forms use map
    /// 0F3A/opcodes 20H, 21H, and 22H. VEX.L must be zero. W is ignored for
    /// VPINSRB, VPINSRW, and VINSERTPS, while opcode 22H uses W to select
    /// doubleword or quadword insertion. The shared parser validates all
    /// ModR/M/SIB/displacement bytes plus imm8 and accepts only segment and
    /// address-size legacy prefixes.
    ///
    /// Classification is O(1) time and O(1) space because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_memory_scalar_insert_fields(&self) -> Option<X86VexScalarInsertMemoryFields> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.pp != 1 || fields.width_256 {
            return None;
        }
        let kind = match (fields.map, fields.opcode, fields.w) {
            (1, 0xC4, _) => X86VexScalarInsertMemoryKind::Vpinsrw,
            (3, 0x20, _) => X86VexScalarInsertMemoryKind::Vpinsrb,
            (3, 0x21, _) => X86VexScalarInsertMemoryKind::Vinsertps,
            (3, 0x22, false) => X86VexScalarInsertMemoryKind::Vpinsrd,
            (3, 0x22, true) => X86VexScalarInsertMemoryKind::Vpinsrq,
            _ => return None,
        };
        Some(X86VexScalarInsertMemoryFields {
            destination: fields.destination,
            source1: fields.source1,
            kind,
            immediate,
            w: fields.w,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(
        destination: u8,
        source1: u8,
        base: u8,
        kind: X86VexScalarInsertMemoryKind,
        immediate: u8,
        wig_w: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        let (map, opcode, w) = match kind {
            X86VexScalarInsertMemoryKind::Vpinsrb => (3, 0x20, wig_w),
            X86VexScalarInsertMemoryKind::Vpinsrw => (1, 0xC4, wig_w),
            X86VexScalarInsertMemoryKind::Vpinsrd => (3, 0x22, false),
            X86VexScalarInsertMemoryKind::Vpinsrq => (3, 0x22, true),
            X86VexScalarInsertMemoryKind::Vinsertps => (3, 0x21, wig_w),
        };
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if base < 8 { 0x20 } else { 0 })
                | map,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn classifies_all_524_288_destination_source_kind_w_and_immediate_cells() {
        let kinds = [
            X86VexScalarInsertMemoryKind::Vpinsrb,
            X86VexScalarInsertMemoryKind::Vpinsrw,
            X86VexScalarInsertMemoryKind::Vpinsrd,
            X86VexScalarInsertMemoryKind::Vpinsrq,
            X86VexScalarInsertMemoryKind::Vinsertps,
        ];
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for kind in kinds {
                    let w_values: &[bool] = match kind {
                        X86VexScalarInsertMemoryKind::Vpinsrd => &[false],
                        X86VexScalarInsertMemoryKind::Vpinsrq => &[true],
                        _ => &[false, true],
                    };
                    for &w in w_values {
                        for immediate in u8::MIN..=u8::MAX {
                            let bytes = instruction(destination, source1, 11, kind, immediate, w);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_scalar_insert_fields(),
                                Some(X86VexScalarInsertMemoryFields {
                                    destination,
                                    source1,
                                    kind,
                                    immediate,
                                    w,
                                }),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 8 * 256);
    }

    #[test]
    fn compact_and_complete_prefixed_address_shapes_classify_exactly() {
        let cases: &[(&[u8], X86VexScalarInsertMemoryFields)] = &[
            (
                &[0xC5, 0xE9, 0xC4, 0x4B, 0x20, 0xA5],
                X86VexScalarInsertMemoryFields {
                    destination: 1,
                    source1: 2,
                    kind: X86VexScalarInsertMemoryKind::Vpinsrw,
                    immediate: 0xA5,
                    w: false,
                },
            ),
            (
                &[0x64, 0xC4, 0x43, 0x29, 0x20, 0x4B, 0x20, 0x0F],
                X86VexScalarInsertMemoryFields {
                    destination: 9,
                    source1: 10,
                    kind: X86VexScalarInsertMemoryKind::Vpinsrb,
                    immediate: 0x0F,
                    w: false,
                },
            ),
            (
                &[0x65, 0xC4, 0x43, 0xA9, 0x21, 0x4C, 0xEC, 0x20, 0xA5],
                X86VexScalarInsertMemoryFields {
                    destination: 9,
                    source1: 10,
                    kind: X86VexScalarInsertMemoryKind::Vinsertps,
                    immediate: 0xA5,
                    w: true,
                },
            ),
            (
                &[
                    0x67, 0xC4, 0x63, 0x69, 0x22, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x03,
                ],
                X86VexScalarInsertMemoryFields {
                    destination: 9,
                    source1: 2,
                    kind: X86VexScalarInsertMemoryKind::Vpinsrd,
                    immediate: 0x03,
                    w: false,
                },
            ),
            (
                &[0xC4, 0x03, 0xA9, 0x22, 0x74, 0xEC, 0x20, 0x01],
                X86VexScalarInsertMemoryFields {
                    destination: 14,
                    source1: 10,
                    kind: X86VexScalarInsertMemoryKind::Vpinsrq,
                    immediate: 0x01,
                    w: true,
                },
            ),
        ];
        for (bytes, expected) in cases {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_memory_scalar_insert_fields(),
                Some(*expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn kind_metadata_masks_lanes_and_widths_exactly() {
        let cases = [
            (X86VexScalarInsertMemoryKind::Vpinsrb, MemWidth::B1, 0x0F),
            (X86VexScalarInsertMemoryKind::Vpinsrw, MemWidth::B2, 0x07),
            (X86VexScalarInsertMemoryKind::Vpinsrd, MemWidth::B4, 0x03),
            (X86VexScalarInsertMemoryKind::Vpinsrq, MemWidth::B8, 0x01),
            (X86VexScalarInsertMemoryKind::Vinsertps, MemWidth::B4, 0x02),
        ];
        for (kind, width, lane) in cases {
            assert_eq!(kind.memory_width(), width);
            assert_eq!(kind.destination_lane(0xAF), lane);
        }
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(
            9,
            10,
            11,
            X86VexScalarInsertMemoryKind::Vinsertps,
            0xA5,
            true,
        );
        let mut cases = Vec::new();

        for (index, value) in [
            (1, (valid[1] & !0x1F) | 1),
            (1, (valid[1] & !0x1F) | 2),
            (2, valid[2] & !3),
            (2, (valid[2] & !3) | 2),
            (2, valid[2] | 4),
            (3, 0x1F),
            (3, 0x23),
        ] {
            let mut bytes = valid.clone();
            bytes[index] = value;
            cases.push(bytes);
        }

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

        let mut forbidden_legacy_prefix = valid.clone();
        forbidden_legacy_prefix.insert(0, 0x66);
        cases.push(forbidden_legacy_prefix);

        let mut non_vex = valid;
        non_vex[0] = 0x62;
        cases.push(non_vex);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_scalar_insert_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
