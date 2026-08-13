//! Exact legacy SSE2 packed-shift replay classification and graph validation.

use std::collections::HashSet;

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    ArchReg, FunctionId, ShiftOp, SignExtend, SourceArch, VecElementType, VecWidth, VirtualId,
    X86Reg,
};
use crate::smir::ir::{SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x5A1F_7002;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    opcode: u8,
    group: Option<u8>,
    elem: VecElementType,
    shift: ShiftOp,
    byte_lane: bool,
}

impl Shape {
    const fn immediate(
        opcode: u8,
        group: u8,
        elem: VecElementType,
        shift: ShiftOp,
        byte_lane: bool,
    ) -> Self {
        Self {
            opcode,
            group: Some(group),
            elem,
            shift,
            byte_lane,
        }
    }

    const fn register(opcode: u8, elem: VecElementType, shift: ShiftOp) -> Self {
        Self {
            opcode,
            group: None,
            elem,
            shift,
            byte_lane: false,
        }
    }

    const fn immediate_count(self) -> bool {
        self.group.is_some()
    }
}

const SHAPES: [Shape; 18] = [
    Shape::immediate(0x71, 2, VecElementType::I16, ShiftOp::Lsr, false),
    Shape::immediate(0x71, 4, VecElementType::I16, ShiftOp::Asr, false),
    Shape::immediate(0x71, 6, VecElementType::I16, ShiftOp::Lsl, false),
    Shape::immediate(0x72, 2, VecElementType::I32, ShiftOp::Lsr, false),
    Shape::immediate(0x72, 4, VecElementType::I32, ShiftOp::Asr, false),
    Shape::immediate(0x72, 6, VecElementType::I32, ShiftOp::Lsl, false),
    Shape::immediate(0x73, 2, VecElementType::I64, ShiftOp::Lsr, false),
    Shape::immediate(0x73, 3, VecElementType::I8, ShiftOp::Lsr, true),
    Shape::immediate(0x73, 6, VecElementType::I64, ShiftOp::Lsl, false),
    Shape::immediate(0x73, 7, VecElementType::I8, ShiftOp::Lsl, true),
    Shape::register(0xD1, VecElementType::I16, ShiftOp::Lsr),
    Shape::register(0xD2, VecElementType::I32, ShiftOp::Lsr),
    Shape::register(0xD3, VecElementType::I64, ShiftOp::Lsr),
    Shape::register(0xE1, VecElementType::I16, ShiftOp::Asr),
    Shape::register(0xE2, VecElementType::I32, ShiftOp::Asr),
    Shape::register(0xF1, VecElementType::I16, ShiftOp::Lsl),
    Shape::register(0xF2, VecElementType::I32, ShiftOp::Lsl),
    Shape::register(0xF3, VecElementType::I64, ShiftOp::Lsl),
];

fn encoding(shape: Shape, rex: Option<u8>, operand: u8, amount: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, shape.opcode]);
    if let Some(group) = shape.group {
        assert!(operand < 8);
        bytes.extend([0xC0 | (group << 3) | operand, amount]);
    } else {
        assert!((0xC0..=0xFF).contains(&operand));
        bytes.push(operand);
    }
    bytes
}

fn expected(shape: Shape, rex: Option<u8>, operand: u8, amount: u8) -> X86LegacyPackedShiftReplay {
    let rex = rex.unwrap_or(0);
    let (destination, count) = if shape.immediate_count() {
        (
            (operand & 7) | ((rex & 1) << 3),
            X86LegacyPackedShiftCount::Immediate {
                amount,
                byte_lane: shape.byte_lane,
            },
        )
    } else {
        (
            ((operand >> 3) & 7) | ((rex & 4) << 1),
            X86LegacyPackedShiftCount::Register {
                source: (operand & 7) | ((rex & 1) << 3),
            },
        )
    };
    X86LegacyPackedShiftReplay {
        destination,
        elem: shape.elem,
        shift: shape.shift,
        count,
    }
}

#[test]
fn classifier_covers_all_10064_shape_rex_and_register_images() {
    let mut classified = 0usize;
    for shape in SHAPES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            let operands: Box<dyn Iterator<Item = u8>> = if shape.immediate_count() {
                Box::new(0..8)
            } else {
                Box::new(0xC0..=0xFF)
            };
            for operand in operands {
                let bytes = encoding(shape, rex, operand, 0xA5);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_packed_shift_replay(),
                    Some(expected(shape, rex, operand, 0xA5)),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 10 * 17 * 8 + 8 * 17 * 64);
}

#[test]
fn classifier_preserves_every_immediate_and_exhausts_opcode_group_frontiers() {
    for shape in SHAPES.into_iter().filter(|shape| shape.immediate_count()) {
        for amount in u8::MIN..=u8::MAX {
            let bytes = encoding(shape, Some(0x4F), 2, amount);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_packed_shift_replay(),
                Some(expected(shape, Some(0x4F), 2, amount)),
                "{bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        for group in 0..8 {
            let bytes = [0x66, 0x4F, 0x0F, opcode, 0xC0 | (group << 3) | 2, 0xA5];
            let expected_shape = SHAPES
                .into_iter()
                .find(|shape| shape.opcode == opcode && shape.group == Some(group));
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_register_packed_shift_replay(),
                expected_shape.map(|shape| expected(shape, Some(0x4F), 2, 0xA5)),
                "{bytes:02X?}"
            );
        }

        let bytes = [0x66, 0x4F, 0x0F, opcode, 0xCA];
        let expected_shape = SHAPES
            .into_iter()
            .find(|shape| shape.opcode == opcode && !shape.immediate_count());
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_packed_shift_replay(),
            expected_shape.map(|shape| expected(shape, Some(0x4F), 0xCA, 0)),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_matches_llvm_rex_roles_and_rejects_noncanonical_frontiers() {
    let immediate = SHAPES[3];
    let register = SHAPES[16];
    for rex in 0x40..=0x4F {
        assert_eq!(
            X86InstructionBytes::new(&encoding(immediate, Some(rex), 0, 1))
                .unwrap()
                .legacy_register_packed_shift_replay(),
            Some(expected(immediate, Some(rex), 0, 1)),
            "immediate REX {rex:02X}"
        );
        assert_eq!(
            X86InstructionBytes::new(&encoding(register, Some(rex), 0xCA, 0))
                .unwrap()
                .legacy_register_packed_shift_replay(),
            Some(expected(register, Some(rex), 0xCA, 0)),
            "shared-count REX {rex:02X}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x72, 0xF0, 1],                   // prefix-free MMX form
        &[0xF2, 0x0F, 0x72, 0xF0, 1],             // wrong mandatory prefix
        &[0xF0, 0x66, 0x0F, 0x72, 0xF0, 1],       // LOCK
        &[0x67, 0x66, 0x0F, 0x72, 0xF0, 1],       // address-size prefix
        &[0x64, 0x66, 0x0F, 0x72, 0xF0, 1],       // segment prefix
        &[0x48, 0x66, 0x0F, 0x72, 0xF0, 1],       // REX not final
        &[0x66, 0x48, 0x49, 0x0F, 0x72, 0xF0, 1], // duplicate REX
        &[0x66, 0xD5, 0, 0x0F, 0x72, 0xF0, 1],    // REX2
        &[0x66, 0x0F, 0x71, 0xD8, 1],             // reserved immediate group /3
        &[0x66, 0x0F, 0x73, 0xE0, 1],             // reserved immediate group /4
        &[0x66, 0x0F, 0x72, 0x30, 1],             // immediate memory ModR/M
        &[0x66, 0x0F, 0xF2, 0x0A],                // shared-count memory source
        &[0x66, 0x0F, 0x72, 0xF0],                // missing immediate
        &[0x66, 0x0F, 0xF2],                      // missing ModR/M
        &[0x66, 0x0F, 0x72, 0xF0, 1, 0],          // trailing byte
        &[0x66, 0x0F, 0xF2, 0xCA, 0],             // trailing byte
        &[0xC5, 0xF1, 0x72, 0xF0, 1],             // VEX
        &[0x62, 0xF1, 0x7D, 0x08, 0x72, 0xF0, 1], // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_packed_shift_replay(),
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
        X86InstructionBytes::new(bytes).expect("legacy packed-shift provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_span(function: &SmirFunction, bytes: &[u8], shape: Shape) {
    let expected_len = 1 + 2 * VecWidth::V128.lanes(shape.elem) as usize;
    assert_eq!(function.blocks[0].ops.len(), expected_len, "{bytes:02X?}");
    for spans in [
        x86_legacy_packed_shift_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, expected_len, "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_rejected(function: &SmirFunction, label: &str) {
    assert!(
        x86_legacy_packed_shift_replay_spans(&function.blocks[0], &function.x86_instruction_bytes,)
            .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn exact_graph_validator_covers_all_30192_shape_rex_register_and_level_cases() {
    let mut validated = 0usize;
    for shape in SHAPES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            let operands: Box<dyn Iterator<Item = u8>> = if shape.immediate_count() {
                Box::new(0..8)
            } else {
                Box::new(0xC0..=0xFF)
            };
            for operand in operands {
                let bytes = encoding(shape, rex, operand, 0xA5);
                for level in LEVELS {
                    assert_span(&function(&bytes, level), &bytes, shape);
                    validated += 1;
                }
            }
        }
    }
    assert_eq!(validated, (10 * 17 * 8 + 8 * 17 * 64) * LEVELS.len());
}

#[test]
fn exact_graph_validator_covers_all_immediates_at_o0_o1_o2() {
    let mut validated = 0usize;
    for shape in SHAPES.into_iter().filter(|shape| shape.immediate_count()) {
        for amount in u8::MIN..=u8::MAX {
            let bytes = encoding(shape, Some(0x45), 2, amount);
            for level in LEVELS {
                assert_span(&function(&bytes, level), &bytes, shape);
                validated += 1;
            }
        }
    }
    assert_eq!(validated, 10 * 256 * LEVELS.len());
}

fn mutation_count(kind: &OpKind) -> usize {
    match kind {
        OpKind::X86PackedShiftImm { .. } => 7,
        OpKind::X86PackedShift { .. } => 6,
        OpKind::VExtractLane { .. } | OpKind::VInsertLane { .. } => 5,
        _ => panic!("unexpected packed-shift graph operation: {kind:?}"),
    }
}

fn mutate(kind: &mut OpKind, mutation: usize) {
    let x31 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(31)));
    match kind {
        OpKind::X86PackedShiftImm {
            dst,
            src,
            width,
            elem,
            shift,
            amount,
            byte_lane,
        } => match mutation {
            0 => *dst = x31,
            1 => *src = x31,
            2 => *width = VecWidth::V64,
            3 => *elem = VecElementType::F32,
            4 => *shift = ShiftOp::Ror,
            5 => *amount = amount.wrapping_add(1),
            6 => *byte_lane = !*byte_lane,
            _ => unreachable!(),
        },
        OpKind::X86PackedShift {
            dst,
            src,
            count,
            width,
            elem,
            shift,
        } => match mutation {
            0 => *dst = x31,
            1 => *src = x31,
            2 => *count = x31,
            3 => *width = VecWidth::V64,
            4 => *elem = VecElementType::F32,
            5 => *shift = ShiftOp::Ror,
            _ => unreachable!(),
        },
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } => match mutation {
            0 => *dst = x31,
            1 => *vec = x31,
            2 => *lane = lane.wrapping_add(1),
            3 => *elem = VecElementType::F32,
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
            0 => *dst = x31,
            1 => *vec = x31,
            2 => *scalar = x31,
            3 => *lane = lane.wrapping_add(1),
            4 => *elem = VecElementType::F32,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[test]
fn graph_validator_rejects_every_operation_field_hint_and_virtual_escape() {
    for shape in SHAPES {
        let operand = if shape.immediate_count() { 2 } else { 0xCA };
        let bytes = encoding(shape, Some(0x45), operand, 0xA5);
        for level in LEVELS {
            let baseline = function(&bytes, level);
            for operation_index in 0..baseline.blocks[0].ops.len() {
                let mut hinted = baseline.clone();
                hinted.blocks[0].ops[operation_index].x86_hint = Some(X86OpHint::RexByteReg);
                assert_rejected(
                    &hinted,
                    &format!("{shape:?} {level:?} hint op {operation_index}"),
                );

                for mutation in 0..mutation_count(&baseline.blocks[0].ops[operation_index].kind) {
                    let mut malformed = baseline.clone();
                    mutate(&mut malformed.blocks[0].ops[operation_index].kind, mutation);
                    assert_rejected(
                        &malformed,
                        &format!("{shape:?} {level:?} op {operation_index} mutation {mutation}"),
                    );
                }
            }
        }

        let baseline = function(&bytes, OptLevel::O0);
        let virtuals: Vec<_> = baseline.blocks[0]
            .ops
            .iter()
            .flat_map(|operation| operation.kind.dests())
            .filter(|register| matches!(register, VReg::Virtual(_)))
            .collect::<HashSet<_>>()
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
            assert_rejected(&escaped, &format!("{shape:?} escaping {temporary:?}"));

            let mut redefined = baseline.clone();
            redefined.blocks[0].push_op(SmirOp::new(
                OpId(0xE000u16.wrapping_add(ordinal as u16)),
                PC + 1,
                OpKind::VMov {
                    dst: temporary,
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    width: VecWidth::V128,
                },
            ));
            assert_rejected(&redefined, &format!("{shape:?} redefined {temporary:?}"));
        }
    }
}

#[test]
fn graph_validator_rejects_missing_mismatched_memory_and_extra_provenance() {
    for shape in SHAPES {
        let operand = if shape.immediate_count() { 2 } else { 0xCA };
        let bytes = encoding(shape, Some(0x45), operand, 0xA5);
        let baseline = function(&bytes, OptLevel::O0);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected(&missing, &format!("{shape:?} missing provenance"));

        let mut wrong_operand = baseline.clone();
        let replacement = if shape.immediate_count() { 3 } else { 0xD3 };
        let wrong_bytes = encoding(shape, Some(0x45), replacement, 0xA5);
        wrong_operand.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&wrong_bytes).unwrap(),
        );
        assert_rejected(&wrong_operand, &format!("{shape:?} wrong operands"));

        let mut extra = baseline;
        extra.blocks[0].push_op(SmirOp::new(OpId(0xD000), PC, OpKind::Nop));
        assert_rejected(&extra, &format!("{shape:?} extra same-PC operation"));
    }

    let memory = function(&[0x66, 0x0F, 0xF2, 0x0A], OptLevel::O2);
    assert_rejected(&memory, "shared-count memory source");
}
