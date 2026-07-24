//! Exact classifier tests for register-only EVEX VPCLMULQDQ replay.

use super::*;

type VpclmulqdqShape = (bool, u8);

fn shapes() -> Vec<VpclmulqdqShape> {
    let mut shapes = Vec::new();
    for ll in 0..=2 {
        for w in [false, true] {
            shapes.push((w, ll));
        }
    }
    shapes
}

fn encoding(
    shape: VpclmulqdqShape,
    destination: u8,
    source1: u8,
    source2: u8,
    immediate: u8,
) -> [u8; 7] {
    let (w, ll) = shape;
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3);
    let mut p0 = 0xF3;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 },
        0x44,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
        immediate,
    ]
}

#[test]
fn classifier_covers_384_wig_vector_length_and_extension_encodings() {
    assert_eq!(
        encoding((false, 2), 2, 0, 1, 0),
        [0x62, 0xF3, 0x7D, 0x48, 0x44, 0xD1, 0x00]
    );
    assert_eq!(
        encoding((true, 2), 25, 26, 27, 0xEF),
        [0x62, 0x03, 0xAD, 0x40, 0x44, 0xCB, 0xEF]
    );

    let registers = [1u8, 9, 17, 25];
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in registers {
            for source1 in registers {
                for source2 in registers {
                    let bytes = encoding(shape, destination, source1, source2, 0xA5);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_vpclmulqdq_needs_vl(),
                        Some(shape.1 != 2),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 384);
}

#[test]
fn classifier_rejects_every_structural_and_reserved_frontier() {
    let register = encoding((false, 0), 1, 2, 3, 0x11);
    let mut invalid = Vec::new();

    let mut bytes = register;
    bytes[0] = 0x61;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] = (bytes[1] & 0xF0) | 2;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] |= 0x08;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[2] &= !0x04;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[2] = (bytes[2] & !0x03) | 2;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[4] = 0x45;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[3] = (bytes[3] & !0x60) | 0x60;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[3] |= 0x10;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[3] |= 0x01;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[3] |= 0x80;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[5] &= 0x3F;
    invalid.push(bytes.to_vec());
    invalid.push(register[..6].to_vec());
    let mut trailing = register.to_vec();
    trailing.push(0);
    invalid.push(trailing);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_vpclmulqdq_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }

    for w in [false, true] {
        for immediate in [0x00, 0x01, 0x10, 0x11, 0xAA, 0xEF] {
            let bytes = encoding((w, 0), 1, 2, 3, immediate);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_vpclmulqdq_needs_vl(),
                Some(true),
                "WIG and ignored immediate bits: {bytes:02X?}"
            );
        }
    }
}

#[test]
fn replay_spans_require_vl_only_for_128_and_256_bit_forms() {
    let pc = 0x4800;
    let mut block = SmirBlock::new(BlockId(24), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((false, 0), 17, 18, 19, 0x00),
        encoding((true, 1), 25, 26, 27, 0x11),
        encoding((false, 2), 1, 2, 3, 0xEF),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(24), pc), instruction)]);
        for spans in [
            x86_evex_vpclmulqdq_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, bytes[3] & 0x60 != 0x40, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
