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

#[test]
fn legacy_high_byte_replay_admits_and_emits_each_documented_family_at_o0_o1_o2() {
    let cases: &[(&str, &[u8])] = &[
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
        ("inc dh", &[0xFE, 0xC6]),
        ("dec bh", &[0xFE, 0xCF]),
        ("setbe ah", &[0x0F, 0x96, 0xC4]),
        ("cmpxchg ch,dh", &[0x0F, 0xB0, 0xF5]),
        ("xadd ah,bh", &[0x0F, 0xC0, 0xFC]),
        ("rol ah,0", &[0xC0, 0xC4, 0x00]),
        ("ror ch,1", &[0xD0, 0xCD]),
        ("rcl dh,2", &[0xC0, 0xD6, 0x02]),
        ("rcr bh,cl", &[0xD2, 0xDF]),
        ("shl ah,8", &[0xC0, 0xE4, 0x08]),
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
                assert_eq!(span.instruction.as_slice(), *bytes, "{name} {level:?}");
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
            let replay_instruction = instruction
                .legacy_high_byte_group2_replay()
                .map(|replay| replay.canonical_instruction)
                .unwrap_or(instruction);
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
fn legacy_high_byte_replay_requires_exact_provenance_and_contiguous_ir() {
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

    let undocumented_group6 = function(&[0xC0, 0xF4, 0x02]);
    assert!(!is_native_clobber_safe(&undocumented_group6));

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
