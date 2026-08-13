//! Exact legacy GFNI replay classification and semantic-graph validation.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, OpWidth, ShiftOp, SignExtend, SourceArch, SrcOperand, VecElementType,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator, X86VexGfniMemoryKind};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xEBE0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const KINDS: [X86VexGfniMemoryKind; 3] = [
    X86VexGfniMemoryKind::Multiply,
    X86VexGfniMemoryKind::Affine,
    X86VexGfniMemoryKind::AffineInverse,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GfniCase {
    kind: X86VexGfniMemoryKind,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
}

impl GfniCase {
    fn bytes(self) -> Vec<u8> {
        assert!(self.rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
        let mut bytes = vec![0x66];
        bytes.extend(self.rex);
        match self.kind {
            X86VexGfniMemoryKind::Multiply => bytes.extend([0x0F, 0x38, 0xCF, self.modrm]),
            X86VexGfniMemoryKind::Affine => {
                bytes.extend([0x0F, 0x3A, 0xCE, self.modrm, self.immediate]);
            }
            X86VexGfniMemoryKind::AffineInverse => {
                bytes.extend([0x0F, 0x3A, 0xCF, self.modrm, self.immediate]);
            }
        }
        bytes
    }

    fn expected(self) -> X86LegacyGfniReplay {
        let rex = self.rex.unwrap_or(0);
        X86LegacyGfniReplay {
            kind: self.kind,
            destination: ((self.modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (self.modrm & 7) | ((rex & 0x01) << 3),
            immediate: (self.kind != X86VexGfniMemoryKind::Multiply).then_some(self.immediate),
        }
    }
}

#[test]
fn classifier_covers_all_558144_canonical_rex_register_and_immediate_images() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for modrm in 0xC0..=0xFF {
            for kind in KINDS {
                if kind == X86VexGfniMemoryKind::Multiply {
                    let case = GfniCase {
                        kind,
                        rex,
                        modrm,
                        immediate: 0,
                    };
                    let bytes = case.bytes();
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_gfni_replay(),
                        Some(case.expected()),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                } else {
                    for immediate in u8::MIN..=u8::MAX {
                        let case = GfniCase {
                            kind,
                            rex,
                            modrm,
                            immediate,
                        };
                        let bytes = case.bytes();
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .legacy_register_gfni_replay(),
                            Some(case.expected()),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 17 * 64 * (1 + 2 * 256));
}

#[test]
fn classifier_matches_llvm23_anchors_and_rejects_noncanonical_frontiers() {
    // Independently assembled by LLVM 23.0.0git.
    let anchors = [
        (
            &[0x66, 0x45, 0x0F, 0x38, 0xCF, 0xCA][..],
            X86VexGfniMemoryKind::Multiply,
            None,
        ),
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0xCE, 0xCA, 0xA5][..],
            X86VexGfniMemoryKind::Affine,
            Some(0xA5),
        ),
        (
            &[0x66, 0x45, 0x0F, 0x3A, 0xCF, 0xCA, 0x5A][..],
            X86VexGfniMemoryKind::AffineInverse,
            Some(0x5A),
        ),
    ];
    for (bytes, kind, immediate) in anchors {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_gfni_replay(),
            Some(X86LegacyGfniReplay {
                kind,
                destination: 9,
                source: 10,
                immediate,
            })
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x67, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x64, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x65, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0xF0, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x66, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x45, 0x66, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x66, 0x45, 0x46, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x66, 0xD5, 0x00, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x0F, 0x38, 0xCF, 0xCA],
        &[0xF2, 0x0F, 0x38, 0xCF, 0xCA],
        &[0x66, 0x0F, 0x38, 0xCE, 0xCA],
        &[0x66, 0x0F, 0x3A, 0xCD, 0xCA, 0xA5],
        &[0x66, 0x0F, 0x38, 0xCF, 0x0A],
        &[0x66, 0x0F, 0x3A, 0xCE, 0x0A, 0xA5],
        &[0x66, 0x0F, 0x3A, 0xCE, 0xCA],
        &[0x66, 0x0F, 0x38, 0xCF, 0xCA, 0x00],
        &[0xC4, 0x42, 0x29, 0xCF, 0xCA],
        &[0x62, 0x42, 0x05, 0x08, 0xCF, 0xCA],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_gfni_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

fn function(case: GfniCase, level: OptLevel) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?}");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("legacy GFNI provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn expected_len(kind: X86VexGfniMemoryKind, level: OptLevel) -> usize {
    match (kind, level) {
        (X86VexGfniMemoryKind::Multiply, OptLevel::O0) => 118,
        (X86VexGfniMemoryKind::Multiply, _) => 112,
        (X86VexGfniMemoryKind::Affine, _) => 142,
        (X86VexGfniMemoryKind::AffineInverse, OptLevel::O0) => 1_260,
        (X86VexGfniMemoryKind::AffineInverse, _) => 1_182,
    }
}

fn assert_span(function: &SmirFunction, case: GfniCase) {
    let bytes = case.bytes();
    for spans in [
        x86_legacy_gfni_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans
            .get(&0)
            .unwrap_or_else(|| panic!("{case:?} {bytes:02X?}"));
        assert_eq!(span.end, function.blocks[0].ops.len(), "{case:?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{case:?}");
        assert!(!span.needs_avx512vl, "{case:?}");
        assert!(!span.needs_avx512dq, "{case:?}");
        assert!(!span.needs_avx512fp16, "{case:?}");
        assert!(!span.preserve_mxcsr_de, "{case:?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_gfni_replay_spans(&function.blocks[0], &function.x86_instruction_bytes)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_9792_kind_rex_register_and_level_images() {
    let mut validated = 0usize;
    for kind in KINDS {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let case = GfniCase {
                    kind,
                    rex,
                    modrm,
                    immediate: 0xA5,
                };
                for level in LEVELS {
                    let function = function(case, level);
                    assert_eq!(function.blocks[0].ops.len(), expected_len(kind, level));
                    assert_span(&function, case);
                    validated += 1;
                }
            }
        }
    }
    assert_eq!(validated, KINDS.len() * 17 * 64 * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_every_affine_immediate_at_o0_o1_o2() {
    let mut validated = 0usize;
    for kind in [
        X86VexGfniMemoryKind::Affine,
        X86VexGfniMemoryKind::AffineInverse,
    ] {
        for immediate in u8::MIN..=u8::MAX {
            let case = GfniCase {
                kind,
                rex: Some(0x45),
                modrm: 0xCA,
                immediate,
            };
            for level in LEVELS {
                assert_span(&function(case, level), case);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, 2 * 256 * LEVELS.len());
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
        OpKind::VAnd { .. } | OpKind::VOr { .. } | OpKind::VXor { .. } => 4,
        OpKind::VSub { .. } | OpKind::VExtractLane { .. } | OpKind::VInsertLane { .. } => 5,
        OpKind::VShift { .. } => 6,
        OpKind::VByteShuffle { .. } => 5,
        _ => panic!("unexpected legacy GFNI graph operation: {kind:?}"),
    }
}

fn mutate(kind: &mut OpKind, mutation: usize) {
    let x0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    match kind {
        OpKind::Mov { dst, src, width } => match mutation {
            0 => *dst = x0,
            1 => *src = SrcOperand::Imm(src.as_imm().unwrap() ^ 0x40),
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
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => match mutation {
            0 => *dst = x0,
            1 => *src1 = x0,
            2 => *src2 = x0,
            3 => *width = VecWidth::V256,
            _ => unreachable!(),
        },
        OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => match mutation {
            0 => *dst = x0,
            1 => *src1 = x0,
            2 => *src2 = x0,
            3 => *elem = different_element(*elem),
            4 => *lanes = lanes.wrapping_add(1),
            _ => unreachable!(),
        },
        OpKind::VShift {
            dst,
            src,
            amount,
            shift,
            elem,
            lanes,
        } => match mutation {
            0 => *dst = x0,
            1 => *src = x0,
            2 => *amount = SrcOperand::Imm(amount.as_imm().unwrap() ^ 0x40),
            3 => {
                *shift = if *shift == ShiftOp::Lsl {
                    ShiftOp::Lsr
                } else {
                    ShiftOp::Lsl
                };
            }
            4 => *elem = different_element(*elem),
            5 => *lanes = lanes.wrapping_add(1),
            _ => unreachable!(),
        },
        OpKind::VByteShuffle {
            dst,
            src,
            control,
            lanes,
            block_lanes,
        } => match mutation {
            0 => *dst = x0,
            1 => *src = x0,
            2 => *control = x0,
            3 => *lanes = lanes.wrapping_add(1),
            4 => *block_lanes = block_lanes.wrapping_add(1),
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
        _ => unreachable!(),
    }
}

#[test]
fn graph_validator_rejects_every_inverse_operation_field_and_hint_at_o0_o2() {
    let case = GfniCase {
        kind: X86VexGfniMemoryKind::AffineInverse,
        rex: Some(0x45),
        modrm: 0xCA,
        immediate: 0xA5,
    };
    for level in [OptLevel::O0, OptLevel::O2] {
        let baseline = function(case, level);
        for operation_index in 0..baseline.blocks[0].ops.len() {
            let mut hinted = baseline.clone();
            hinted.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
            assert_rejected(&hinted, &format!("{level:?} hint op {operation_index}"));
            for mutation in 0..mutation_count(&baseline.blocks[0].ops[operation_index].kind) {
                let mut malformed = baseline.clone();
                mutate(&mut malformed.blocks[0].ops[operation_index].kind, mutation);
                assert_rejected(
                    &malformed,
                    &format!("{level:?} op {operation_index} mutation {mutation}"),
                );
            }
        }
    }
}

#[test]
fn graph_validator_rejects_virtual_escapes_redefinitions_and_provenance_mismatches() {
    let case = GfniCase {
        kind: X86VexGfniMemoryKind::AffineInverse,
        rex: Some(0x45),
        modrm: 0xCA,
        immediate: 0xA5,
    };
    let baseline = function(case, OptLevel::O2);
    assert_span(&baseline, case);
    let virtuals: Vec<_> = baseline.blocks[0]
        .ops
        .iter()
        .flat_map(|operation| operation.kind.dests())
        .filter(|register| matches!(register, VReg::Virtual(_)))
        .collect();
    assert_eq!(virtuals.len(), expected_len(case.kind, OptLevel::O2) - 16);
    assert_eq!(
        virtuals
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        virtuals.len(),
        "every exact GFNI graph temporary must have one definition"
    );
    let sample_indices = [
        0,
        virtuals.len() / 4,
        virtuals.len() / 2,
        3 * virtuals.len() / 4,
        virtuals.len() - 1,
    ];
    for (ordinal, index) in sample_indices.into_iter().enumerate() {
        let temporary = virtuals[index];
        let mut escaped = baseline.clone();
        escaped.blocks[0].push_op(SmirOp::new(
            OpId(0xF000 + ordinal as u16),
            PC + 1,
            OpKind::VMov {
                dst: VReg::Virtual(VirtualId(0xF000 + ordinal as u32)),
                src: temporary,
                width: VecWidth::V128,
            },
        ));
        assert_rejected(&escaped, &format!("escaping {temporary:?}"));

        let mut redefined = baseline.clone();
        redefined.blocks[0].push_op(SmirOp::new(
            OpId(0xE000 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: temporary,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected(&redefined, &format!("redefined {temporary:?}"));
    }

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert_rejected(&missing, "missing provenance");

    let metadata = [
        GfniCase {
            modrm: 0xD3,
            ..case
        }
        .bytes(),
        GfniCase {
            immediate: 0xA4,
            ..case
        }
        .bytes(),
        GfniCase {
            kind: X86VexGfniMemoryKind::Affine,
            ..case
        }
        .bytes(),
        vec![0x66, 0x45, 0x0F, 0x3A, 0xCF, 0x0A, 0xA5],
        {
            let mut reserved = vec![0x67];
            reserved.extend(GfniCase { rex: None, ..case }.bytes());
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
