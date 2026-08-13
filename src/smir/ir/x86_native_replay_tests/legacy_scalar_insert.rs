//! Exact legacy MMX/SSE scalar-insert replay classification and graph validation.

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

const PC: u64 = 0xE5E0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    PinsB,
    PinsD,
    PinsQ,
    PinsWMap1Mmx,
    PinsWMap1Xmm,
}

impl Family {
    const ALL: [Self; 5] = [
        Self::PinsB,
        Self::PinsD,
        Self::PinsQ,
        Self::PinsWMap1Mmx,
        Self::PinsWMap1Xmm,
    ];

    fn kind(self) -> X86LegacyScalarInsertKind {
        match self {
            Self::PinsB => X86LegacyScalarInsertKind::PinsB,
            Self::PinsD => X86LegacyScalarInsertKind::PinsD,
            Self::PinsQ => X86LegacyScalarInsertKind::PinsQ,
            Self::PinsWMap1Mmx => X86LegacyScalarInsertKind::PinsWMap1Mmx,
            Self::PinsWMap1Xmm => X86LegacyScalarInsertKind::PinsWMap1Xmm,
        }
    }

    fn map1(self) -> bool {
        matches!(self, Self::PinsWMap1Mmx | Self::PinsWMap1Xmm)
    }

    fn mmx(self) -> bool {
        self == Self::PinsWMap1Mmx
    }

    fn lanes(self) -> u8 {
        match self {
            Self::PinsB => 16,
            Self::PinsD | Self::PinsWMap1Mmx => 4,
            Self::PinsQ => 2,
            Self::PinsWMap1Xmm => 8,
        }
    }

    fn rex_images(self) -> Vec<Option<u8>> {
        match self {
            Self::PinsD => [None].into_iter().chain((0x40..=0x47).map(Some)).collect(),
            Self::PinsQ => (0x48..=0x4F).map(Some).collect(),
            _ => [None].into_iter().chain((0x40..=0x4F).map(Some)).collect(),
        }
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if !family.mmx() {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    if family.map1() {
        bytes.extend([0x0F, 0xC4, modrm, immediate]);
    } else {
        let opcode = match family {
            Family::PinsB => 0x20,
            Family::PinsD | Family::PinsQ => 0x22,
            Family::PinsWMap1Mmx | Family::PinsWMap1Xmm => unreachable!(),
        };
        bytes.extend([0x0F, 0x3A, opcode, modrm, immediate]);
    }
    bytes
}

fn expected(
    family: Family,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
) -> X86LegacyScalarInsertReplay {
    let rex = rex.unwrap_or(0);
    let destination = (modrm >> 3) & 7;
    X86LegacyScalarInsertReplay {
        kind: family.kind(),
        destination: if family.mmx() {
            destination
        } else {
            destination | ((rex & 0x04) << 1)
        },
        source: (modrm & 7) | ((rex & 0x01) << 3),
        lane: immediate & (family.lanes() - 1),
    }
}

#[test]
fn classifier_covers_all_1114112_canonical_rex_register_and_immediate_images() {
    let mut classified = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(family, rex, modrm, immediate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_scalar_insert_replay(),
                        Some(expected(family, rex, modrm, immediate)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, (3 * 17 + 9 + 8) * 64 * 256);
}

#[test]
fn classifier_matches_llvm23_anchors_and_rejects_noncanonical_frontiers() {
    // Independently assembled by LLVM 23.0.0git.
    let anchors: &[(Family, &[u8])] = &[
        (Family::PinsB, &[0x66, 0x45, 0x0F, 0x3A, 0x20, 0xCB, 0x0F]),
        (Family::PinsB, &[0x66, 0x44, 0x0F, 0x3A, 0x20, 0xCC, 0x0F]),
        (Family::PinsD, &[0x66, 0x45, 0x0F, 0x3A, 0x22, 0xCB, 0x03]),
        (Family::PinsD, &[0x66, 0x44, 0x0F, 0x3A, 0x22, 0xCD, 0x03]),
        (Family::PinsQ, &[0x66, 0x4D, 0x0F, 0x3A, 0x22, 0xCB, 0x01]),
        (Family::PinsQ, &[0x66, 0x4C, 0x0F, 0x3A, 0x22, 0xCC, 0x01]),
        (Family::PinsWMap1Xmm, &[0x66, 0x45, 0x0F, 0xC4, 0xCB, 0x07]),
        (Family::PinsWMap1Xmm, &[0x66, 0x44, 0x0F, 0xC4, 0xCD, 0x07]),
        (Family::PinsWMap1Mmx, &[0x41, 0x0F, 0xC4, 0xFB, 0x03]),
        (Family::PinsWMap1Mmx, &[0x0F, 0xC4, 0xFC, 0x03]),
    ];
    for (family, bytes) in anchors {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_scalar_insert_replay()
                .map(|replay| replay.kind),
            Some(family.kind()),
            "{bytes:02X?}"
        );
    }

    for rex in 0x40..=0x4F {
        let family = if rex & 0x08 == 0 {
            Family::PinsD
        } else {
            Family::PinsQ
        };
        let bytes = encoding(family, Some(rex), 0xCA, 0xA5);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_scalar_insert_replay(),
            Some(expected(family, Some(rex), 0xCA, 0xA5)),
            "{bytes:02X?}"
        );
    }

    // Intel SDM 325383-092, Vol. 2A, classifies address-size and segment
    // prefixes on register-only MMX forms as reserved/unpredictable. The same
    // exact-source policy rejects them for every scalar-insert form.
    let invalid: &[&[u8]] = &[
        &[0x67, 0x0F, 0xC4, 0xCA, 0x03],
        &[0x64, 0x0F, 0xC4, 0xCA, 0x03],
        &[0x65, 0x0F, 0xC4, 0xCA, 0x03],
        &[0x67, 0x66, 0x0F, 0xC4, 0xCA, 0x07],
        &[0x64, 0x66, 0x0F, 0x3A, 0x20, 0xCA, 0x0F],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x20, 0xCA, 0x0F],
        &[0xF2, 0x66, 0x0F, 0x3A, 0x22, 0xCA, 0x03],
        &[0x48, 0x66, 0x0F, 0x3A, 0x22, 0xCA, 0x01],
        &[0x66, 0x48, 0x49, 0x0F, 0x3A, 0x22, 0xCA, 0x01],
        &[0x66, 0x48, 0x67, 0x0F, 0x3A, 0x20, 0xCA, 0x0F],
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x20, 0xCA, 0x0F],
        &[0x0F, 0x3A, 0x20, 0xCA, 0x0F],
        &[0x66, 0x0F, 0x3A, 0x1F, 0xCA, 0x00],
        &[0x66, 0x0F, 0x3A, 0x23, 0xCA, 0x00],
        &[0x66, 0x0F, 0xC4, 0x0A, 0x07],
        &[0x0F, 0xC4, 0x0A, 0x03],
        &[0x66, 0x0F, 0x3A, 0x20, 0x0A, 0x0F],
        &[0x66, 0x0F, 0xC4, 0xCA],
        &[0x66, 0x0F, 0x3A, 0x20, 0xCA],
        &[0x66, 0x0F, 0xC4, 0xCA, 0x07, 0x00],
        &[0x66, 0x0F, 0x3A, 0x20, 0xCA, 0x0F, 0x00],
        &[0xC5, 0xF9, 0xC4, 0xCA, 0x07],
        &[0x62, 0xF3, 0x7D, 0x08, 0x20, 0xCA, 0x0F],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_scalar_insert_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn state_backed_rewrite_preserves_kind_destination_lane_and_rex_w() {
    let mut rewritten = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let original = instruction.legacy_register_scalar_insert_replay().unwrap();
                let actual = instruction
                    .legacy_scalar_insert_with_source_rax()
                    .unwrap()
                    .legacy_register_scalar_insert_replay()
                    .unwrap();
                assert_eq!(actual.kind, original.kind, "{bytes:02X?}");
                assert_eq!(actual.destination, original.destination, "{bytes:02X?}");
                assert_eq!(actual.source, 0, "{bytes:02X?}");
                assert_eq!(actual.lane, original.lane, "{bytes:02X?}");
                rewritten += 1;
            }
        }
    }
    assert_eq!(rewritten, (3 * 17 + 9 + 8) * 64);
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
        X86InstructionBytes::new(bytes).expect("legacy scalar-insert provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8], family: Family) {
    let expected_end = if family.mmx() {
        2
    } else {
        usize::from(4 * family.lanes() + 2)
    };
    assert_eq!(function.blocks[0].ops.len(), expected_end, "{bytes:02X?}");
    let start = usize::from(family.mmx());
    for spans in [
        x86_legacy_scalar_insert_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&start).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, expected_end, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_scalar_insert_replay_spans(
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
fn exact_graph_validator_covers_all_13056_rex_register_and_level_images() {
    let mut validated = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                for level in LEVELS {
                    assert_span(&function(&bytes, level), &bytes, family);
                    validated += 1;
                }
            }
        }
    }
    assert_eq!(validated, (3 * 17 + 9 + 8) * 64 * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_every_masked_immediate_at_o0_o1_o2() {
    let mut validated = 0usize;
    for family in Family::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let rex = if family == Family::PinsQ {
                Some(0x48)
            } else {
                Some(0x40)
            };
            let bytes = encoding(family, rex, 0xCA, immediate);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes, family);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, Family::ALL.len() * 256 * LEVELS.len());
}

fn mutation_count(kind: &OpKind) -> usize {
    match kind {
        OpKind::Mov { .. } | OpKind::VMov { .. } => 3,
        OpKind::VExtractLane { .. } | OpKind::VInsertLane { .. } => 5,
        OpKind::VBroadcast { .. } => 4,
        _ => panic!("unexpected scalar-insert graph operation: {kind:?}"),
    }
}

fn mutate(kind: &mut OpKind, mutation: usize) {
    let x0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    match kind {
        OpKind::Mov { dst, src, width } => match mutation {
            0 => *dst = x0,
            1 => *src = SrcOperand::Imm(1),
            2 => *width = OpWidth::W32,
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
            3 => *elem = VecElementType::F64,
            4 => *sign = SignExtend::Sign,
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
            2 => *elem = VecElementType::F64,
            3 => *lanes = 3,
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
            4 => *elem = VecElementType::F64,
            _ => unreachable!(),
        },
        OpKind::VMov { dst, src, width } => match mutation {
            0 => *dst = x0,
            1 => *src = x0,
            2 => *width = VecWidth::V256,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[test]
fn graph_validator_rejects_every_operation_field_hint_and_virtual_escape() {
    for family in [
        Family::PinsB,
        Family::PinsD,
        Family::PinsQ,
        Family::PinsWMap1Xmm,
    ] {
        let rex = if family == Family::PinsQ {
            Some(0x4D)
        } else {
            Some(0x45)
        };
        let bytes = encoding(family, rex, 0xCB, 0xA5);
        for level in LEVELS {
            let baseline = function(&bytes, level);
            for operation_index in 0..baseline.blocks[0].ops.len() {
                let mut hinted = baseline.clone();
                hinted.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
                assert_rejected(
                    &hinted,
                    &format!("{family:?} {level:?} hint op {operation_index}"),
                );
                for mutation in 0..mutation_count(&baseline.blocks[0].ops[operation_index].kind) {
                    let mut malformed = baseline.clone();
                    mutate(&mut malformed.blocks[0].ops[operation_index].kind, mutation);
                    assert_rejected(
                        &malformed,
                        &format!("{family:?} {level:?} op {operation_index} mutation {mutation}"),
                    );
                }
            }
        }
    }

    let bytes = encoding(Family::PinsB, Some(0x45), 0xCB, 0xA5);
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
            OpKind::VMov {
                dst: VReg::Virtual(VirtualId(0xF000u32.wrapping_add(ordinal as u32))),
                src: temporary,
                width: VecWidth::V128,
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

    let mmx_bytes = encoding(Family::PinsWMap1Mmx, Some(0x4D), 0xFB, 0xA5);
    let mmx = function(&mmx_bytes, OptLevel::O0);
    for operation_index in 0..mmx.blocks[0].ops.len() {
        let mut wrong_kind = mmx.clone();
        wrong_kind.blocks[0].ops[operation_index].kind = OpKind::Nop;
        assert_rejected(&wrong_kind, &format!("MMX op {operation_index}"));

        let mut wrong_hint = mmx.clone();
        wrong_hint.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
        assert_rejected(&wrong_hint, &format!("MMX hint {operation_index}"));
    }
}

#[test]
fn graph_validator_rejects_missing_mismatched_memory_and_reserved_provenance() {
    for (index, family) in Family::ALL.into_iter().enumerate() {
        let rex = if family == Family::PinsQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        let bytes = encoding(family, rex, 0xCA, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{family:?} missing"));

        let mismatch_family = Family::ALL[(index + 1) % Family::ALL.len()];
        let mismatch_rex = if mismatch_family == Family::PinsQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        for (label, metadata) in [
            (
                "family",
                encoding(mismatch_family, mismatch_rex, 0xCA, 0xA5),
            ),
            ("destination/source", encoding(family, rex, 0xD3, 0xA5)),
            ("lane", encoding(family, rex, 0xCA, 0xA6)),
            ("memory", encoding(family, rex, 0x0A, 0xA5)),
            ("reserved prefix", {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(family, None, 0xCA, 0xA5));
                reserved
            }),
        ] {
            let mut malformed = baseline.clone();
            malformed.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&metadata).unwrap(),
            );
            assert_rejected(&malformed, &format!("{family:?} {label}"));
        }
    }
}
