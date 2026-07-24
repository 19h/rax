//! Exact classifier tests for register-only EVEX GFNI replay.

use super::*;

type GfniShape = (u8, u8, bool, u8, bool);

fn shapes() -> Vec<GfniShape> {
    let mut shapes = Vec::new();
    for ll in 0..=2 {
        shapes.push((2, 0xCF, false, ll, false));
        shapes.push((3, 0xCE, true, ll, true));
        shapes.push((3, 0xCF, true, ll, true));
    }
    shapes
}

fn encoding(
    shape: GfniShape,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> Vec<u8> {
    let (map, opcode, w, ll, has_immediate) = shape;
    assert!(matches!(
        (map, opcode, w, has_immediate),
        (2, 0xCF, false, false) | (3, 0xCE | 0xCF, true, true)
    ));
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3);
    assert!(mask < 8 && (!zeroing || mask != 0));
    let mut p0 = 0xF0 | map;
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
    let mut bytes = vec![
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 } | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
    ];
    if has_immediate {
        bytes.push(immediate);
    }
    bytes
}

#[test]
fn classifier_covers_1728_extension_mask_and_length_encodings() {
    assert_eq!(
        encoding((2, 0xCF, false, 2, false), 2, 0, 1, 0, false, 0),
        [0x62, 0xF2, 0x7D, 0x48, 0xCF, 0xD1]
    );
    assert_eq!(
        encoding((3, 0xCE, true, 2, true), 25, 26, 27, 2, true, 0x63),
        [0x62, 0x03, 0xAD, 0xC2, 0xCE, 0xCB, 0x63]
    );

    let registers = [1u8, 9, 17, 25];
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in registers {
            for source1 in registers {
                for source2 in registers {
                    for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        let bytes =
                            encoding(shape, destination, source1, source2, mask, zeroing, 0xA5);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_gfni_needs_vl(),
                            Some(shape.3 != 2),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 1_728);
}

#[test]
fn classifier_rejects_every_structural_and_reserved_frontier() {
    let affine = encoding((3, 0xCE, true, 0, true), 1, 2, 3, 1, false, 0x63);
    let multiply = encoding((2, 0xCF, false, 0, false), 1, 2, 3, 1, false, 0);
    let mut invalid = Vec::new();

    let mut bytes = affine.clone();
    bytes[0] = 0x61;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[1] = (bytes[1] & 0xF0) | 1;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[1] |= 0x08;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[2] &= !0x04;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[2] = (bytes[2] & !0x03) | 2;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[2] &= !0x80;
    invalid.push(bytes);
    let mut bytes = multiply.clone();
    bytes[2] |= 0x80;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[4] = 0xCD;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[3] = (bytes[3] & !0x60) | 0x60;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[3] |= 0x10;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[3] = (bytes[3] & !0x07) | 0x80;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes[5] &= 0x3F;
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes.pop();
    invalid.push(bytes);
    let mut bytes = affine.clone();
    bytes.push(0);
    invalid.push(bytes);
    let mut bytes = multiply.clone();
    bytes.push(0);
    invalid.push(bytes);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_gfni_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_vl_only_for_128_and_256_bit_forms() {
    let pc = 0x4400;
    let mut block = SmirBlock::new(BlockId(23), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((2, 0xCF, false, 0, false), 17, 18, 19, 1, false, 0),
        encoding((3, 0xCE, true, 1, true), 25, 26, 27, 2, true, 0x63),
        encoding((3, 0xCF, true, 2, true), 1, 2, 3, 0, false, 0xA5),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(23), pc), instruction)]);
        for spans in [
            x86_evex_gfni_replay_spans(&block, &provenance),
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
