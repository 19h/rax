//! Native source replay for legacy AH/CH/DH/BH scalar register operations.

use super::*;
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x4849_4748;

fn function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn high_byte_temporary(function: &SmirFunction) -> VReg {
    match function.blocks[0].ops[0].kind {
        OpKind::Shr {
            dst: temporary @ VReg::Virtual(_),
            ..
        } => temporary,
        _ => unreachable!("high-byte replay starts with a virtual SHR destination"),
    }
}

fn grouped_replay_instruction(instruction: X86InstructionBytes) -> X86InstructionBytes {
    let instruction = instruction
        .legacy_high_byte_multiply_replay()
        .map(|replay| replay.canonical_instruction)
        .unwrap_or(instruction);
    instruction
        .non_memory_prefix_canonical()
        .unwrap_or(instruction)
}

#[test]
fn legacy_high_byte_replay_admits_and_emits_each_supported_family_at_o0_o1_o2() {
    let cases: &[(&str, &[u8])] = &[
        ("mov ah,0xa5", &[0xB4, 0xA5]),
        (
            "prefixed mov bh,0x5a",
            &[0x65, 0x66, 0x67, 0xF3, 0xB7, 0x5A],
        ),
        ("add ah,al", &[0x00, 0xC4]),
        ("or ah,al", &[0x0A, 0xE0]),
        ("adc ch,bh", &[0x10, 0xFD]),
        ("sbb bh,dh", &[0x1A, 0xFE]),
        ("and ah,bl", &[0x20, 0xDC]),
        ("sub ah,al", &[0x2A, 0xE0]),
        ("xor bh,dh", &[0x30, 0xF7]),
        ("cmp ah,al", &[0x3A, 0xE0]),
        ("test al,ah", &[0x84, 0xE0]),
        ("xchg al,ah", &[0x86, 0xE0]),
        ("mov ah,bl", &[0x88, 0xDC]),
        ("mov al,ah", &[0x8A, 0xC4]),
        ("sub ah,0x81", &[0x80, 0xEC, 0x81]),
        ("mov bh,0x5a", &[0xC6, 0xC7, 0x5A]),
        ("test ch,0xa5", &[0xF6, 0xC5, 0xA5]),
        ("not dh", &[0xF6, 0xD6]),
        ("neg bh", &[0xF6, 0xDF]),
        ("mul ah", &[0xF6, 0xE4]),
        ("prefixed mul bh", &[0x65, 0x66, 0x67, 0xF3, 0xF6, 0xE7]),
        ("imul ah", &[0xF6, 0xEC]),
        ("prefixed imul bh", &[0x65, 0x66, 0x67, 0xF3, 0xF6, 0xEF]),
        ("inc dh", &[0xFE, 0xC6]),
        ("dec bh", &[0xFE, 0xCF]),
        ("setbe ah", &[0x0F, 0x96, 0xC4]),
        ("cmpxchg ch,dh", &[0x0F, 0xB0, 0xF5]),
        ("xadd ah,bh", &[0x0F, 0xC0, 0xFC]),
        ("crc32 eax,ah", &[0xF2, 0x0F, 0x38, 0xF0, 0xC4]),
        ("rol ah,0", &[0xC0, 0xC4, 0x00]),
        ("ror ch,1", &[0xD0, 0xCD]),
        ("rcl dh,2", &[0xC0, 0xD6, 0x02]),
        ("rcr bh,cl", &[0xD2, 0xDF]),
        ("shl ah,8", &[0xC0, 0xE4, 0x08]),
        ("sal ah,8", &[0xC0, 0xF4, 0x08]),
        ("sal bh,cl", &[0xD2, 0xF7]),
        ("shr ch,9", &[0xC0, 0xED, 0x09]),
        ("sar dh,31", &[0xC0, 0xFE, 0x1F]),
        (
            "prefixed shl ah,8",
            &[0x65, 0x66, 0x67, 0xF3, 0xC0, 0xE4, 0x08],
        ),
        ("prefixed add ah,ch", &[0x65, 0x66, 0x67, 0xF3, 0x00, 0xEC]),
    ];

    let mut lowered = 0usize;
    for (name, bytes) in cases {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(instruction.is_legacy_high_byte_register_replay(), "{name}");
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = function(bytes);
            optimize_function(&mut function, level);

            for spans in [
                crate::smir::ir::x86_legacy_high_byte_replay_spans(
                    &function.blocks[0],
                    &function.x86_instruction_bytes,
                ),
                crate::smir::ir::x86_native_replay_spans(
                    &function.blocks[0],
                    &function.x86_instruction_bytes,
                ),
            ] {
                let span = spans
                    .get(&0)
                    .unwrap_or_else(|| panic!("{name} {level:?}: missing replay span"));
                assert_eq!(span.end, function.blocks[0].ops.len(), "{name} {level:?}");
                let expected_instruction = grouped_replay_instruction(instruction);
                assert_eq!(span.instruction, expected_instruction, "{name} {level:?}");
            }

            assert!(is_native_clobber_safe(&function), "{name} {level:?}");
            assert!(
                !is_x86_aarch64_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                ),
                "{name} {level:?}: AArch64 host must retain interpreter fallback"
            );
            assert_eq!(
                x86_native_replay_feature_requirements(
                    &function,
                    &std::collections::HashMap::new(),
                ),
                X86NativeReplayFeatureRequirements::default(),
                "{name} {level:?}: scalar replay must not request vector features"
            );
            assert!(
                !uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new(),),
                "{name} {level:?}: scalar replay must not marshal vector state"
            );

            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            let replay_instruction = grouped_replay_instruction(instruction)
                .legacy_high_byte_group2_replay()
                .map(|replay| replay.canonical_instruction)
                .or_else(|| {
                    grouped_replay_instruction(instruction)
                        .legacy_high_byte_multiply_replay()
                        .map(|replay| replay.canonical_instruction)
                })
                .unwrap_or_else(|| grouped_replay_instruction(instruction));
            let replay_bytes = replay_instruction.as_slice();
            assert!(
                code.windows(replay_bytes.len())
                    .any(|window| window == replay_bytes),
                "{name} {level:?}: validated replay bytes absent from {code:02X?}"
            );
            if let Some(destination) = instruction.legacy_high_byte_cmpxchg_destination_index() {
                let expected = [0x3A, 0xC0 | destination, 0x9C, 0x0F, 0xB0];
                assert!(
                    code.windows(expected.len())
                        .any(|window| window == expected),
                    "{name} {level:?}: architectural compare/flag-save wrapper absent from {code:02X?}"
                );
                assert!(
                    code.windows(2)
                        .any(|window| window == [bytes[bytes.len() - 1], 0x9D]),
                    "{name} {level:?}: replay flags are not restored in {code:02X?}"
                );
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, cases.len() * 3);
}

const SETCC_SCANNER_PREFIXES: &[&[u8]] =
    &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];

#[test]
fn all_10752_high_byte_setcc_scanner_graphs_admit_and_emit_at_every_opt_level() {
    let mut encodings = 0usize;
    let mut profiles = 0usize;
    for prefix in SETCC_SCANNER_PREFIXES {
        for opcode in 0x90u8..=0x9F {
            for ignored_reg in 0u8..8 {
                for rm in 4u8..8 {
                    let mut bytes = prefix.to_vec();
                    bytes.extend([0x0F, opcode, 0xC0 | (ignored_reg << 3) | rm]);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    let grouped_instruction = grouped_replay_instruction(instruction);
                    let replay = instruction
                        .legacy_high_byte_setcc_replay()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(replay.parent, rm - 4, "{bytes:02X?}");
                    let canonical = [0x0F, opcode, 0xC0 | rm];
                    assert_eq!(
                        replay.canonical_instruction.as_slice(),
                        canonical,
                        "{bytes:02X?}"
                    );

                    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                        let mut function = function(&bytes);
                        optimize_function(&mut function, level);
                        assert_eq!(function.blocks[0].ops.len(), 5, "{bytes:02X?} {level:?}");
                        for spans in [
                            crate::smir::ir::x86_legacy_high_byte_replay_spans(
                                &function.blocks[0],
                                &function.x86_instruction_bytes,
                            ),
                            crate::smir::ir::x86_native_replay_spans(
                                &function.blocks[0],
                                &function.x86_instruction_bytes,
                            ),
                        ] {
                            let span = spans
                                .get(&0)
                                .unwrap_or_else(|| panic!("{bytes:02X?} {level:?}"));
                            assert_eq!(span.end, 5, "{bytes:02X?} {level:?}");
                            assert_eq!(
                                span.instruction, grouped_instruction,
                                "{bytes:02X?} {level:?}"
                            );
                        }
                        assert!(is_native_clobber_safe(&function), "{bytes:02X?} {level:?}");

                        let mut lowerer = X86_64Lowerer::new();
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
                        let code = lowerer.finalize().unwrap();
                        assert!(
                            code.windows(canonical.len())
                                .any(|window| window == canonical),
                            "canonical SETcc replay absent for {bytes:02X?} {level:?}: {code:02X?}"
                        );
                        profiles += 1;
                    }
                    encodings += 1;
                }
            }
        }
    }
    assert_eq!(encodings, 7 * 16 * 8 * 4);
    assert_eq!(profiles, 3 * 7 * 16 * 8 * 4);
}

fn assert_high_byte_setcc_rejected(function: &SmirFunction, label: &str) {
    assert!(
        crate::smir::ir::x86_legacy_high_byte_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "family selector admitted {label}"
    );
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "aggregate selector admitted {label}"
    );
    assert!(!is_native_clobber_safe(function), "gate admitted {label}");
}

#[test]
fn high_byte_setcc_replay_rejects_graph_provenance_and_ssa_mutations() {
    let base = function(&[0x0F, 0x96, 0xCC]); // ignored reg=1, SETBE AH
    assert_eq!(base.blocks[0].ops.len(), 5);

    let mut condition = base.clone();
    let OpKind::SetCC { cond, .. } = &mut condition.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *cond = Condition::Sgt;
    assert_high_byte_setcc_rejected(&condition, "condition");

    let mut set_width = base.clone();
    let OpKind::SetCC { width, .. } = &mut set_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = OpWidth::W64;
    assert_high_byte_setcc_rejected(&set_width, "SETcc width");

    let mut set_hint = base.clone();
    set_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert_high_byte_setcc_rejected(&set_hint, "SETcc hint");

    let mut byte_mask = base.clone();
    let OpKind::And { src2, .. } = &mut byte_mask.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *src2 = SrcOperand::Imm(0x7F);
    assert_high_byte_setcc_rejected(&byte_mask, "byte mask");

    let mut byte_flags = base.clone();
    let OpKind::And { flags, .. } = &mut byte_flags.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *flags = FlagUpdate::All;
    assert_high_byte_setcc_rejected(&byte_flags, "byte-mask flags");

    let mut shift = base.clone();
    let OpKind::Shl { amount, .. } = &mut shift.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *amount = SrcOperand::Imm(7);
    assert_high_byte_setcc_rejected(&shift, "high-byte shift");

    let mut parent = base.clone();
    let OpKind::And { src1, .. } = &mut parent.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *src1 = x86(X86Reg::Rcx);
    assert_high_byte_setcc_rejected(&parent, "preserved parent");

    let mut parent_mask = base.clone();
    let OpKind::And { src2, .. } = &mut parent_mask.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *src2 = SrcOperand::Imm(!0xFFFFu64 as i64);
    assert_high_byte_setcc_rejected(&parent_mask, "parent mask");

    let mut merge = base.clone();
    let OpKind::Or { dst, .. } = &mut merge.blocks[0].ops[4].kind else {
        unreachable!()
    };
    *dst = x86(X86Reg::Rcx);
    assert_high_byte_setcc_rejected(&merge, "merge destination");

    let mut extra = base.clone();
    let mut op = extra.blocks[0].ops[4].clone();
    op.id = crate::smir::ir::types::OpId(5);
    extra.blocks[0].ops.push(op);
    assert_high_byte_setcc_rejected(&extra, "extra same-PC operation");

    let mut wrong_opcode = base.clone();
    wrong_opcode.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x0F, 0x9F, 0xCC]).unwrap(),
    );
    assert_high_byte_setcc_rejected(&wrong_opcode, "condition provenance");

    let mut wrong_parent = base.clone();
    wrong_parent.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x0F, 0x96, 0xCD]).unwrap(),
    );
    assert_high_byte_setcc_rejected(&wrong_parent, "parent provenance");

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert_high_byte_setcc_rejected(&missing, "missing provenance");

    let condition_value = match base.blocks[0].ops[0].kind {
        OpKind::SetCC { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaped = base;
    escaped.blocks[0].terminator = Terminator::Return {
        values: vec![condition_value],
    };
    assert_high_byte_setcc_rejected(&escaped, "escaped SETcc temporary");
}

#[test]
fn all_56_scanner_high_byte_multiply_cells_admit_at_every_opt_level() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];

    let mut encodings = 0usize;
    let mut profiles = 0usize;
    for prefix in PREFIXES {
        for extension in [4u8, 5] {
            for rm in 4u8..8 {
                let mut bytes = prefix.to_vec();
                bytes.extend([0xF6, 0xC0 | (extension << 3) | rm]);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert!(instruction.is_legacy_high_byte_register_replay());
                let canonical = instruction
                    .legacy_high_byte_multiply_replay()
                    .unwrap()
                    .canonical_instruction;

                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let mut function = function(&bytes);
                    optimize_function(&mut function, level);
                    assert_eq!(function.blocks[0].ops.len(), 2, "{bytes:02X?} {level:?}");
                    let spans = crate::smir::ir::x86_legacy_high_byte_replay_spans(
                        &function.blocks[0],
                        &function.x86_instruction_bytes,
                    );
                    assert_eq!(spans.get(&0).map(|span| span.end), Some(2));
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?} {level:?}");

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&function)
                        .unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
                    let code = lowerer.finalize().unwrap();
                    assert!(
                        code.windows(canonical.as_slice().len())
                            .any(|window| window == canonical.as_slice()),
                        "exact multiply replay absent for {bytes:02X?} {level:?}: {code:02X?}"
                    );
                    profiles += 1;
                }
                encodings += 1;
            }
        }
    }
    assert_eq!(encodings, 56);
    assert_eq!(profiles, 168);
}

#[test]
fn all_32_scanner_high_byte_crc32_cells_admit_and_emit_at_every_opt_level() {
    let mut encodings = 0usize;
    let mut profiles = 0usize;
    for destination in 0u8..8 {
        for rm in 4u8..8 {
            let bytes = [0xF2, 0x0F, 0x38, 0xF0, 0xC0 | (destination << 3) | rm];
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let replay = instruction.legacy_high_byte_crc32_replay().unwrap();
            assert_eq!(replay.destination, destination);
            assert_eq!(replay.parent, rm - 4);

            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let mut function = function(&bytes);
                optimize_function(&mut function, level);
                assert_eq!(function.blocks[0].ops.len(), 2, "{bytes:02X?} {level:?}");
                for spans in [
                    crate::smir::ir::x86_legacy_high_byte_replay_spans(
                        &function.blocks[0],
                        &function.x86_instruction_bytes,
                    ),
                    crate::smir::ir::x86_native_replay_spans(
                        &function.blocks[0],
                        &function.x86_instruction_bytes,
                    ),
                ] {
                    assert_eq!(
                        spans.get(&0),
                        Some(&crate::smir::ir::X86NativeReplaySpan {
                            end: 2,
                            instruction,
                            needs_avx512vl: false,
                            needs_avx512dq: false,
                            needs_avx512fp16: false,
                            preserve_mxcsr_de: false,
                        }),
                        "{bytes:02X?} {level:?}"
                    );
                }
                assert!(is_native_clobber_safe(&function), "{bytes:02X?} {level:?}");
                assert!(
                    !is_x86_aarch64_native_clobber_safe_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                    ),
                    "{bytes:02X?} {level:?}: AArch64 host must retain fallback"
                );
                #[cfg(target_arch = "x86_64")]
                assert_eq!(
                    x86_native_scalar_features_supported_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                    ),
                    std::is_x86_feature_detected!("sse4.2"),
                    "{bytes:02X?} {level:?}"
                );
                #[cfg(not(target_arch = "x86_64"))]
                assert!(!x86_native_scalar_features_supported_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                ));

                let mut lowerer = X86_64Lowerer::new();
                lowerer
                    .lower_function(&function)
                    .unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
                let code = lowerer.finalize().unwrap();
                if matches!(destination, 4 | 5) {
                    let mut expected = vec![
                        0x56,
                        0x57,
                        0x48,
                        0x8B,
                        0x7D,
                        X86_STATE_PTR_AT_RBP as u8,
                        0x8B,
                        0x77,
                        destination * 8,
                    ];
                    expected.extend_from_slice(replay.state_backed_instruction.as_slice());
                    expected.extend_from_slice(&[0x48, 0x89, 0x77, destination * 8]);
                    if destination == 5 {
                        expected.extend_from_slice(&[0x48, 0x89, 0x75, 0x00]);
                    }
                    expected.extend_from_slice(&[0x5F, 0x5E]);
                    assert!(
                        code.windows(expected.len())
                            .any(|window| window == expected),
                        "state-backed replay absent for {bytes:02X?} {level:?}: expected={expected:02X?} code={code:02X?}"
                    );
                } else {
                    assert!(
                        code.windows(bytes.len()).any(|window| window == bytes),
                        "exact replay absent for {bytes:02X?} {level:?}: {code:02X?}"
                    );
                }
                profiles += 1;
            }
            encodings += 1;
        }
    }
    assert_eq!(encodings, 32);
    assert_eq!(profiles, 96);
}

#[test]
fn high_byte_crc32_replay_requires_exact_graph_provenance_and_ssa_confinement() {
    let base = function(&[0xF2, 0x0F, 0x38, 0xF0, 0xC4]); // crc32 eax,ah
    let temporary = high_byte_temporary(&base);
    let assert_rejected = |name: &str, function: &SmirFunction| {
        assert!(
            crate::smir::ir::x86_native_replay_spans(
                &function.blocks[0],
                &function.x86_instruction_bytes,
            )
            .is_empty(),
            "{name}"
        );
        assert!(!is_native_clobber_safe(function), "{name}");
    };

    let mut wrong_extract_shift = base.clone();
    let OpKind::Shr { amount, .. } = &mut wrong_extract_shift.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *amount = SrcOperand::Imm(7);
    assert_rejected("wrong extract shift", &wrong_extract_shift);

    let mut wrong_extract_width = base.clone();
    let OpKind::Shr { width, .. } = &mut wrong_extract_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = OpWidth::W32;
    assert_rejected("wrong extract width", &wrong_extract_width);

    let mut wrong_extract_flags = base.clone();
    let OpKind::Shr { flags, .. } = &mut wrong_extract_flags.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *flags = FlagUpdate::All;
    assert_rejected("wrong extract flags", &wrong_extract_flags);

    let mut hinted_extract = base.clone();
    hinted_extract.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert_rejected("unexpected extract hint", &hinted_extract);

    let mut wrong_extract_parent = base.clone();
    let OpKind::Shr { src, .. } = &mut wrong_extract_parent.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *src = x86(X86Reg::Rbx);
    assert_rejected("wrong extract parent", &wrong_extract_parent);

    let mut wrong_accumulator = base.clone();
    let OpKind::Crc32C { crc, .. } = &mut wrong_accumulator.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *crc = x86(X86Reg::Rcx);
    assert_rejected("wrong CRC accumulator", &wrong_accumulator);

    let mut wrong_destination = base.clone();
    let OpKind::Crc32C { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *dst = x86(X86Reg::Rcx);
    assert_rejected("wrong CRC destination", &wrong_destination);

    let mut wrong_data = base.clone();
    let OpKind::Crc32C { data, .. } = &mut wrong_data.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *data = x86(X86Reg::Rdx);
    assert_rejected("wrong CRC data", &wrong_data);

    let mut wrong_width = base.clone();
    let OpKind::Crc32C { data_width, .. } = &mut wrong_width.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *data_width = OpWidth::W16;
    assert_rejected("wrong CRC width", &wrong_width);

    let mut hinted = base.clone();
    hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::Mulx);
    assert_rejected("unexpected CRC hint", &hinted);

    let mut mismatched_metadata = base.clone();
    mismatched_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0xF2, 0x0F, 0x38, 0xF0, 0xCC]).unwrap(),
    );
    assert_rejected("mismatched destination metadata", &mismatched_metadata);

    let mut escaped_use = base.clone();
    let mut escape = escaped_use.blocks[0].ops[0].clone();
    escape.guest_pc = PC + 5;
    escape.kind = OpKind::Mov {
        dst: x86(X86Reg::Rbx),
        src: SrcOperand::Reg(temporary),
        width: OpWidth::W64,
    };
    escaped_use.blocks[0].ops.push(escape);
    assert_rejected("CRC extract temporary escaped", &escaped_use);

    let mut returned = base;
    returned.blocks[0].terminator = Terminator::Return {
        values: vec![temporary],
    };
    assert_rejected("CRC extract temporary returned", &returned);
}

#[test]
fn all_7168_scanner_high_byte_mov_immediates_admit_at_every_opt_level() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];

    let mut encodings = 0usize;
    let mut optimization_profiles = 0usize;
    for prefix in PREFIXES {
        for opcode in 0xB4u8..=0xB7 {
            for immediate in u8::MIN..=u8::MAX {
                let mut bytes = prefix.to_vec();
                bytes.extend([opcode, immediate]);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let grouped_instruction = grouped_replay_instruction(instruction);
                assert!(
                    instruction.is_legacy_high_byte_register_replay(),
                    "{bytes:02X?}"
                );

                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let mut function = function(&bytes);
                    optimize_function(&mut function, level);
                    assert_eq!(function.blocks[0].ops.len(), 5, "{bytes:02X?}, {level:?}");

                    for spans in [
                        crate::smir::ir::x86_legacy_high_byte_replay_spans(
                            &function.blocks[0],
                            &function.x86_instruction_bytes,
                        ),
                        crate::smir::ir::x86_native_replay_spans(
                            &function.blocks[0],
                            &function.x86_instruction_bytes,
                        ),
                    ] {
                        assert_eq!(
                            spans.get(&0),
                            Some(&crate::smir::ir::X86NativeReplaySpan {
                                end: 5,
                                instruction: grouped_instruction,
                                needs_avx512vl: false,
                                needs_avx512dq: false,
                                needs_avx512fp16: false,
                                preserve_mxcsr_de: false,
                            }),
                            "{bytes:02X?}, {level:?}"
                        );
                    }

                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}, {level:?}");
                    assert!(
                        !is_x86_aarch64_native_clobber_safe_excluding(
                            &function,
                            &std::collections::HashMap::new(),
                        ),
                        "{bytes:02X?}, {level:?}: AArch64 host must retain fallback"
                    );
                    optimization_profiles += 1;
                }
                encodings += 1;
            }
        }
    }

    assert_eq!(encodings, 7_168);
    assert_eq!(optimization_profiles, 21_504);
}

#[test]
fn legacy_carry_rotate_nonunit_counts_use_deterministic_state_backed_lowering() {
    let cases: &[(&str, &[u8])] = &[
        ("rcl al,0", &[0xC0, 0xD0, 0x00]),
        ("rcr al,2", &[0xC0, 0xD8, 0x02]),
        ("rcl eax,cl", &[0xD3, 0xD0]),
        ("rcr ax,17", &[0x66, 0xC1, 0xD8, 0x11]),
        ("rcl rax,64", &[0x48, 0xC1, 0xD0, 0x40]),
    ];

    for (name, bytes) in cases {
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_high_byte_group2_replay()
                .is_none(),
            "{name}: ordinary low/full-width form must not use high-byte replay"
        );
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = function(bytes);
            optimize_function(&mut function, level);
            assert!(is_native_clobber_safe(&function), "{name} {level:?}");

            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
        }
    }
}

#[test]
fn legacy_high_byte_replay_requires_exact_provenance_contiguity_and_ssa_confinement() {
    let bytes = [0x00, 0xC4];
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory = base.clone();
    memory.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x00, 0x04]).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory));

    let mut rex = base.clone();
    rex.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x40, 0x00, 0xC4]).unwrap(),
    );
    assert!(!is_native_clobber_safe(&rex));

    let group6 = function(&[0xC0, 0xF4, 0x02]);
    assert!(is_native_clobber_safe(&group6));

    let mut group6_without_provenance = group6;
    group6_without_provenance.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&group6_without_provenance));

    let mut malformed_imul = function(&[0xF6, 0xEC]);
    let crate::smir::ir::ops::OpKind::Shr { amount, .. } =
        &mut malformed_imul.blocks[0].ops[0].kind
    else {
        unreachable!("high-byte multiply starts with SHR")
    };
    *amount = SrcOperand::Imm(7);
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &malformed_imul.blocks[0],
            &malformed_imul.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&malformed_imul));

    let mut mismatched_signedness = function(&[0xF6, 0xE4]);
    mismatched_signedness.blocks[0].ops[1].kind =
        match mismatched_signedness.blocks[0].ops[1].kind.clone() {
            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            },
            _ => unreachable!("high-byte MUL graph ends with unsigned multiply"),
        };
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &mismatched_signedness.blocks[0],
            &mismatched_signedness.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&mismatched_signedness));

    let mut escaped_use = function(&[0xF6, 0xEC]);
    let temporary = high_byte_temporary(&escaped_use);
    let mut escape = escaped_use.blocks[0].ops[0].clone();
    escape.guest_pc = PC + 2;
    escape.kind = OpKind::Mov {
        dst: x86(X86Reg::Rbx),
        src: SrcOperand::Reg(temporary),
        width: OpWidth::W64,
    };
    escaped_use.blocks[0].ops.push(escape);
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &escaped_use.blocks[0],
            &escaped_use.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&escaped_use));

    let mut redefined = function(&[0xF6, 0xEC]);
    let temporary = high_byte_temporary(&redefined);
    let mut redefine = redefined.blocks[0].ops[0].clone();
    redefine.guest_pc = PC + 2;
    redefine.kind = OpKind::Mov {
        dst: temporary,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    };
    redefined.blocks[0].ops.push(redefine);
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &redefined.blocks[0],
            &redefined.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&redefined));

    let mut returned = function(&[0xF6, 0xEC]);
    let temporary = high_byte_temporary(&returned);
    returned.blocks[0].terminator = Terminator::Return {
        values: vec![temporary],
    };
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &returned.blocks[0],
            &returned.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&returned));

    let mut phi_redefined = function(&[0xF6, 0xEC]);
    let temporary = high_byte_temporary(&phi_redefined);
    phi_redefined.blocks[0].phis.push(PhiNode {
        dst: temporary,
        sources: Vec::new(),
    });
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &phi_redefined.blocks[0],
            &phi_redefined.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&phi_redefined));

    let mut noncontiguous = base;
    let mut split = noncontiguous.blocks[0].ops[0].clone();
    split.guest_pc = PC + 2;
    noncontiguous.blocks[0].ops.insert(1, split);
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &noncontiguous.blocks[0],
            &noncontiguous.x86_instruction_bytes,
        )
        .is_empty()
    );
    assert!(!is_native_clobber_safe(&noncontiguous));
}
