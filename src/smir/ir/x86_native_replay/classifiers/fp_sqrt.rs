//! x86 floating-point square-root replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX `VSQRTPS`, `VSQRTPD`, `VSQRTSS`, or
    /// `VSQRTSD` instruction whose final source is memory and return
    /// `(destination, scalar merge source, element, width, memory size, W)`.
    ///
    /// Packed forms reserve VEX.vvvv and derive their exact 128- or 256-bit
    /// memory footprint from VEX.L. Scalar forms consume VEX.vvvv as their
    /// upper-lane merge source and read exactly 4 or 8 bytes. Although the
    /// scalar opcode table labels VEX.L as ignored, Intel documents VEX.L=1
    /// behavior as generation-dependent unpredictable, so only VEX.L=0 is
    /// admitted. VEX.W is ignored for all four instructions. Runtime and
    /// auxiliary space are O(1).
    pub(crate) fn vex_memory_fp_sqrt_fields(
        &self,
    ) -> Option<(u8, Option<u8>, VecElementType, VecWidth, u32, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.opcode != 0x51 {
            return None;
        }

        let (scalar, elem) = match fields.pp {
            0 => (false, VecElementType::F32),
            1 => (false, VecElementType::F64),
            2 => (true, VecElementType::F32),
            3 => (true, VecElementType::F64),
            _ => unreachable!("VEX pp is a two-bit field"),
        };
        if scalar {
            if fields.width_256 {
                return None;
            }
            return Some((
                fields.destination,
                Some(fields.source1),
                elem,
                VecWidth::V128,
                elem.bytes(),
                fields.w,
            ));
        }
        if fields.source1 != 0 {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        Some((
            fields.destination,
            None,
            elem,
            width,
            width.bytes(),
            fields.w,
        ))
    }

    /// Validate one register-only legacy SSE or AVX VEX
    /// `SQRTPS`/`SQRTPD`/`SQRTSS`/`SQRTSD` instruction and report whether it
    /// requires AVX.
    ///
    /// Legacy forms accept the canonical mandatory-prefix position, an
    /// optional REX prefix, and a register ModR/M source. VEX forms require map
    /// 0F and a register source. Packed VEX forms reserve `vvvv`, while scalar
    /// forms use it as the upper-lane merge source. Scalar `VEX.L=1` is kept at
    /// the interpreter boundary because Intel documents generation-dependent
    /// unpredictable behavior for that encoding. Memory forms remain excluded
    /// so replay cannot bypass guest translation or fault handling.
    pub fn legacy_vex_register_fp_sqrt_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, 0x51, modrm] => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x0F, 0x51, modrm] => Some(*modrm),
            [0x40..=0x4F, 0x0F, 0x51, modrm] => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x40..=0x4F, 0x0F, 0x51, modrm] => Some(*modrm),
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return None,
        };
        if opcode != 0x51 || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        let packed = pp <= 1;
        if (packed && p1 & 0x78 != 0x78) || (!packed && p1 & 0x04 != 0) {
            return None;
        }
        Some(true)
    }

    /// Validate one register-only EVEX `VSQRTPS`, `VSQRTPD`, `VSQRTSS`,
    /// `VSQRTSD`, or `VSQRTPH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Packed 128-bit and
    /// 256-bit forms require AVX-512VL, except that register-source
    /// `EVEX.b=1` selects a 512-bit operation and uses `L'L` as embedded
    /// rounding control. Scalar forms are LLIG and never require AVX-512VL;
    /// without embedded rounding, they accept only the three defined EVEX
    /// vector-length encodings.
    /// Binary16 packed forms require AVX-512-FP16. `VSQRTSH` remains owned by
    /// the disjoint scalar-FP16 arithmetic replay classifier. Memory forms and
    /// every reserved EVEX field fail closed.
    pub fn evex_register_fp_sqrt_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || opcode != 0x51 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (scalar, needs_fp16) = match (map, pp, w) {
            // VSQRTPS, VSQRTPD, VSQRTSS, and VSQRTSD.
            (1, 0, false) | (1, 1, true) => (false, false),
            (1, 2, false) | (1, 3, true) => (true, false),
            // VSQRTPH. MAP5/F3 is VSQRTSH and is deliberately classified by
            // evex_register_scalar_fp16_arithmetic_needs_vl instead.
            (5, 0, false) => (false, true),
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if zeroing && mask == 0 {
            return None;
        }

        if scalar {
            // Scalar forms consume vvvv/V' as source 1. L'L is LLIG when b=0,
            // where the reserved 11b vector-length encoding remains invalid,
            // and selects one of four rounding controls when b=1.
            return (embedded_control || ll != 3).then_some((false, false));
        }

        // Packed forms reserve vvvv/V' to their all-ones encodings.
        if p1 & 0x78 != 0x78 || p2 & 0x08 == 0 {
            return None;
        }
        if embedded_control {
            // Register-source EVEX.b implies VL=512 and all four L'L values
            // are valid rounding controls.
            Some((false, needs_fp16))
        } else {
            match ll {
                0 | 1 => Some((true, needs_fp16)),
                2 => Some((false, needs_fp16)),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    enum Form {
        C5,
        C4 { w: bool },
    }

    fn instruction(
        destination: u8,
        source1: Option<u8>,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        form: Form,
    ) -> Vec<u8> {
        let pp = match (source1.is_some(), elem) {
            (false, VecElementType::F32) => 0,
            (false, VecElementType::F64) => 1,
            (true, VecElementType::F32) => 2,
            (true, VecElementType::F64) => 3,
            _ => unreachable!(),
        };
        let encoded_vvvv = source1.map_or(0x0F, |index| !index & 0x0F);
        let l = u8::from(width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (l << 2) | pp,
                    0x51,
                    modrm,
                    0x20,
                ]
            }
            Form::C4 { w } => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(w) << 7) | (encoded_vvvv << 3) | (l << 2) | pp,
                0x51,
                modrm,
                0x20,
            ],
        }
    }

    #[test]
    fn classifies_all_2880_vex_sqrt_operand_format_width_w_and_base_cells() {
        let mut classified = 0usize;
        let packed_sources = [None];
        let scalar_sources = (0..16).map(Some).collect::<Vec<_>>();
        let packed_widths = [VecWidth::V128, VecWidth::V256];
        let scalar_widths = [VecWidth::V128];
        for (source1s, elem, widths) in [
            (
                packed_sources.as_slice(),
                VecElementType::F32,
                packed_widths.as_slice(),
            ),
            (
                packed_sources.as_slice(),
                VecElementType::F64,
                packed_widths.as_slice(),
            ),
            (
                scalar_sources.as_slice(),
                VecElementType::F32,
                scalar_widths.as_slice(),
            ),
            (
                scalar_sources.as_slice(),
                VecElementType::F64,
                scalar_widths.as_slice(),
            ),
        ] {
            for &source1 in source1s {
                for &width in widths {
                    for destination in 0..16 {
                        let bytes = instruction(destination, source1, 3, elem, width, Form::C5);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_memory_fp_sqrt_fields(),
                            Some((
                                destination,
                                source1,
                                elem,
                                width,
                                if source1.is_some() {
                                    elem.bytes()
                                } else {
                                    width.bytes()
                                },
                                false,
                            )),
                            "{bytes:02X?}"
                        );
                        classified += 1;

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes = instruction(
                                    destination,
                                    source1,
                                    base,
                                    elem,
                                    width,
                                    Form::C4 { w },
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_memory_fp_sqrt_fields(),
                                    Some((
                                        destination,
                                        source1,
                                        elem,
                                        width,
                                        if source1.is_some() {
                                            elem.bytes()
                                        } else {
                                            width.bytes()
                                        },
                                        w,
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
        assert_eq!(classified, 2_880);
    }

    #[test]
    fn complete_address_shapes_and_malformed_vex_sqrt_encodings_fail_closed() {
        // addr32 FS: VSQRTSD xmm14,xmm13,[r14d+r15d*2+0x44332211]
        let complete = [
            0x64, 0x67, 0xC4, 0x01, 0x93, 0x51, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        assert_eq!(
            X86InstructionBytes::new(&complete)
                .unwrap()
                .vex_memory_fp_sqrt_fields(),
            Some((14, Some(13), VecElementType::F64, VecWidth::V128, 8, true,))
        );

        let scalar = instruction(
            9,
            Some(10),
            11,
            VecElementType::F64,
            VecWidth::V128,
            Form::C4 { w: true },
        );
        let packed = instruction(
            9,
            None,
            11,
            VecElementType::F64,
            VecWidth::V256,
            Form::C4 { w: true },
        );
        let mut cases = Vec::new();

        let mut unpredictable_scalar_l1 = scalar.clone();
        unpredictable_scalar_l1[2] |= 0x04;
        cases.push(unpredictable_scalar_l1);

        let mut nonreserved_packed_vvvv = packed;
        nonreserved_packed_vvvv[2] &= !0x08;
        cases.push(nonreserved_packed_vvvv);

        let mut wrong_map = scalar.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_opcode = scalar.clone();
        wrong_opcode[3] = 0x52;
        cases.push(wrong_opcode);

        let mut register_source = scalar.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut truncated_displacement = scalar.clone();
        truncated_displacement[4] = (truncated_displacement[4] & 0x3F) | 0x80;
        cases.push(truncated_displacement);

        let mut trailing = scalar.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut forbidden_prefix = scalar;
        forbidden_prefix.insert(0, 0xF3);
        cases.push(forbidden_prefix);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_fp_sqrt_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
