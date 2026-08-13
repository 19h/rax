//! Exact legacy MMX/SSE widening doubleword-multiply replay classifiers.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xD4D0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    MmxUnsigned,
    XmmUnsigned,
    XmmSigned,
}

impl Shape {
    fn replay(self, rex: Option<u8>, modrm: u8) -> X86LegacyWideningDwordMultiplyReplay {
        let extension = if self == Self::MmxUnsigned {
            0
        } else {
            rex.unwrap_or(0)
        };
        X86LegacyWideningDwordMultiplyReplay {
            destination: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
            source: (modrm & 7) | ((extension & 0x01) << 3),
            signed: self == Self::XmmSigned,
            mmx: self == Self::MmxUnsigned,
        }
    }
}

const SHAPES: [Shape; 3] = [Shape::MmxUnsigned, Shape::XmmUnsigned, Shape::XmmSigned];

fn encoding(shape: Shape, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if shape != Shape::MmxUnsigned {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    bytes.extend(match shape {
        Shape::MmxUnsigned | Shape::XmmUnsigned => vec![0x0F, 0xF4, modrm],
        Shape::XmmSigned => vec![0x0F, 0x38, 0x28, modrm],
    });
    bytes
}

#[test]
fn classifier_covers_all_3264_canonical_rex_register_encodings() {
    let mut classified = 0usize;
    for shape in SHAPES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(shape, rex, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_widening_dword_multiply_replay(),
                    Some(shape.replay(rex, modrm)),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 3 * 17 * 64);
}

#[test]
fn classifier_exhausts_opcode_modrm_and_canonical_prefix_frontiers() {
    for opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let unsigned = [0x66, 0x4F, 0x0F, opcode, modrm];
            assert_eq!(
                X86InstructionBytes::new(&unsigned)
                    .unwrap()
                    .legacy_register_widening_dword_multiply_replay()
                    .is_some(),
                opcode == 0xF4 && modrm >> 6 == 3,
                "{unsigned:02X?}"
            );

            let signed = [0x66, 0x4F, 0x0F, 0x38, opcode, modrm];
            assert_eq!(
                X86InstructionBytes::new(&signed)
                    .unwrap()
                    .legacy_register_widening_dword_multiply_replay()
                    .is_some(),
                opcode == 0x28 && modrm >> 6 == 3,
                "{signed:02X?}"
            );

            let mmx = [0x4F, 0x0F, opcode, modrm];
            assert_eq!(
                X86InstructionBytes::new(&mmx)
                    .unwrap()
                    .legacy_register_widening_dword_multiply_replay()
                    .is_some(),
                opcode == 0xF4 && modrm >> 6 == 3,
                "{mmx:02X?}"
            );
        }
    }

    // Intel SDM specifies ignored REX bits when they have no meaning. LLVM
    // 23.0.0 independently decodes all 16 MMX REX images as the same operands.
    for rex in 0x40..=0x4F {
        let bytes = encoding(Shape::MmxUnsigned, Some(rex), 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_widening_dword_multiply_replay(),
            Some(X86LegacyWideningDwordMultiplyReplay {
                destination: 1,
                source: 2,
                signed: false,
                mmx: true,
            }),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x67, 0x0F, 0xF4, 0xCA],             // address-size prefix
        &[0x64, 0x0F, 0xF4, 0xCA],             // segment override
        &[0x65, 0x66, 0x0F, 0xF4, 0xCA],       // XMM segment override
        &[0xF2, 0x0F, 0xF4, 0xCA],             // repeat prefix
        &[0xF0, 0x0F, 0xF4, 0xCA],             // lock prefix
        &[0x48, 0x66, 0x0F, 0xF4, 0xCA],       // REX not final
        &[0x66, 0x66, 0x0F, 0xF4, 0xCA],       // duplicate mandatory 66
        &[0xD5, 0x00, 0x0F, 0xF4, 0xCA],       // REX2
        &[0x0F, 0x38, 0x28, 0xCA],             // no MMX PMULDQ form
        &[0x66, 0x0F, 0xF4, 0x0A],             // XMM memory source
        &[0x0F, 0xF4, 0x0A],                   // MMX memory source
        &[0x66, 0x0F, 0xF5, 0xCA],             // neighboring opcode
        &[0x0F, 0xF4],                         // missing ModR/M
        &[0x66, 0x0F, 0x38, 0x28],             // missing ModR/M
        &[0x0F, 0xF4, 0xCA, 0x00],             // trailing byte
        &[0x66, 0x0F, 0xF4, 0xCA, 0x00],       // trailing byte
        &[0xC5, 0xF1, 0xF4, 0xCA],             // VEX
        &[0x62, 0xF1, 0xFD, 0x08, 0xF4, 0xCA], // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_widening_dword_multiply_replay(),
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
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy widening-multiply provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8]) {
    for spans in [
        x86_legacy_widening_dword_multiply_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        ),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        let replay = X86InstructionBytes::new(bytes)
            .unwrap()
            .legacy_register_widening_dword_multiply_replay()
            .unwrap();
        assert_eq!(
            span.end,
            function.blocks[0].ops.len() - usize::from(replay.mmx),
            "{bytes:02X?}"
        );
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_widening_dword_multiply_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_survives_o0_o1_o2_and_rejects_every_op_mutation() {
    for shape in SHAPES {
        let bytes = encoding(shape, Some(0x4F), 0xCA);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = function(&bytes, level);
            assert_eq!(
                function.blocks[0].ops.len(),
                if shape == Shape::MmxUnsigned { 8 } else { 15 },
                "{level:?} {bytes:02X?}"
            );
            assert_span(&function, &bytes);
        }

        let baseline = function(&bytes, OptLevel::O0);
        for index in 0..baseline.blocks[0].ops.len() {
            let mut mutated = baseline.clone();
            mutated.blocks[0].ops[index].kind = OpKind::Nop;
            assert_rejected(&mutated, &format!("{shape:?} op {index}"));

            let mut hinted = baseline.clone();
            hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
            assert_rejected(&hinted, &format!("{shape:?} hinted op {index}"));
        }

        let temporary = baseline.blocks[0]
            .ops
            .iter()
            .flat_map(|op| op.kind.dests())
            .find(|register| matches!(register, VReg::Virtual(_)))
            .expect("legacy widening-multiply temporary");
        let mut escaped = baseline.clone();
        escaped.blocks[0].set_terminator(Terminator::Return {
            values: vec![temporary],
        });
        assert_rejected(&escaped, &format!("{shape:?} escaped temporary"));
    }
}

#[test]
fn graph_validator_rejects_mismatched_missing_memory_and_reserved_provenance() {
    for shape in SHAPES {
        let bytes = encoding(shape, Some(0x45), 0xCA);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{shape:?} missing provenance"));

        let mismatched_shape = match shape {
            Shape::MmxUnsigned => Shape::XmmUnsigned,
            Shape::XmmUnsigned => Shape::XmmSigned,
            Shape::XmmSigned => Shape::XmmUnsigned,
        };
        for (label, metadata) in [
            (
                "mismatched shape",
                encoding(mismatched_shape, Some(0x45), 0xCA),
            ),
            ("wrong destination", encoding(shape, Some(0x45), 0xD2)),
            ("wrong source", encoding(shape, Some(0x45), 0xC9)),
            ("memory provenance", encoding(shape, Some(0x45), 0x0A)),
        ] {
            let mut malformed = baseline.clone();
            malformed.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&metadata).unwrap(),
            );
            assert_rejected(&malformed, &format!("{shape:?} {label}"));
        }
    }
}
