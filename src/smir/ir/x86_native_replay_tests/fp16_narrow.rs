//! Exact classifier tests for register-only EVEX FP16 narrowing replay.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NarrowKind {
    F64ToF16,
    F32ToF16Immediate,
    F32ToF16X,
}

impl NarrowKind {
    const ALL: [Self; 3] = [Self::F64ToF16, Self::F32ToF16Immediate, Self::F32ToF16X];

    fn fields(self) -> (u8, u8, bool, u8, bool, bool) {
        match self {
            Self::F64ToF16 => (5, 1, true, 0x5A, false, true),
            Self::F32ToF16Immediate => (3, 1, false, 0x1D, true, false),
            Self::F32ToF16X => (5, 1, false, 0x1D, false, true),
        }
    }
}

fn encoding(
    kind: NarrowKind,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> Vec<u8> {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let (map, pp, w, opcode, has_immediate, _) = kind.fields();
    let (modrm_reg, modrm_rm) = if has_immediate {
        // VCVTPS2PH is exceptional: ModRM.reg is the source and ModRM.r/m is
        // the destination, including their respective EVEX extension bits.
        (source, destination)
    } else {
        (destination, source)
    };
    let mut p0 = 0xF0 | map;
    if modrm_reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if modrm_reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if modrm_rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if modrm_rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    let mut bytes = vec![
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | 0x08
            | mask,
        opcode,
        0xC0 | ((modrm_reg & 0x07) << 3) | (modrm_rm & 0x07),
    ];
    if has_immediate {
        bytes.push(immediate);
    }
    bytes
}

fn controls(kind: NarrowKind) -> &'static [(u8, bool)] {
    match kind {
        NarrowKind::F32ToF16Immediate => &[(0, false), (1, false), (2, false), (0, true)],
        NarrowKind::F64ToF16 | NarrowKind::F32ToF16X => &[
            (0, false),
            (1, false),
            (2, false),
            (0, true),
            (1, true),
            (2, true),
            (3, true),
        ],
    }
}

fn requirements(kind: NarrowKind, ll: u8, embedded_control: bool) -> (bool, bool) {
    (!embedded_control && ll != 2, kind.fields().5)
}

#[test]
fn classifier_accepts_exactly_1_800_sampled_legal_register_encodings() {
    let registers = [0u8, 7, 8, 16, 31];
    let masks = [(0u8, false), (1, false), (2, true), (7, true)];
    let mut classified = 0usize;

    for kind in NarrowKind::ALL {
        for &(ll, embedded_control) in controls(kind) {
            let expected = Some(requirements(kind, ll, embedded_control));
            for destination in registers {
                for source in registers {
                    for (mask, zeroing) in masks {
                        let immediate = destination
                            .wrapping_mul(17)
                            .wrapping_add(source.wrapping_mul(29))
                            .wrapping_add(mask);
                        let bytes = encoding(
                            kind,
                            ll,
                            embedded_control,
                            destination,
                            source,
                            mask,
                            zeroing,
                            immediate,
                        );
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_fp16_narrow_requirements(),
                            expected,
                            "{kind:?} {bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 1_800);

    // Independently assembled by LLVM 21.1.8. These include both ModR/M
    // directions, EGPR extension channels, all ER modes, masking, and imm8.
    for (bytes, expected) in [
        (&[0x62, 0xF5, 0xFD, 0x08, 0x5A, 0xCA][..], (true, true)),
        (&[0x62, 0x55, 0xFD, 0x28, 0x5A, 0xCA], (true, true)),
        (&[0x62, 0xA5, 0xFD, 0x48, 0x5A, 0xCA], (false, true)),
        (&[0x62, 0x05, 0xFD, 0x18, 0x5A, 0xEE], (false, true)),
        (&[0x62, 0x05, 0xFD, 0x38, 0x5A, 0xEE], (false, true)),
        (&[0x62, 0x05, 0xFD, 0x58, 0x5A, 0xEE], (false, true)),
        (&[0x62, 0x05, 0xFD, 0x78, 0x5A, 0xEE], (false, true)),
        (&[0x62, 0xA3, 0x7D, 0x09, 0x1D, 0xD1, 0x00], (true, false)),
        (&[0x62, 0x53, 0x7D, 0xAA, 0x1D, 0xD1, 0x04], (true, false)),
        (&[0x62, 0xA3, 0x7D, 0x48, 0x1D, 0xD1, 0xFF], (false, false)),
        (&[0x62, 0x03, 0x7D, 0x99, 0x1D, 0xF5, 0x03], (false, false)),
        (&[0x62, 0xF5, 0x7D, 0x08, 0x1D, 0xCA], (true, true)),
        (&[0x62, 0x55, 0x7D, 0x28, 0x1D, 0xCA], (true, true)),
        (&[0x62, 0xA5, 0x7D, 0x48, 0x1D, 0xCA], (false, true)),
        (&[0x62, 0x05, 0x7D, 0x18, 0x1D, 0xEE], (false, true)),
        (&[0x62, 0x05, 0x7D, 0x38, 0x1D, 0xEE], (false, true)),
        (&[0x62, 0x05, 0x7D, 0x58, 0x1D, 0xEE], (false, true)),
        (&[0x62, 0x05, 0x7D, 0x78, 0x1D, 0xEE], (false, true)),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp16_narrow_requirements(),
            Some(expected),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let canonical = encoding(NarrowKind::F32ToF16X, 0, false, 17, 18, 1, false, 0);
    let mut invalid = vec![
        vec![
            0x61,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ],
        canonical[..5].to_vec(),
        canonical.iter().copied().chain([0xA5]).collect::<Vec<_>>(),
    ];
    for (index, value) in [
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (2, canonical[2] | 0x80),  // W1.
        (2, canonical[2] & !0x08), // Reserved vvvv.
        (3, canonical[3] & !0x08), // Reserved V'.
        (3, 0x88),                 // Zeroing with k0.
        (4, 0x1C),                 // Unrelated opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical.clone();
        bytes[index] = value;
        invalid.push(bytes);
    }

    for kind in NarrowKind::ALL {
        invalid.push(encoding(kind, 3, false, 1, 2, 0, false, 0).to_vec());
    }
    for ll in 1..=3 {
        invalid.push(encoding(NarrowKind::F32ToF16Immediate, ll, true, 1, 2, 0, false, 0).to_vec());
    }
    // Presence of the immediate byte is instruction-specific.
    let mut missing_immediate = encoding(
        NarrowKind::F32ToF16Immediate,
        0,
        false,
        1,
        2,
        0,
        false,
        0xFF,
    );
    missing_immediate.pop();
    invalid.push(missing_immediate);
    let mut spurious_immediate = encoding(NarrowKind::F64ToF16, 0, false, 1, 2, 0, false, 0);
    spurious_immediate.push(0xFF);
    invalid.push(spurious_immediate);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_narrow_requirements(),
            None,
            "{bytes:02X?}"
        );
    }

    // Each neighboring map/prefix/W/opcode tuple remains disjoint.
    for (map, pp, w, opcode, immediate) in [
        (2, 1, false, 0x1D, false),
        (3, 0, false, 0x1D, true),
        (3, 1, true, 0x1D, true),
        (5, 0, false, 0x1D, false),
        (5, 1, true, 0x1D, false),
        (5, 0, true, 0x5A, false),
        (5, 1, false, 0x5A, false),
        (6, 1, false, 0x1D, false),
    ] {
        let mut bytes = vec![
            0x62,
            0xF0 | map,
            0x7C | pp | if w { 0x80 } else { 0 },
            0x08,
            opcode,
            0xCA,
        ];
        if immediate {
            bytes.push(0xA5);
        }
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_narrow_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_vl_and_fp16_requirements() {
    let pc = 0x5A1D;
    let mut block = SmirBlock::new(BlockId(62), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for kind in NarrowKind::ALL {
        for &(ll, embedded_control) in controls(kind) {
            let bytes = encoding(kind, ll, embedded_control, 29, 30, 3, true, 0xE5);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = std::collections::HashMap::from([((BlockId(62), pc), instruction)]);
            let (expected_vl, expected_fp16) = requirements(kind, ll, embedded_control);
            for spans in [
                x86_evex_fp16_narrow_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                assert_eq!(span.needs_avx512vl, expected_vl, "{bytes:02X?}");
                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                assert_eq!(span.needs_avx512fp16, expected_fp16, "{bytes:02X?}");
            }
        }
    }
}
