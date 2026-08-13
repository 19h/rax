//! Exact classifier and semantic-graph validation for register-only legacy
//! SHA-NI replay.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86Sha32Op};
use crate::smir::ir::types::{ArchReg, FunctionId, SignExtend, SourceArch, VecElementType, X86Reg};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x5A32;
const OPERATIONS: [(X86Sha32Op, u8, u8, bool); 7] = [
    (X86Sha32Op::Sha1Nexte, 0x38, 0xC8, false),
    (X86Sha32Op::Sha1Msg1, 0x38, 0xC9, false),
    (X86Sha32Op::Sha1Msg2, 0x38, 0xCA, false),
    (X86Sha32Op::Sha256Rounds2, 0x38, 0xCB, false),
    (X86Sha32Op::Sha256Msg1, 0x38, 0xCC, false),
    (X86Sha32Op::Sha256Msg2, 0x38, 0xCD, false),
    (X86Sha32Op::Sha1Rounds4, 0x3A, 0xCC, true),
];
const INERT_PREFIXES: [Option<u8>; 4] = [None, Some(0x64), Some(0x65), Some(0x67)];

fn encoding(
    map: u8,
    opcode: u8,
    has_immediate: bool,
    inert_prefix: Option<u8>,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
) -> Vec<u8> {
    assert!(inert_prefix.is_none_or(|byte| matches!(byte, 0x64 | 0x65 | 0x67)));
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    bytes.extend(inert_prefix);
    bytes.extend(rex);
    bytes.extend([0x0F, map, opcode, modrm]);
    if has_immediate {
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
fn classifier_covers_all_30_464_inert_rex_register_shapes_and_all_immediates() {
    let mut classified = 0usize;
    for inert_prefix in INERT_PREFIXES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for (op, map, opcode, has_immediate) in OPERATIONS {
                for modrm in 0xC0..=0xFF {
                    let immediate = modrm ^ rex.unwrap_or(0) ^ inert_prefix.unwrap_or(0);
                    let bytes = encoding(
                        map,
                        opcode,
                        has_immediate,
                        inert_prefix,
                        rex,
                        modrm,
                        immediate,
                    );
                    let (destination, source) = expected_registers(rex, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_sha_replay(),
                        Some(X86LegacyShaReplay {
                            destination,
                            source,
                            op,
                            immediate: if has_immediate { immediate } else { 0 },
                        }),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 4 * 17 * 7 * 64);

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(0x3A, 0xCC, true, Some(0x65), Some(0x45), 0xE5, immediate);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_sha_replay(),
            Some(X86LegacyShaReplay {
                destination: 12,
                source: 13,
                op: X86Sha32Op::Sha1Rounds4,
                immediate,
            }),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_accepts_independent_llvm_23_encodings() {
    let cases: &[(&[u8], X86LegacyShaReplay)] = &[
        (
            &[0x0F, 0x38, 0xC9, 0xCB],
            X86LegacyShaReplay {
                destination: 1,
                source: 3,
                op: X86Sha32Op::Sha1Msg1,
                immediate: 0,
            },
        ),
        (
            &[0x45, 0x0F, 0x38, 0xCA, 0xCB],
            X86LegacyShaReplay {
                destination: 9,
                source: 11,
                op: X86Sha32Op::Sha1Msg2,
                immediate: 0,
            },
        ),
        (
            &[0x45, 0x0F, 0x38, 0xC8, 0xC7],
            X86LegacyShaReplay {
                destination: 8,
                source: 15,
                op: X86Sha32Op::Sha1Nexte,
                immediate: 0,
            },
        ),
        (
            &[0x44, 0x0F, 0x3A, 0xCC, 0xF8, 0x5A],
            X86LegacyShaReplay {
                destination: 15,
                source: 0,
                op: X86Sha32Op::Sha1Rounds4,
                immediate: 0x5A,
            },
        ),
        (
            &[0x45, 0x0F, 0x38, 0xCC, 0xD6],
            X86LegacyShaReplay {
                destination: 10,
                source: 14,
                op: X86Sha32Op::Sha256Msg1,
                immediate: 0,
            },
        ),
        (
            &[0x45, 0x0F, 0x38, 0xCD, 0xE5],
            X86LegacyShaReplay {
                destination: 12,
                source: 13,
                op: X86Sha32Op::Sha256Msg2,
                immediate: 0,
            },
        ),
        (
            &[0x0F, 0x38, 0xCB, 0xD7],
            X86LegacyShaReplay {
                destination: 2,
                source: 7,
                op: X86Sha32Op::Sha256Rounds2,
                immediate: 0,
            },
        ),
    ];
    for &(bytes, expected) in cases {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_sha_replay(),
            Some(expected),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_memory_opcode_neighbors_and_nonexact_prefix_shapes() {
    for (_, map, opcode, has_immediate) in OPERATIONS {
        for modrm in 0x00..=0xBF {
            let bytes = encoding(
                map,
                opcode,
                has_immediate,
                Some(0x67),
                Some(0x4F),
                modrm,
                0xA5,
            );
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_sha_replay(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        for (map, has_immediate) in [(0x38, false), (0x3A, true)] {
            let bytes = encoding(map, opcode, has_immediate, None, None, 0xCB, 0xA5);
            let expected =
                map == 0x38 && (0xC8..=0xCD).contains(&opcode) || map == 0x3A && opcode == 0xCC;
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_sha_replay()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x66, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF2, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF3, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF0, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x66, 0x48, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF2, 0x48, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF3, 0x48, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xF0, 0x48, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x64, 0x65, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x67, 0x67, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x48, 0x67, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x67, 0x48, 0x64, 0x0F, 0x38, 0xC8, 0xCB],
        &[0xD5, 0x00, 0x0F, 0x38, 0xC8, 0xCB],
        &[0x0F, 0x38, 0xC8],
        &[0x0F, 0x38, 0xC8, 0xCB, 0x00],
        &[0x0F, 0x3A, 0xCC, 0xCB],
        &[0x0F, 0x3A, 0xCC, 0xCB, 0x5A, 0x00],
        &[0xC4, 0xE2, 0x71, 0xC8, 0xCB],
        &[0x62, 0xF2, 0x75, 0x08, 0xC8, 0xCB],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_sha_replay(),
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

fn assert_exact_span(
    function: &SmirFunction,
    source_bytes: &[u8],
    replay_bytes: &[u8],
    level: OptLevel,
) {
    assert_eq!(
        function.blocks[0].ops.len(),
        9,
        "{level:?} {source_bytes:02X?}"
    );
    for spans in [
        x86_legacy_sha_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans
            .get(&0)
            .unwrap_or_else(|| panic!("{level:?} {source_bytes:02X?}"));
        assert_eq!(span.end, 9, "{level:?} {source_bytes:02X?}");
        assert_eq!(
            span.instruction.as_slice(),
            replay_bytes,
            "{level:?} {source_bytes:02X?}"
        );
        assert!(!span.needs_avx512vl, "{level:?} {source_bytes:02X?}");
        assert!(!span.needs_avx512dq, "{level:?} {source_bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{level:?} {source_bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{level:?} {source_bytes:02X?}");
    }
}

#[test]
fn lifted_o0_o2_graphs_admit_all_60_928_inert_rex_register_cases_and_immediates() {
    let mut admitted = 0usize;
    for inert_prefix in INERT_PREFIXES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for (_, map, opcode, has_immediate) in OPERATIONS {
                for modrm in 0xC0..=0xFF {
                    let bytes = encoding(
                        map,
                        opcode,
                        has_immediate,
                        inert_prefix,
                        rex,
                        modrm,
                        modrm ^ 0xA5,
                    );
                    let replay_bytes =
                        encoding(map, opcode, has_immediate, None, rex, modrm, modrm ^ 0xA5);
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = function(&bytes, level);
                        assert_exact_span(&function, &bytes, &replay_bytes, level);
                        admitted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(admitted, 2 * 4 * 17 * 7 * 64);

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(0x3A, 0xCC, true, None, Some(0x45), 0xE5, immediate);
        for level in [OptLevel::O0, OptLevel::O2] {
            assert_exact_span(&function(&bytes, level), &bytes, &bytes, level);
        }
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_sha_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
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
    let bytes = encoding(0x38, 0xCB, false, Some(0x67), Some(0x45), 0xCB, 0);
    let replay_bytes = encoding(0x38, 0xCB, false, None, Some(0x45), 0xCB, 0);
    let baseline = function(&bytes, OptLevel::O0);
    assert_exact_span(&baseline, &bytes, &replay_bytes, OptLevel::O0);

    for index in 0..9 {
        let mut malformed = baseline.clone();
        malformed.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
        assert_rejected(&malformed, &format!("hint on operation {index}"));
    }

    let mut mutations: Vec<(&str, SmirFunction)> = Vec::new();
    let mut wrong_sha_dst = baseline.clone();
    if let OpKind::X86Sha32 { dst, .. } = &mut wrong_sha_dst.blocks[0].ops[0].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(9)));
    }
    mutations.push(("architectural SHA destination", wrong_sha_dst));

    let mut wrong_src1 = baseline.clone();
    if let OpKind::X86Sha32 { src1, .. } = &mut wrong_src1.blocks[0].ops[0].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong SHA source1", wrong_src1));

    let mut wrong_src2 = baseline.clone();
    if let OpKind::X86Sha32 { src2, .. } = &mut wrong_src2.blocks[0].ops[0].kind {
        *src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong SHA source2", wrong_src2));

    let mut missing_wk = baseline.clone();
    if let OpKind::X86Sha32 { wk, .. } = &mut missing_wk.blocks[0].ops[0].kind {
        *wk = None;
    }
    mutations.push(("missing implicit XMM0", missing_wk));

    let mut wrong_wk = baseline.clone();
    if let OpKind::X86Sha32 { wk, .. } = &mut wrong_wk.blocks[0].ops[0].kind {
        *wk = Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))));
    }
    mutations.push(("wrong implicit XMM0", wrong_wk));

    let mut wrong_operation = baseline.clone();
    if let OpKind::X86Sha32 { op, .. } = &mut wrong_operation.blocks[0].ops[0].kind {
        *op = X86Sha32Op::Sha1Msg1;
    }
    mutations.push(("wrong SHA operation", wrong_operation));

    let mut wrong_immediate = baseline.clone();
    if let OpKind::X86Sha32 { imm, .. } = &mut wrong_immediate.blocks[0].ops[0].kind {
        *imm = 1;
    }
    mutations.push(("nonzero unencoded immediate", wrong_immediate));

    let mut wrong_extract_vector = baseline.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut wrong_extract_vector.blocks[0].ops[1].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong extract vector", wrong_extract_vector));

    let mut wrong_extract_lane = baseline.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_extract_lane.blocks[0].ops[1].kind {
        *lane = 1;
    }
    mutations.push(("wrong extract lane", wrong_extract_lane));

    let mut wrong_extract_element = baseline.clone();
    if let OpKind::VExtractLane { elem, .. } = &mut wrong_extract_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I64;
    }
    mutations.push(("wrong extract element", wrong_extract_element));

    let mut wrong_extract_sign = baseline.clone();
    if let OpKind::VExtractLane { sign, .. } = &mut wrong_extract_sign.blocks[0].ops[1].kind {
        *sign = SignExtend::Sign;
    }
    mutations.push(("wrong extract extension", wrong_extract_sign));

    let mut wrong_insert_lane = baseline.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_insert_lane.blocks[0].ops[5].kind {
        *lane = 1;
    }
    mutations.push(("wrong insert lane", wrong_insert_lane));

    let mut wrong_insert_vector = baseline.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut wrong_insert_vector.blocks[0].ops[5].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong merge source", wrong_insert_vector));

    let mut wrong_insert_destination = baseline.clone();
    if let OpKind::VInsertLane { dst, .. } = &mut wrong_insert_destination.blocks[0].ops[5].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("wrong merge destination", wrong_insert_destination));

    let mut wrong_insert_scalar = baseline.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut wrong_insert_scalar.blocks[0].ops[5].kind {
        *scalar = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    }
    mutations.push(("wrong merge scalar", wrong_insert_scalar));

    let mut wrong_insert_element = baseline.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut wrong_insert_element.blocks[0].ops[5].kind {
        *elem = VecElementType::I64;
    }
    mutations.push(("wrong merge element", wrong_insert_element));

    let mut reordered = baseline.clone();
    reordered.blocks[0].ops.swap(1, 2);
    mutations.push(("reordered extracts", reordered));

    let mut wrong_provenance = baseline.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&encoding(0x38, 0xC9, false, None, None, 0xCB, 0)).unwrap(),
    );
    mutations.push(("mismatched source provenance", wrong_provenance));

    for (label, malformed) in mutations {
        assert_rejected(&malformed, label);
    }

    let temporaries: Vec<VReg> = baseline.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .filter(|register| matches!(register, VReg::Virtual(_)))
        .collect();
    assert_eq!(temporaries.len(), 5);
    for temporary in temporaries {
        let mut escaped = baseline.clone();
        escaped.blocks[0].set_terminator(Terminator::Return {
            values: vec![temporary],
        });
        assert_rejected(&escaped, &format!("escaped virtual {temporary:?}"));
    }

    for lane in 0..4usize {
        let mut wrong_lane = baseline.clone();
        if let OpKind::VExtractLane { lane: actual, .. } =
            &mut wrong_lane.blocks[0].ops[1 + lane].kind
        {
            *actual = ((lane + 1) & 3) as u8;
        }
        assert_rejected(&wrong_lane, &format!("extract lane {lane}"));

        let mut wrong_lane = baseline.clone();
        if let OpKind::VInsertLane { lane: actual, .. } =
            &mut wrong_lane.blocks[0].ops[5 + lane].kind
        {
            *actual = ((lane + 1) & 3) as u8;
        }
        assert_rejected(&wrong_lane, &format!("insert lane {lane}"));
    }

    let no_wk_bytes = encoding(0x38, 0xC9, false, None, None, 0xCB, 0);
    let mut unexpected_wk = function(&no_wk_bytes, OptLevel::O0);
    if let OpKind::X86Sha32 { wk, .. } = &mut unexpected_wk.blocks[0].ops[0].kind {
        *wk = Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))));
    }
    assert_rejected(&unexpected_wk, "unexpected implicit XMM0");

    let high_imm_bytes = encoding(0x3A, 0xCC, true, None, None, 0xCB, 0xA6);
    let mut same_round_different_immediate = function(&high_imm_bytes, OptLevel::O0);
    same_round_different_immediate.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&encoding(0x3A, 0xCC, true, None, None, 0xCB, 0xA2)).unwrap(),
    );
    assert_rejected(
        &same_round_different_immediate,
        "SHA1RNDS4 provenance with equal low immediate bits",
    );
}

#[test]
fn memory_forms_remain_outside_source_replay() {
    for (_, map, opcode, has_immediate) in OPERATIONS {
        let bytes = encoding(map, opcode, has_immediate, None, None, 0x01, 0x5A);
        let function = function(&bytes, OptLevel::O2);
        assert_rejected(&function, &format!("memory {bytes:02X?}"));
    }
}
