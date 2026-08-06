//! Byte classification, independent LLVM anchors, and reserved controls.

use super::*;

#[test]
fn evex_extract_rewrites_match_seven_independent_llvm_23_memory_and_replay_anchors() {
    // Both source and replay encodings were produced independently by
    // llvm-mc 23.0.0git. Source displacements are 127 times each instruction's
    // compressed Tuple2/Tuple4/Tuple8 or Tuple1 Scalar width.
    let chunk_anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0x53, 0x7D, 0x4B, 0x19, 0x4A, 0x7F, 0x03],
            &[0x62, 0x73, 0x7D, 0x4B, 0x19, 0x0C, 0x24, 0x03],
        ),
        (
            &[0x62, 0x43, 0xFD, 0x2E, 0x19, 0x65, 0x7F, 0x01],
            &[0x62, 0x63, 0xFD, 0x2E, 0x19, 0x24, 0x24, 0x01],
        ),
        (
            &[0x62, 0xE3, 0x7D, 0x4A, 0x1B, 0x4E, 0x7F, 0x01],
            &[0x62, 0xE3, 0x7D, 0x4A, 0x1B, 0x0C, 0x24, 0x01],
        ),
        (
            &[0x62, 0x43, 0xFD, 0x4D, 0x3B, 0x73, 0x7F, 0x01],
            &[0x62, 0x63, 0xFD, 0x4D, 0x3B, 0x34, 0x24, 0x01],
        ),
    ];
    for (memory, replay) in chunk_anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_chunk_extract_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *replay,
            "{memory:02X?}"
        );
    }

    let scalar_anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0x43, 0x7D, 0x08, 0x17, 0x4A, 0x7F, 0x03],
            &[0x62, 0x63, 0x7D, 0x08, 0x17, 0xC8, 0x03],
        ),
        (
            &[0x62, 0x43, 0x7D, 0x08, 0x14, 0x63, 0x7F, 0x0F],
            &[0x62, 0x63, 0x7D, 0x08, 0x14, 0xE0, 0x0F],
        ),
        (
            &[0x62, 0x43, 0xFD, 0x08, 0x16, 0x75, 0x7F, 0x01],
            &[0x62, 0x63, 0xFD, 0x08, 0x16, 0xF0, 0x01],
        ),
    ];
    for (memory, replay) in scalar_anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_scalar_extract_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(
            encoding.register_instruction.as_slice(),
            *replay,
            "{memory:02X?}"
        );
    }
}

#[test]
fn scalar_extract_classifier_crosses_every_shape_source_apx_axis_and_immediate() {
    let mut structural_cells = 0usize;
    for shape in SCALAR_SHAPES {
        for source in 0..32u8 {
            for base_high in [false, true] {
                for index_high in [false, true] {
                    for immediate in [0x00, 0xA5, 0xFF] {
                        let case = ExtractMemoryCase::Scalar {
                            shape,
                            source,
                            immediate,
                        };
                        let mut bytes = memory_encoding(case, true);
                        bytes[1] |= u8::from(base_high) << 3;
                        if index_high {
                            bytes[2] &= !0x04;
                        }
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_scalar_extract_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(encoding.source, source);
                        assert_eq!(encoding.lane, immediate & shape.lane_mask);
                        assert_eq!(encoding.elem, shape.elem);
                        assert_eq!(encoding.memory_width, shape.memory_width);
                        assert_eq!(encoding.w, shape.w);
                        assert_eq!(encoding.opcode, shape.opcode);
                        assert_eq!(encoding.immediate, immediate);
                        assert_eq!(encoding.needs_avx512bw, shape.needs_avx512bw);
                        assert_eq!(encoding.needs_avx512dq, shape.needs_avx512dq);
                        assert_eq!(
                            encoding
                                .register_instruction
                                .evex_register_scalar_lane_transfer_requires_dq(),
                            Some(shape.needs_avx512dq)
                        );
                        let replay = encoding.register_instruction.as_slice();
                        assert_eq!(replay[1] & 0x68, 0x60, "{bytes:02X?}");
                        assert_eq!(replay[2] & 0x04, 0x04, "{bytes:02X?}");
                        structural_cells += 1;
                    }
                }
            }
        }
    }
    assert_eq!(structural_cells, 8 * 32 * 4 * 3);

    let mut immediate_cells = 0usize;
    for shape in SCALAR_SHAPES {
        for immediate in u8::MIN..=u8::MAX {
            let case = ExtractMemoryCase::Scalar {
                shape,
                source: 31,
                immediate,
            };
            let encoding = X86InstructionBytes::new(&case.bytes())
                .unwrap()
                .evex_scalar_extract_memory_encoding()
                .unwrap();
            assert_eq!(encoding.immediate, immediate);
            assert_eq!(encoding.lane, immediate & shape.lane_mask);
            immediate_cells += 1;
        }
    }
    assert_eq!(immediate_cells, 8 * 256);
}

#[test]
fn chunk_extract_classifier_crosses_every_shape_source_mask_apx_axis_and_immediate() {
    let mut structural_cells = 0usize;
    for shape in CHUNK_SHAPES {
        for source in 0..32u8 {
            for mask in 0..8u8 {
                for base_high in [false, true] {
                    for index_high in [false, true] {
                        for immediate in [0x00, 0x01, 0x03, 0xFF] {
                            let case = ExtractMemoryCase::Chunk {
                                shape,
                                source,
                                writemask: (mask != 0).then_some(mask),
                                immediate,
                            };
                            let mut bytes = memory_encoding(case, true);
                            bytes[1] |= u8::from(base_high) << 3;
                            if index_high {
                                bytes[2] &= !0x04;
                            }
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_chunk_extract_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.source_width, shape.source_width);
                            assert_eq!(encoding.chunk_width, shape.chunk_width());
                            assert_eq!(encoding.elem, shape.elem());
                            assert_eq!(encoding.source, source);
                            assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(encoding.w, shape.w);
                            assert_eq!(encoding.opcode, shape.opcode);
                            assert_eq!(encoding.immediate, immediate);
                            assert_eq!(encoding.first_lane as usize, case.selected_first_lane());
                            assert_eq!(
                                encoding.needs_avx512vl,
                                shape.source_width != VecWidth::V512
                            );
                            assert_eq!(encoding.needs_avx512dq, shape.needs_avx512dq());
                            let replay = encoding.stack_instruction.as_slice();
                            assert_eq!(replay[1] & 0x68, 0x60, "{bytes:02X?}");
                            assert_eq!(replay[2] & 0x04, 0x04, "{bytes:02X?}");
                            assert_eq!(replay[5] & 0xC7, 0x04, "{bytes:02X?}");
                            assert_eq!(replay[6], 0x24, "{bytes:02X?}");
                            assert_eq!(replay[7], immediate, "{bytes:02X?}");
                            structural_cells += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(structural_cells, 12 * 32 * 8 * 4 * 4);

    let mut immediate_cells = 0usize;
    for shape in CHUNK_SHAPES {
        for immediate in u8::MIN..=u8::MAX {
            let case = ExtractMemoryCase::Chunk {
                shape,
                source: 31,
                writemask: Some(7),
                immediate,
            };
            let encoding = X86InstructionBytes::new(&case.bytes())
                .unwrap()
                .evex_chunk_extract_memory_encoding()
                .unwrap();
            assert_eq!(encoding.immediate, immediate);
            assert_eq!(encoding.first_lane as usize, case.selected_first_lane());
            immediate_cells += 1;
        }
    }
    assert_eq!(immediate_cells, 12 * 256);
}

#[test]
fn evex_extract_classifiers_reject_reserved_nonowned_and_trailing_shapes() {
    let scalar_case = ExtractMemoryCase::Scalar {
        shape: SCALAR_SHAPES[7],
        source: 17,
        immediate: 0xA5,
    };
    let scalar = scalar_case.bytes();
    let mut malformed_scalar = vec![scalar[..scalar.len() - 1].to_vec()];
    let mut trailing = scalar.clone();
    trailing.push(0);
    malformed_scalar.push(trailing);
    let mut register = scalar.clone();
    register[5] |= 0xC0;
    malformed_scalar.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x01), // mandatory prefix
        (2, 0x08), // reserved vvvv
        (3, 0x08), // reserved V'
        (3, 0x20), // reserved L'L
        (3, 0x10), // reserved EVEX.b
        (3, 0x01), // reserved writemask
        (3, 0x80), // reserved EVEX.z
        (4, 0x08), // non-owned opcode
    ] {
        let mut bytes = scalar.clone();
        bytes[index] ^= mask;
        malformed_scalar.push(bytes);
    }
    let mut forbidden_legacy = scalar.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed_scalar.push(forbidden_legacy);
    for bytes in malformed_scalar {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_extract_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let chunk_case = ExtractMemoryCase::Chunk {
        shape: CHUNK_SHAPES[11],
        source: 17,
        writemask: Some(3),
        immediate: 0xA5,
    };
    let chunk = chunk_case.bytes();
    let mut malformed_chunk = vec![chunk[..chunk.len() - 1].to_vec()];
    let mut trailing = chunk.clone();
    trailing.push(0);
    malformed_chunk.push(trailing);
    let mut register = chunk.clone();
    register[5] |= 0xC0;
    malformed_chunk.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x01), // mandatory prefix
        (2, 0x08), // reserved vvvv
        (3, 0x08), // reserved V'
        (3, 0x10), // reserved EVEX.b
        (3, 0x80), // reserved EVEX.z for memory
        (4, 0x04), // non-owned opcode
    ] {
        let mut bytes = chunk.clone();
        bytes[index] ^= mask;
        malformed_chunk.push(bytes);
    }
    let mut wrong_ll = chunk.clone();
    wrong_ll[3] = (wrong_ll[3] & !0x60) | 0x20;
    malformed_chunk.push(wrong_ll);
    let mut forbidden_legacy = chunk.clone();
    forbidden_legacy.insert(0, 0xF3);
    malformed_chunk.push(forbidden_legacy);
    for bytes in malformed_chunk {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_chunk_extract_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&chunk);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_chunk_extract_memory_encoding()
            .is_some()
    );
}
