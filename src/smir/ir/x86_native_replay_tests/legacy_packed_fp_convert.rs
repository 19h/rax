//! Exact legacy MMX/SSE packed floating-point conversion replay classifiers.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xD6D0;
const KINDS: [X86LegacyPackedFpConvertKind; 4] = [
    X86LegacyPackedFpConvertKind::Cvtpi2ps,
    X86LegacyPackedFpConvertKind::Cvttps2pi,
    X86LegacyPackedFpConvertKind::Cvtps2pi,
    X86LegacyPackedFpConvertKind::Cvtps2pd,
];

fn opcode(kind: X86LegacyPackedFpConvertKind) -> u8 {
    match kind {
        X86LegacyPackedFpConvertKind::Cvtpi2ps => 0x2A,
        X86LegacyPackedFpConvertKind::Cvttps2pi => 0x2C,
        X86LegacyPackedFpConvertKind::Cvtps2pi => 0x2D,
        X86LegacyPackedFpConvertKind::Cvtps2pd => 0x5A,
    }
}

fn encoding(kind: X86LegacyPackedFpConvertKind, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    bytes.extend(rex);
    bytes.extend([0x0F, opcode(kind), modrm]);
    bytes
}

fn expected(
    kind: X86LegacyPackedFpConvertKind,
    rex: Option<u8>,
    modrm: u8,
) -> X86LegacyPackedFpConvertReplay {
    let rex = rex.unwrap_or(0);
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let rex_r = (rex & 0x04) << 1;
    let rex_b = (rex & 0x01) << 3;
    let (destination, source) = match kind {
        X86LegacyPackedFpConvertKind::Cvtpi2ps => (reg | rex_r, rm),
        X86LegacyPackedFpConvertKind::Cvttps2pi | X86LegacyPackedFpConvertKind::Cvtps2pi => {
            (reg, rm | rex_b)
        }
        X86LegacyPackedFpConvertKind::Cvtps2pd => (reg | rex_r, rm | rex_b),
    };
    X86LegacyPackedFpConvertReplay {
        kind,
        destination,
        source,
    }
}

#[test]
fn classifier_covers_all_4352_canonical_rex_register_encodings() {
    let mut classified = 0usize;
    for kind in KINDS {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(kind, rex, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_packed_fp_convert_replay(),
                    Some(expected(kind, rex, modrm)),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, KINDS.len() * 17 * 64);
}

#[test]
fn classifier_exhausts_opcode_modrm_and_canonical_prefix_frontiers() {
    for candidate_opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = [0x4F, 0x0F, candidate_opcode, modrm];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_packed_fp_convert_replay()
                    .is_some(),
                matches!(candidate_opcode, 0x2A | 0x2C | 0x2D | 0x5A) && modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    // LLVM 23.0.0 independently decodes all 16 REX images with R extending
    // only XMM ModR/M.reg operands, B extending only XMM ModR/M.r/m operands,
    // and W/X plus every MMX-side extension bit ignored.
    for rex in 0x40..=0x4F {
        for kind in KINDS {
            let bytes = encoding(kind, Some(rex), 0xCA);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_packed_fp_convert_replay(),
                Some(expected(kind, Some(rex), 0xCA)),
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x66, 0x0F, 0x2A, 0xCA],             // CVTPI2PD, not CVTPI2PS
        &[0x66, 0x0F, 0x2C, 0xCA],             // CVTTPD2PI, not CVTTPS2PI
        &[0x66, 0x0F, 0x2D, 0xCA],             // CVTPD2PI, not CVTPS2PI
        &[0x66, 0x0F, 0x5A, 0xCA],             // CVTPD2PS, not CVTPS2PD
        &[0xF2, 0x0F, 0x2D, 0xCA],             // scalar CVTSD2SI
        &[0xF3, 0x0F, 0x2A, 0xCA],             // scalar CVTSI2SS
        &[0xF0, 0x0F, 0x5A, 0xCA],             // lock prefix
        &[0x67, 0x0F, 0x2A, 0xCA],             // address-size prefix
        &[0x64, 0x0F, 0x2C, 0xCA],             // FS override
        &[0x65, 0x0F, 0x5A, 0xCA],             // GS override
        &[0x48, 0x67, 0x0F, 0x2A, 0xCA],       // REX not final
        &[0x48, 0x49, 0x0F, 0x2A, 0xCA],       // duplicate REX
        &[0xD5, 0x00, 0x0F, 0x2A, 0xCA],       // REX2
        &[0x0F, 0x2A, 0x0A],                   // memory source
        &[0x0F, 0x2A],                         // missing ModR/M
        &[0x0F, 0x5A, 0xCA, 0x00],             // trailing byte
        &[0xC5, 0xF8, 0x5A, 0xCA],             // VEX
        &[0x62, 0xF1, 0x7C, 0x08, 0x5A, 0xCA], // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_packed_fp_convert_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy packed-conversion provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8]) {
    let replay = X86InstructionBytes::new(bytes)
        .unwrap()
        .legacy_register_packed_fp_convert_replay()
        .unwrap();
    for spans in [
        x86_legacy_packed_fp_convert_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        ),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(
            span.end,
            function.blocks[0].ops.len() - usize::from(replay.kind.touches_mmx()),
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
        x86_legacy_packed_fp_convert_replay_spans(
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
    for kind in KINDS {
        let bytes = encoding(kind, Some(0x4F), 0xCA);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = function(&bytes, level);
            assert_eq!(
                function.blocks[0].ops.len(),
                if kind.touches_mmx() { 2 } else { 1 },
                "{level:?} {bytes:02X?}"
            );
            assert_span(&function, &bytes);
        }

        let baseline = function(&bytes, OptLevel::O0);
        for index in 0..baseline.blocks[0].ops.len() {
            let mut mutated = baseline.clone();
            mutated.blocks[0].ops[index].kind = OpKind::Nop;
            assert_rejected(&mutated, &format!("{kind:?} op {index}"));

            let mut hinted = baseline.clone();
            hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
            assert_rejected(&hinted, &format!("{kind:?} hinted op {index}"));
        }

        let mut extra = baseline.clone();
        let extra_id = OpId(extra.blocks[0].ops.len() as u16);
        extra.blocks[0].push_op(SmirOp::new(extra_id, PC, OpKind::Nop));
        assert_rejected(&extra, &format!("{kind:?} extra same-PC op"));
    }
}

#[test]
fn graph_validator_rejects_mismatched_missing_memory_and_reserved_provenance() {
    for (index, kind) in KINDS.into_iter().enumerate() {
        let bytes = encoding(kind, Some(0x45), 0xCA);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{kind:?} missing provenance"));

        let next_kind = KINDS[(index + 1) % KINDS.len()];
        for (label, metadata) in [
            ("mismatched kind", encoding(next_kind, Some(0x45), 0xCA)),
            ("wrong destination", encoding(kind, Some(0x45), 0xD2)),
            ("wrong source", encoding(kind, Some(0x45), 0xC9)),
            ("memory provenance", encoding(kind, Some(0x45), 0x0A)),
            ("reserved prefix", {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(kind, None, 0xCA));
                reserved
            }),
        ] {
            let mut malformed = baseline.clone();
            malformed.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&metadata).unwrap(),
            );
            assert_rejected(&malformed, &format!("{kind:?} {label}"));
        }
    }
}
