//! AVX-512ER packed Type-E2 memory-source classification and admission.

use super::*;

const ER_OPERATIONS: [PackedUnaryOperation; 6] = [
    PackedUnaryOperation::Exp2F32,
    PackedUnaryOperation::Exp2F64,
    PackedUnaryOperation::Recip28F32,
    PackedUnaryOperation::Recip28F64,
    PackedUnaryOperation::Rsqrt28F32,
    PackedUnaryOperation::Rsqrt28F64,
];

fn er_semantic(kind: &OpKind) -> bool {
    matches!(
        kind,
        OpKind::X86Exp2 { .. } | OpKind::X86Recip28 { .. } | OpKind::X86Rsqrt28 { .. }
    )
}

#[test]
fn packed_er_rewrites_match_ten_independent_llvm_23_anchors() {
    // Assembled independently with llvm-mc 23.0.0git. The first six pairs
    // validate vector-to-register rewrites; the remaining pairs cover scalar
    // broadcasts and masked-vector stack staging.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xC8, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x48, 0xC8, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xC8, 0x0A],
            &[0x62, 0xF2, 0xFD, 0x48, 0xC8, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xCA, 0x0A],
            &[0x62, 0xF2, 0xFD, 0x48, 0xCA, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xCC, 0x0A],
            &[0x62, 0xF2, 0xFD, 0x48, 0xCC, 0xC8],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0xDB, 0xC8, 0x0C, 0x24],
            &[0x62, 0xF2, 0x7D, 0xDB, 0xC8, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x5B, 0xCA, 0x0C, 0x24],
            &[0x62, 0xF2, 0xFD, 0x5B, 0xCA, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x4B, 0xCC, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x4B, 0xCC, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0xFD, 0xCB, 0xCC, 0x0A],
            &[0x62, 0xF2, 0xFD, 0xCB, 0xCC, 0x0C, 0x24],
        ),
    ];

    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_packed_fp_unary_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert!(encoding.needs_avx512er, "{memory:02X?}");
        assert!(!encoding.needs_avx512vl, "{memory:02X?}");
        let replay = match encoding.replay {
            X86EvexPackedFpUnaryMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexPackedFpUnaryMemoryReplay::Broadcast { stack_instruction }
            | X86EvexPackedFpUnaryMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *expected, "{memory:02X?}");
    }
}

#[test]
fn packed_er_sequence_fails_closed_for_every_semantic_and_provenance_mutation() {
    for (ordinal, operation) in ER_OPERATIONS.into_iter().enumerate() {
        let case = PackedUnaryMemoryCase {
            operation,
            width: VecWidth::V512,
            destination: 17,
            form: if ordinal & 1 == 0 {
                SourceForm::Vector
            } else {
                SourceForm::Broadcast
            },
            control: if ordinal % 3 == 0 {
                MaskControl::None
            } else if ordinal % 3 == 1 {
                MaskControl::Merge
            } else {
                MaskControl::Zero
            },
        };
        for level in LEVELS {
            let canonical = optimize(lift_case(case), level);
            let exact = sequence(&canonical, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", canonical.blocks[0].ops));
            assert_eq!(exact.encoding.kind, operation.kind());
            assert!(exact.encoding.needs_avx512er);
            assert_eq!(exact.consumed, canonical.blocks[0].ops.len());

            let semantic_index = canonical.blocks[0]
                .ops
                .iter()
                .position(|op| er_semantic(&op.kind))
                .expect("packed ER semantic operation");

            let mut wrong_source = canonical.clone();
            match &mut wrong_source.blocks[0].ops[semantic_index].kind {
                OpKind::X86Exp2 { src, .. }
                | OpKind::X86Recip28 { src, .. }
                | OpKind::X86Rsqrt28 { src, .. } => {
                    *src = vector(case.destination, VecWidth::V512);
                }
                _ => unreachable!(),
            }
            assert_rejected("packed ER wrong memory consumer", &wrong_source);

            let mut wrong_sae = canonical.clone();
            match &mut wrong_sae.blocks[0].ops[semantic_index].kind {
                OpKind::X86Exp2 {
                    suppress_exceptions,
                    ..
                }
                | OpKind::X86Recip28 {
                    suppress_exceptions,
                    ..
                }
                | OpKind::X86Rsqrt28 {
                    suppress_exceptions,
                    ..
                } => *suppress_exceptions = true,
                _ => unreachable!(),
            }
            assert_rejected("packed ER memory SAE", &wrong_sae);

            let mut wrong_lanes = canonical.clone();
            match &mut wrong_lanes.blocks[0].ops[semantic_index].kind {
                OpKind::X86Exp2 { lanes, .. }
                | OpKind::X86Recip28 { lanes, .. }
                | OpKind::X86Rsqrt28 { lanes, .. } => *lanes -= 1,
                _ => unreachable!(),
            }
            assert_rejected("packed ER lane count", &wrong_lanes);

            let mut wrong_hint = canonical.clone();
            wrong_hint.blocks[0].ops[semantic_index].x86_hint = None;
            assert_rejected("packed ER missing exact hint", &wrong_hint);

            let replacement = PackedUnaryMemoryCase {
                operation: ER_OPERATIONS[(ordinal + 1) % ER_OPERATIONS.len()],
                ..case
            };
            let mut wrong_provenance = canonical.clone();
            wrong_provenance.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&replacement.bytes()).unwrap(),
            );
            assert_rejected("packed ER mismatched provenance", &wrong_provenance);
        }
    }
}
