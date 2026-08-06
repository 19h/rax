//! VEX/EVEX binary16 narrowing-conversion replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{FpRoundMode, VecWidth};

/// One complete EVEX `VCVTPS2PH` memory-destination encoding rewritten to
/// target a borrowed vector register while retaining its architectural mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFp16NarrowMemoryEncoding {
    pub(crate) source: u8,
    pub(crate) scratch: u8,
    pub(crate) source_width: VecWidth,
    pub(crate) result_width: VecWidth,
    pub(crate) lanes: u8,
    pub(crate) memory_size: u32,
    pub(crate) writemask: Option<u8>,
    pub(crate) round: FpRoundMode,
    pub(crate) immediate: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

/// One complete F16C VEX `VCVTPS2PH` memory-destination encoding rewritten
/// to target a borrowed low XMM register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexFp16NarrowMemoryEncoding {
    pub(crate) source: u8,
    pub(crate) scratch: u8,
    pub(crate) source_width: VecWidth,
    pub(crate) lanes: u8,
    pub(crate) memory_size: u32,
    pub(crate) round: FpRoundMode,
    pub(crate) immediate: u8,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate one register-destination F16C VEX `VCVTPS2PH` instruction.
    ///
    /// The instruction requires map 0F3A, `pp=66`, `W=0`, and reserved
    /// `VEX.vvvv=1111b`; `VEX.L` selects four or eight FP32 source elements.
    /// ModRM.reg names the source and ModRM.r/m names the destination. VEX.X
    /// and all five high immediate bits are ignored but retained in the exact
    /// source-byte universe. Memory destinations and malformed shapes fail
    /// closed.
    pub fn is_vex_register_fp16_narrow(&self) -> bool {
        matches!(
            self.as_slice(),
            [0xC4, p0, p1, 0x1D, modrm, _]
                if p0 & 0x1F == 3 && p1 & 0xFB == 0x79 && modrm >> 6 == 3
        )
    }

    /// Return the architectural VEX destination after exact validation. The
    /// destination uses ModRM.r/m plus inverted VEX.B, not ModRM.reg/VEX.R.
    pub(crate) fn vex_fp16_narrow_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_fp16_narrow() {
            return None;
        }
        let [0xC4, p0, _, 0x1D, modrm, _] = self.as_slice() else {
            unreachable!("VEX FP16 narrowing shape was validated");
        };
        Some((modrm & 7) + if p0 & 0x20 == 0 { 8 } else { 0 })
    }

    /// Return the architectural FP32 source after exact register-form
    /// validation. The source uses ModRM.reg plus inverted VEX.R.
    pub(crate) fn vex_fp16_narrow_source_index(&self) -> Option<u8> {
        if !self.is_vex_register_fp16_narrow() {
            return None;
        }
        let [0xC4, p0, _, 0x1D, modrm, _] = self.as_slice() else {
            unreachable!("VEX FP16 narrowing shape was validated");
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }

    /// Rewrite only the ModR/M memory destination of one complete F16C VEX
    /// instruction, preserving the trailing imm8 exactly.
    fn vex_fp16_memory_with_register_destination(&self, destination: u8) -> Option<Self> {
        let (immediate, instruction) = self.as_slice().split_last()?;
        let instruction = Self::new(instruction)?;
        let rewritten = instruction.vex_memory_with_register_source(destination)?;
        let mut bytes = [0u8; 15];
        let len = rewritten.as_slice().len();
        if len == bytes.len() {
            return None;
        }
        bytes[..len].copy_from_slice(rewritten.as_slice());
        bytes[len] = *immediate;
        Self::new(&bytes[..=len])
    }

    /// Validate and rewrite one F16C VEX `VCVTPS2PH` memory destination.
    ///
    /// The instruction is VEX.128/256.66.0F3A.W0 1D /r ib, reserves
    /// VEX.vvvv=`1111b`, and converts four/eight FP32 source lanes selected by
    /// ModRM.reg into an unaligned 8-/16-byte memory destination. VEX.X
    /// participates in SIB index extension in the original memory form, then
    /// becomes ignored in the register rewrite; imm8[7:3] is ignored in both.
    /// Both fields remain exact source-byte provenance. A low XMM register
    /// distinct from the source is borrowed as the rewritten destination.
    pub(crate) fn vex_fp16_narrow_memory_encoding(&self) -> Option<X86VexFp16NarrowMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3
            || fields.pp != 1
            || fields.w
            || fields.opcode != 0x1D
            || fields.source1 != 0
        {
            return None;
        }
        let source_width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let (lanes, memory_size) = if fields.width_256 { (8, 16) } else { (4, 8) };
        let round = if immediate & 4 != 0 {
            FpRoundMode::Dynamic
        } else {
            match immediate & 3 {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        };
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one VEX source leaves fifteen low scratch registers");
        let register_instruction = self.vex_fp16_memory_with_register_destination(scratch)?;
        if register_instruction.vex_fp16_narrow_source_index() != Some(fields.destination)
            || register_instruction.vex_fp16_narrow_destination_index() != Some(scratch)
        {
            return None;
        }
        Some(X86VexFp16NarrowMemoryEncoding {
            source: fields.destination,
            scratch,
            source_width,
            lanes,
            memory_size,
            round,
            immediate,
            register_instruction,
        })
    }

    /// Validate and rewrite one EVEX `VCVTPS2PH` memory destination.
    ///
    /// Intel defines EVEX.128/256/512.66.0F3A.W0 1D /r ib with a Half-Mem
    /// 8-/16-/32-byte destination. EVEX.vvvv/V', EVEX.b, EVEX.z, and L'L=3
    /// are reserved for a memory destination; K1-K7 suppress individual
    /// 2-byte destination accesses. The rewritten register form preserves the
    /// source, vector length, writemask, and complete immediate while replacing
    /// only ModR/M.r/m with a borrowed low vector register. Segment,
    /// address-size, and APX B4/X4 state remain confined to helper address
    /// evaluation. Classification is O(1) time and O(1) space.
    pub(crate) fn evex_fp16_narrow_memory_encoding(
        &self,
    ) -> Option<X86EvexFp16NarrowMemoryEncoding> {
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
        if operand_end.checked_add(1)? != bytes.len()
            || p0 & 7 != 3
            || p1 & 0x83 != 0x01
            || p1 & 0x78 != 0x78
            || p2 & 0x98 != 0x08
            || opcode != 0x1D
        {
            return None;
        }

        let ll = (p2 >> 5) & 3;
        let (source_width, result_width, lanes, memory_size) = match ll {
            0 => (VecWidth::V128, VecWidth::V128, 4, 8),
            1 => (VecWidth::V256, VecWidth::V128, 8, 16),
            2 => (VecWidth::V512, VecWidth::V256, 16, 32),
            _ => return None,
        };
        let source =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let scratch = (0..32u8)
            .find(|candidate| *candidate != source)
            .expect("one EVEX source leaves thirty-one scratch registers");
        let immediate = bytes[operand_end];
        let round = if immediate & 4 != 0 {
            FpRoundMode::Dynamic
        } else {
            match immediate & 3 {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        };
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F3A, select an unextended register
            // destination, and clear memory-only APX B4/X4 state.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U after removing
            // APX X4 state from the helper-owned address.
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            immediate,
        ])?;
        let needs_avx512vl = source_width != VecWidth::V512;
        if register_instruction.evex_register_fp16_narrow_requirements()
            != Some((needs_avx512vl, false))
        {
            return None;
        }

        Some(X86EvexFp16NarrowMemoryEncoding {
            source,
            scratch,
            source_width,
            result_width,
            lanes,
            memory_size,
            writemask: (p2 & 7 != 0).then_some(p2 & 7),
            round,
            immediate,
            register_instruction,
            needs_avx512vl,
        })
    }

    /// Validate one register-only EVEX `VCVTPD2PH`, `VCVTPS2PH`, or
    /// `VCVTPS2PHX` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Ordinary 128-bit and
    /// 256-bit source forms require AVX-512VL. `VCVTPD2PH` and `VCVTPS2PHX`
    /// use all four `L'L` values as embedded rounding control when
    /// register-source `EVEX.b=1` implies a 512-bit source. `VCVTPS2PH` uses
    /// its immediate for rounding; register-source `EVEX.b=1` implies a
    /// 512-bit source with SAE and makes all four L'L bit images defined. The
    /// legacy-map `VCVTPS2PH` requires AVX-512F;
    /// `VCVTPD2PH` and `VCVTPS2PHX` require AVX-512-FP16. Memory forms and
    /// every reserved EVEX field fail closed.
    pub fn evex_register_fp16_narrow_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (needs_fp16, has_immediate) = match (map, pp, w, opcode) {
            // VCVTPS2PH is the AVX-512F conversion retained from F16C.
            (3, 1, false, 0x1D) => (false, true),
            // VCVTPD2PH and VCVTPS2PHX are AVX-512-FP16 conversions.
            (5, 1, true, 0x5A) | (5, 1, false, 0x1D) => (true, false),
            _ => return None,
        };
        if bytes.len() != if has_immediate { 7 } else { 6 } {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        if embedded_control {
            if has_immediate {
                // The immediate retains rounding control while EVEX.b fixes
                // the source width at 512 bits and supplies SAE.
                return Some((false, false));
            }
            // L'L encodes RN/RD/RU/RZ for the 512-bit ER forms.
            return Some((false, true));
        }
        match ll {
            0 | 1 => Some((true, needs_fp16)),
            2 => Some((false, needs_fp16)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(
        source: u8,
        base: u8,
        width_256: bool,
        immediate: u8,
        encoded_x: bool,
    ) -> Vec<u8> {
        assert!(source < 16 && base < 16);
        let mut bytes = vec![
            0xC4,
            (if source < 8 { 0x80 } else { 0 })
                | (u8::from(encoded_x) << 6)
                | (if base < 8 { 0x20 } else { 0 })
                | 3,
            0x79 | (u8::from(width_256) << 2),
            0x1D,
            0x40 | ((source & 7) << 3) | if base & 7 == 4 { 4 } else { base & 7 },
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.extend_from_slice(&[0x20, immediate]);
        bytes
    }

    #[test]
    fn memory_classifier_covers_sources_bases_widths_immediates_and_encoded_x() {
        let mut classified = 0usize;
        for source in 0..16 {
            for base in 0..16 {
                for width_256 in [false, true] {
                    for immediate in u8::MIN..=u8::MAX {
                        for encoded_x in [false, true] {
                            let bytes = instruction(source, base, width_256, immediate, encoded_x);
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_fp16_narrow_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.source, source);
                            assert_ne!(encoding.scratch, source);
                            assert_eq!(
                                encoding.source_width,
                                if width_256 {
                                    VecWidth::V256
                                } else {
                                    VecWidth::V128
                                }
                            );
                            assert_eq!(encoding.lanes, if width_256 { 8 } else { 4 });
                            assert_eq!(encoding.memory_size, if width_256 { 16 } else { 8 });
                            assert_eq!(encoding.immediate, immediate);
                            assert_eq!(
                                encoding.round,
                                if immediate & 4 != 0 {
                                    FpRoundMode::Dynamic
                                } else {
                                    match immediate & 3 {
                                        0 => FpRoundMode::RoundNearest,
                                        1 => FpRoundMode::RoundDown,
                                        2 => FpRoundMode::RoundUp,
                                        _ => FpRoundMode::RoundTowardZero,
                                    }
                                }
                            );
                            assert!(encoding.register_instruction.is_vex_register_fp16_narrow());
                            assert_eq!(
                                encoding.register_instruction.vex_fp16_narrow_source_index(),
                                Some(source)
                            );
                            assert_eq!(
                                encoding
                                    .register_instruction
                                    .vex_fp16_narrow_destination_index(),
                                Some(encoding.scratch)
                            );
                            assert_eq!(
                                encoding.register_instruction.as_slice().last(),
                                Some(&immediate)
                            );
                            assert_eq!(
                                encoding.register_instruction.as_slice()[1] & 0x40,
                                bytes[1] & 0x40,
                                "encoded VEX.X must survive the rewrite"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 256 * 2);
    }

    #[test]
    fn memory_classifier_accepts_defined_address_prefixes_and_shapes() {
        for bytes in [
            vec![0x64, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0xA5],
            vec![0x67, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0xA5],
            vec![0xC4, 0xE3, 0x79, 0x1D, 0x14, 0x8D, 0, 0, 0, 0, 0xA5],
            vec![0xC4, 0xE3, 0x79, 0x1D, 0x95, 0, 0, 0, 0, 0xA5],
        ] {
            let encoding = X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_fp16_narrow_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(encoding.immediate, 0xA5);
            assert!(encoding.register_instruction.is_vex_register_fp16_narrow());
        }
    }

    #[test]
    fn memory_classifier_rejects_reserved_register_and_nonexact_images() {
        let valid = instruction(9, 11, true, 0xA5, false);
        let mut invalid = Vec::new();

        let mut w1 = valid.clone();
        w1[2] |= 0x80;
        invalid.push(w1);
        let mut vvvv = valid.clone();
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut map = valid.clone();
        map[1] = (map[1] & !0x1F) | 2;
        invalid.push(map);
        let mut pp = valid.clone();
        pp[2] = (pp[2] & !3) | 2;
        invalid.push(pp);
        let mut opcode = valid.clone();
        opcode[3] = 0x1C;
        invalid.push(opcode);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.remove(5);
        invalid.push(register);
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);
        for end in 0..valid.len() {
            invalid.push(valid[..end].to_vec());
        }

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_fp16_narrow_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
