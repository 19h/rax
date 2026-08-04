//! Source-byte classification, rewrite, and malformed-encoding coverage.

use super::*;

fn shaped_encoding(case: FpShuffleMemoryCase, shape: u8) -> Vec<u8> {
    match shape {
        0 => case.bytes(),
        1 => memory_encoding(case, true),
        2 => {
            let mut bytes = case.bytes();
            bytes[5] = (bytes[5] & 0x38) | 0x43;
            bytes.insert(bytes.len() - 1, 0xA5);
            bytes
        }
        3 => {
            let mut bytes = case.bytes();
            bytes[5] = (bytes[5] & 0x38) | 0x83;
            for byte in 0x1122_3344u32.to_le_bytes() {
                bytes.insert(bytes.len() - 1, byte);
            }
            bytes
        }
        4 => {
            let mut bytes = memory_encoding(case, true);
            bytes.insert(0, 0x67);
            bytes.insert(0, 0x64);
            bytes
        }
        5 => {
            let mut bytes = memory_encoding(case, true);
            // APX B4/X4 extend the base and index used only by helper address
            // evaluation; replay rewrites both to a nonextended operand.
            bytes[1] |= 0x08;
            bytes[2] &= !0x04;
            bytes
        }
        _ => unreachable!("test address shape"),
    }
}

#[test]
fn shuffle_rewrites_match_four_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF1, 0x6C, 0x8A, 0xC6, 0x0A, 0xE4],
            &[0x62, 0xF1, 0x6C, 0x8A, 0xC6, 0xC8, 0xE4],
        ),
        (
            &[0x62, 0x71, 0x2C, 0x3B, 0xC6, 0x0A, 0x1B],
            &[0x62, 0x71, 0x2C, 0x3B, 0xC6, 0x0C, 0x24, 0x1B],
        ),
        (
            &[0x62, 0xE1, 0xED, 0x45, 0xC6, 0x0A, 0xA5],
            &[0x62, 0xE1, 0xED, 0x45, 0xC6, 0xC8, 0xA5],
        ),
        (
            &[0x62, 0x61, 0xAD, 0xD1, 0xC6, 0x0A, 0x5A],
            &[0x62, 0x61, 0xAD, 0xD1, 0xC6, 0x0C, 0x24, 0x5A],
        ),
    ];

    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_fp_shuffle_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let actual = match encoding.replay {
            X86EvexFpShuffleMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexFpShuffleMemoryReplay::Broadcast {
                stack_instruction, ..
            } => stack_instruction,
        };
        assert_eq!(actual.as_slice(), *expected, "{memory:02X?}");
    }
}

#[test]
fn classifier_exhausts_737_280_operand_control_tuple_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0u8..32 {
                for source1 in 0u8..32 {
                    for mask in 0u8..8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            for tuple in TupleKind::ALL {
                                let case = FpShuffleMemoryCase {
                                    kind,
                                    width,
                                    destination,
                                    source1,
                                    control: MaskControl::None,
                                    tuple,
                                    immediate: case_immediate(kind, width, destination, source1),
                                };
                                let mut canonical = memory_encoding(case, true);
                                canonical[3] =
                                    (canonical[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical.clone();
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_fp_shuffle_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.immediate, case.immediate,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.memory_size,
                                            case.memory_size(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            width != VecWidth::V512,
                                            "{bytes:02X?}"
                                        );
                                        let mut expected = case.expected_replay();
                                        expected[3] =
                                            (expected[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                                        match encoding.replay {
                                            X86EvexFpShuffleMemoryReplay::Vector {
                                                scratch,
                                                register_instruction,
                                            } => {
                                                assert!(!tuple.is_broadcast());
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, source1, "{bytes:02X?}");
                                                assert_eq!(
                                                    register_instruction.as_slice(),
                                                    expected,
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    register_instruction
                                                        .evex_register_fp_shuffle_needs_vl(),
                                                    Some(width != VecWidth::V512),
                                                    "{bytes:02X?}"
                                                );
                                            }
                                            X86EvexFpShuffleMemoryReplay::Broadcast {
                                                memory_width,
                                                stack_instruction,
                                            } => {
                                                assert!(tuple.is_broadcast());
                                                assert_eq!(memory_width, kind.memory_width());
                                                assert_eq!(
                                                    stack_instruction.as_slice(),
                                                    expected,
                                                    "{bytes:02X?}"
                                                );
                                            }
                                        }
                                        accepted += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 737_280);
}

#[test]
fn classifier_accepts_all_sib_displacement_prefix_and_apx_address_shapes() {
    let mut accepted = 0usize;
    for case in all_cases() {
        for shape in 0..6 {
            let bytes = shaped_encoding(case, shape);
            let encoding = X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_fp_shuffle_memory_encoding()
                .unwrap_or_else(|| panic!("shape={shape} {bytes:02X?}"));
            assert_eq!(encoding.width, case.width, "{bytes:02X?}");
            assert_eq!(encoding.elem, case.kind.elem, "{bytes:02X?}");
            assert_eq!(encoding.destination, case.destination, "{bytes:02X?}");
            assert_eq!(encoding.source1, case.source1, "{bytes:02X?}");
            assert_eq!(encoding.immediate, case.immediate, "{bytes:02X?}");
            assert_eq!(encoding.memory_size, case.memory_size(), "{bytes:02X?}");
            accepted += 1;
        }
    }
    assert_eq!(accepted, 108 * 6);
}

#[test]
fn classifier_preserves_every_imm8_value_across_semantic_shapes() {
    let mut accepted = 0usize;
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in MaskControl::ALL {
                for tuple in TupleKind::ALL {
                    for immediate in u8::MIN..=u8::MAX {
                        let case = FpShuffleMemoryCase {
                            kind,
                            width,
                            destination: 25,
                            source1: 26,
                            control,
                            tuple,
                            immediate,
                        };
                        let encoding = X86InstructionBytes::new(&case.bytes())
                            .unwrap()
                            .evex_fp_shuffle_memory_encoding()
                            .unwrap_or_else(|| panic!("{case:?}"));
                        assert_eq!(encoding.immediate, immediate, "{case:?}");
                        let replay = match encoding.replay {
                            X86EvexFpShuffleMemoryReplay::Vector {
                                register_instruction,
                                ..
                            } => register_instruction,
                            X86EvexFpShuffleMemoryReplay::Broadcast {
                                stack_instruction, ..
                            } => stack_instruction,
                        };
                        assert_eq!(replay.as_slice().last(), Some(&immediate), "{case:?}");
                        accepted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 2 * 3 * 3 * 2 * 256);
}

#[test]
fn classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let ps = FpShuffleMemoryCase {
        kind: ShuffleKind::ALL[0],
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        control: MaskControl::Merge,
        tuple: TupleKind::Full,
        immediate: 0xE4,
    };
    let valid = ps.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x02), // map
        (2, 0x01), // PS with 66
        (2, 0x80), // PS with W1
        (4, 0x01), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let pd = FpShuffleMemoryCase {
        kind: ShuffleKind::ALL[1],
        ..ps
    }
    .bytes();
    let mut pd_w0 = pd.clone();
    pd_w0[2] &= !0x80;
    malformed.push(pd_w0);
    let mut pd_no_66 = pd;
    pd_no_66[2] &= !1;
    malformed.push(pd_no_66);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_fp_shuffle_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    for tuple in TupleKind::ALL {
        let mut prefixed = vec![0x64, 0x67];
        prefixed.extend_from_slice(&FpShuffleMemoryCase { tuple, ..ps }.bytes());
        assert!(
            X86InstructionBytes::new(&prefixed)
                .unwrap()
                .evex_fp_shuffle_memory_encoding()
                .is_some(),
            "FS/address-size prefixes belong to helper address evaluation"
        );
    }
}
