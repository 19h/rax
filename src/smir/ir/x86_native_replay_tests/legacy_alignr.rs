//! Exact legacy SSSE3 XMM `PALIGNR` replay classification and graph validation.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, OpWidth, SignExtend, SourceArch, SrcOperand, VecElementType, VirtualId,
    X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE9E0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn encoding(rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, 0x0F, modrm, immediate]);
    bytes
}

fn expected(rex: Option<u8>, modrm: u8, immediate: u8) -> X86LegacyAlignrReplay {
    let rex = rex.unwrap_or(0);
    X86LegacyAlignrReplay {
        destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        source: (modrm & 7) | ((rex & 0x01) << 3),
        immediate,
    }
}

#[test]
fn classifier_covers_all_278528_canonical_rex_register_and_immediate_images() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for modrm in 0xC0..=0xFF {
            for immediate in u8::MIN..=u8::MAX {
                let bytes = encoding(rex, modrm, immediate);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_alignr_replay(),
                    Some(expected(rex, modrm, immediate)),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 17 * 64 * 256);
}

#[test]
fn classifier_matches_llvm23_anchor_and_rejects_noncanonical_frontiers() {
    // Independently assembled by LLVM 23.0.0git.
    let anchor = [0x66, 0x45, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5];
    assert_eq!(
        X86InstructionBytes::new(&anchor)
            .unwrap()
            .legacy_register_alignr_replay(),
        Some(X86LegacyAlignrReplay {
            destination: 9,
            source: 10,
            immediate: 0xA5,
        })
    );

    // Intel SDM Order No. 325383-092US, Vol. 2A, classifies address-size and
    // segment prefixes on register-only forms as reserved/unpredictable. Exact
    // replay also rejects MMX and all duplicate, reordered, or malformed
    // source images.
    let invalid: &[&[u8]] = &[
        &[0x67, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x64, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x65, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x66, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x45, 0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x66, 0x45, 0x46, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x66, 0x45, 0x67, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0xF2, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        &[0x66, 0x0F, 0x38, 0x0F, 0xCA, 0xA5],
        &[0x66, 0x0F, 0x3A, 0x0E, 0xCA, 0xA5],
        &[0x66, 0x0F, 0x3A, 0x0F, 0x0A, 0xA5],
        &[0x66, 0x0F, 0x3A, 0x0F, 0xCA],
        &[0x66, 0x0F, 0x3A, 0x0F, 0xCA, 0xA5, 0x00],
        &[0xC4, 0xE3, 0x71, 0x0F, 0xCA, 0xA5],
        &[0x62, 0xF3, 0x75, 0x08, 0x0F, 0xCA, 0xA5],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_alignr_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy PALIGNR provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8]) {
    assert_eq!(function.blocks[0].ops.len(), 67, "{bytes:02X?}");
    for spans in [
        x86_legacy_alignr_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, 67, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_alignr_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_3264_rex_register_and_level_images() {
    let mut validated = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for modrm in 0xC0..=0xFF {
            let bytes = encoding(rex, modrm, 0xA5);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, 17 * 64 * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_every_immediate_at_o0_o1_o2() {
    let mut validated = 0usize;
    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(Some(0x45), 0xCA, immediate);
        for level in LEVELS {
            assert_span(&function(&bytes, level), &bytes);
            validated += 1;
        }
    }
    assert_eq!(validated, 256 * LEVELS.len());
}

fn different_element(element: VecElementType) -> VecElementType {
    if element == VecElementType::I8 {
        VecElementType::F64
    } else {
        VecElementType::I8
    }
}

fn mutation_count(kind: &OpKind) -> usize {
    match kind {
        OpKind::Mov { .. } => 3,
        OpKind::VBroadcast { .. } => 4,
        OpKind::VInsertLane { .. } | OpKind::VExtractLane { .. } => 5,
        OpKind::VShuffle { .. } => 6,
        _ => panic!("unexpected legacy PALIGNR graph operation: {kind:?}"),
    }
}

fn mutate(kind: &mut OpKind, mutation: usize) {
    let x0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    match kind {
        OpKind::Mov { dst, src, width } => match mutation {
            0 => *dst = x0,
            1 => {
                let value = src.as_imm().expect("selector immediate");
                *src = SrcOperand::Imm(value ^ 0x40);
            }
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        },
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } => match mutation {
            0 => *dst = x0,
            1 => *scalar = x0,
            2 => *elem = different_element(*elem),
            3 => *lanes = lanes.wrapping_add(1),
            _ => unreachable!(),
        },
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } => match mutation {
            0 => *dst = x0,
            1 => *vec = x0,
            2 => *scalar = x0,
            3 => *lane = lane.wrapping_add(1),
            4 => *elem = different_element(*elem),
            _ => unreachable!(),
        },
        OpKind::VShuffle {
            dst,
            src1,
            src2,
            indices,
            elem,
            lanes,
        } => match mutation {
            0 => *dst = x0,
            1 => *src1 = x0,
            2 => *src2 = None,
            3 => *indices = x0,
            4 => *elem = different_element(*elem),
            5 => *lanes = lanes.wrapping_add(1),
            _ => unreachable!(),
        },
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } => match mutation {
            0 => *dst = x0,
            1 => *vec = x0,
            2 => *lane = lane.wrapping_add(1),
            3 => *elem = different_element(*elem),
            4 => *sign = SignExtend::Sign,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[test]
fn graph_validator_rejects_every_operation_field_hint_and_virtual_escape() {
    for immediate in [0u8, 17, 32, 255] {
        let bytes = encoding(Some(0x45), 0xCA, immediate);
        for level in LEVELS {
            let baseline = function(&bytes, level);
            for operation_index in 0..baseline.blocks[0].ops.len() {
                let mut hinted = baseline.clone();
                hinted.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
                assert_rejected(
                    &hinted,
                    &format!("imm {immediate} {level:?} hint op {operation_index}"),
                );
                for mutation in 0..mutation_count(&baseline.blocks[0].ops[operation_index].kind) {
                    let mut malformed = baseline.clone();
                    mutate(&mut malformed.blocks[0].ops[operation_index].kind, mutation);
                    assert_rejected(
                        &malformed,
                        &format!(
                            "imm {immediate} {level:?} op {operation_index} mutation {mutation}"
                        ),
                    );
                }
            }
        }
    }

    let bytes = encoding(Some(0x45), 0xCA, 0xA5);
    let baseline = function(&bytes, OptLevel::O0);
    let virtuals: Vec<_> = baseline.blocks[0]
        .ops
        .iter()
        .flat_map(|operation| operation.kind.dests())
        .filter(|register| matches!(register, VReg::Virtual(_)))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    for (ordinal, temporary) in virtuals.into_iter().enumerate() {
        let mut escaped = baseline.clone();
        escaped.blocks[0].push_op(SmirOp::new(
            OpId(0xF000u16.wrapping_add(ordinal as u16)),
            PC + 1,
            OpKind::VShuffle {
                dst: VReg::Virtual(VirtualId(0xF000u32.wrapping_add(ordinal as u32))),
                src1: temporary,
                src2: None,
                indices: temporary,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ));
        assert_rejected(&escaped, &format!("escaping {temporary:?}"));

        let mut redefined = baseline.clone();
        redefined.blocks[0].push_op(SmirOp::new(
            OpId(0xE000u16.wrapping_add(ordinal as u16)),
            PC + 1,
            OpKind::Mov {
                dst: temporary,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected(&redefined, &format!("redefined {temporary:?}"));
    }
}

#[test]
fn graph_validator_rejects_missing_mismatched_memory_and_reserved_provenance() {
    let bytes = encoding(Some(0x45), 0xCA, 0xA5);
    let baseline = function(&bytes, OptLevel::O0);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert_rejected(&missing, "missing provenance");

    // Every unsigned immediate at or above 32 bytes has the same specified
    // all-zero result and therefore the same canonical semantic graph.
    let equivalent_bytes = encoding(Some(0x45), 0xCA, 0xA6);
    let mut equivalent = baseline.clone();
    equivalent.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&equivalent_bytes).unwrap(),
    );
    assert_span(&equivalent, &equivalent_bytes);

    let metadata = [
        encoding(Some(0x45), 0xD3, 0xA5),
        encoding(Some(0x45), 0xCA, 0x1F),
        encoding(Some(0x45), 0x0A, 0xA5),
        vec![0x0F, 0x3A, 0x0F, 0xCA, 0xA5],
        {
            let mut reserved = vec![0x67];
            reserved.extend(encoding(None, 0xCA, 0xA5));
            reserved
        },
    ];
    for bytes in metadata {
        let mut malformed = baseline.clone();
        malformed
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected(&malformed, &format!("{bytes:02X?}"));
    }
}
