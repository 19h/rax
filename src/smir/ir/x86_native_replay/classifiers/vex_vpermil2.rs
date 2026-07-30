//! AMD XOP `VPERMIL2PS`/`VPERMIL2PD` replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// One complete VPERMIL2 memory encoding rewritten to consume a helper-loaded
/// value from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexVpermil2MemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) is4: u8,
    pub(crate) scratch: u8,
    pub(crate) w: bool,
    pub(crate) immediate: u8,
    pub(crate) memory_size: u32,
    pub(crate) stack_segment: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX-encoded VPERMIL2
    /// instruction.
    ///
    /// AMD APM Volume 4, revision 3.26 assigns opcodes 48H and 49H in map
    /// 0F3A with mandatory 66H. VEX.W swaps the ModR/M and SRS source roles,
    /// VEX.L selects 128 or 256 bits, and all VEX.vvvv and immediate values
    /// are legal. Memory forms remain excluded so native replay cannot bypass
    /// guest-memory translation or precise fault handling.
    pub fn is_vex_register_vpermil2(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 3 && p1 & 0x03 == 1 && matches!(opcode, 0x48 | 0x49) && modrm >> 6 == 3
    }

    /// Architectural destination selected by an exact register-only
    /// VPERMIL2 encoding. ModR/M.reg is extended by inverted VEX.R.
    pub(crate) fn vex_vpermil2_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_vpermil2() {
            return None;
        }
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VPERMIL2 memory source and rewrite only its
    /// ModR/M memory operand to a borrowed low vector register.
    ///
    /// AMD APM Volume 4, revision 3.26 defines both opcodes for 128- and
    /// 256-bit operands. VEX.W swaps the ModR/M and `/is4` source roles; it
    /// does not change the 16-/32-byte memory footprint. Bits 7:4 of imm8
    /// select the `/is4` register, bits 1:0 select M2Z behavior, and bits 3:2
    /// are retained even though the architecture ignores them. Segment and
    /// address-size prefixes are consumed by guest address evaluation and
    /// omitted from the register rewrite.
    pub(crate) fn vex_vpermil2_memory_encoding(&self) -> Option<X86VexVpermil2MemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || !matches!(fields.opcode, 0x48 | 0x49) {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let elem = if fields.opcode == 0x48 {
            VecElementType::I32
        } else {
            VecElementType::I64
        };
        let is4 = immediate >> 4;
        let scratch = (0..16u8)
            .find(|candidate| {
                *candidate != fields.destination
                    && *candidate != fields.source1
                    && *candidate != is4
            })
            .expect("three operands cannot consume every low vector register");

        let bytes = self.as_slice();
        let start = bytes
            .iter()
            .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
            .count();
        if bytes.get(start) != Some(&0xC4) {
            return None;
        }
        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let modrm = *bytes.get(start + 4)?;
        let register_bytes = [
            0xC4,
            // Preserve VEX.R and the map, canonicalize the ignored X bit, and
            // encode the borrowed scratch through inverted VEX.B.
            (p0 & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1,
            fields.opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            immediate,
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        if !register_instruction.is_vex_register_vpermil2()
            || register_instruction.vex_vpermil2_destination_index() != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexVpermil2MemoryEncoding {
            width,
            elem,
            destination: fields.destination,
            source1: fields.source1,
            is4,
            scratch,
            w: fields.w,
            immediate,
            memory_size: width.bytes(),
            stack_segment: fields.stack_segment,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_encoding(
        opcode: u8,
        w: bool,
        width_256: bool,
        destination: u8,
        source1: u8,
        base: u8,
        immediate: u8,
    ) -> [u8; 6] {
        assert!(matches!(opcode, 0x48 | 0x49) && destination < 16 && source1 < 16 && base < 16);
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (u8::from(width_256) << 2) | 1,
            opcode,
            ((destination & 7) << 3) | (base & 7),
            immediate,
        ]
    }

    #[test]
    fn memory_classifier_exhaustively_covers_524_288_operand_and_immediate_cells() {
        let mut classified = 0usize;
        for opcode in [0x48, 0x49] {
            for w in [false, true] {
                for width_256 in [false, true] {
                    for destination in 0..16 {
                        for source1 in 0..16 {
                            for is4 in 0..16 {
                                for ignored_low in 0..16 {
                                    let immediate = (is4 << 4) | ignored_low;
                                    let base = if ignored_low & 1 == 0 { 3 } else { 11 };
                                    let bytes = memory_encoding(
                                        opcode,
                                        w,
                                        width_256,
                                        destination,
                                        source1,
                                        base,
                                        immediate,
                                    );
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_vpermil2_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    let width = if width_256 {
                                        VecWidth::V256
                                    } else {
                                        VecWidth::V128
                                    };
                                    let elem = if opcode == 0x48 {
                                        VecElementType::I32
                                    } else {
                                        VecElementType::I64
                                    };
                                    let scratch = (0..16u8)
                                        .find(|candidate| {
                                            *candidate != destination
                                                && *candidate != source1
                                                && *candidate != is4
                                        })
                                        .unwrap();
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                    assert_eq!(encoding.is4, is4, "{bytes:02X?}");
                                    assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                                    assert_eq!(encoding.w, w, "{bytes:02X?}");
                                    assert_eq!(encoding.immediate, immediate, "{bytes:02X?}");
                                    assert_eq!(encoding.memory_size, width.bytes(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.register_instruction.as_slice()[5],
                                        immediate,
                                        "{bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 524_288);
    }

    #[test]
    fn memory_classifier_preserves_complete_address_and_segment_shapes() {
        for (name, bytes, stack_segment) in [
            (
                "SS addr32 SIB",
                &[
                    0x36, 0x67, 0xC4, 0x03, 0xA5, 0x48, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xC7,
                ][..],
                true,
            ),
            (
                "FS addr32 SIB",
                &[
                    0x64, 0x67, 0xC4, 0x03, 0xA5, 0x49, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xC7,
                ][..],
                false,
            ),
            (
                "RIP relative",
                &[0xC4, 0xE3, 0x69, 0x48, 0x0D, 0x11, 0x22, 0x33, 0x44, 0xC7][..],
                false,
            ),
            (
                "RBP default SS",
                &[0xC4, 0xE3, 0x69, 0x49, 0x4D, 0x20, 0xC7][..],
                true,
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_vpermil2_memory_encoding()
                .unwrap_or_else(|| panic!("{name}: {bytes:02X?}"));
            assert_eq!(encoding.stack_segment, stack_segment, "{name}");
            assert_eq!(encoding.immediate, 0xC7, "{name}");
            assert_eq!(encoding.is4, 12, "{name}");
            assert!(encoding.register_instruction.is_vex_register_vpermil2());
        }
    }

    #[test]
    fn memory_classifier_rejects_every_structural_frontier() {
        let canonical = memory_encoding(0x48, true, true, 9, 10, 11, 0xC7);
        let mut invalid = vec![
            canonical[..5].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
        ];
        for (index, value) in [
            (0, 0xC5),
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 2),
            (1, (canonical[1] & !0x1F) | 4),
            (2, canonical[2] & !0x03),
            (2, (canonical[2] & !0x03) | 2),
            (2, (canonical[2] & !0x03) | 3),
            (3, 0x47),
            (3, 0x4A),
            (4, canonical[4] | 0xC0),
        ] {
            let mut bytes = canonical;
            bytes[index] = value;
            invalid.push(bytes.to_vec());
        }
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_vpermil2_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
