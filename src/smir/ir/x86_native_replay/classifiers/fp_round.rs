use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, FpRoundMode, VReg, VecElementType, VecWidth, X86Reg};

/// Decoded architectural operands and controls of one exact register-only
/// legacy SSE4.1 `ROUNDPS`, `ROUNDPD`, `ROUNDSS`, or `ROUNDSD` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyRoundReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) elem: VecElementType,
    pub(crate) lanes: u8,
    pub(crate) scalar_source: bool,
    pub(crate) mode: FpRoundMode,
    pub(crate) suppress_precision: bool,
}

fn round_mode(immediate: u8) -> FpRoundMode {
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
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

/// Validate the complete one-operation graph emitted for a register-only
/// legacy SSE4.1 ROUND instruction. Replay is admitted only when operands,
/// element width, scalar merge semantics, rounding controls, exception
/// suppression, and legacy upper-lane preservation match the source bytes.
pub(crate) fn x86_legacy_round_shape_matches(ops: &[SmirOp], replay: X86LegacyRoundReplay) -> bool {
    let [operation] = ops else {
        return false;
    };
    operation.x86_hint.is_none()
        && matches!(
            &operation.kind,
            OpKind::X86Round {
                dst,
                merge,
                src,
                elem,
                width: VecWidth::V128,
                lanes,
                scalar_source,
                zero_upper: false,
                mode,
                suppress_precision,
            } if *dst == xmm(replay.destination)
                && *merge == *dst
                && *src == xmm(replay.source)
                && *elem == replay.elem
                && *lanes == replay.lanes
                && *scalar_source == replay.scalar_source
                && *mode == replay.mode
                && *suppress_precision == replay.suppress_precision
        )
}

/// One complete VEX floating-point round memory encoding rewritten to consume
/// the helper-loaded r/m source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexRoundMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) merge: Option<u8>,
    pub(crate) scratch: u8,
    pub(crate) immediate: u8,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexRoundMemoryEncoding {
    pub(crate) fn mode(self) -> FpRoundMode {
        round_mode(self.immediate)
    }

    pub(crate) fn suppress_precision(self) -> bool {
        self.immediate & 8 != 0
    }
}

impl X86InstructionBytes {
    /// Decode one exact register-only legacy SSE4.1 floating-point ROUND.
    ///
    /// All four forms require mandatory 66H followed by an optional final REX
    /// prefix, map 0F3A, a register ModR/M source, and an imm8. REX.R/B extend
    /// the two XMM operands; REX.W/X and imm8[7:4] do not affect the specified
    /// operation but remain retained in the exact replay bytes. Memory, other
    /// or reordered prefixes, non-final or duplicate REX, REX2/VEX/EVEX,
    /// truncated, and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_round_replay(&self) -> Option<X86LegacyRoundReplay> {
        let (rex, opcode, modrm, immediate) = match self.as_slice() {
            [
                0x66,
                rex @ 0x40..=0x4F,
                0x0F,
                0x3A,
                opcode,
                modrm,
                immediate,
            ] => (Some(*rex), *opcode, *modrm, *immediate),
            [0x66, 0x0F, 0x3A, opcode, modrm, immediate] => (None, *opcode, *modrm, *immediate),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let (elem, lanes, scalar_source) = match opcode {
            0x08 => (VecElementType::F32, 4, false),
            0x09 => (VecElementType::F64, 2, false),
            0x0A => (VecElementType::F32, 1, true),
            0x0B => (VecElementType::F64, 1, true),
            _ => return None,
        };
        let rex = rex.unwrap_or(0);
        Some(X86LegacyRoundReplay {
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            elem,
            lanes,
            scalar_source,
            mode: round_mode(immediate),
            suppress_precision: immediate & 8 != 0,
        })
    }

    /// Validate one register-only AVX VEX `VROUNDPS`, `VROUNDPD`, `VROUNDSS`,
    /// or `VROUNDSD` instruction and return its architectural destination.
    ///
    /// All four instructions use map 0F3A and mandatory prefix 66. Packed
    /// forms reserve `VEX.vvvv=1111b` and use `VEX.L` to select 128 or 256
    /// bits. Scalar forms consume `VEX.vvvv` as their merge source and define
    /// `VEX.L` as ignored. `VEX.W` and register-form `VEX.X` are ignored, and
    /// all immediate-byte values are defined through their low control bits.
    /// Memory forms and non-exact source byte strings fail closed.
    pub fn vex_round_destination_index(&self) -> Option<u8> {
        let &[0xC4, p0, p1, opcode, modrm, _imm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x08..=0x0B)
            || modrm >> 6 != 3
            || (matches!(opcode, 0x08 | 0x09) && p1 & 0x78 != 0x78)
        {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3))
    }

    /// Validate one complete AVX VEX floating-point round whose source is
    /// memory and rewrite only that source to a borrowed low vector register.
    ///
    /// Packed forms reserve encoded `VEX.vvvv=1111b` and use `VEX.L` for
    /// 128-/256-bit width. Scalar forms use `VEX.vvvv` as the upper-lane merge
    /// source and define `VEX.L` as ignored. All forms use map 0F3A, mandatory
    /// prefix 66H, WIG, and an imm8; segment and address-size prefixes are
    /// consumed by guest effective-address evaluation and omitted from the
    /// register rewrite.
    pub(crate) fn vex_round_memory_encoding(&self) -> Option<X86VexRoundMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || !matches!(fields.opcode, 0x08..=0x0B) {
            return None;
        }

        let scalar = matches!(fields.opcode, 0x0A | 0x0B);
        if !scalar && fields.source1 != 0 {
            return None;
        }
        let elem = if matches!(fields.opcode, 0x08 | 0x0A) {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let width = if scalar {
            VecWidth::V128
        } else if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let merge = scalar.then_some(fields.source1);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && merge != Some(*candidate))
            .expect("two VEX round operands leave at least fourteen scratch registers");

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
            // Preserve VEX.R and the map, canonicalize X, and encode the
            // borrowed scratch through inverted VEX.B.
            (p0 & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1,
            fields.opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            immediate,
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        let [0xC4, register_p0, _, _, register_modrm, _] = register_instruction.as_slice() else {
            unreachable!("rewritten VEX round has a validated shape")
        };
        let rewritten_source = (register_modrm & 7) | (u8::from(register_p0 & 0x20 == 0) << 3);
        if register_instruction.vex_round_destination_index() != Some(fields.destination)
            || rewritten_source != scratch
        {
            return None;
        }

        Some(X86VexRoundMemoryEncoding {
            width,
            elem,
            destination: fields.destination,
            merge,
            scratch,
            immediate,
            memory_size: if scalar {
                match elem {
                    VecElementType::F32 => 4,
                    VecElementType::F64 => 8,
                    _ => unreachable!("validated VEX scalar round element"),
                }
            } else {
                width.bytes()
            },
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct Case {
        opcode: u8,
        w: bool,
        l: bool,
        destination: u8,
        merge: u8,
        base: u8,
        immediate: u8,
    }

    impl Case {
        fn scalar(self) -> bool {
            matches!(self.opcode, 0x0A | 0x0B)
        }

        fn bytes(self) -> [u8; 6] {
            assert!(self.destination < 16 && self.merge < 16 && self.base < 16);
            [
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 3,
                (u8::from(self.w) << 7)
                    | if self.scalar() {
                        ((!self.merge) & 15) << 3
                    } else {
                        0x78
                    }
                    | (u8::from(self.l) << 2)
                    | 1,
                self.opcode,
                ((self.destination & 7) << 3) | (self.base & 7),
                self.immediate,
            ]
        }

        fn expected(self) -> X86VexRoundMemoryEncoding {
            let scalar = self.scalar();
            let elem = if matches!(self.opcode, 0x08 | 0x0A) {
                VecElementType::F32
            } else {
                VecElementType::F64
            };
            let width = if scalar || !self.l {
                VecWidth::V128
            } else {
                VecWidth::V256
            };
            let merge = scalar.then_some(self.merge);
            let scratch = (0..16)
                .find(|candidate| *candidate != self.destination && merge != Some(*candidate))
                .unwrap();
            let original = self.bytes();
            let register_bytes = [
                0xC4,
                (original[1] & 0x9F) | 0x40 | if scratch < 8 { 0x20 } else { 0 },
                original[2],
                self.opcode,
                0xC0 | (original[4] & 0x38) | (scratch & 7),
                self.immediate,
            ];
            X86VexRoundMemoryEncoding {
                width,
                elem,
                destination: self.destination,
                merge,
                scratch,
                immediate: self.immediate,
                memory_size: if scalar {
                    if elem == VecElementType::F32 { 4 } else { 8 }
                } else {
                    width.bytes()
                },
                register_instruction: X86InstructionBytes::new(&register_bytes).unwrap(),
            }
        }
    }

    #[test]
    fn memory_classifier_covers_every_w_l_destination_and_scalar_merge_shape() {
        let mut classified = 0usize;
        for opcode in 0x08..=0x0B {
            for w in [false, true] {
                for l in [false, true] {
                    for destination in 0..16 {
                        let merges: &[u8] = if opcode < 0x0A {
                            &[0]
                        } else {
                            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
                        };
                        for &merge in merges {
                            let case = Case {
                                opcode,
                                w,
                                l,
                                destination,
                                merge,
                                base: (destination & 8) | 2,
                                immediate: destination.wrapping_mul(17) ^ merge,
                            };
                            let actual = X86InstructionBytes::new(&case.bytes())
                                .unwrap()
                                .vex_round_memory_encoding();
                            assert_eq!(actual, Some(case.expected()), "{case:?}");
                            let encoding = actual.unwrap();
                            assert_ne!(encoding.scratch, destination, "{case:?}");
                            assert_ne!(Some(encoding.scratch), encoding.merge, "{case:?}");
                            assert_eq!(
                                encoding.register_instruction.vex_round_destination_index(),
                                Some(destination),
                                "{case:?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 2_176);
    }

    #[test]
    fn every_immediate_maps_to_the_architectural_round_controls() {
        for opcode in 0x08..=0x0B {
            for immediate in u8::MIN..=u8::MAX {
                let case = Case {
                    opcode,
                    w: immediate & 0x80 != 0,
                    l: immediate & 0x40 != 0,
                    destination: 9,
                    merge: 10,
                    base: 11,
                    immediate,
                };
                let encoding = X86InstructionBytes::new(&case.bytes())
                    .unwrap()
                    .vex_round_memory_encoding()
                    .unwrap();
                let expected_mode = if immediate & 4 != 0 {
                    FpRoundMode::Dynamic
                } else {
                    match immediate & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    }
                };
                assert_eq!(encoding.mode(), expected_mode, "{case:?}");
                assert_eq!(
                    encoding.suppress_precision(),
                    immediate & 8 != 0,
                    "{case:?}"
                );
            }
        }
    }

    #[test]
    fn complete_prefixed_modrm_sib_displacement_shapes_are_accepted() {
        let cases: &[(&[u8], u8, Option<u8>, u32)] = &[
            // vroundps xmm1, fs:[rip + 0x44332211], 0xa5
            (
                &[
                    0x64, 0xC4, 0xE3, 0x79, 0x08, 0x0D, 0x11, 0x22, 0x33, 0x44, 0xA5,
                ],
                1,
                None,
                16,
            ),
            // vroundpd ymm9, gs:[r12 + r13*8 + 0x20], 0x5a
            (
                &[0x65, 0xC4, 0x03, 0xFD, 0x09, 0x4C, 0xEC, 0x20, 0x5A],
                9,
                None,
                32,
            ),
            // vroundss xmm14, xmm10, addr32 [esi*2 + 0x44332211], 0x03
            (
                &[
                    0x67, 0xC4, 0x63, 0x29, 0x0A, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0x03,
                ],
                14,
                Some(10),
                4,
            ),
            // vroundsd xmm0, xmm15, [r13], 0xfc
            (&[0xC4, 0xC3, 0x81, 0x0B, 0x45, 0x00, 0xFC], 0, Some(15), 8),
        ];
        for &(bytes, destination, merge, memory_size) in cases {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_round_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(encoding.destination, destination, "{bytes:02X?}");
            assert_eq!(encoding.merge, merge, "{bytes:02X?}");
            assert_eq!(encoding.memory_size, memory_size, "{bytes:02X?}");
            assert_eq!(
                encoding.register_instruction.as_slice().len(),
                6,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_reserved_register_and_nonexact_memory_shapes_fail_closed() {
        let valid = Case {
            opcode: 0x08,
            w: true,
            l: true,
            destination: 9,
            merge: 0,
            base: 11,
            immediate: 0xA5,
        }
        .bytes();
        let mut invalid = Vec::new();
        for (index, xor) in [(1, 1), (2, 3), (3, 0x10), (4, 0xC0)] {
            let mut bytes = valid.to_vec();
            bytes[index] ^= xor;
            invalid.push(bytes);
        }
        let mut reserved_vvvv = valid.to_vec();
        reserved_vvvv[2] &= !0x08;
        invalid.push(reserved_vvvv);
        let mut trailing = valid.to_vec();
        trailing.push(0);
        invalid.push(trailing);
        for end in 0..valid.len() {
            invalid.push(valid[..end].to_vec());
        }
        let mut legacy_prefix = valid.to_vec();
        legacy_prefix.insert(0, 0x66);
        invalid.push(legacy_prefix);
        let mut repeat_prefix = valid.to_vec();
        repeat_prefix.insert(0, 0xF3);
        invalid.push(repeat_prefix);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_round_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
