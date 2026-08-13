//! Exact legacy SSE2/SSE3 lane-shuffle replay classification and graph validation.

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

const PC: u64 = 0xE7E0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    MovDDup,
    MovShDup,
    MovSlDup,
    PshufD,
    PshufHighW,
    PshufLowW,
}

impl Family {
    const ALL: [Self; 6] = [
        Self::MovDDup,
        Self::MovShDup,
        Self::MovSlDup,
        Self::PshufD,
        Self::PshufHighW,
        Self::PshufLowW,
    ];

    const SHUFFLES: [Self; 3] = [Self::PshufD, Self::PshufHighW, Self::PshufLowW];

    fn kind(self) -> X86LegacyLaneShuffleKind {
        match self {
            Self::MovDDup => X86LegacyLaneShuffleKind::MovDDup,
            Self::MovShDup => X86LegacyLaneShuffleKind::MovShDup,
            Self::MovSlDup => X86LegacyLaneShuffleKind::MovSlDup,
            Self::PshufD => X86LegacyLaneShuffleKind::PshufD,
            Self::PshufHighW => X86LegacyLaneShuffleKind::PshufHighW,
            Self::PshufLowW => X86LegacyLaneShuffleKind::PshufLowW,
        }
    }

    fn prefix(self) -> u8 {
        match self {
            Self::MovDDup | Self::PshufLowW => 0xF2,
            Self::MovShDup | Self::MovSlDup | Self::PshufHighW => 0xF3,
            Self::PshufD => 0x66,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::MovShDup => 0x16,
            Self::MovDDup | Self::MovSlDup => 0x12,
            Self::PshufD | Self::PshufHighW | Self::PshufLowW => 0x70,
        }
    }

    fn has_immediate(self) -> bool {
        Self::SHUFFLES.contains(&self)
    }

    fn lanes(self) -> usize {
        match self {
            Self::MovDDup => 2,
            Self::MovShDup | Self::MovSlDup | Self::PshufD => 4,
            Self::PshufHighW | Self::PshufLowW => 8,
        }
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![family.prefix()];
    bytes.extend(rex);
    bytes.extend([0x0F, family.opcode(), modrm]);
    if family.has_immediate() {
        bytes.push(immediate);
    }
    bytes
}

fn expected(
    family: Family,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
) -> X86LegacyLaneShuffleReplay {
    let rex = rex.unwrap_or(0);
    X86LegacyLaneShuffleReplay {
        kind: family.kind(),
        destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
        source: (modrm & 7) | ((rex & 0x01) << 3),
        immediate: family.has_immediate().then_some(immediate),
    }
}

#[test]
fn classifier_covers_all_838848_canonical_rex_register_and_immediate_images() {
    let mut classified = 0usize;
    for family in Family::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                if family.has_immediate() {
                    for immediate in u8::MIN..=u8::MAX {
                        let bytes = encoding(family, rex, modrm, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .legacy_register_lane_shuffle_replay(),
                            Some(expected(family, rex, modrm, immediate)),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                } else {
                    let bytes = encoding(family, rex, modrm, 0);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_lane_shuffle_replay(),
                        Some(expected(family, rex, modrm, 0)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 3 * 17 * 64 + 3 * 17 * 64 * 256);
}

#[test]
fn classifier_matches_llvm23_anchors_and_rejects_noncanonical_frontiers() {
    // Independently assembled by LLVM 23.0.0git.
    let anchors: &[(Family, &[u8])] = &[
        (Family::MovDDup, &[0xF2, 0x45, 0x0F, 0x12, 0xCA]),
        (Family::MovShDup, &[0xF3, 0x45, 0x0F, 0x16, 0xCA]),
        (Family::MovSlDup, &[0xF3, 0x45, 0x0F, 0x12, 0xCA]),
        (Family::PshufD, &[0x66, 0x45, 0x0F, 0x70, 0xCA, 0x1B]),
        (Family::PshufHighW, &[0xF3, 0x45, 0x0F, 0x70, 0xCA, 0x1B]),
        (Family::PshufLowW, &[0xF2, 0x45, 0x0F, 0x70, 0xCA, 0x1B]),
    ];
    for (family, bytes) in anchors {
        let replay = X86InstructionBytes::new(bytes)
            .unwrap()
            .legacy_register_lane_shuffle_replay()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(replay.kind, family.kind(), "{bytes:02X?}");
        assert_eq!(replay.destination, 9, "{bytes:02X?}");
        assert_eq!(replay.source, 10, "{bytes:02X?}");
    }

    // Intel SDM Order No. 325383-092US, Vol. 2A, classifies address-size and
    // segment prefixes on register-only forms as reserved/unpredictable. Exact
    // replay also rejects all other duplicate, reordered, and noncanonical
    // prefix images.
    let invalid: &[&[u8]] = &[
        &[0x67, 0xF2, 0x0F, 0x12, 0xCA],
        &[0x64, 0xF3, 0x0F, 0x16, 0xCA],
        &[0x65, 0x66, 0x0F, 0x70, 0xCA, 0x1B],
        &[0xF0, 0xF2, 0x0F, 0x70, 0xCA, 0x1B],
        &[0x66, 0xF3, 0x0F, 0x12, 0xCA],
        &[0x48, 0xF2, 0x0F, 0x12, 0xCA],
        &[0xF2, 0x48, 0x49, 0x0F, 0x12, 0xCA],
        &[0xF3, 0x48, 0x67, 0x0F, 0x16, 0xCA],
        &[0x66, 0xD5, 0x00, 0x0F, 0x70, 0xCA, 0x1B],
        &[0x0F, 0x12, 0xCA],
        &[0x66, 0x0F, 0x12, 0xCA],
        &[0xF2, 0x0F, 0x16, 0xCA],
        &[0xF3, 0x0F, 0x17, 0xCA],
        &[0x66, 0x0F, 0x71, 0xCA, 0x1B],
        &[0xF2, 0x0F, 0x12, 0x0A],
        &[0xF3, 0x0F, 0x16, 0x0A],
        &[0x66, 0x0F, 0x70, 0x0A, 0x1B],
        &[0xF2, 0x0F, 0x12],
        &[0xF3, 0x0F, 0x16, 0xCA, 0x00],
        &[0x66, 0x0F, 0x70, 0xCA],
        &[0xF2, 0x0F, 0x70, 0xCA, 0x1B, 0x00],
        &[0xC5, 0xFA, 0x12, 0xCA],
        &[0x62, 0xF1, 0x7E, 0x08, 0x16, 0xCA],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_lane_shuffle_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy lane-shuffle provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8], family: Family) {
    let expected_end = 4 * family.lanes() + 3;
    assert_eq!(function.blocks[0].ops.len(), expected_end, "{bytes:02X?}");
    for spans in [
        x86_legacy_lane_shuffle_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
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
        x86_legacy_lane_shuffle_replay_spans(&function.blocks[0], &function.x86_instruction_bytes,)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_19584_rex_register_and_level_images() {
    let mut validated = 0usize;
    for family in Family::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                for level in LEVELS {
                    assert_span(&function(&bytes, level), &bytes, family);
                    validated += 1;
                }
            }
        }
    }
    assert_eq!(validated, Family::ALL.len() * 17 * 64 * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_every_shuffle_immediate_at_o0_o1_o2() {
    let mut validated = 0usize;
    for family in Family::SHUFFLES {
        for immediate in u8::MIN..=u8::MAX {
            let bytes = encoding(family, Some(0x45), 0xCA, immediate);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes, family);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, Family::SHUFFLES.len() * 256 * LEVELS.len());
}

fn different_element(element: VecElementType) -> VecElementType {
    if element == VecElementType::F64 {
        VecElementType::I8
    } else {
        VecElementType::F64
    }
}

fn mutation_count(kind: &OpKind) -> usize {
    match kind {
        OpKind::Mov { .. } => 3,
        OpKind::VBroadcast { .. } => 4,
        OpKind::VInsertLane { .. } | OpKind::VExtractLane { .. } => 5,
        OpKind::VShuffle { .. } => 6,
        _ => panic!("unexpected legacy lane-shuffle graph operation: {kind:?}"),
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
            2 => *src2 = Some(x0),
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
    for family in Family::ALL {
        let bytes = encoding(family, Some(0x45), 0xCA, 0xA5);
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

    let bytes = encoding(Family::PshufHighW, Some(0x45), 0xCA, 0xA5);
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
                elem: VecElementType::I16,
                lanes: 8,
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
    for (index, family) in Family::ALL.into_iter().enumerate() {
        let bytes = encoding(family, Some(0x45), 0xCA, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{family:?} missing"));

        let mismatch_family = Family::ALL[(index + 1) % Family::ALL.len()];
        let mut metadata = vec![
            encoding(mismatch_family, Some(0x45), 0xCA, 0xA5),
            encoding(family, Some(0x45), 0xD3, 0xA5),
            encoding(family, Some(0x45), 0x0A, 0xA5),
            {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(family, None, 0xCA, 0xA5));
                reserved
            },
        ];
        if family.has_immediate() {
            metadata.push(encoding(family, Some(0x45), 0xCA, 0xA6));
        }
        for bytes in metadata {
            let mut malformed = baseline.clone();
            malformed
                .x86_instruction_bytes
                .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
            assert_rejected(&malformed, &format!("{family:?} {bytes:02X?}"));
        }
    }
}
