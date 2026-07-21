//! Strict-lifting coverage for the valid D9 transcendental register forms.

use super::*;
use crate::smir::ir::ops::X86X87TranscendentalKind;

const FORMS: [(u8, X86X87TranscendentalKind); 8] = [
    (0xF0, X86X87TranscendentalKind::Exp2MinusOne),
    (0xF1, X86X87TranscendentalKind::YLog2X),
    (0xF2, X86X87TranscendentalKind::Tangent),
    (0xF3, X86X87TranscendentalKind::Arctangent),
    (0xF9, X86X87TranscendentalKind::YLog2Xp1),
    (0xFB, X86X87TranscendentalKind::SineCosine),
    (0xFE, X86X87TranscendentalKind::Sine),
    (0xFF, X86X87TranscendentalKind::Cosine),
];

fn assert_transcendental_frontier(bytes: &[u8], expected: X86X87TranscendentalKind, rex2: bool) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!("valid x87 transcendental entered fallback: {bytes:02X?}: {error:?}")
    });
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(result.branch_targets.is_empty());
    assert_eq!(result.ops.len(), 1 + usize::from(rex2), "{bytes:02X?}");
    if rex2 {
        assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    }
    let op = result.ops.last().unwrap();
    let modrm = *bytes.last().unwrap();
    assert!(
        matches!(
            op.kind,
            OpKind::X86X87Data {
                kind: X86X87DataKind::Transcendental(kind),
                addr: None,
                st,
                fop,
            } if kind == expected && st == modrm & 7 && fop == 0x0100 | modrm as u16
        ),
        "{bytes:02X?}: {op:?}"
    );
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16), "{bytes:02X?}");
        assert_eq!(op.guest_pc, 0x1000, "{bytes:02X?}");
    }
}

#[test]
fn every_valid_x87_transcendental_form_has_a_strict_smir_frontier() {
    for leader in [
        &[][..],
        &[0x66][..],
        &[0xF2][..],
        &[0xF3][..],
        &[0x48][..],
        &[0xD5, 0x00][..],
        &[0xD5, 0x7F][..],
    ] {
        for (modrm, expected) in FORMS {
            let mut bytes = leader.to_vec();
            bytes.extend_from_slice(&[0xD9, modrm]);
            assert_transcendental_frontier(&bytes, expected, leader.first() == Some(&0xD5));
        }
    }
}

#[test]
fn every_map0_rex2_payload_retains_the_apx_guard_and_exact_x87_operation() {
    for payload in 0x00..=0x7F {
        for (modrm, expected) in FORMS {
            let bytes = [0xD5, payload, 0xD9, modrm];
            assert_transcendental_frontier(&bytes, expected, true);
        }
    }
}
