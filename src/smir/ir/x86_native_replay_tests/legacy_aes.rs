//! Exact classifier and semantic-graph validation for register-only legacy
//! AES-NI replay.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, SourceArch, VecElementType, VecWidth, X86AesOp, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xA35E;
const OPERATIONS: [(X86AesOp, u8, bool); 6] = [
    (X86AesOp::InvMixColumns, 0xDB, false),
    (X86AesOp::Enc, 0xDC, false),
    (X86AesOp::EncLast, 0xDD, false),
    (X86AesOp::Dec, 0xDE, false),
    (X86AesOp::DecLast, 0xDF, false),
    (X86AesOp::KeygenAssist, 0xDF, true),
];

fn encoding(opcode: u8, keygen: bool, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, if keygen { 0x3A } else { 0x38 }, opcode, modrm]);
    if keygen {
        bytes.push(immediate);
    }
    bytes
}

fn expected_registers(rex: Option<u8>, modrm: u8) -> (u8, u8) {
    let rex = rex.unwrap_or(0);
    (
        ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        (modrm & 7) | ((rex & 0x01) << 3),
    )
}

#[test]
fn classifier_covers_all_6_528_rex_register_shapes_and_all_keygen_immediates() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for (op, opcode, keygen) in OPERATIONS {
            for modrm in 0xC0..=0xFF {
                let immediate = modrm ^ rex.unwrap_or(0);
                let bytes = encoding(opcode, keygen, rex, modrm, immediate);
                let (destination, source) = expected_registers(rex, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_aes_replay(),
                    Some(X86LegacyAesReplay {
                        destination,
                        source,
                        op,
                        immediate: if keygen { immediate } else { 0 },
                    }),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 6 * 17 * 64);

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(0xDF, true, Some(0x45), 0xE5, immediate);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_aes_replay(),
            Some(X86LegacyAesReplay {
                destination: 12,
                source: 13,
                op: X86AesOp::KeygenAssist,
                immediate,
            }),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_accepts_independent_llvm_23_encodings() {
    let cases: &[(&[u8], X86LegacyAesReplay)] = &[
        (
            &[0x66, 0x0F, 0x38, 0xDC, 0xCB],
            X86LegacyAesReplay {
                destination: 1,
                source: 3,
                op: X86AesOp::Enc,
                immediate: 0,
            },
        ),
        (
            &[0x66, 0x45, 0x0F, 0x38, 0xDD, 0xCB],
            X86LegacyAesReplay {
                destination: 9,
                source: 11,
                op: X86AesOp::EncLast,
                immediate: 0,
            },
        ),
        (
            &[0x66, 0x45, 0x0F, 0x38, 0xDE, 0xC7],
            X86LegacyAesReplay {
                destination: 8,
                source: 15,
                op: X86AesOp::Dec,
                immediate: 0,
            },
        ),
        (
            &[0x66, 0x44, 0x0F, 0x38, 0xDF, 0xF8],
            X86LegacyAesReplay {
                destination: 15,
                source: 0,
                op: X86AesOp::DecLast,
                immediate: 0,
            },
        ),
        (
            &[0x66, 0x45, 0x0F, 0x38, 0xDB, 0xD6],
            X86LegacyAesReplay {
                destination: 10,
                source: 14,
                op: X86AesOp::InvMixColumns,
                immediate: 0,
            },
        ),
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0xDF, 0xE5, 0x5A],
            X86LegacyAesReplay {
                destination: 12,
                source: 13,
                op: X86AesOp::KeygenAssist,
                immediate: 0x5A,
            },
        ),
    ];
    for &(bytes, expected) in cases {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_aes_replay(),
            Some(expected),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_memory_all_opcode_neighbors_and_nonexact_prefix_shapes() {
    for (op, opcode, keygen) in OPERATIONS {
        for modrm in 0x00..=0xBF {
            let bytes = encoding(opcode, keygen, Some(0x4F), modrm, 0xA5);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_aes_replay(),
                None,
                "{op:?} {bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        for (map, immediate) in [(0x38, None), (0x3A, Some(0xA5))] {
            let mut bytes = vec![0x66, 0x0F, map, opcode, 0xCB];
            bytes.extend(immediate);
            let expected =
                map == 0x38 && (0xDB..=0xDF).contains(&opcode) || map == 0x3A && opcode == 0xDF;
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_aes_replay()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x38, 0xDC, 0xCB],
        &[0xF2, 0x0F, 0x38, 0xDC, 0xCB],
        &[0xF3, 0x0F, 0x38, 0xDC, 0xCB],
        &[0x66, 0x66, 0x0F, 0x38, 0xDC, 0xCB],
        &[0x48, 0x66, 0x0F, 0x38, 0xDC, 0xCB],
        &[0x66, 0x48, 0x67, 0x0F, 0x38, 0xDC, 0xCB],
        &[0x66, 0xD5, 0x00, 0x0F, 0x38, 0xDC, 0xCB],
        &[0x66, 0x0F, 0x38, 0xDC],
        &[0x66, 0x0F, 0x38, 0xDC, 0xCB, 0x00],
        &[0x66, 0x0F, 0x3A, 0xDF, 0xCB],
        &[0x66, 0x0F, 0x3A, 0xDF, 0xCB, 0x5A, 0x00],
        &[0xC4, 0xE2, 0x71, 0xDC, 0xCB],
        &[0x62, 0xF2, 0x75, 0x08, 0xDC, 0xCB],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_aes_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

fn function(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("x86 instruction provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_span(function: &SmirFunction, bytes: &[u8], level: OptLevel) {
    assert_eq!(function.blocks[0].ops.len(), 5, "{level:?} {bytes:02X?}");
    for spans in [
        x86_legacy_aes_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans
            .get(&0)
            .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
        assert_eq!(span.end, 5, "{level:?} {bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{level:?} {bytes:02X?}");
        assert!(!span.needs_avx512vl, "{level:?} {bytes:02X?}");
        assert!(!span.needs_avx512dq, "{level:?} {bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{level:?} {bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{level:?} {bytes:02X?}");
    }
}

#[test]
fn lifted_o0_o2_graphs_admit_all_13_056_rex_register_cases() {
    let mut admitted = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for (_, opcode, keygen) in OPERATIONS {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(opcode, keygen, rex, modrm, modrm ^ 0xA5);
                for level in [OptLevel::O0, OptLevel::O2] {
                    let function = function(&bytes, level);
                    assert_exact_span(&function, &bytes, level);
                    admitted += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 2 * 6 * 17 * 64);
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_aes_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn semantic_graph_validator_rejects_each_field_hint_order_and_virtual_escape_frontier() {
    let bytes = encoding(0xDC, false, Some(0x45), 0xCB, 0);
    let baseline = function(&bytes, OptLevel::O0);
    assert_exact_span(&baseline, &bytes, OptLevel::O0);

    for index in 0..5 {
        let mut malformed = baseline.clone();
        malformed.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
        assert_rejected(&malformed, &format!("hint on operation {index}"));
    }

    let mut mutations: Vec<(&str, SmirFunction)> = Vec::new();
    let mut wrong_aes_dst = baseline.clone();
    if let OpKind::X86Aes { dst, .. } = &mut wrong_aes_dst.blocks[0].ops[0].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(9)));
    }
    mutations.push(("architectural AES destination", wrong_aes_dst));

    let mut wrong_src1 = baseline.clone();
    if let OpKind::X86Aes { src1, .. } = &mut wrong_src1.blocks[0].ops[0].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong AES source1", wrong_src1));

    let mut missing_src2 = baseline.clone();
    if let OpKind::X86Aes { src2, .. } = &mut missing_src2.blocks[0].ops[0].kind {
        *src2 = None;
    }
    mutations.push(("missing round key", missing_src2));

    let mut wrong_width = baseline.clone();
    if let OpKind::X86Aes { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }
    mutations.push(("wrong AES width", wrong_width));

    let mut wrong_operation = baseline.clone();
    if let OpKind::X86Aes { op, .. } = &mut wrong_operation.blocks[0].ops[0].kind {
        *op = X86AesOp::Dec;
    }
    mutations.push(("wrong AES operation", wrong_operation));

    let mut wrong_immediate = baseline.clone();
    if let OpKind::X86Aes { imm, .. } = &mut wrong_immediate.blocks[0].ops[0].kind {
        *imm = 1;
    }
    mutations.push(("nonzero round immediate", wrong_immediate));

    let mut wrong_extract_lane = baseline.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_extract_lane.blocks[0].ops[1].kind {
        *lane = 1;
    }
    mutations.push(("wrong extract lane", wrong_extract_lane));

    let mut wrong_extract_element = baseline.clone();
    if let OpKind::VExtractLane { elem, .. } = &mut wrong_extract_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I32;
    }
    mutations.push(("wrong extract element", wrong_extract_element));

    let mut wrong_insert_lane = baseline.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_insert_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }
    mutations.push(("wrong insert lane", wrong_insert_lane));

    let mut wrong_insert_vector = baseline.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut wrong_insert_vector.blocks[0].ops[3].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong merge source", wrong_insert_vector));

    let mut reordered = baseline.clone();
    reordered.blocks[0].ops.swap(1, 2);
    mutations.push(("reordered extracts", reordered));

    let raw = baseline.blocks[0].ops[0].kind.dests()[0];
    let mut escaped = baseline.clone();
    escaped.blocks[0].set_terminator(Terminator::Return { values: vec![raw] });
    mutations.push(("raw virtual escape", escaped));

    let mut wrong_provenance = baseline.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&encoding(0xDE, false, Some(0x45), 0xCB, 0)).unwrap(),
    );
    mutations.push(("mismatched source provenance", wrong_provenance));

    for (label, malformed) in mutations {
        assert_rejected(&malformed, label);
    }
}

#[test]
fn memory_forms_remain_outside_source_replay() {
    for (_, opcode, keygen) in OPERATIONS {
        let bytes = encoding(opcode, keygen, None, 0x01, 0x5A);
        let function = function(&bytes, OptLevel::O2);
        assert_rejected(&function, &format!("memory {bytes:02X?}"));
    }
}
