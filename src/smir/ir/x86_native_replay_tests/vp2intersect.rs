//! Exact classifier tests for register-only EVEX VP2INTERSECTD/Q replay.

use super::*;

type Vp2IntersectShape = (bool, u8);

fn shapes() -> Vec<Vp2IntersectShape> {
    let mut shapes = Vec::new();
    for ll in 0..=2 {
        for w in [false, true] {
            shapes.push((w, ll));
        }
    }
    shapes
}

fn encoding(shape: Vp2IntersectShape, destination: u8, source1: u8, source2: u8) -> [u8; 6] {
    let (w, ll) = shape;
    assert!(destination < 8 && source1 < 32 && source2 < 32 && ll < 3);
    let mut p0 = 0xF2;
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x07 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 },
        0x68,
        0xC0 | (destination << 3) | (source2 & 0x07),
    ]
}

#[test]
fn classifier_covers_768_element_length_destination_and_source_extension_encodings() {
    assert_eq!(
        encoding((false, 2), 2, 0, 1),
        [0x62, 0xF2, 0x7F, 0x48, 0x68, 0xD1]
    );
    assert_eq!(
        encoding((true, 2), 7, 27, 28),
        [0x62, 0x92, 0xA7, 0x40, 0x68, 0xFC]
    );

    let source1_registers = [3u8, 11, 19, 27];
    let source2_registers = [4u8, 12, 20, 28];
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in 0u8..8 {
            for source1 in source1_registers {
                for source2 in source2_registers {
                    let bytes = encoding(shape, destination, source1, source2);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_vp2intersect_needs_vl(),
                        Some(shape.1 != 2),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 768);
}

#[test]
fn classifier_rejects_every_structural_and_reserved_frontier() {
    let register = encoding((false, 0), 1, 2, 3);
    let mut invalid = Vec::new();

    let mut bytes = register;
    bytes[0] = 0x61;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] = (bytes[1] & 0xF0) | 1;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] |= 0x08;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] &= !0x80;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[1] &= !0x10;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[2] &= !0x04;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[2] = (bytes[2] & !0x03) | 1;
    invalid.push(bytes.to_vec());
    let mut bytes = register;
    bytes[4] = 0x69;
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
    invalid.push(register[..5].to_vec());
    let mut trailing = register.to_vec();
    trailing.push(0);
    invalid.push(trailing);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_vp2intersect_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }

    for w in [false, true] {
        let bytes = encoding((w, 0), 7, 31, 31);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_vp2intersect_needs_vl(),
            Some(true),
            "W selects D/Q and odd destination aliases its even pair: {bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_vl_only_for_128_and_256_bit_forms() {
    let pc = 0x4C00;
    let mut block = SmirBlock::new(BlockId(25), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((false, 0), 1, 18, 19),
        encoding((true, 1), 7, 26, 27),
        encoding((false, 2), 0, 2, 3),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(25), pc), instruction)]);
        for spans in [
            x86_evex_vp2intersect_replay_spans(&block, &provenance),
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
