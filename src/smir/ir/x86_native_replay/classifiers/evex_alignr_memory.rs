//! EVEX `VPALIGNR` memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Exact EVEX `VPALIGNR` Full Mem encoding and its byte-validated
/// register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexAlignrMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    /// EVEX.vvvv supplies the high half of each 16-byte concatenation.
    pub(crate) high: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX `VPALIGNR` Full Mem source and select an exact
    /// register-source replay.
    ///
    /// Intel specifies map 0F3A, mandatory 66H, WIG, a Full Mem tuple,
    /// byte-granular writemasking, and Type E4NF.nb exceptions. EVEX.b is
    /// therefore reserved and the complete vector access is unconditional.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_alignr_memory_encoding(&self) -> Option<X86EvexAlignrMemoryEncoding> {
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
        let zeroing = p2 & 0x80 != 0;
        if p0 & 0x07 != 3
            || p1 & 0x03 != 1
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || opcode != 0x0F
            || (zeroing && mask == 0)
            || operand_end.checked_add(1)? != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let high = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != high)
            .expect("two operands cannot consume every low vector register");
        let needs_avx512vl = width != VecWidth::V512;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Clear APX B4 and retain destination extensions.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            bytes[operand_end],
        ])
        .unwrap();
        if register_instruction.evex_register_bw_immediate_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        Some(X86EvexAlignrMemoryEncoding {
            width,
            destination,
            high,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            immediate: bytes[operand_end],
            w: p1 & 0x80 != 0,
            scratch,
            register_instruction,
            needs_avx512vl,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(
        width: VecWidth,
        destination: u8,
        high: u8,
        mask: u8,
        zeroing: bool,
        w: bool,
        immediate: u8,
    ) -> Vec<u8> {
        assert!(destination < 32 && high < 32);
        assert!(mask < 8 && (!zeroing || mask != 0));
        let ll = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!("EVEX VPALIGNR width"),
        };
        let p0 = 0x63
            | if destination & 8 == 0 { 0x80 } else { 0 }
            | if destination & 16 == 0 { 0x10 } else { 0 };
        vec![
            0x62,
            p0,
            (u8::from(w) << 7) | (((!high) & 0x0F) << 3) | 0x05,
            (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(high < 16) << 3) | mask,
            0x0F,
            ((destination & 7) << 3) | 3,
            immediate,
        ]
    }

    #[test]
    fn classifier_exhaustively_covers_18_432_wig_width_register_and_mask_cells() {
        let controls = [(0u8, false), (1, false), (2, true)];
        let mut classified = 0usize;
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0u8..32 {
                for high in 0u8..32 {
                    for (mask, zeroing) in controls {
                        for w in [false, true] {
                            let immediate = destination
                                .wrapping_mul(17)
                                .wrapping_add(high.wrapping_mul(29))
                                .wrapping_add(mask);
                            let bytes =
                                encoding(width, destination, high, mask, zeroing, w, immediate);
                            let classified_encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_alignr_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(classified_encoding.width, width, "{bytes:02X?}");
                            assert_eq!(
                                classified_encoding.destination, destination,
                                "{bytes:02X?}"
                            );
                            assert_eq!(classified_encoding.high, high, "{bytes:02X?}");
                            assert_eq!(
                                classified_encoding.writemask,
                                (mask != 0).then_some(mask),
                                "{bytes:02X?}"
                            );
                            assert_eq!(classified_encoding.zeroing, zeroing, "{bytes:02X?}");
                            assert_eq!(classified_encoding.immediate, immediate, "{bytes:02X?}");
                            assert_eq!(classified_encoding.w, w, "{bytes:02X?}");
                            assert_eq!(
                                classified_encoding.needs_avx512vl,
                                width != VecWidth::V512,
                                "{bytes:02X?}"
                            );
                            assert_ne!(classified_encoding.scratch, destination, "{bytes:02X?}");
                            assert_ne!(classified_encoding.scratch, high, "{bytes:02X?}");
                            assert_eq!(
                                classified_encoding
                                    .register_instruction
                                    .evex_register_bw_immediate_needs_vl(),
                                Some(width != VecWidth::V512),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 3 * 32 * 32 * controls.len() * 2);
    }

    #[test]
    fn rewrites_match_four_independent_llvm_23_anchors() {
        let anchors: [(&[u8], &[u8]); 4] = [
            (
                &[0x62, 0xF3, 0x6D, 0x0B, 0x0F, 0x0C, 0x24, 0xA5],
                &[0x62, 0xF3, 0x6D, 0x0B, 0x0F, 0xC8, 0xA5],
            ),
            (
                &[0x62, 0xE3, 0x6D, 0xA4, 0x0F, 0x0C, 0x24, 0x00],
                &[0x62, 0xE3, 0x6D, 0xA4, 0x0F, 0xC8, 0x00],
            ),
            (
                &[0x62, 0x63, 0x2D, 0x45, 0x0F, 0x0C, 0x24, 0xFF],
                &[0x62, 0x63, 0x2D, 0x45, 0x0F, 0xC8, 0xFF],
            ),
            (
                &[0x62, 0x53, 0x0D, 0x29, 0x0F, 0x4B, 0x01, 0x63],
                &[0x62, 0x73, 0x0D, 0x29, 0x0F, 0xC8, 0x63],
            ),
        ];
        for (memory, expected) in anchors {
            let classified = X86InstructionBytes::new(memory)
                .unwrap()
                .evex_alignr_memory_encoding()
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
        let mut prefixed = encoding(VecWidth::V512, 25, 26, 5, false, true, 0xA5);
        prefixed.splice(0..0, [0x64, 0x67]);
        prefixed[3] |= 0x08; // APX B4
        prefixed[4] &= !0x04; // APX X4 / EVEX.U=0
        let classified = X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_alignr_memory_encoding()
            .expect("segment, addr32, and APX address controls");
        assert_eq!(
            classified.register_instruction.as_slice(),
            [0x62, 0x63, 0xAD, 0x45, 0x0F, 0xC8, 0xA5]
        );

        let base = encoding(VecWidth::V256, 9, 14, 1, false, false, 0x63);
        let mut candidates = Vec::new();
        for (index, mask) in [(1, 0x07), (2, 0x01), (4, 0x01)] {
            let mut bytes = base.clone();
            bytes[index] ^= mask;
            candidates.push(bytes);
        }
        let mut broadcast = base.clone();
        broadcast[3] |= 0x10;
        candidates.push(broadcast);
        let mut register = base.clone();
        register[5] |= 0xC0;
        candidates.push(register);
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
                    .evex_alignr_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
