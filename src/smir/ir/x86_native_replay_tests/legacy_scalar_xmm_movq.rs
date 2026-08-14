//! Exact classifier and semantic-graph validation for register-only legacy
//! scalar-XMM MOVQ replay.

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, OpWidth, SignExtend, SourceArch, SrcOperand, VecElementType, X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xD67E;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    RmDestination,
    RegDestination,
}

impl Direction {
    const ALL: [Self; 2] = [Self::RmDestination, Self::RegDestination];

    fn prefix(self) -> u8 {
        match self {
            Self::RmDestination => 0x66,
            Self::RegDestination => 0xF3,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::RmDestination => 0xD6,
            Self::RegDestination => 0x7E,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MovqCase {
    direction: Direction,
    rex: Option<u8>,
    modrm: u8,
}

impl MovqCase {
    fn bytes(self) -> Vec<u8> {
        assert!(self.rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
        let mut bytes = vec![self.direction.prefix()];
        bytes.extend(self.rex);
        bytes.extend([0x0F, self.direction.opcode(), self.modrm]);
        bytes
    }

    fn expected(self) -> X86LegacyScalarXmmMovqReplay {
        let rex = self.rex.unwrap_or(0);
        let reg = ((rex & 0x04) << 1) | ((self.modrm >> 3) & 7);
        let rm = ((rex & 0x01) << 3) | (self.modrm & 7);
        let (destination, source) = match self.direction {
            Direction::RmDestination => (rm, reg),
            Direction::RegDestination => (reg, rm),
        };
        X86LegacyScalarXmmMovqReplay {
            destination,
            source,
        }
    }
}

fn canonical_cases() -> impl Iterator<Item = MovqCase> {
    Direction::ALL.into_iter().flat_map(|direction| {
        [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .flat_map(move |rex| {
                (0xC0..=0xFF).map(move |modrm| MovqCase {
                    direction,
                    rex,
                    modrm,
                })
            })
    })
}

#[test]
fn classifier_covers_all_2176_canonical_rex_register_images() {
    let mut classified = 0usize;
    for case in canonical_cases() {
        let bytes = case.bytes();
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_scalar_xmm_movq_replay(),
            Some(case.expected()),
            "{case:?} {bytes:02X?}"
        );
        classified += 1;
    }
    assert_eq!(classified, Direction::ALL.len() * 17 * 64);
}

#[test]
fn classifier_matches_llvm23_and_intel_opcode_direction_anchors() {
    // The F3 encodings were independently assembled by LLVM 23.0.0git. The
    // 66 encodings exercise Intel SDM's alternate ModR/M field direction.
    for (bytes, destination, source) in [
        (&[0xF3, 0x45, 0x0F, 0x7E, 0xD1][..], 10, 9),
        (&[0xF3, 0x45, 0x0F, 0x7E, 0xCA][..], 9, 10),
        (&[0x66, 0x45, 0x0F, 0xD6, 0xD1][..], 9, 10),
        (&[0x66, 0x45, 0x0F, 0xD6, 0xCA][..], 10, 9),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_scalar_xmm_movq_replay(),
            Some(X86LegacyScalarXmmMovqReplay {
                destination,
                source,
            }),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_exhausts_opcode_modrm_and_noncanonical_prefix_frontiers() {
    for direction in Direction::ALL {
        for opcode in u8::MIN..=u8::MAX {
            let bytes = [direction.prefix(), 0x0F, opcode, 0xCA];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_scalar_xmm_movq_replay()
                    .is_some(),
                opcode == direction.opcode(),
                "{bytes:02X?}"
            );
        }
        for modrm in u8::MIN..=u8::MAX {
            let bytes = [direction.prefix(), 0x0F, direction.opcode(), modrm];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_scalar_xmm_movq_replay()
                    .is_some(),
                modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0xD6, 0xCA],
        &[0xF2, 0x0F, 0xD6, 0xCA],
        &[0xF3, 0x0F, 0xD6, 0xCA],
        &[0x66, 0x0F, 0x7E, 0xCA],
        &[0x66, 0x66, 0x0F, 0xD6, 0xCA],
        &[0xF3, 0xF3, 0x0F, 0x7E, 0xCA],
        &[0x45, 0x66, 0x0F, 0xD6, 0xCA],
        &[0x66, 0x45, 0x46, 0x0F, 0xD6, 0xCA],
        &[0x66, 0x45, 0x67, 0x0F, 0xD6, 0xCA],
        &[0x67, 0x66, 0x45, 0x0F, 0xD6, 0xCA],
        &[0x66, 0xD5, 0x00, 0x0F, 0xD6, 0xCA],
        &[0x66, 0x0F, 0xD6, 0x0A],
        &[0xF3, 0x4F, 0x0F, 0x7E, 0x8A],
        &[0x66, 0x0F, 0xD6],
        &[0xF3, 0x0F, 0x7E, 0xCA, 0x00],
        &[0xC5, 0xFA, 0x7E, 0xCA],
        &[0x62, 0xF1, 0xFE, 0x08, 0x7E, 0xCA],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_scalar_xmm_movq_replay(),
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
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy scalar-XMM MOVQ provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_span(function: &SmirFunction, expected_bytes: &[u8], label: &str) {
    assert_eq!(function.blocks[0].ops.len(), 8, "{label}");
    for spans in [
        x86_legacy_scalar_xmm_movq_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        ),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{label}"));
        assert_eq!(span.end, 8, "{label}");
        assert_eq!(span.instruction.as_slice(), expected_bytes, "{label}");
        assert!(!span.needs_avx512vl, "{label}");
        assert!(!span.needs_avx512dq, "{label}");
        assert!(!span.needs_avx512fp16, "{label}");
        assert!(!span.preserve_mxcsr_de, "{label}");
    }
}

#[test]
fn lifted_o0_o1_o2_graphs_admit_all_6528_rex_register_images() {
    let mut admitted = 0usize;
    for case in canonical_cases() {
        let bytes = case.bytes();
        for level in LEVELS {
            let function = function(&bytes, level);
            assert_exact_span(
                &function,
                &bytes,
                &format!("{level:?} {case:?} {bytes:02X?}"),
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, Direction::ALL.len() * 17 * 64 * LEVELS.len());
}

#[test]
fn address_and_segment_prefixes_canonicalize_without_changing_semantics() {
    let source = [0x67, 0x65, 0x66, 0x4F, 0x0F, 0xD6, 0xD1];
    let canonical = [0x66, 0x4F, 0x0F, 0xD6, 0xD1];
    assert!(
        X86InstructionBytes::new(&source)
            .unwrap()
            .legacy_scalar_xmm_movq_replay()
            .is_none()
    );
    for level in LEVELS {
        let function = function(&source, level);
        assert_exact_span(&function, &canonical, &format!("{level:?} {source:02X?}"));
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_scalar_xmm_movq_replay_spans(
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
fn semantic_graph_validator_rejects_fields_hints_order_alias_and_virtual_escape() {
    let case = MovqCase {
        direction: Direction::RegDestination,
        rex: Some(0x45),
        modrm: 0xD1,
    };
    let bytes = case.bytes();
    let baseline = function(&bytes, OptLevel::O0);
    assert_exact_span(&baseline, &bytes, "baseline");

    for index in 0..8 {
        let mut malformed = baseline.clone();
        malformed.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
        assert_rejected(&malformed, &format!("hint on operation {index}"));
    }

    let mut mutations: Vec<(&str, SmirFunction)> = Vec::new();
    let mut wrong_source = baseline.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut wrong_source.blocks[0].ops[0].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    mutations.push(("source register", wrong_source));

    let mut wrong_source_lane = baseline.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_source_lane.blocks[0].ops[0].kind {
        *lane = 1;
    }
    mutations.push(("source lane", wrong_source_lane));

    let mut wrong_source_sign = baseline.clone();
    if let OpKind::VExtractLane { sign, .. } = &mut wrong_source_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    mutations.push(("source sign extension", wrong_source_sign));

    let mut wrong_zero = baseline.clone();
    if let OpKind::Mov { src, .. } = &mut wrong_zero.blocks[0].ops[1].kind {
        *src = SrcOperand::Imm(1);
    }
    mutations.push(("zero immediate", wrong_zero));

    let mut wrong_zero_width = baseline.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[1].kind {
        *width = OpWidth::W32;
    }
    mutations.push(("zero width", wrong_zero_width));

    let mut wrong_lanes = baseline.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_lanes.blocks[0].ops[2].kind {
        *lanes = 4;
    }
    mutations.push(("broadcast lanes", wrong_lanes));

    let mut wrong_element = baseline.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut wrong_element.blocks[0].ops[3].kind {
        *elem = VecElementType::I32;
    }
    mutations.push(("insert element", wrong_element));

    let mut wrong_result_lane = baseline.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_result_lane.blocks[0].ops[5].kind {
        *lane = 0;
    }
    mutations.push(("high extract lane", wrong_result_lane));

    let mut wrong_destination = baseline.clone();
    if let OpKind::VInsertLane { dst, vec, .. } = &mut wrong_destination.blocks[0].ops[6].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
        *vec = *dst;
    }
    mutations.push(("destination register", wrong_destination));

    let mut wrong_destination_lane = baseline.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_destination_lane.blocks[0].ops[7].kind {
        *lane = 0;
    }
    mutations.push(("destination lane", wrong_destination_lane));

    let mut reordered = baseline.clone();
    reordered.blocks[0].ops.swap(4, 5);
    mutations.push(("operation order", reordered));

    let mut extra = baseline.clone();
    extra.blocks[0]
        .ops
        .push(SmirOp::new(OpId(99), PC, OpKind::Nop));
    mutations.push(("extra same-PC operation", extra));

    let scalar = match baseline.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut aliased_virtuals = baseline.clone();
    let zero = match aliased_virtuals.blocks[0].ops[1].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    if let OpKind::VExtractLane { dst, .. } = &mut aliased_virtuals.blocks[0].ops[0].kind {
        *dst = zero;
    }
    if let OpKind::VInsertLane { scalar, .. } = &mut aliased_virtuals.blocks[0].ops[3].kind {
        *scalar = zero;
    }
    mutations.push(("aliased virtual registers", aliased_virtuals));

    let mut escaped_use = baseline.clone();
    escaped_use.blocks[0].set_terminator(Terminator::Return {
        values: vec![scalar],
    });
    mutations.push(("virtual terminator escape", escaped_use));

    let mut duplicate_definition = baseline.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(100),
        PC + 1,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("duplicate virtual definition", duplicate_definition));

    for (label, malformed) in mutations {
        assert_rejected(&malformed, label);
    }

    let mut missing_provenance = baseline.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected(&missing_provenance, "missing provenance");

    let mut memory_provenance = baseline;
    let mut memory = bytes;
    *memory.last_mut().unwrap() &= 0x3F;
    memory_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&memory).unwrap());
    assert_rejected(&memory_provenance, "memory provenance");
}
