//! Exact source-byte replay classification for EVEX scalar moves.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKind {
    F16,
    F32,
    F64,
}

impl MoveKind {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    fn fields(self) -> (u8, u8, bool, bool) {
        match self {
            Self::F16 => (5, 2, false, true),
            Self::F32 => (1, 2, false, false),
            Self::F64 => (1, 3, true, false),
        }
    }
}

fn encoding(
    kind: MoveKind,
    opcode: u8,
    ll: u8,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (map, pp, w, _) = kind.fields();
    assert!(matches!(opcode, 0x10 | 0x11));
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);

    // Opcode 10 encodes the destination in ModR/M.reg; opcode 11 aliases the
    // same operation with the destination in ModR/M.r/m.
    let (reg, rm) = if opcode == 0x10 {
        (destination, source)
    } else {
        (source, destination)
    };
    let mut p0 = 0xF0 | map;
    if reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!merge) & 0x0F) << 3) | 0x04 | pp | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 }) | (ll << 5) | if merge < 16 { 0x08 } else { 0 } | mask,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
    ]
}

#[test]
fn classifier_covers_12000_legal_alias_llig_mask_and_extension_encodings() {
    let registers = [0u8, 8, 16, 24, 31];
    let masks = [(0u8, false), (1, false), (1, true), (7, true)];
    let mut classified = 0usize;

    for kind in MoveKind::ALL {
        for opcode in [0x10, 0x11] {
            for ll in 0..=3 {
                for destination in registers {
                    for merge in registers {
                        for source in registers {
                            for (mask, zeroing) in masks {
                                let bytes = encoding(
                                    kind,
                                    opcode,
                                    ll,
                                    destination,
                                    merge,
                                    source,
                                    mask,
                                    zeroing,
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_scalar_move_requires_fp16(),
                                    Some(kind.fields().3),
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

    assert_eq!(classified, 12_000);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x6E, 0x09, 0x10, 0xCB],    // not EVEX
        &[0x62, 0xF2, 0x6E, 0x09, 0x10, 0xCB],    // wrong map
        &[0x62, 0xF9, 0x6E, 0x09, 0x10, 0xCB],    // reserved P0 bit 3
        &[0x62, 0xF1, 0x6A, 0x09, 0x10, 0xCB],    // missing P1 fixed-one bit
        &[0x62, 0xF1, 0x6F, 0x09, 0x10, 0xCB],    // wrong pp for W0
        &[0x62, 0xF1, 0xEE, 0x09, 0x10, 0xCB],    // wrong W for VMOVSS
        &[0x62, 0xF5, 0x6F, 0x09, 0x10, 0xCB],    // wrong pp for VMOVSH
        &[0x62, 0xF5, 0xEE, 0x09, 0x10, 0xCB],    // wrong W for VMOVSH
        &[0x62, 0xF1, 0x6E, 0x09, 0x12, 0xCB],    // wrong opcode
        &[0x62, 0xF1, 0x6E, 0x09, 0x10, 0x0B],    // memory source
        &[0x62, 0xF1, 0x6E, 0x19, 0x10, 0xCB],    // EVEX.b is reserved
        &[0x62, 0xF1, 0x6E, 0x88, 0x10, 0xCB],    // zeroing with k0
        &[0x62, 0xF1, 0x6E, 0x09, 0x10],          // missing ModR/M
        &[0x62, 0xF1, 0x6E, 0x09, 0x10, 0xCB, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_scalar_move_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    // Both opcode directions consume all four extension channels plus
    // EVEX.vvvv/V', and L'L is ignored for every scalar family.
    for kind in MoveKind::ALL {
        for opcode in [0x10, 0x11] {
            for ll in 0..=3 {
                let bytes = encoding(kind, opcode, ll, 31, 30, 29, 7, true);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_register_scalar_move_requires_fp16(),
                    Some(kind.fields().3),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn replay_spans_encode_exact_fp16_requirement_without_vl_or_dq() {
    let pc = 0x1011;
    let mut block = SmirBlock::new(BlockId(33), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for kind in MoveKind::ALL {
        for opcode in [0x10, 0x11] {
            let bytes = encoding(kind, opcode, 3, 31, 30, 29, 7, true);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = HashMap::from([((BlockId(33), pc), instruction)]);
            for spans in [
                x86_evex_scalar_move_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                assert!(!span.needs_avx512vl, "{bytes:02X?}");
                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                assert_eq!(span.needs_avx512fp16, kind.fields().3, "{bytes:02X?}");
            }
        }
    }
}
