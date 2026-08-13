//! Exact legacy SSE4.1 floating-point ROUND replay classification.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FpRoundMode, FunctionId, SourceArch, VecElementType, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x0B0A_0800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl Kind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::PackedF32 => 0x08,
            Self::PackedF64 => 0x09,
            Self::ScalarF32 => 0x0A,
            Self::ScalarF64 => 0x0B,
        }
    }

    fn fields(self) -> (VecElementType, u8, bool) {
        match self {
            Self::PackedF32 => (VecElementType::F32, 4, false),
            Self::PackedF64 => (VecElementType::F64, 2, false),
            Self::ScalarF32 => (VecElementType::F32, 1, true),
            Self::ScalarF64 => (VecElementType::F64, 1, true),
        }
    }
}

fn encoding(kind: Kind, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, kind.opcode(), modrm, immediate]);
    bytes
}

fn mode(immediate: u8) -> FpRoundMode {
    if immediate & 4 != 0 {
        FpRoundMode::Dynamic
    } else {
        match immediate & 3 {
            0 => FpRoundMode::RoundNearest,
            1 => FpRoundMode::RoundDown,
            2 => FpRoundMode::RoundUp,
            _ => FpRoundMode::RoundTowardZero,
        }
    }
}

fn expected(kind: Kind, rex: Option<u8>, modrm: u8, immediate: u8) -> X86LegacyRoundReplay {
    let rex = rex.unwrap_or(0);
    let (elem, lanes, scalar_source) = kind.fields();
    X86LegacyRoundReplay {
        destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        source: (modrm & 7) | ((rex & 0x01) << 3),
        elem,
        lanes,
        scalar_source,
        mode: mode(immediate),
        suppress_precision: immediate & 8 != 0,
    }
}

#[test]
fn classifier_covers_all_1_114_112_rex_register_immediate_encodings() {
    let mut classified = 0usize;
    for kind in Kind::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(kind, rex, modrm, immediate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_round_replay(),
                        Some(expected(kind, rex, modrm, immediate)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, Kind::ALL.len() * 17 * 64 * 256);
}

#[test]
fn classifier_exhausts_opcode_modrm_controls_and_canonical_frontiers() {
    for opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = [0x66, 0x4F, 0x0F, 0x3A, opcode, modrm, 0xA5];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_round_replay()
                    .is_some(),
                matches!(opcode, 0x08..=0x0B) && modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    // LLVM 23 independently decodes every REX image with R/B extending only
    // the two XMM operands and W/X ignored.
    for rex in 0x40..=0x4F {
        for kind in Kind::ALL {
            let bytes = encoding(kind, Some(rex), 0xCA, 0xA5);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_round_replay(),
                Some(expected(kind, Some(rex), 0xCA, 0xA5)),
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x3A, 0x08, 0xCA, 0xA5],             // missing mandatory 66
        &[0xF2, 0x0F, 0x3A, 0x08, 0xCA, 0xA5],       // wrong mandatory prefix
        &[0xF0, 0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // LOCK
        &[0x67, 0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // reserved address prefix
        &[0x64, 0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // segment prefix excluded
        &[0x48, 0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // REX not final
        &[0x66, 0x48, 0x49, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // duplicate REX
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x08, 0xCA, 0xA5], // REX2
        &[0x66, 0x0F, 0x38, 0x08, 0xCA, 0xA5],       // wrong map
        &[0x66, 0x0F, 0x3A, 0x07, 0xCA, 0xA5],       // adjacent opcode
        &[0x66, 0x0F, 0x3A, 0x08, 0x0A, 0xA5],       // memory source
        &[0x66, 0x0F, 0x3A, 0x08, 0xCA],             // missing immediate
        &[0x66, 0x0F, 0x3A, 0x08, 0xCA, 0xA5, 0],    // trailing byte
        &[0xC4, 0xE3, 0x79, 0x08, 0xCA, 0xA5],       // VEX
        &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCA, 0xA5], // EVEX neighbor
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_round_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy ROUND provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8]) {
    for spans in [
        x86_legacy_round_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, 1, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_round_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_controls_rex_aliases_and_o0_o1_o2() {
    let register_shapes = [0xC0, 0xCA, 0xDB, 0xFF];
    for kind in Kind::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in register_shapes {
                for control in 0u8..16 {
                    let immediate = [0x00, 0xA0, 0xF0][usize::from(control) % 3] | control;
                    let bytes = encoding(kind, rex, modrm, immediate);
                    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                        let function = function(&bytes, level);
                        assert_eq!(function.blocks[0].ops.len(), 1, "{level:?} {bytes:02X?}");
                        assert_span(&function, &bytes);
                    }
                }
            }
        }
    }
}

#[test]
fn graph_validator_rejects_every_semantic_field_and_provenance_mutation() {
    for kind in Kind::ALL {
        let bytes = encoding(kind, Some(0x45), 0xCA, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);

        for mutation in 0..11 {
            let mut malformed = baseline.clone();
            let operation = &mut malformed.blocks[0].ops[0];
            if mutation == 0 {
                operation.x86_hint = Some(X86OpHint::RexByteReg);
            } else {
                let OpKind::X86Round {
                    dst,
                    merge,
                    src,
                    elem,
                    width,
                    lanes,
                    scalar_source,
                    zero_upper,
                    mode,
                    suppress_precision,
                } = &mut operation.kind
                else {
                    panic!("expected X86Round")
                };
                match mutation {
                    1 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    2 => *merge = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    3 => *src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    4 => {
                        *elem = if *elem == VecElementType::F32 {
                            VecElementType::F64
                        } else {
                            VecElementType::F32
                        }
                    }
                    5 => *width = VecWidth::V256,
                    6 => *lanes = lanes.wrapping_add(1),
                    7 => *scalar_source = !*scalar_source,
                    8 => *zero_upper = true,
                    9 => {
                        *mode = if *mode == FpRoundMode::RoundDown {
                            FpRoundMode::RoundUp
                        } else {
                            FpRoundMode::RoundDown
                        }
                    }
                    10 => *suppress_precision = !*suppress_precision,
                    _ => unreachable!(),
                }
            }
            assert_rejected(&malformed, &format!("{kind:?} mutation {mutation}"));
        }

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{kind:?} missing provenance"));

        let mut wrong_bytes = baseline.clone();
        wrong_bytes.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&encoding(kind, Some(0x45), 0xD3, 0xA5)).unwrap(),
        );
        assert_rejected(&wrong_bytes, &format!("{kind:?} operands"));

        let mut extra = baseline.clone();
        extra.blocks[0].push_op(SmirOp::new(OpId(1), PC, OpKind::Nop));
        assert_rejected(&extra, &format!("{kind:?} extra operation"));
    }
}
