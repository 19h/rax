//! Byte-provenance and replay-rewrite coverage.

use super::*;

#[test]
fn multiply_rewrites_match_eight_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8], bool)] = &[
        (
            &[0x62, 0xE2, 0xED, 0x00, 0x28, 0x0A],
            &[0x62, 0xE2, 0xED, 0x00, 0x28, 0xC8],
            false,
        ),
        (
            &[0x62, 0x72, 0x2D, 0x2B, 0x0B, 0x0A],
            &[0x62, 0x72, 0x2D, 0x2B, 0x0B, 0x0C, 0x24],
            false,
        ),
        (
            &[0x62, 0xE1, 0x6D, 0xC1, 0xE4, 0x0A],
            &[0x62, 0xE1, 0x6D, 0xC1, 0xE4, 0x0C, 0x24],
            false,
        ),
        (
            &[0x62, 0xE1, 0x6D, 0x00, 0xE5, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x00, 0xE5, 0xC8],
            false,
        ),
        (
            &[0x62, 0x72, 0x2D, 0xBD, 0x40, 0x0A],
            &[0x62, 0x72, 0x2D, 0xBD, 0x40, 0x0C, 0x24],
            false,
        ),
        (
            &[0x62, 0xE2, 0xED, 0x51, 0x40, 0x0A],
            &[0x62, 0xE2, 0xED, 0x51, 0x40, 0x0C, 0x24],
            true,
        ),
        (
            &[0x62, 0xF1, 0x5D, 0x8A, 0xD5, 0x1A],
            &[0x62, 0xF1, 0x5D, 0x8A, 0xD5, 0x1C, 0x24],
            false,
        ),
        (
            &[0x62, 0x71, 0xAD, 0x3B, 0xF4, 0x0A],
            &[0x62, 0x71, 0xAD, 0x3B, 0xF4, 0x0C, 0x24],
            false,
        ),
    ];
    for (memory, llvm, needs_dq) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_arithmetic_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert!(encoding.is_integer_multiply(), "{memory:02X?}");
        assert_eq!(encoding.needs_avx512dq, *needs_dq, "{memory:02X?}");
        let replay = match encoding.replay {
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexIntegerArithmeticMemoryReplay::Broadcast { stack_instruction }
            | X86EvexIntegerArithmeticMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn multiply_memory_classifier_exhausts_2_949_120_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in MultiplyKind::ALL {
        for w in [false, true] {
            if !kind.is_wig() && w != kind.fixed_w() {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for form in [SourceForm::Vector, SourceForm::Broadcast] {
                            if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                                continue;
                            }
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let case = MultiplyMemoryCase {
                                        kind,
                                        width,
                                        destination,
                                        source1,
                                        form,
                                        control: MaskControl::None,
                                        w,
                                    };
                                    let canonical =
                                        memory_encoding_with_controls(case, true, mask, zeroing);
                                    for base_high in [false, true] {
                                        for index_high in [false, true] {
                                            let mut bytes = canonical.clone();
                                            bytes[1] |= u8::from(base_high) << 3;
                                            if index_high {
                                                bytes[2] &= !0x04;
                                            }
                                            let encoding = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_integer_arithmetic_memory_encoding()
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            assert!(encoding.is_integer_multiply(), "{bytes:02X?}");
                                            assert_eq!(encoding.map, kind.map(), "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.opcode,
                                                kind.opcode(),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.w, w, "{bytes:02X?}");
                                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                                            assert_eq!(encoding.elem, kind.elem(), "{bytes:02X?}");
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
                                                encoding.needs_avx512vl,
                                                width != VecWidth::V512,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.needs_avx512dq,
                                                kind.needs_avx512dq(),
                                                "{bytes:02X?}"
                                            );
                                            match encoding.replay {
                                                X86EvexIntegerArithmeticMemoryReplay::Vector {
                                                    scratch,
                                                    register_instruction,
                                                } => {
                                                    assert_eq!(mask, 0, "{bytes:02X?}");
                                                    assert_eq!(
                                                        form,
                                                        SourceForm::Vector,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_ne!(
                                                        scratch, destination,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_ne!(scratch, source1, "{bytes:02X?}");
                                                    assert_eq!(
                                                        register_instruction
                                                            .evex_register_integer_multiply_requirements(),
                                                        Some((
                                                            width != VecWidth::V512,
                                                            kind.needs_avx512dq()
                                                        )),
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                                                    ..
                                                } => assert_eq!(
                                                    form,
                                                    SourceForm::Broadcast,
                                                    "{bytes:02X?}"
                                                ),
                                                X86EvexIntegerArithmeticMemoryReplay::MaskedVector {
                                                    ..
                                                } => {
                                                    assert_ne!(mask, 0, "{bytes:02X?}");
                                                    assert_eq!(
                                                        form,
                                                        SourceForm::Vector,
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
    }
    assert_eq!(accepted, 2_949_120);
}

#[test]
fn multiply_register_classifier_exhausts_17_694_720_legal_cells() {
    let mut accepted = 0usize;
    for kind in MultiplyKind::ALL {
        for w in [false, true] {
            if !kind.is_wig() && w != kind.fixed_w() {
                continue;
            }
            for extensions in 0u8..16 {
                for encoded_vvvv in 0u8..16 {
                    for encoded_v_prime in [false, true] {
                        for ll in 0u8..=2 {
                            for mask in 0u8..8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let p0 = (extensions << 4) | kind.map_bits();
                                    let p1 = (u8::from(w) << 7) | (encoded_vvvv << 3) | 0x05;
                                    let p2 = (u8::from(zeroing) << 7)
                                        | (ll << 5)
                                        | (u8::from(encoded_v_prime) << 3)
                                        | mask;
                                    for modrm in 0xC0u8..=0xFF {
                                        let bytes = [0x62, p0, p1, p2, kind.opcode(), modrm];
                                        assert_eq!(
                                            X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_register_integer_multiply_requirements(),
                                            Some((ll != 2, kind.needs_avx512dq())),
                                            "{bytes:02X?}"
                                        );
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
    assert_eq!(accepted, 17_694_720);
}

#[test]
fn multiply_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = MultiplyMemoryCase {
        kind: MultiplyKind::LowDword,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
        w: false,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x01), // mandatory prefix
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
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_arithmetic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    for kind in [
        MultiplyKind::SignedDwordToQword,
        MultiplyKind::UnsignedDwordToQword,
    ] {
        let fixed_w1 = MultiplyMemoryCase {
            kind,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            w: true,
        };
        let mut wrong_w = fixed_w1.bytes();
        wrong_w[2] &= !0x80;
        assert!(
            X86InstructionBytes::new(&wrong_w)
                .unwrap()
                .evex_integer_arithmetic_memory_encoding()
                .is_none(),
            "{wrong_w:02X?}"
        );
    }

    for kind in [
        MultiplyKind::RoundedHighSignedWord,
        MultiplyKind::HighUnsignedWord,
        MultiplyKind::HighSignedWord,
        MultiplyKind::LowWord,
    ] {
        let word = MultiplyMemoryCase {
            kind,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: false,
        };
        let mut forbidden_broadcast = word.bytes();
        forbidden_broadcast[3] |= 0x10;
        assert!(
            X86InstructionBytes::new(&forbidden_broadcast)
                .unwrap()
                .evex_integer_arithmetic_memory_encoding()
                .is_none(),
            "{forbidden_broadcast:02X?}"
        );
    }

    let mut prefixed = vec![0x65, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_arithmetic_memory_encoding()
            .is_some(),
        "GS/address-size prefixes belong to helper address evaluation"
    );
}
