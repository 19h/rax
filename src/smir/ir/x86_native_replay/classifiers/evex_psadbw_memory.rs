//! EVEX `VPSADBW` memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PsadbwFields {
    width: VecWidth,
    destination: u8,
    source1: u8,
    w: bool,
}

/// Exact EVEX `VPSADBW` Full Mem encoding and its byte-validated
/// register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPsadbwMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) w: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

fn fields(p0: u8, p1: u8, p2: u8, opcode: u8, modrm: u8, memory: bool) -> Option<PsadbwFields> {
    let map = if memory { p0 & 0x07 } else { p0 & 0x0F };
    if map != 1
        || p1 & 0x03 != 1
        || (!memory && p1 & 0x04 == 0)
        || p2 & 0x97 != 0
        || opcode != 0xF6
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
    Some(PsadbwFields {
        width,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
        w: p1 & 0x80 != 0,
    })
}

fn register_fields(bytes: &[u8]) -> Option<(PsadbwFields, u8)> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    let classified = fields(*p0, *p1, *p2, *opcode, *modrm, false)?;
    let source2 = (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (modrm & 7);
    Some((classified, source2))
}

impl X86InstructionBytes {
    /// Validate one EVEX `VPSADBW` Full Mem source and select an exact
    /// register-source replay.
    ///
    /// Intel specifies map 0F, mandatory 66H, WIG, no writemask or broadcast,
    /// a Full Mem tuple, and Type E4NF.nb exceptions. The complete 16/32/64-
    /// byte access is therefore unconditional. Segment/address-size prefixes
    /// and APX B4/X4 address extensions remain confined to helper evaluation.
    pub(crate) fn evex_psadbw_memory_encoding(&self) -> Option<X86EvexPsadbwMemoryEncoding> {
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
        if memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }
        let classified = fields(p0, p1, p2, opcode, modrm, true)?;
        let scratch = (0..16u8)
            .find(|candidate| {
                *candidate != classified.destination && *candidate != classified.source1
            })
            .expect("two operands cannot consume every low vector register");
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Scratch is low, so X is one; clear APX B4.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve WIG/vvvv/66 and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve L'L and V'; z, b, and aaa were validated as zero.
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])?;
        if register_fields(register_instruction.as_slice()) != Some((classified, scratch)) {
            return None;
        }

        Some(X86EvexPsadbwMemoryEncoding {
            width: classified.width,
            destination: classified.destination,
            source1: classified.source1,
            w: classified.w,
            scratch,
            register_instruction,
            needs_avx512vl: classified.width != VecWidth::V512,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(width: VecWidth, destination: u8, source1: u8, w: bool, base: u8) -> Vec<u8> {
        let ll = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        };
        vec![
            0x62,
            0x41 | (u8::from(destination & 8 == 0) << 7)
                | (u8::from(destination & 16 == 0) << 4)
                | (u8::from(base & 8 == 0) << 5),
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
            (ll << 5) | (u8::from(source1 < 16) << 3),
            0xF6,
            ((destination & 7) << 3) | (base & 7),
        ]
    }

    #[test]
    fn classifies_all_24576_operand_width_wig_and_apx_address_cells() {
        let mut classified = 0usize;
        for destination in 0..32 {
            for source1 in 0..32 {
                for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                    for w in [false, true] {
                        let mut canonical = encoding(width, destination, source1, w, 2);
                        canonical[5] = (canonical[5] & 0x38) | 4;
                        canonical.push(0x48); // [RAX + RCX*2]
                        for b4 in [false, true] {
                            for x4 in [false, true] {
                                let mut bytes = canonical.clone();
                                bytes[1] |= u8::from(b4) << 3;
                                if x4 {
                                    bytes[2] &= !0x04;
                                }
                                let actual = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_psadbw_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(actual.width, width, "{bytes:02X?}");
                                assert_eq!(actual.destination, destination, "{bytes:02X?}");
                                assert_eq!(actual.source1, source1, "{bytes:02X?}");
                                assert_eq!(actual.w, w, "{bytes:02X?}");
                                assert_ne!(actual.scratch, destination, "{bytes:02X?}");
                                assert_ne!(actual.scratch, source1, "{bytes:02X?}");
                                assert_eq!(
                                    register_fields(actual.register_instruction.as_slice()),
                                    Some((
                                        PsadbwFields {
                                            width,
                                            destination,
                                            source1,
                                            w,
                                        },
                                        actual.scratch,
                                    )),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 32 * 32 * 3 * 2 * 2 * 2);
    }

    #[test]
    fn accepts_prefix_sib_displacement_and_apx_address_extensions() {
        // addr32 FS: VPSADBW zmm25,zmm18,[r20d+r29d*2+0x44332211]
        let mut bytes = encoding(VecWidth::V512, 25, 18, true, 4);
        bytes[1] &= !0x40; // X' selects index bit 3.
        bytes[5] |= 0x80; // mod=10, SIB, disp32.
        bytes.extend_from_slice(&[0x6C, 0x11, 0x22, 0x33, 0x44]);
        bytes.splice(0..0, [0x64, 0x67]);
        bytes[3] |= 0x08; // APX B4 selects base bit 4.
        bytes[4] &= !0x04; // APX X4 / EVEX.U=0 selects index bit 4.
        let actual = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_psadbw_memory_encoding()
            .unwrap();
        assert_eq!(actual.width, VecWidth::V512);
        assert_eq!(actual.destination, 25);
        assert_eq!(actual.source1, 18);
        assert!(actual.w);
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = encoding(VecWidth::V256, 19, 27, false, 10);
        let mut cases = Vec::new();
        for (index, mask) in [(1, 0x07), (2, 0x03), (3, 0x10), (3, 0x80), (3, 0x07)] {
            let mut bytes = valid.clone();
            bytes[index] ^= mask;
            cases.push(bytes);
        }
        let mut reserved_ll = valid.clone();
        reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
        cases.push(reserved_ll);
        let mut wrong_opcode = valid.clone();
        wrong_opcode[4] = 0xF5;
        cases.push(wrong_opcode);
        let mut register_source = valid.clone();
        register_source[5] |= 0xC0;
        cases.push(register_source);
        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);
        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);
        let mut truncated = valid;
        truncated.pop();
        cases.push(truncated);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_psadbw_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
