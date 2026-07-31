//! EVEX AVX-512BW shuffle/multiply-add memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Exact operation selected by one EVEX AVX-512BW Full Mem encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexBwShuffleMaddKind {
    ByteShuffle,
    MultiplyAddUnsignedBytes,
    MultiplyAddWords,
}

impl X86EvexBwShuffleMaddKind {
    fn map_opcode(self) -> (u8, u8) {
        match self {
            Self::ByteShuffle => (2, 0x00),
            Self::MultiplyAddUnsignedBytes => (2, 0x04),
            Self::MultiplyAddWords => (1, 0xF5),
        }
    }
}

/// Exact EVEX VPSHUFB/VPMADDUBSW/VPMADDWD memory encoding and its
/// byte-validated register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexBwShuffleMaddMemoryEncoding {
    pub(crate) kind: X86EvexBwShuffleMaddKind,
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) w: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BwShuffleMaddFields {
    kind: X86EvexBwShuffleMaddKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    zeroing: bool,
    w: bool,
}

fn operation(map: u8, opcode: u8) -> Option<X86EvexBwShuffleMaddKind> {
    match (map, opcode) {
        (2, 0x00) => Some(X86EvexBwShuffleMaddKind::ByteShuffle),
        (2, 0x04) => Some(X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes),
        (1, 0xF5) => Some(X86EvexBwShuffleMaddKind::MultiplyAddWords),
        _ => None,
    }
}

fn fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
    memory: bool,
) -> Option<BwShuffleMaddFields> {
    let map = if memory { p0 & 0x07 } else { p0 & 0x0F };
    let mask = p2 & 0x07;
    if p1 & 0x03 != 1
        || (!memory && p1 & 0x04 == 0)
        || p2 & 0x10 != 0
        || (p2 & 0x80 != 0 && mask == 0)
        || (memory == (modrm >> 6 == 3))
    {
        return None;
    }
    let width = match (p2 >> 5) & 3 {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    Some(BwShuffleMaddFields {
        kind: operation(map, opcode)?,
        width,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        writemask: (mask != 0).then_some(mask),
        zeroing: p2 & 0x80 != 0,
        w: p1 & 0x80 != 0,
    })
}

fn register_fields(bytes: &[u8]) -> Option<(BwShuffleMaddFields, u8)> {
    if bytes.len() != 6 || bytes[0] != 0x62 {
        return None;
    }
    let p0 = bytes[1];
    let result = fields(p0, bytes[2], bytes[3], bytes[4], bytes[5], false)?;
    let source2 =
        (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (bytes[5] & 7);
    Some((result, source2))
}

fn memory_fields(bytes: &[u8]) -> Option<(BwShuffleMaddFields, usize, usize)> {
    let start = vector_legacy_prefix_len(bytes);
    if bytes.get(start) != Some(&0x62) {
        return None;
    }
    let modrm_index = start + 5;
    if memory_operand_end(bytes, modrm_index)? != bytes.len() {
        return None;
    }
    Some((
        fields(
            *bytes.get(start + 1)?,
            *bytes.get(start + 2)?,
            *bytes.get(start + 3)?,
            *bytes.get(start + 4)?,
            *bytes.get(modrm_index)?,
            true,
        )?,
        start,
        modrm_index,
    ))
}

impl X86InstructionBytes {
    /// Validate one EVEX VPSHUFB, VPMADDUBSW, or VPMADDWD Full Mem source and
    /// select an exact helper-backed register replay.
    ///
    /// Intel assigns all three operations Type E4NF.nb semantics: the complete
    /// vector tuple is read irrespective of the destination writemask. EVEX.W
    /// is ignored but preserved in the replay. Segment/address-size prefixes
    /// and APX B4/X4 address extensions remain confined to helper evaluation.
    pub(crate) fn evex_bw_shuffle_madd_memory_encoding(
        &self,
    ) -> Option<X86EvexBwShuffleMaddMemoryEncoding> {
        let bytes = self.as_slice();
        let (classified, start, modrm_index) = memory_fields(bytes)?;
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[modrm_index];
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
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa exactly.
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
        if register_fields(register_instruction.as_slice()) != Some((classified, scratch)) {
            return None;
        }

        Some(X86EvexBwShuffleMaddMemoryEncoding {
            kind: classified.kind,
            width: classified.width,
            destination: classified.destination,
            source1: classified.source1,
            writemask: classified.writemask,
            zeroing: classified.zeroing,
            w: classified.w,
            scratch,
            register_instruction,
            memory_size: classified.width.bytes(),
            needs_avx512vl: classified.width != VecWidth::V512,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [X86EvexBwShuffleMaddKind; 3] = [
        X86EvexBwShuffleMaddKind::ByteShuffle,
        X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes,
        X86EvexBwShuffleMaddKind::MultiplyAddWords,
    ];

    fn encoding(
        kind: X86EvexBwShuffleMaddKind,
        width: VecWidth,
        destination: u8,
        source1: u8,
        mask: u8,
        zeroing: bool,
        w: bool,
    ) -> Vec<u8> {
        let ll = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        };
        let (map, opcode) = kind.map_opcode();
        vec![
            0x62,
            0x60 | map
                | (u8::from(destination & 8 == 0) << 7)
                | (u8::from(destination & 16 == 0) << 4),
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
            (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(source1 < 16) << 3) | mask,
            opcode,
            ((destination & 7) << 3) | 2,
        ]
    }

    #[test]
    fn classifier_exhaustively_covers_55_296_kind_width_register_mask_and_w_cells() {
        let controls = [(0u8, false), (3, false), (5, true)];
        let mut classified_count = 0usize;
        for kind in KINDS {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0u8..32 {
                    for source1 in 0u8..32 {
                        for (mask, zeroing) in controls {
                            for w in [false, true] {
                                let bytes =
                                    encoding(kind, width, destination, source1, mask, zeroing, w);
                                let classified = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_bw_shuffle_madd_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(classified.kind, kind, "{bytes:02X?}");
                                assert_eq!(classified.width, width, "{bytes:02X?}");
                                assert_eq!(classified.destination, destination, "{bytes:02X?}");
                                assert_eq!(classified.source1, source1, "{bytes:02X?}");
                                assert_eq!(
                                    classified.writemask,
                                    (mask != 0).then_some(mask),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(classified.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(classified.w, w, "{bytes:02X?}");
                                assert_ne!(classified.scratch, destination, "{bytes:02X?}");
                                assert_ne!(classified.scratch, source1, "{bytes:02X?}");
                                assert_eq!(
                                    register_fields(classified.register_instruction.as_slice()),
                                    Some((
                                        BwShuffleMaddFields {
                                            kind,
                                            width,
                                            destination,
                                            source1,
                                            writemask: (mask != 0).then_some(mask),
                                            zeroing,
                                            w,
                                        },
                                        classified.scratch,
                                    )),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(classified.memory_size, width.bytes());
                                assert_eq!(classified.needs_avx512vl, width != VecWidth::V512);
                                classified_count += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified_count, 3 * 3 * 32 * 32 * 3 * 2);
    }

    #[test]
    fn rewrites_match_six_independent_llvm_23_anchors() {
        let anchors: [(&[u8], &[u8]); 6] = [
            (
                &[0x62, 0xF2, 0x6D, 0x0B, 0x00, 0x0C, 0x24],
                &[0x62, 0xF2, 0x6D, 0x0B, 0x00, 0xC8],
            ),
            (
                &[0x62, 0xE2, 0x6D, 0xA4, 0x00, 0x0C, 0x24],
                &[0x62, 0xE2, 0x6D, 0xA4, 0x00, 0xC8],
            ),
            (
                &[0x62, 0x42, 0x2D, 0x45, 0x00, 0x4B, 0x01],
                &[0x62, 0x62, 0x2D, 0x45, 0x00, 0xC8],
            ),
            (
                &[0x62, 0x62, 0x7D, 0x87, 0x04, 0x3C, 0x24],
                &[0x62, 0x62, 0x7D, 0x87, 0x04, 0xF8],
            ),
            (
                &[0x62, 0xF2, 0x75, 0x4E, 0x04, 0x04, 0x24],
                &[0x62, 0xF2, 0x75, 0x4E, 0x04, 0xC2],
            ),
            (
                &[0x62, 0x51, 0x0D, 0x29, 0xF5, 0x4B, 0x01],
                &[0x62, 0x71, 0x0D, 0x29, 0xF5, 0xC8],
            ),
        ];
        for (memory, expected) in anchors {
            let classified = X86InstructionBytes::new(memory)
                .unwrap()
                .evex_bw_shuffle_madd_memory_encoding()
                .unwrap_or_else(|| panic!("{memory:02X?}"));
            assert_eq!(
                classified.register_instruction.as_slice(),
                expected,
                "{memory:02X?}"
            );
        }
    }

    #[test]
    fn classifier_preserves_address_controls_and_rejects_nonowned_shapes() {
        let mut prefixed = encoding(
            X86EvexBwShuffleMaddKind::MultiplyAddWords,
            VecWidth::V512,
            25,
            26,
            5,
            false,
            true,
        );
        prefixed.splice(0..0, [0x64, 0x67]);
        prefixed[3] |= 0x08; // APX B4
        prefixed[4] &= !0x04; // APX X4 / EVEX.U=0
        assert!(
            X86InstructionBytes::new(&prefixed)
                .unwrap()
                .evex_bw_shuffle_madd_memory_encoding()
                .is_some()
        );

        let base = encoding(
            X86EvexBwShuffleMaddKind::ByteShuffle,
            VecWidth::V256,
            9,
            14,
            1,
            false,
            false,
        );
        let mut candidates = Vec::new();
        let mut wrong_map = base.clone();
        wrong_map[1] = (wrong_map[1] & !0x07) | 3;
        candidates.push(wrong_map);
        let mut wrong_prefix = base.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        candidates.push(wrong_prefix);
        let mut wrong_opcode = base.clone();
        wrong_opcode[4] = 0x01;
        candidates.push(wrong_opcode);
        let mut register = base.clone();
        register[5] |= 0xC0;
        candidates.push(register);
        let mut broadcast = base.clone();
        broadcast[3] |= 0x10;
        candidates.push(broadcast);
        let mut reserved_length = base.clone();
        reserved_length[3] = (reserved_length[3] & !0x60) | 0x60;
        candidates.push(reserved_length);
        let mut reserved_zeroing = base.clone();
        reserved_zeroing[3] = (reserved_zeroing[3] & !0x07) | 0x80;
        candidates.push(reserved_zeroing);
        candidates.push(base[..base.len() - 1].to_vec());
        candidates.push(base.iter().copied().chain([0]).collect());

        for bytes in candidates {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_bw_shuffle_madd_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
