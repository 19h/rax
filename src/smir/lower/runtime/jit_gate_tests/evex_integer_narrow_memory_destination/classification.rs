//! Byte classification, independent encoding anchors, and reserved controls.

use super::*;

#[test]
fn integer_narrow_rewrites_match_six_independent_llvm_23_memory_anchors() {
    // Source and unmasked `[rsp]` replay encodings were produced independently
    // by llvm-mc 23.0.0git. Source displacements are 127 times each narrowing
    // instruction's compressed memory tuple.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0x42, 0x7E, 0x4B, 0x31, 0x4A, 0x7F],
            &[0x62, 0x62, 0x7E, 0x48, 0x31, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7E, 0x0E, 0x21, 0x65, 0x7F],
            &[0x62, 0xE2, 0x7E, 0x08, 0x21, 0x24, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7E, 0x4F, 0x10, 0x4E, 0x7F],
            &[0x62, 0xE2, 0x7E, 0x48, 0x10, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7E, 0x2A, 0x22, 0x53, 0x7F],
            &[0x62, 0xE2, 0x7E, 0x28, 0x22, 0x14, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7E, 0x4D, 0x15, 0x7C, 0x24, 0x7F],
            &[0x62, 0xE2, 0x7E, 0x48, 0x15, 0x3C, 0x24],
        ),
        (
            &[0x62, 0x42, 0x7E, 0x29, 0x34, 0x7F, 0x7F],
            &[0x62, 0x62, 0x7E, 0x28, 0x34, 0x3C, 0x24],
        ),
    ];
    for (memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_narrow_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *replay,
            "{memory:02X?}"
        );
    }
}

#[test]
fn integer_narrow_classifier_exhausts_55_296_operand_mask_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in NarrowOperation::all() {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source in 0..32u8 {
                for mask in 0..8u8 {
                    let case = NarrowMemoryCase {
                        operation,
                        width,
                        source,
                        writemask: (mask != 0).then_some(mask),
                    };
                    let canonical = memory_encoding(case, true);
                    for base_high in [false, true] {
                        for index_high in [false, true] {
                            let mut bytes = canonical.clone();
                            bytes[1] |= u8::from(base_high) << 3;
                            if index_high {
                                bytes[2] &= !0x04;
                            }
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_integer_narrow_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                            assert_eq!(encoding.src_elem, operation.src_elem, "{bytes:02X?}");
                            assert_eq!(encoding.dst_elem, operation.dst_elem, "{bytes:02X?}");
                            assert_eq!(encoding.mode, operation.mode, "{bytes:02X?}");
                            assert_eq!(encoding.source, source, "{bytes:02X?}");
                            assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                            assert_eq!(encoding.needs_avx512bw, operation.needs_avx512bw());
                            assert_eq!(
                                encoding.stack_instruction.as_slice(),
                                stack_encoding(case),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 55_296);
}

#[test]
fn integer_narrow_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = NarrowMemoryCase {
        operation: NarrowOperation {
            src_elem: VecElementType::I64,
            dst_elem: VecElementType::I16,
            mode: X86NarrowMode::SignedSaturate,
        },
        width: VecWidth::V256,
        source: 17,
        writemask: Some(3),
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
        (2, 0x80), // reserved W1
        (2, 0x08), // reserved vvvv
        (3, 0x08), // reserved V'
        (3, 0x10), // reserved EVEX.b
        (3, 0x80), // reserved EVEX.z for memory
        (4, 0x08), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0xF3);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_narrow_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_narrow_memory_encoding()
            .is_some()
    );
}

#[test]
fn integer_narrow_opcode_matrix_is_exactly_18_unique_operations() {
    let operations = NarrowOperation::all();
    assert_eq!(operations.len(), 18);
    let opcodes = operations
        .iter()
        .map(|operation| operation.opcode())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(opcodes.len(), 18);
    assert_eq!(
        opcodes,
        [
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x30, 0x31,
            0x32, 0x33, 0x34, 0x35,
        ]
        .into_iter()
        .collect()
    );
}
