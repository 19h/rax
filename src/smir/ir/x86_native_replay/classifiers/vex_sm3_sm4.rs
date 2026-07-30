//! Intel VEX SM3/SM4 register and memory-source replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

/// Exact Intel SM3/SM4 operation selected by one VEX encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexSm3Sm4MemoryKind {
    Sm3Msg1,
    Sm3Msg2,
    Sm3Rounds2,
    Sm4Key4,
    Sm4Rounds4,
}

impl X86VexSm3Sm4MemoryKind {
    pub(crate) fn needs_sm3(self) -> bool {
        matches!(self, Self::Sm3Msg1 | Self::Sm3Msg2 | Self::Sm3Rounds2)
    }

    pub(crate) fn needs_sm4(self) -> bool {
        matches!(self, Self::Sm4Key4 | Self::Sm4Rounds4)
    }
}

/// One complete VEX SM3/SM4 memory encoding rewritten to consume a
/// helper-loaded value from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexSm3Sm4MemoryEncoding {
    pub(crate) kind: X86VexSm3Sm4MemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) immediate: Option<u8>,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct X86VexSm3Sm4RegisterEncoding {
    kind: X86VexSm3Sm4MemoryKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    source2: u8,
    immediate: Option<u8>,
}

impl X86InstructionBytes {
    /// Validate one complete register-only VEX SM3 or SM4 instruction.
    ///
    /// Intel SDM Volume 2 defines SM3 message operations as VEX.128 map
    /// 0F38/0F3A forms and SM4 as VEX.128/VEX.256 map 0F38 forms. Every form
    /// requires W=0; only VSM3RNDS2 carries imm8. VEX.X is ignored for a
    /// register source, while inverted VEX.R/B extend the destination and
    /// second source to XMM/YMM8..15.
    fn vex_register_sm3_sm4_encoding(&self) -> Option<X86VexSm3Sm4RegisterEncoding> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 5 | 6) || bytes.first() != Some(&0xC4) {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];
        if p1 & 0x80 != 0 || modrm >> 6 != 3 {
            return None;
        }
        let map = p0 & 0x1F;
        let pp = p1 & 0x03;
        let width_256 = p1 & 0x04 != 0;
        let (kind, width, immediate) = match (map, pp, opcode, width_256, bytes.len()) {
            (2, 0, 0xDA, false, 5) => (X86VexSm3Sm4MemoryKind::Sm3Msg1, VecWidth::V128, None),
            (2, 1, 0xDA, false, 5) => (X86VexSm3Sm4MemoryKind::Sm3Msg2, VecWidth::V128, None),
            (3, 1, 0xDE, false, 6) => (
                X86VexSm3Sm4MemoryKind::Sm3Rounds2,
                VecWidth::V128,
                Some(bytes[5]),
            ),
            (2, 2, 0xDA, false, 5) => (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V128, None),
            (2, 2, 0xDA, true, 5) => (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V256, None),
            (2, 3, 0xDA, false, 5) => (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V128, None),
            (2, 3, 0xDA, true, 5) => (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V256, None),
            _ => return None,
        };

        Some(X86VexSm3Sm4RegisterEncoding {
            kind,
            width,
            destination: (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            source1: (!p1 >> 3) & 0x0F,
            source2: (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            immediate,
        })
    }

    /// Validate one complete VEX SM3/SM4 memory source and rewrite only its
    /// ModR/M r/m operand to a borrowed low vector register.
    ///
    /// SM3 memory operands are 16 bytes. SM4 memory operands match the
    /// 16-/32-byte VEX vector length. Segment and address-size prefixes are
    /// consumed by guest effective-address evaluation and omitted from the
    /// register rewrite.
    pub(crate) fn vex_sm3_sm4_memory_encoding(&self) -> Option<X86VexSm3Sm4MemoryEncoding> {
        let (fields, kind, width, immediate, memory_instruction) =
            if let Some(fields) = self.vex_memory_fields() {
                if fields.w {
                    return None;
                }
                let (kind, width) = match (fields.map, fields.pp, fields.opcode, fields.width_256) {
                    (2, 0, 0xDA, false) => (X86VexSm3Sm4MemoryKind::Sm3Msg1, VecWidth::V128),
                    (2, 1, 0xDA, false) => (X86VexSm3Sm4MemoryKind::Sm3Msg2, VecWidth::V128),
                    (2, 2, 0xDA, false) => (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V128),
                    (2, 2, 0xDA, true) => (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V256),
                    (2, 3, 0xDA, false) => (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V128),
                    (2, 3, 0xDA, true) => (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V256),
                    _ => return None,
                };
                (fields, kind, width, None, *self)
            } else {
                let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
                if fields.map != 3
                    || fields.pp != 1
                    || fields.opcode != 0xDE
                    || fields.width_256
                    || fields.w
                {
                    return None;
                }
                let instruction = X86InstructionBytes::new(
                    &self.as_slice()[..self.as_slice().len().checked_sub(1)?],
                )?;
                (
                    fields,
                    X86VexSm3Sm4MemoryKind::Sm3Rounds2,
                    VecWidth::V128,
                    Some(immediate),
                    instruction,
                )
            };

        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two operands cannot consume every low vector register");
        let rewritten = memory_instruction.vex_memory_with_register_source(scratch)?;
        let register_instruction = if let Some(immediate) = immediate {
            let rewritten_bytes = rewritten.as_slice();
            let len = rewritten_bytes.len().checked_add(1)?;
            let mut bytes = [0u8; 15];
            if len > bytes.len() {
                return None;
            }
            bytes[..rewritten_bytes.len()].copy_from_slice(rewritten_bytes);
            bytes[rewritten_bytes.len()] = immediate;
            X86InstructionBytes::new(&bytes[..len])?
        } else {
            rewritten
        };
        let register = register_instruction.vex_register_sm3_sm4_encoding()?;
        if register.kind != kind
            || register.width != width
            || register.destination != fields.destination
            || register.source1 != fields.source1
            || register.source2 != scratch
            || register.immediate != immediate
        {
            return None;
        }

        Some(X86VexSm3Sm4MemoryEncoding {
            kind,
            width,
            destination: fields.destination,
            source1: fields.source1,
            scratch,
            immediate,
            memory_size: width.bytes(),
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct Family {
        kind: X86VexSm3Sm4MemoryKind,
        map: u8,
        pp: u8,
        opcode: u8,
        width: VecWidth,
        has_immediate: bool,
    }

    const FAMILIES: [Family; 7] = [
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm3Msg1,
            map: 2,
            pp: 0,
            opcode: 0xDA,
            width: VecWidth::V128,
            has_immediate: false,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm3Msg2,
            map: 2,
            pp: 1,
            opcode: 0xDA,
            width: VecWidth::V128,
            has_immediate: false,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
            map: 3,
            pp: 1,
            opcode: 0xDE,
            width: VecWidth::V128,
            has_immediate: true,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm4Key4,
            map: 2,
            pp: 2,
            opcode: 0xDA,
            width: VecWidth::V128,
            has_immediate: false,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm4Key4,
            map: 2,
            pp: 2,
            opcode: 0xDA,
            width: VecWidth::V256,
            has_immediate: false,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm4Rounds4,
            map: 2,
            pp: 3,
            opcode: 0xDA,
            width: VecWidth::V128,
            has_immediate: false,
        },
        Family {
            kind: X86VexSm3Sm4MemoryKind::Sm4Rounds4,
            map: 2,
            pp: 3,
            opcode: 0xDA,
            width: VecWidth::V256,
            has_immediate: false,
        },
    ];

    fn memory_bytes(
        family: Family,
        destination: u8,
        source1: u8,
        base: u8,
        immediate: u8,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        let mut bytes = vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if base < 8 { 0x20 } else { 0 })
                | family.map,
            (((!source1) & 0x0F) << 3)
                | (u8::from(family.width == VecWidth::V256) << 2)
                | family.pp,
            family.opcode,
            ((destination & 7) << 3) | (base & 7),
        ];
        if family.has_immediate {
            bytes.push(immediate);
        }
        bytes
    }

    #[test]
    fn memory_classifier_exhaustively_covers_67_072_family_operand_and_immediate_cells() {
        let mut classified = 0usize;
        for family in FAMILIES {
            for destination in 0..16 {
                for source1 in 0..16 {
                    let immediate_range = if family.has_immediate {
                        0..=u8::MAX
                    } else {
                        0..=0
                    };
                    for immediate in immediate_range {
                        let base = if (destination ^ source1 ^ immediate) & 1 == 0 {
                            3
                        } else {
                            11
                        };
                        let bytes = memory_bytes(family, destination, source1, base, immediate);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_sm3_sm4_memory_encoding()
                            .unwrap_or_else(|| panic!("{family:?}: {bytes:02X?}"));
                        let scratch = (0..16u8)
                            .find(|candidate| *candidate != destination && *candidate != source1)
                            .unwrap();
                        assert_eq!(encoding.kind, family.kind, "{bytes:02X?}");
                        assert_eq!(encoding.width, family.width, "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                        assert_eq!(
                            encoding.immediate,
                            family.has_immediate.then_some(immediate),
                            "{bytes:02X?}"
                        );
                        assert_eq!(encoding.memory_size, family.width.bytes(), "{bytes:02X?}");
                        let register = encoding
                            .register_instruction
                            .vex_register_sm3_sm4_encoding()
                            .unwrap();
                        assert_eq!(register.kind, family.kind, "{bytes:02X?}");
                        assert_eq!(register.width, family.width, "{bytes:02X?}");
                        assert_eq!(register.destination, destination, "{bytes:02X?}");
                        assert_eq!(register.source1, source1, "{bytes:02X?}");
                        assert_eq!(register.source2, scratch, "{bytes:02X?}");
                        assert_eq!(register.immediate, encoding.immediate, "{bytes:02X?}");
                        assert_eq!(family.kind.needs_sm3(), !family.kind.needs_sm4());
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 67_072);
    }

    #[test]
    fn memory_classifier_accepts_complete_segment_address_and_displacement_shapes() {
        for (name, bytes, memory_size) in [
            (
                "FS addr32 SIB SM3RNDS2",
                &[
                    0x64, 0x67, 0xC4, 0x03, 0x21, 0xDE, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xFF,
                ][..],
                16,
            ),
            (
                "SS addr32 SIB VSM4KEY4",
                &[
                    0x36, 0x67, 0xC4, 0x02, 0x26, 0xDA, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44,
                ][..],
                32,
            ),
            (
                "RIP-relative VSM3MSG1",
                &[0xC4, 0xE2, 0x20, 0xDA, 0x0D, 0x11, 0x22, 0x33, 0x44][..],
                16,
            ),
            (
                "RBP displacement VSM4RNDS4",
                &[0xC4, 0xE2, 0x23, 0xDA, 0x4D, 0x20][..],
                16,
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_sm3_sm4_memory_encoding()
                .unwrap_or_else(|| panic!("{name}: {bytes:02X?}"));
            assert_eq!(encoding.memory_size, memory_size, "{name}");
            assert!(
                encoding
                    .register_instruction
                    .vex_register_sm3_sm4_encoding()
                    .is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn register_and_memory_classifiers_reject_structural_frontiers() {
        let canonical = memory_bytes(FAMILIES[2], 9, 11, 10, 0xFF);
        let mut invalid = vec![
            canonical[..canonical.len() - 1].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
        ];
        for (index, value) in [
            (0, 0xC5),
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 2),
            (2, canonical[2] | 0x80),
            (2, canonical[2] | 0x04),
            (2, canonical[2] & !0x03),
            (3, 0xDD),
            (4, canonical[4] | 0xC0),
        ] {
            let mut bytes = canonical.clone();
            bytes[index] = value;
            invalid.push(bytes);
        }
        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_sm3_sm4_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }

        let register = X86InstructionBytes::new(&canonical)
            .unwrap()
            .vex_sm3_sm4_memory_encoding()
            .unwrap()
            .register_instruction;
        assert!(
            register.vex_register_sm3_sm4_encoding().is_some(),
            "{:02X?}",
            register.as_slice()
        );
        let canonical = register.as_slice();
        let mut invalid = vec![
            canonical[..canonical.len() - 1].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
        ];
        for (index, value) in [
            (0, 0xC5),
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 2),
            (2, canonical[2] | 0x80),
            (2, canonical[2] | 0x04),
            (2, canonical[2] & !0x03),
            (3, 0xDD),
            (4, canonical[4] & !0xC0),
        ] {
            let mut bytes = canonical.to_vec();
            bytes[index] = value;
            invalid.push(bytes);
        }
        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_sm3_sm4_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
