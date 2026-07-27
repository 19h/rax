//! Exact source-byte replay classification for EVEX floating-point compares.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareKind {
    PackedF16,
    PackedF32,
    PackedF64,
    ScalarF16,
    ScalarF32,
    ScalarF64,
}

impl CompareKind {
    const ALL: [Self; 6] = [
        Self::PackedF16,
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF16,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn fields(self) -> (u8, u8, bool, bool, bool) {
        match self {
            Self::PackedF16 => (3, 0, false, false, true),
            Self::PackedF32 => (1, 0, false, false, false),
            Self::PackedF64 => (1, 1, true, false, false),
            Self::ScalarF16 => (3, 2, false, true, true),
            Self::ScalarF32 => (1, 2, false, true, false),
            Self::ScalarF64 => (1, 3, true, true, false),
        }
    }

    fn controls(self) -> Vec<(u8, bool)> {
        if self.fields().3 {
            (0..=2)
                .flat_map(|ll| [(ll, false), (ll, true)])
                .chain([(3, true)])
                .collect()
        } else {
            (0..=2).map(|ll| (ll, false)).chain([(0, true)]).collect()
        }
    }
}

fn encoding(
    kind: CompareKind,
    ll: u8,
    suppress_exceptions: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    writemask: u8,
    predicate: u8,
) -> [u8; 7] {
    let (map, pp, w, scalar, _) = kind.fields();
    assert!(ll < 4);
    assert!(destination < 8 && source1 < 32 && source2 < 32 && writemask < 8);
    assert!(predicate < 32);
    assert!(scalar || !suppress_exceptions || ll == 0);
    assert!(scalar || suppress_exceptions || ll < 3);

    let mut p0 = 0xF0 | map;
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x04 | pp | if w { 0x80 } else { 0 },
        (ll << 5)
            | if suppress_exceptions { 0x10 } else { 0 }
            | if source1 < 16 { 0x08 } else { 0 }
            | writemask,
        0xC2,
        0xC0 | (destination << 3) | (source2 & 0x07),
        predicate,
    ]
}

fn requirements(kind: CompareKind, ll: u8, suppress_exceptions: bool) -> (bool, bool) {
    let (_, _, _, scalar, fp16) = kind.fields();
    (!scalar && !suppress_exceptions && ll != 2, fp16)
}

#[test]
fn classifier_covers_158400_legal_control_mask_extension_and_predicate_encodings() {
    let sources = [0u8, 8, 16, 24, 31];
    let destinations = [0u8, 7];
    let writemasks = [0u8, 1, 7];
    let mut classified = 0usize;

    for kind in CompareKind::ALL {
        for (ll, suppress_exceptions) in kind.controls() {
            for destination in destinations {
                for source1 in sources {
                    for source2 in sources {
                        for writemask in writemasks {
                            for predicate in 0..32 {
                                let bytes = encoding(
                                    kind,
                                    ll,
                                    suppress_exceptions,
                                    destination,
                                    source1,
                                    source2,
                                    writemask,
                                    predicate,
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_fp_compare_requirements(),
                                    Some(requirements(kind, ll, suppress_exceptions)),
                                    "{kind:?} {bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(classified, 158_400);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x7C, 0x09, 0xC2, 0xC8, 0x03], // not EVEX
        &[0x62, 0xF2, 0x7C, 0x09, 0xC2, 0xC8, 0x03], // wrong map
        &[0x62, 0xF9, 0x7C, 0x09, 0xC2, 0xC8, 0x03], // reserved P0 bit 3
        &[0x62, 0x71, 0x7C, 0x09, 0xC2, 0xC8, 0x03], // extended K destination via R
        &[0x62, 0xE1, 0x7C, 0x09, 0xC2, 0xC8, 0x03], // extended K destination via R'
        &[0x62, 0xF1, 0x78, 0x09, 0xC2, 0xC8, 0x03], // missing P1 fixed-one bit
        &[0x62, 0xF1, 0x7D, 0x09, 0xC2, 0xC8, 0x03], // wrong pp for W0
        &[0x62, 0xF1, 0xFC, 0x09, 0xC2, 0xC8, 0x03], // wrong W for packed F32
        &[0x62, 0xF3, 0x7D, 0x09, 0xC2, 0xC8, 0x03], // wrong pp for packed F16
        &[0x62, 0xF3, 0xFC, 0x09, 0xC2, 0xC8, 0x03], // wrong W for packed F16
        &[0x62, 0xF1, 0x7C, 0x09, 0xC3, 0xC8, 0x03], // wrong opcode
        &[0x62, 0xF1, 0x7C, 0x09, 0xC2, 0x08, 0x03], // memory source
        &[0x62, 0xF1, 0x7C, 0x89, 0xC2, 0xC8, 0x03], // EVEX.z is reserved
        &[0x62, 0xF1, 0x7C, 0x69, 0xC2, 0xC8, 0x03], // packed L'L=3 without SAE
        &[0x62, 0xF1, 0x7C, 0x39, 0xC2, 0xC8, 0x03], // packed SAE requires L'L=0
        &[0x62, 0xF1, 0x7C, 0x09, 0xC2, 0xC8, 0x20], // reserved immediate bits 7:5
        &[0x62, 0xF1, 0x7C, 0x09, 0xC2, 0xC8],       // missing immediate
        &[0x62, 0xF1, 0x7C, 0x09, 0xC2, 0xC8, 0x03, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_compare_requirements(),
            None,
            "{bytes:02X?}"
        );
    }

    // Intel SDM revision 092 specifies scalar LLIG and SAE; Intel XED
    // 2026.07.15 independently accepts SAE L'L=11b while rejecting the same
    // control value without SAE.
    for kind in [
        CompareKind::ScalarF16,
        CompareKind::ScalarF32,
        CompareKind::ScalarF64,
    ] {
        for ll in 0..=3 {
            for suppress_exceptions in [false, true] {
                for predicate in 0..32 {
                    let bytes = encoding(kind, ll, suppress_exceptions, 7, 31, 31, 7, predicate);
                    let expected =
                        (suppress_exceptions || ll != 3).then_some((false, kind.fields().4));
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_fp_compare_requirements(),
                        expected,
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }
}

#[test]
fn replay_spans_encode_exact_vl_and_fp16_requirements() {
    let pc = 0xC200;
    let mut block = SmirBlock::new(BlockId(32), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (kind, ll, suppress_exceptions) in [
        (CompareKind::PackedF16, 0, false),
        (CompareKind::PackedF16, 0, true),
        (CompareKind::PackedF32, 1, false),
        (CompareKind::PackedF64, 2, false),
        (CompareKind::ScalarF16, 2, true),
        (CompareKind::ScalarF32, 2, false),
        (CompareKind::ScalarF64, 2, true),
    ] {
        let bytes = encoding(kind, ll, suppress_exceptions, 7, 17, 24, 1, 31);
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(32), pc), instruction)]);
        let expected = requirements(kind, ll, suppress_exceptions);
        for spans in [
            x86_evex_fp_compare_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, expected.0, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert_eq!(span.needs_avx512fp16, expected.1, "{bytes:02X?}");
        }
    }
}
