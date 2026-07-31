//! Byte-provenance and deterministic replay-rewrite coverage.

use super::*;

#[test]
fn rewrites_match_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8], SourceForm)] = &[
        (
            &[0x62, 0xF2, 0xED, 0x08, 0x83, 0x0A],
            &[0x62, 0xF2, 0xED, 0x08, 0x83, 0xC8],
            SourceForm::Vector,
        ),
        (
            &[0x62, 0x72, 0xAD, 0xAB, 0x83, 0x0A],
            &[0x62, 0x72, 0xAD, 0xAB, 0x83, 0xC8],
            SourceForm::Vector,
        ),
        (
            &[0x62, 0xE2, 0xED, 0x55, 0x83, 0x0A],
            &[0x62, 0xE2, 0xED, 0x55, 0x83, 0x0C, 0x24],
            SourceForm::Broadcast,
        ),
    ];
    for (memory, llvm, form) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_multishift_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexMultiShiftMemoryReplay::Vector {
                register_instruction,
                ..
            } => {
                assert_eq!(*form, SourceForm::Vector, "{memory:02X?}");
                register_instruction
            }
            X86EvexMultiShiftMemoryReplay::Broadcast { stack_instruction } => {
                assert_eq!(*form, SourceForm::Broadcast, "{memory:02X?}");
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn classifier_exhausts_368_640_operand_control_mask_tuple_and_apx_cells() {
    let mut accepted = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for destination in 0..32u8 {
            for control_register in 0..32u8 {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for mask in 0..8u8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            let case = MultiShiftMemoryCase {
                                width,
                                destination,
                                control_register,
                                form,
                                mask_control: MaskControl::None,
                            };
                            let canonical =
                                memory_encoding_with_controls(case, true, mask, zeroing);
                            for base_high in [false, true] {
                                for index_high in [false, true] {
                                    let mut bytes = canonical.clone();
                                    if base_high {
                                        bytes[1] |= 0x08;
                                    }
                                    if index_high {
                                        bytes[2] &= !0x04;
                                    }
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_multishift_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.control, control_register, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.writemask,
                                        (mask != 0).then_some(mask),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.memory_size,
                                        if form == SourceForm::Broadcast {
                                            8
                                        } else {
                                            width.bytes()
                                        },
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512vl,
                                        width != VecWidth::V512,
                                        "{bytes:02X?}"
                                    );
                                    match encoding.replay {
                                        X86EvexMultiShiftMemoryReplay::Vector {
                                            scratch,
                                            register_instruction,
                                        } => {
                                            assert_eq!(form, SourceForm::Vector, "{bytes:02X?}");
                                            assert_eq!(scratch, case.scratch(), "{bytes:02X?}");
                                            assert_ne!(scratch, destination, "{bytes:02X?}");
                                            assert_ne!(scratch, control_register, "{bytes:02X?}");
                                            let mut expected =
                                                register_encoding(case, case.scratch());
                                            expected[3] = (expected[3] & !0x87)
                                                | (u8::from(zeroing) << 7)
                                                | mask;
                                            assert_eq!(
                                                register_instruction.as_slice(),
                                                expected,
                                                "{bytes:02X?}"
                                            );
                                        }
                                        X86EvexMultiShiftMemoryReplay::Broadcast {
                                            stack_instruction,
                                        } => {
                                            assert_eq!(form, SourceForm::Broadcast, "{bytes:02X?}");
                                            let mut expected = stack_encoding(case);
                                            expected[3] = (expected[3] & !0x87)
                                                | (u8::from(zeroing) << 7)
                                                | mask;
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
    assert_eq!(accepted, 368_640);
}

#[test]
fn classifier_rejects_reserved_non_owned_truncated_and_trailing_shapes() {
    let case = MultiShiftMemoryCase {
        width: VecWidth::V256,
        destination: 9,
        control_register: 10,
        form: SourceForm::Broadcast,
        mask_control: MaskControl::Zero,
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
        (0, 0x01), // EVEX lead byte
        (1, 0x01), // map
        (2, 0x01), // mandatory prefix
        (2, 0x80), // required W=1
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

    let mut missing_sib = memory_encoding_with_controls(case, true, case.mask(), case.zeroing());
    missing_sib.pop();
    malformed.push(missing_sib);

    let mut truncated_disp = valid.clone();
    truncated_disp[5] = (truncated_disp[5] & 0x38) | 5;
    truncated_disp.extend_from_slice(&[0, 0, 0]);
    malformed.push(truncated_disp);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_multishift_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x65, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_multishift_memory_encoding()
            .is_some(),
        "GS/address-size prefixes belong to helper address evaluation"
    );
}
