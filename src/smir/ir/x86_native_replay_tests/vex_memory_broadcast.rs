//! Exact AVX/AVX2 VEX memory-broadcast classifiers.

use super::*;
use crate::smir::ir::types::{VecElementType, VecWidth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    opcode: u8,
    elem: VecElementType,
    source_lanes: u8,
    width: VecWidth,
    needs_avx2: bool,
}

const SHAPES: [Shape; 13] = [
    Shape {
        opcode: 0x18,
        elem: VecElementType::F32,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x18,
        elem: VecElementType::F32,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x19,
        elem: VecElementType::F64,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x1A,
        elem: VecElementType::F32,
        source_lanes: 4,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x58,
        elem: VecElementType::I32,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x58,
        elem: VecElementType::I32,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x59,
        elem: VecElementType::I64,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x59,
        elem: VecElementType::I64,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x5A,
        elem: VecElementType::I32,
        source_lanes: 4,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x78,
        elem: VecElementType::I8,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x78,
        elem: VecElementType::I8,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x79,
        elem: VecElementType::I16,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x79,
        elem: VecElementType::I16,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
];

fn encoding(shape: Shape, destination: u8, base: u8, encoded_x: bool, w: bool) -> Vec<u8> {
    assert!(destination < 16 && base < 16);
    let mut bytes = vec![
        0xC4,
        (if destination < 8 { 0x80 } else { 0 })
            | (u8::from(encoded_x) << 6)
            | (if base < 8 { 0x20 } else { 0 })
            | 2,
        (u8::from(w) << 7) | 0x78 | (u8::from(shape.width == VecWidth::V256) << 2) | 1,
        shape.opcode,
        0x40 | ((destination & 7) << 3) | (base & 7),
    ];
    if base & 7 == 4 {
        bytes.push(0x24);
    }
    bytes.push(0x20);
    bytes
}

fn assert_shape(bytes: &[u8], shape: Shape, destination: u8) {
    let fields = X86InstructionBytes::new(bytes)
        .unwrap()
        .vex_memory_broadcast_fields()
        .unwrap_or_else(|| panic!("{bytes:02X?}"));
    assert_eq!(fields.destination, destination, "{bytes:02X?}");
    assert_eq!(fields.elem, shape.elem, "{bytes:02X?}");
    assert_eq!(fields.source_lanes, shape.source_lanes, "{bytes:02X?}");
    assert_eq!(fields.width, shape.width, "{bytes:02X?}");
    assert_eq!(fields.opcode, shape.opcode, "{bytes:02X?}");
    assert_eq!(
        fields.memory_size,
        u32::from(shape.source_lanes) * shape.elem.bytes(),
        "{bytes:02X?}"
    );
    assert_eq!(fields.needs_avx2, shape.needs_avx2, "{bytes:02X?}");
}

fn shape_for(opcode: u8, width_256: bool) -> Option<Shape> {
    SHAPES
        .into_iter()
        .find(|shape| shape.opcode == opcode && (shape.width == VecWidth::V256) == width_256)
}

#[test]
fn classifier_exhausts_all_2097152_map_pp_w_vvvv_l_and_opcode_cells() {
    let mut accepted = 0usize;
    let mut tested = 0usize;
    for map in 0u8..32 {
        for pp in 0u8..4 {
            for w in [false, true] {
                for vvvv in 0u8..16 {
                    for width_256 in [false, true] {
                        for opcode in u8::MIN..=u8::MAX {
                            let bytes = [
                                0xC4,
                                0xE0 | map,
                                (u8::from(w) << 7)
                                    | (((!vvvv) & 0x0F) << 3)
                                    | (u8::from(width_256) << 2)
                                    | pp,
                                opcode,
                                0x43,
                                0,
                            ];
                            let fields = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_memory_broadcast_fields();
                            let expected = (map == 2 && pp == 1 && !w && vvvv == 0)
                                .then(|| shape_for(opcode, width_256))
                                .flatten();
                            assert_eq!(fields.is_some(), expected.is_some(), "{bytes:02X?}");
                            if let (Some(fields), Some(shape)) = (fields, expected) {
                                assert_eq!(fields.destination, 0, "{bytes:02X?}");
                                assert_eq!(fields.elem, shape.elem, "{bytes:02X?}");
                                assert_eq!(fields.source_lanes, shape.source_lanes, "{bytes:02X?}");
                                assert_eq!(fields.width, shape.width, "{bytes:02X?}");
                                assert_eq!(fields.opcode, shape.opcode, "{bytes:02X?}");
                                assert_eq!(fields.needs_avx2, shape.needs_avx2, "{bytes:02X?}");
                                accepted += 1;
                            }
                            tested += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, SHAPES.len());
    assert_eq!(tested, 2_097_152);
}

#[test]
fn classifier_covers_all_832_destination_base_shape_and_ignored_x_cells() {
    let mut classified = 0usize;
    for shape in SHAPES {
        for destination in 0..16 {
            for base in [3, 12] {
                for encoded_x in [false, true] {
                    let bytes = encoding(shape, destination, base, encoded_x, false);
                    assert_shape(&bytes, shape, destination);
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 13 * 16 * 2 * 2);
}

#[test]
fn classifier_rejects_w1_reserved_widths_and_every_semantic_frontier() {
    for shape in SHAPES {
        let bytes = encoding(shape, 9, 11, true, true);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_broadcast_fields(),
            None,
            "{bytes:02X?}"
        );
    }

    for (opcode, width) in [
        (0x19, VecWidth::V128),
        (0x1A, VecWidth::V128),
        (0x5A, VecWidth::V128),
    ] {
        let shape = Shape {
            opcode,
            elem: VecElementType::I8,
            source_lanes: 1,
            width,
            needs_avx2: false,
        };
        let bytes = encoding(shape, 9, 11, true, false);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_broadcast_fields(),
            None,
            "{bytes:02X?}"
        );
    }

    let valid = encoding(SHAPES[10], 9, 11, true, false);
    let mut invalid = Vec::new();

    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
    invalid.push(wrong_map);
    let mut wrong_prefix = valid.clone();
    wrong_prefix[2] &= !3;
    invalid.push(wrong_prefix);
    let mut nonreserved_vvvv = valid.clone();
    nonreserved_vvvv[2] &= !0x08;
    invalid.push(nonreserved_vvvv);
    let mut unrelated_opcode = valid.clone();
    unrelated_opcode[3] = 0x17;
    invalid.push(unrelated_opcode);
    let mut register_source = valid.clone();
    register_source[4] |= 0xC0;
    register_source.pop();
    invalid.push(register_source);
    let mut truncated = valid.clone();
    truncated.pop();
    invalid.push(truncated);
    let mut trailing = valid.clone();
    trailing.push(0);
    invalid.push(trailing);
    let mut forbidden_prefix = valid;
    forbidden_prefix.insert(0, 0xF3);
    invalid.push(forbidden_prefix);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_broadcast_fields(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_accepts_llvm_23_address_samples_and_legacy_address_prefixes() {
    let samples: &[(Shape, u8, &[u8])] = &[
        (SHAPES[0], 9, &[0xC4, 0x62, 0x79, 0x18, 0x4B, 0x20]),
        (
            SHAPES[2],
            15,
            &[0xC4, 0x02, 0x7D, 0x19, 0xBC, 0xEC, 0x44, 0x33, 0x22, 0x11],
        ),
        (
            SHAPES[3],
            14,
            &[
                0x64, 0xC4, 0x62, 0x7D, 0x1A, 0x34, 0x8D, 0x44, 0x33, 0x22, 0x11,
            ],
        ),
        (
            SHAPES[4],
            13,
            &[0xC4, 0x62, 0x79, 0x58, 0x2D, 0x44, 0x33, 0x22, 0x11],
        ),
        (SHAPES[7], 12, &[0x65, 0xC4, 0x42, 0x7D, 0x59, 0x62, 0xE0]),
        (
            SHAPES[8],
            3,
            &[0x67, 0xC4, 0x82, 0x7D, 0x5A, 0x5C, 0x48, 0x20],
        ),
        (SHAPES[9], 10, &[0xC4, 0x42, 0x79, 0x78, 0x53, 0x20]),
        (
            SHAPES[12],
            1,
            &[0x67, 0xC4, 0xE2, 0x7D, 0x79, 0x4C, 0x77, 0x20],
        ),
    ];
    for &(shape, destination, bytes) in samples {
        assert_shape(bytes, shape, destination);
    }
}
