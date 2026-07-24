//! Exact classifier tests for register-only EVEX VMOVHLPS/VMOVLHPS.

use super::*;

fn encoding(opcode: u8, destination: u8, merge: u8, source: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x12 | 0x16));
    assert!(destination < 32 && merge < 32 && source < 32);

    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }

    [
        0x62,
        p0,
        ((!merge) & 0x0F) << 3 | 0x04,
        if merge < 16 { 0x08 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn high_low_move_classifier_covers_every_register_encoding_and_fails_closed() {
    let mut classified = 0usize;
    for opcode in [0x12, 0x16] {
        for destination in 0..32 {
            for merge in 0..32 {
                for source in 0..32 {
                    let bytes = encoding(opcode, destination, merge, source);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_high_low_move_needs_vl(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 65_536);

    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x6C, 0x08, 0x12, 0xCB],       // not EVEX
        &[0x62, 0xF2, 0x6C, 0x08, 0x12, 0xCB],       // wrong map
        &[0x62, 0xF1, 0x68, 0x08, 0x12, 0xCB],       // missing fixed-one bit
        &[0x62, 0xF1, 0x6D, 0x08, 0x12, 0xCB],       // mandatory prefix
        &[0x62, 0xF1, 0xEC, 0x08, 0x12, 0xCB],       // W1
        &[0x62, 0xF1, 0x6C, 0x28, 0x12, 0xCB],       // EVEX.256
        &[0x62, 0xF1, 0x6C, 0x48, 0x12, 0xCB],       // EVEX.512
        &[0x62, 0xF1, 0x6C, 0x68, 0x12, 0xCB],       // reserved L'L
        &[0x62, 0xF1, 0x6C, 0x18, 0x12, 0xCB],       // EVEX.b
        &[0x62, 0xF1, 0x6C, 0x09, 0x12, 0xCB],       // opmask
        &[0x62, 0xF1, 0x6C, 0x88, 0x12, 0xCB],       // zeroing
        &[0x62, 0xF1, 0x6C, 0x08, 0x12, 0x0B],       // memory ModR/M
        &[0x62, 0xF1, 0x6C, 0x08, 0x13, 0xCB],       // unrelated opcode
        &[0x62, 0xF1, 0x6C, 0x08, 0x12],             // missing ModR/M
        &[0x62, 0xF1, 0x6C, 0x08, 0x12, 0xCB, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_high_low_move_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn high_low_move_replay_spans_require_only_base_avx512_state() {
    let pc = 0x1014;
    let mut block = SmirBlock::new(BlockId(36), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [encoding(0x12, 31, 30, 29), encoding(0x16, 17, 16, 31)] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(36), pc), instruction)]);
        for spans in [
            x86_evex_high_low_move_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
