//! Exact classifier tests for register-only EVEX scalar-integer moves.

use super::*;

fn vmovq_encoding(opcode: u8, destination: u8, source: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x7E | 0xD6));
    assert!(destination < 32 && source < 32);
    let (reg, rm, pp) = if opcode == 0x7E {
        (destination, source, 2)
    } else {
        (source, destination, 1)
    };
    let mut p0 = 0xF1;
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
        0xFC | pp,
        0x08,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
    ]
}

fn vmovw_encoding(opcode: u8, w: bool, xmm: u8, gpr: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x6E | 0x7E));
    assert!(xmm < 32 && gpr < 16);
    let mut p0 = 0xF5;
    if xmm & 0x08 != 0 {
        p0 &= !0x80;
    }
    if xmm & 0x10 != 0 {
        p0 &= !0x10;
    }
    if gpr & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        0x08,
        opcode,
        0xC0 | ((xmm & 0x07) << 3) | (gpr & 0x07),
    ]
}

#[test]
fn scalar_integer_move_classifier_accepts_exact_register_set_and_fails_closed() {
    for opcode in [0x7E, 0xD6] {
        for destination in 0..32 {
            for source in 0..32 {
                let bytes = vmovq_encoding(opcode, destination, source);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_register_scalar_integer_move_requires_fp16(),
                    Some(false),
                    "{bytes:02X?}"
                );
            }
        }
    }

    for opcode in [0x6E, 0x7E] {
        for w in [false, true] {
            for xmm in 0..32 {
                for gpr in (0..16).filter(|gpr| !matches!(gpr, 4 | 5)) {
                    let bytes = vmovw_encoding(opcode, w, xmm, gpr);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_scalar_integer_move_requires_fp16(),
                        Some(true),
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0xFE, 0x08, 0x7E, 0xC8],       // not EVEX
        &[0x62, 0xF2, 0xFE, 0x08, 0x7E, 0xC8],       // wrong map
        &[0x62, 0xF1, 0xFA, 0x08, 0x7E, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF1, 0xF6, 0x08, 0x7E, 0xC8],       // nonreserved vvvv
        &[0x62, 0xF1, 0x7E, 0x08, 0x7E, 0xC8],       // VMOVQ requires W1
        &[0x62, 0xF1, 0xFE, 0x00, 0x7E, 0xC8],       // reserved V'
        &[0x62, 0xF1, 0xFE, 0x09, 0x7E, 0xC8],       // reserved opmask
        &[0x62, 0xF1, 0xFE, 0x88, 0x7E, 0xC8],       // reserved zeroing
        &[0x62, 0xF1, 0xFE, 0x18, 0x7E, 0xC8],       // reserved EVEX.b
        &[0x62, 0xF1, 0xFE, 0x28, 0x7E, 0xC8],       // not EVEX.128
        &[0x62, 0xF1, 0xFE, 0x08, 0x7E, 0x08],       // memory source
        &[0x62, 0xF1, 0xFE, 0x08, 0x6E, 0xC8],       // unrelated opcode
        &[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0x08],       // VMOVW memory source
        &[0x62, 0xB5, 0x7D, 0x08, 0x6E, 0xC0],       // VMOVW fabricated GPR bit 4
        &[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0xC4],       // VMOVW source RSP
        &[0x62, 0xF5, 0x7D, 0x08, 0x7E, 0xC5],       // VMOVW destination RBP
        &[0x62, 0xF5, 0x7D, 0x28, 0x6E, 0xC0],       // VMOVW not EVEX.128
        &[0x62, 0xF5, 0x7D, 0x08, 0x6E],             // missing ModR/M
        &[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0xC0, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_scalar_integer_move_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn scalar_integer_move_replay_spans_encode_exact_feature_requirements() {
    let pc = 0x1012;
    let mut block = SmirBlock::new(BlockId(34), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_fp16) in [
        (vmovq_encoding(0x7E, 31, 30), false),
        (vmovq_encoding(0xD6, 31, 30), false),
        (vmovw_encoding(0x6E, false, 31, 15), true),
        (vmovw_encoding(0x7E, true, 31, 15), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(34), pc), instruction)]);
        for spans in [
            x86_evex_scalar_integer_move_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert_eq!(span.needs_avx512fp16, needs_fp16, "{bytes:02X?}");
        }
    }
}
