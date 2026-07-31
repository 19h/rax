//! EVEX VPERMI2*/VPERMT2* memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Native replay strategy for one exact EVEX two-table-permute memory
/// encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexTwoTablePermuteMemoryReplay {
    /// An unconditional complete-vector helper load followed by a
    /// register-source rewrite using one borrowed low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// An unconditional scalar helper load followed by the original broadcast
    /// operation rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        memory_width: MemWidth,
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX VPERMI2*/VPERMT2* memory encoding and its byte-validated native
/// replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexTwoTablePermuteMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    /// EVEX.vvvv: table 1 for VPERMI2*, indices for VPERMT2*.
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) overwrite_table: bool,
    pub(crate) replay: X86EvexTwoTablePermuteMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512vbmi: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TwoTableFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    zeroing: bool,
    overwrite_table: bool,
    broadcast: bool,
}

fn width(p2: u8) -> Option<VecWidth> {
    match (p2 >> 5) & 3 {
        0 => Some(VecWidth::V128),
        1 => Some(VecWidth::V256),
        2 => Some(VecWidth::V512),
        _ => None,
    }
}

fn operation(opcode: u8, w: bool) -> Option<(VecElementType, bool)> {
    let overwrite_table = matches!(opcode, 0x7D..=0x7F);
    let elem = match (opcode, w) {
        (0x75 | 0x7D, false) => VecElementType::I8,
        (0x75 | 0x7D, true) => VecElementType::I16,
        (0x76 | 0x7E, false) => VecElementType::I32,
        (0x76 | 0x7E, true) => VecElementType::I64,
        (0x77 | 0x7F, false) => VecElementType::F32,
        (0x77 | 0x7F, true) => VecElementType::F64,
        _ => return None,
    };
    Some((elem, overwrite_table))
}

fn fields(p0: u8, p1: u8, p2: u8, opcode: u8, modrm: u8, memory: bool) -> Option<TwoTableFields> {
    // Memory EVEX.U may encode APX X4; register EVEX.U is fixed to one.
    if p0 & 0x07 != 2
        || p1 & 0x03 != 1
        || (!memory && p1 & 0x04 == 0)
        || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        || (memory == (modrm >> 6 == 3))
    {
        return None;
    }
    let width = width(p2)?;
    let (elem, overwrite_table) = operation(opcode, p1 & 0x80 != 0)?;
    let broadcast = p2 & 0x10 != 0;
    if (!memory && broadcast)
        || (broadcast && matches!(elem, VecElementType::I8 | VecElementType::I16))
    {
        return None;
    }
    Some(TwoTableFields {
        width,
        elem,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        writemask: (p2 & 0x07 != 0).then_some(p2 & 0x07),
        zeroing: p2 & 0x80 != 0,
        overwrite_table,
        broadcast,
    })
}

fn register_fields(bytes: &[u8]) -> Option<(TwoTableFields, u8)> {
    if bytes.len() != 6 || bytes[0] != 0x62 {
        return None;
    }
    let p0 = bytes[1];
    let result = fields(p0, bytes[2], bytes[3], bytes[4], bytes[5], false)?;
    let source2 =
        (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (bytes[5] & 7);
    Some((result, source2))
}

fn memory_fields(bytes: &[u8]) -> Option<(TwoTableFields, usize, usize)> {
    let start = vector_legacy_prefix_len(bytes);
    if bytes.get(start) != Some(&0x62) {
        return None;
    }
    let modrm_index = start + 5;
    let operand_end = memory_operand_end(bytes, modrm_index)?;
    if operand_end != bytes.len() {
        return None;
    }
    let result = fields(
        *bytes.get(start + 1)?,
        *bytes.get(start + 2)?,
        *bytes.get(start + 3)?,
        *bytes.get(start + 4)?,
        *bytes.get(modrm_index)?,
        true,
    )?;
    Some((result, start, modrm_index))
}

impl X86InstructionBytes {
    /// Validate one EVEX VPERMI2*/VPERMT2* full-vector or scalar-broadcast
    /// memory source and select an exact helper-backed native replay.
    ///
    /// Intel assigns Type E4NF.nb to byte/word forms and Type E4NF to
    /// dword/qword/binary32/binary64 forms. Both access classes read the
    /// complete memory tuple irrespective of index selection and writemasking.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_two_table_permute_memory_encoding(
        &self,
    ) -> Option<X86EvexTwoTablePermuteMemoryEncoding> {
        let bytes = self.as_slice();
        let (classified, start, modrm_index) = memory_fields(bytes)?;
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[modrm_index];
        let needs_avx512vl = classified.width != VecWidth::V512;
        let needs_avx512vbmi = classified.elem == VecElementType::I8;

        let replay = if classified.broadcast {
            let memory_width = match classified.elem {
                VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
                VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
                _ => return None,
            };
            let rewritten = [
                0x62,
                // Preserve R/R' and map 0F38, select unextended SIB
                // index/base, and clear APX B4 for architectural RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore ordinary EVEX.U.
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ];
            let stack_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            let (rewritten_fields, _, rewritten_modrm) =
                memory_fields(stack_instruction.as_slice())?;
            if rewritten_fields != classified
                || stack_instruction.as_slice()[rewritten_modrm] & 7 != 4
            {
                return None;
            }
            X86EvexTwoTablePermuteMemoryReplay::Broadcast {
                memory_width,
                stack_instruction,
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| {
                    *candidate != classified.destination && *candidate != classified.source1
                })
                .expect("two operands cannot consume every low vector register");
            let rewritten = [
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Scratch is low, so X is one; clear APX B4.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ];
            let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            if register_fields(register_instruction.as_slice()) != Some((classified, scratch)) {
                return None;
            }
            X86EvexTwoTablePermuteMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexTwoTablePermuteMemoryReplay::Vector { .. } => classified.width.bytes(),
            X86EvexTwoTablePermuteMemoryReplay::Broadcast { memory_width, .. } => {
                memory_width.bytes()
            }
        };

        Some(X86EvexTwoTablePermuteMemoryEncoding {
            width: classified.width,
            elem: classified.elem,
            destination: classified.destination,
            source1: classified.source1,
            writemask: classified.writemask,
            zeroing: classified.zeroing,
            overwrite_table: classified.overwrite_table,
            replay,
            memory_size,
            needs_avx512vl,
            needs_avx512vbmi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Kind {
        elem: VecElementType,
        overwrite_table: bool,
    }

    impl Kind {
        fn opcode_w(self) -> (u8, bool) {
            match (self.elem, self.overwrite_table) {
                (VecElementType::I8, false) => (0x75, false),
                (VecElementType::I16, false) => (0x75, true),
                (VecElementType::I32, false) => (0x76, false),
                (VecElementType::I64, false) => (0x76, true),
                (VecElementType::F32, false) => (0x77, false),
                (VecElementType::F64, false) => (0x77, true),
                (VecElementType::I8, true) => (0x7D, false),
                (VecElementType::I16, true) => (0x7D, true),
                (VecElementType::I32, true) => (0x7E, false),
                (VecElementType::I64, true) => (0x7E, true),
                (VecElementType::F32, true) => (0x7F, false),
                (VecElementType::F64, true) => (0x7F, true),
                _ => unreachable!(),
            }
        }
    }

    const KINDS: [Kind; 12] = [
        Kind {
            elem: VecElementType::I8,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::I16,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::I32,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::I64,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::F32,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::F64,
            overwrite_table: false,
        },
        Kind {
            elem: VecElementType::I8,
            overwrite_table: true,
        },
        Kind {
            elem: VecElementType::I16,
            overwrite_table: true,
        },
        Kind {
            elem: VecElementType::I32,
            overwrite_table: true,
        },
        Kind {
            elem: VecElementType::I64,
            overwrite_table: true,
        },
        Kind {
            elem: VecElementType::F32,
            overwrite_table: true,
        },
        Kind {
            elem: VecElementType::F64,
            overwrite_table: true,
        },
    ];

    fn encoding(
        kind: Kind,
        width: VecWidth,
        destination: u8,
        source1: u8,
        mask: u8,
        zeroing: bool,
        broadcast: bool,
    ) -> Vec<u8> {
        let ll = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        };
        let (opcode, w) = kind.opcode_w();
        vec![
            0x62,
            0x62 | (u8::from(destination & 8 == 0) << 7) | (u8::from(destination & 16 == 0) << 4),
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
            (u8::from(zeroing) << 7)
                | (ll << 5)
                | (u8::from(broadcast) << 4)
                | (u8::from(source1 < 16) << 3)
                | mask,
            opcode,
            ((destination & 7) << 3) | 2,
        ]
    }

    #[test]
    fn classifier_exhaustively_covers_184_320_family_width_register_mask_and_tuple_cells() {
        let controls = [(0u8, false), (3, false), (5, true)];
        let mut classified_count = 0usize;
        for kind in KINDS {
            let broadcasts: &[bool] =
                if matches!(kind.elem, VecElementType::I8 | VecElementType::I16) {
                    &[false]
                } else {
                    &[false, true]
                };
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0u8..32 {
                    for source1 in 0u8..32 {
                        for (mask, zeroing) in controls {
                            for &broadcast in broadcasts {
                                let bytes = encoding(
                                    kind,
                                    width,
                                    destination,
                                    source1,
                                    mask,
                                    zeroing,
                                    broadcast,
                                );
                                let classified = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_two_table_permute_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(classified.width, width, "{bytes:02X?}");
                                assert_eq!(classified.elem, kind.elem, "{bytes:02X?}");
                                assert_eq!(classified.destination, destination, "{bytes:02X?}");
                                assert_eq!(classified.source1, source1, "{bytes:02X?}");
                                assert_eq!(
                                    classified.writemask,
                                    (mask != 0).then_some(mask),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(classified.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(
                                    classified.overwrite_table, kind.overwrite_table,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    classified.memory_size,
                                    if broadcast {
                                        kind.elem.bytes()
                                    } else {
                                        width.bytes()
                                    },
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    classified.needs_avx512vl,
                                    width != VecWidth::V512,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    classified.needs_avx512vbmi,
                                    kind.elem == VecElementType::I8,
                                    "{bytes:02X?}"
                                );
                                match classified.replay {
                                    X86EvexTwoTablePermuteMemoryReplay::Vector {
                                        scratch,
                                        register_instruction,
                                    } => {
                                        assert!(!broadcast, "{bytes:02X?}");
                                        assert_ne!(scratch, destination, "{bytes:02X?}");
                                        assert_ne!(scratch, source1, "{bytes:02X?}");
                                        assert_eq!(
                                            register_fields(register_instruction.as_slice()),
                                            Some((
                                                TwoTableFields {
                                                    width,
                                                    elem: kind.elem,
                                                    destination,
                                                    source1,
                                                    writemask: (mask != 0).then_some(mask),
                                                    zeroing,
                                                    overwrite_table: kind.overwrite_table,
                                                    broadcast: false,
                                                },
                                                scratch,
                                            )),
                                            "{bytes:02X?}"
                                        );
                                    }
                                    X86EvexTwoTablePermuteMemoryReplay::Broadcast {
                                        memory_width,
                                        stack_instruction,
                                    } => {
                                        assert!(broadcast, "{bytes:02X?}");
                                        assert_eq!(memory_width.bytes(), kind.elem.bytes());
                                        assert!(
                                            memory_fields(stack_instruction.as_slice()).is_some()
                                        );
                                    }
                                }
                                classified_count += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified_count, 3 * 32 * 32 * controls.len() * 20);
    }

    #[test]
    fn rewrites_match_six_independent_llvm_23_anchors() {
        let anchors: [(&[u8], &[u8]); 6] = [
            (
                &[0x62, 0xF2, 0x6D, 0x0B, 0x75, 0x0C, 0x24],
                &[0x62, 0xF2, 0x6D, 0x0B, 0x75, 0xC8],
            ),
            (
                &[0x62, 0xE2, 0xED, 0xA4, 0x7D, 0x0C, 0x24],
                &[0x62, 0xE2, 0xED, 0xA4, 0x7D, 0xC8],
            ),
            (
                &[0x62, 0x62, 0x2D, 0x55, 0x76, 0x0C, 0x24],
                &[0x62, 0x62, 0x2D, 0x55, 0x76, 0x0C, 0x24],
            ),
            (
                &[0x62, 0x52, 0x8D, 0x29, 0x7E, 0x4B, 0x01],
                &[0x62, 0x72, 0x8D, 0x29, 0x7E, 0xC8],
            ),
            (
                &[0x62, 0x62, 0x7D, 0x97, 0x77, 0x3C, 0x24],
                &[0x62, 0x62, 0x7D, 0x97, 0x77, 0x3C, 0x24],
            ),
            (
                &[0x62, 0xF2, 0xF5, 0x5E, 0x7F, 0x04, 0x24],
                &[0x62, 0xF2, 0xF5, 0x5E, 0x7F, 0x04, 0x24],
            ),
        ];
        for (memory, expected) in anchors {
            let classified = X86InstructionBytes::new(memory)
                .unwrap()
                .evex_two_table_permute_memory_encoding()
                .unwrap_or_else(|| panic!("{memory:02X?}"));
            let actual = match classified.replay {
                X86EvexTwoTablePermuteMemoryReplay::Vector {
                    register_instruction,
                    ..
                } => register_instruction,
                X86EvexTwoTablePermuteMemoryReplay::Broadcast {
                    stack_instruction, ..
                } => stack_instruction,
            };
            assert_eq!(actual.as_slice(), expected, "{memory:02X?}");
        }
    }

    #[test]
    fn classifier_preserves_address_controls_and_rejects_nonowned_shapes() {
        let kind = Kind {
            elem: VecElementType::I64,
            overwrite_table: true,
        };
        let mut prefixed = encoding(kind, VecWidth::V512, 25, 26, 5, false, false);
        prefixed.splice(0..0, [0x64, 0x67]);
        prefixed[3] |= 0x08; // APX B4
        prefixed[4] &= !0x04; // APX X4 / EVEX.U=0
        let classified = X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_two_table_permute_memory_encoding()
            .expect("segment, addr32, and APX address controls");
        assert!(matches!(
            classified.replay,
            X86EvexTwoTablePermuteMemoryReplay::Vector { .. }
        ));

        let base = encoding(kind, VecWidth::V256, 9, 14, 1, false, false);
        let mut candidates = Vec::new();
        for (index, mask) in [(1, 0x07), (2, 0x01), (4, 0x80)] {
            let mut bytes = base.clone();
            bytes[index] ^= mask;
            candidates.push(bytes);
        }
        let mut register = base.clone();
        register[5] |= 0xC0;
        candidates.push(register);
        let mut reserved_length = base.clone();
        reserved_length[3] = (reserved_length[3] & !0x60) | 0x60;
        candidates.push(reserved_length);
        let mut reserved_zeroing = base.clone();
        reserved_zeroing[3] = (reserved_zeroing[3] & !0x07) | 0x80;
        candidates.push(reserved_zeroing);
        let mut byte_broadcast = encoding(
            Kind {
                elem: VecElementType::I8,
                overwrite_table: false,
            },
            VecWidth::V128,
            1,
            2,
            0,
            false,
            false,
        );
        byte_broadcast[3] |= 0x10;
        candidates.push(byte_broadcast);
        candidates.push(base[..base.len() - 1].to_vec());
        candidates.push(base.iter().copied().chain([0]).collect());

        for bytes in candidates {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_two_table_permute_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
