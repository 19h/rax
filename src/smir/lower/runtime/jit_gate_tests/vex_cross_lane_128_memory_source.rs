//! Exact helper-backed VEX 128-bit cross-lane memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexCrossLane128MemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_cross_lane_128_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::{HashMap, HashSet};

mod semantics;

const PC: u64 = 0xC128;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrossLane {
    PermuteF128,
    InsertF128,
    InsertI128,
    PermuteI128,
}

impl CrossLane {
    const ALL: [Self; 4] = [
        Self::PermuteF128,
        Self::InsertF128,
        Self::InsertI128,
        Self::PermuteI128,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::PermuteF128 => 0x06,
            Self::InsertF128 => 0x18,
            Self::InsertI128 => 0x38,
            Self::PermuteI128 => 0x46,
        }
    }

    fn is_insert(self) -> bool {
        matches!(self, Self::InsertF128 | Self::InsertI128)
    }

    fn needs_avx2(self) -> bool {
        matches!(self, Self::InsertI128 | Self::PermuteI128)
    }

    fn source_width(self) -> VecWidth {
        if self.is_insert() {
            VecWidth::V128
        } else {
            VecWidth::V256
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrossLaneMemoryCase {
    operation: CrossLane,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
    clear_ignored_x: bool,
}

impl CrossLaneMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (((!self.source1) & 0x0F) << 3) | 0x05,
            self.operation.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
            self.immediate,
        ]
    }

    fn emitted_bytes(self) -> [u8; 6] {
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 3,
            (((!self.source1) & 0x0F) << 3) | 0x05,
            self.operation.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
            self.immediate,
        ]
    }
}

fn scanner_cases() -> Vec<CrossLaneMemoryCase> {
    let mut cases = Vec::with_capacity(512);
    for operation in CrossLane::ALL {
        for destination in 0..16u8 {
            for shape in 0..8u8 {
                let source1 = match shape {
                    0 => destination,
                    1 => destination.wrapping_add(1) & 15,
                    2 => 15,
                    3 => 0,
                    _ => destination.wrapping_add(shape.wrapping_mul(3)) & 15,
                };
                cases.push(CrossLaneMemoryCase {
                    operation,
                    destination,
                    source1,
                    base: if shape & 1 == 0 { 3 } else { 11 },
                    immediate: destination
                        .wrapping_mul(17)
                        .wrapping_add(source1.wrapping_mul(11))
                        .wrapping_add(shape.wrapping_mul(29)),
                    clear_ignored_x: shape & 2 != 0,
                });
            }
        }
    }
    cases
}

fn immediate_cases() -> Vec<CrossLaneMemoryCase> {
    let shapes = [(1, 2, 7), (9, 10, 11), (15, 15, 14), (0, 0, 7)];
    let mut cases = Vec::with_capacity(1_024);
    for operation in CrossLane::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let (destination, source1, base) = shapes[usize::from(immediate) % shapes.len()];
            cases.push(CrossLaneMemoryCase {
                operation,
                destination,
                source1,
                base,
                immediate,
                clear_ignored_x: immediate & 2 != 0,
            });
        }
    }
    cases
}

fn x86(register: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(register))
}

fn ymm(index: u8) -> VReg {
    x86(X86Reg::Ymm(index))
}

fn expected_address(case: CrossLaneMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexCrossLane128MemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_cross_lane_128_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn expected_permute_selections(case: CrossLaneMemoryCase) -> Vec<(u8, VReg, u8)> {
    let mut selected = Vec::with_capacity(4);
    for (output_half, control_shift, zero_bit) in [(0u8, 0u8, 3u8), (1, 4, 7)] {
        if case.immediate >> zero_bit & 1 != 0 {
            continue;
        }
        let control = case.immediate >> control_shift & 3;
        let source = if control < 2 {
            ymm(case.source1)
        } else {
            VReg::Virtual(VirtualId(u32::MAX))
        };
        let source_half = control & 1;
        for lane_in_half in 0..2u8 {
            selected.push((
                output_half * 2 + lane_in_half,
                source,
                source_half * 2 + lane_in_half,
            ));
        }
    }
    selected
}

fn assert_exact_graph(function: &SmirFunction, case: CrossLaneMemoryCase) {
    let block = &function.blocks[0];
    assert!(block.ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert!(block.ops.iter().skip(1).all(|op| op.x86_hint.is_none()));

    let loaded = match block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            ref addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(width, case.operation.source_width(), "{case:?}");
            assert_eq!(
                block.ops[0].x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
                "{case:?}"
            );
            loaded
        }
        ref other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    let mut seen = HashSet::from([loaded]);

    if case.operation.is_insert() {
        assert_eq!(block.ops.len(), 7, "{case:?}");
        let raw = match block.ops[1].kind {
            OpKind::VAnd {
                dst: raw @ VReg::Virtual(_),
                src1,
                src2,
                width: VecWidth::V256,
            } => {
                assert_eq!(src1, ymm(case.source1), "{case:?}");
                assert_eq!(src2, ymm(case.source1), "{case:?}");
                assert!(seen.insert(raw), "{case:?}");
                raw
            }
            ref other => panic!("{case:?}: expected source copy, got {other:?}"),
        };
        let first_lane = (case.immediate & 1) * 2;
        for lane in 0..2u8 {
            let extract_index = 2 + usize::from(lane) * 2;
            let scalar = match block.ops[extract_index].kind {
                OpKind::VExtractLane {
                    dst: scalar @ VReg::Virtual(_),
                    vec,
                    lane: extracted_lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                } => {
                    assert_eq!(vec, loaded, "{case:?}");
                    assert_eq!(extracted_lane, lane, "{case:?}");
                    assert!(seen.insert(scalar), "{case:?}");
                    scalar
                }
                ref other => panic!("{case:?}: expected source extract, got {other:?}"),
            };
            assert!(matches!(
                block.ops[extract_index + 1].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted_scalar,
                    lane: inserted_lane,
                    elem: VecElementType::I64,
                } if dst == raw
                    && vec == raw
                    && inserted_scalar == scalar
                    && inserted_lane == first_lane + lane
            ));
        }
        assert!(matches!(
            block.ops[6].kind,
            OpKind::VMov {
                dst,
                src,
                width: VecWidth::V256,
            } if dst == ymm(case.destination) && src == raw
        ));
    } else {
        let expected = expected_permute_selections(case);
        assert_eq!(block.ops.len(), 4 + expected.len() * 2, "{case:?}");
        let mut cursor = 1usize;
        let mut selected = Vec::with_capacity(expected.len());
        for (output_lane, expected_source, source_lane) in expected {
            let scalar = match block.ops[cursor].kind {
                OpKind::VExtractLane {
                    dst: scalar @ VReg::Virtual(_),
                    vec,
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                } => {
                    let expected_source = if matches!(expected_source, VReg::Virtual(_)) {
                        loaded
                    } else {
                        expected_source
                    };
                    assert_eq!(vec, expected_source, "{case:?}");
                    assert_eq!(lane, source_lane, "{case:?}");
                    assert!(seen.insert(scalar), "{case:?}");
                    scalar
                }
                ref other => panic!("{case:?}: expected selected extract, got {other:?}"),
            };
            selected.push((output_lane, scalar));
            cursor += 1;
        }
        let zero = match block.ops[cursor].kind {
            OpKind::Mov {
                dst: zero @ VReg::Virtual(_),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => {
                assert!(seen.insert(zero), "{case:?}");
                zero
            }
            ref other => panic!("{case:?}: expected zero scalar, got {other:?}"),
        };
        cursor += 1;
        let output = match block.ops[cursor].kind {
            OpKind::VBroadcast {
                dst: output @ VReg::Virtual(_),
                scalar,
                elem: VecElementType::I64,
                lanes: 4,
            } => {
                assert_eq!(scalar, zero, "{case:?}");
                assert!(seen.insert(output), "{case:?}");
                output
            }
            ref other => panic!("{case:?}: expected zero vector, got {other:?}"),
        };
        cursor += 1;
        for (lane, scalar) in selected {
            assert!(matches!(
                block.ops[cursor].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted_scalar,
                    lane: inserted_lane,
                    elem: VecElementType::I64,
                } if dst == output
                    && vec == output
                    && inserted_scalar == scalar
                    && inserted_lane == lane
            ));
            cursor += 1;
        }
        assert!(matches!(
            block.ops[cursor].kind,
            OpKind::VMov {
                dst,
                src,
                width: VecWidth::V256,
            } if dst == ymm(case.destination) && src == output
        ));
        assert_eq!(cursor + 1, block.ops.len(), "{case:?}");
    }

    let sequence = classified_sequence(function, true).expect("classified cross-lane sequence");
    assert_eq!(sequence.consumed, block.ops.len(), "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(
        sequence.encoding.opcode,
        case.operation.opcode(),
        "{case:?}"
    );
    assert_eq!(sequence.encoding.immediate, case.immediate, "{case:?}");
    assert_eq!(
        sequence.encoding.source_width,
        case.operation.source_width(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.memory_size,
        case.operation.source_width().bytes(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.needs_avx2,
        case.operation.needs_avx2(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.emitted_bytes(),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn lift_case(case: CrossLaneMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(
    function: &SmirFunction,
    case: CrossLaneMemoryCase,
) -> (Vec<u8>, usize, X86JitVexCrossLane128MemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified cross-lane sequence");
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx2,
        case.operation.needs_avx2(),
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX cross-lane failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX cross-lane"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_1_536_scanner_domain_family_operand_alias_and_optimization_cells_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 512);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, _) = lower(&function, case);
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 1_536);
}

#[test]
fn all_3_072_immediate_family_and_optimization_cells_lower_exact_bytes() {
    let cases = immediate_cases();
    assert_eq!(cases.len(), 1_024);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, sequence) = lower(&function, case);
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.emitted_bytes(),
                "{level:?} {case:?}"
            );
            assert!(
                code.windows(6).any(|window| window == case.emitted_bytes()),
                "{level:?} {case:?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 3_072);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    for (case, memory, register) in [
        (
            CrossLaneMemoryCase {
                operation: CrossLane::PermuteF128,
                destination: 1,
                source1: 2,
                base: 7,
                immediate: 0x31,
                clear_ignored_x: false,
            },
            &[0xC4, 0xE3, 0x6D, 0x06, 0x4F, 0x20, 0x31][..],
            &[0xC4, 0xE3, 0x6D, 0x06, 0xC8, 0x31][..],
        ),
        (
            CrossLaneMemoryCase {
                operation: CrossLane::InsertF128,
                destination: 9,
                source1: 10,
                base: 11,
                immediate: 0xFF,
                clear_ignored_x: false,
            },
            &[0xC4, 0x43, 0x2D, 0x18, 0x4B, 0x20, 0xFF],
            &[0xC4, 0x63, 0x2D, 0x18, 0xC8, 0xFF],
        ),
        (
            CrossLaneMemoryCase {
                operation: CrossLane::InsertI128,
                destination: 15,
                source1: 15,
                base: 14,
                immediate: 0xA4,
                clear_ignored_x: false,
            },
            &[0xC4, 0x43, 0x05, 0x38, 0x7E, 0x20, 0xA4],
            &[0xC4, 0x63, 0x05, 0x38, 0xF8, 0xA4],
        ),
        (
            CrossLaneMemoryCase {
                operation: CrossLane::PermuteI128,
                destination: 9,
                source1: 10,
                base: 11,
                immediate: 0x82,
                clear_ignored_x: false,
            },
            &[0xC4, 0x43, 0x2D, 0x46, 0x4B, 0x20, 0x82],
            &[0xC4, 0x63, 0x2D, 0x46, 0xC8, 0x82],
        ),
    ] {
        assert_eq!(case.bytes(), memory, "{case:?}");
        assert_eq!(case.emitted_bytes(), register, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[(&[u8], CrossLaneMemoryCase, usize)] = &[
        (
            &[0xC4, 0xE3, 0x75, 0x06, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x31],
            CrossLaneMemoryCase {
                operation: CrossLane::PermuteF128,
                destination: 1,
                source1: 1,
                base: 0,
                immediate: 0x31,
                clear_ignored_x: false,
            },
            12,
        ),
        (
            &[0x64, 0x67, 0xC4, 0xE3, 0x65, 0x18, 0x4C, 0x70, 0x01, 0x81],
            CrossLaneMemoryCase {
                operation: CrossLane::InsertF128,
                destination: 1,
                source1: 3,
                base: 0,
                immediate: 0x81,
                clear_ignored_x: false,
            },
            7,
        ),
        (
            &[
                0x65, 0x67, 0xC4, 0x03, 0x2D, 0x46, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x82,
            ],
            CrossLaneMemoryCase {
                operation: CrossLane::PermuteI128,
                destination: 14,
                source1: 10,
                base: 0,
                immediate: 0x82,
                clear_ignored_x: true,
            },
            8,
        ),
    ];

    let mut lowered = 0usize;
    for (bytes, case, expected_ops) in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function, *case);
            assert_eq!(function.blocks[0].ops.len(), *expected_ops, "{level:?}");
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_eq!(sequence.encoding.opcode, case.operation.opcode());
            assert_eq!(sequence.encoding.immediate, case.immediate);
            assert_eq!(
                sequence.encoding.source_width,
                case.operation.source_width()
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed cross-lane graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed cross-lane graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed cross-lane graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated encoding fits metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_provenance_and_shared_invariants() {
    let case = CrossLaneMemoryCase {
        operation: CrossLane::PermuteI128,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0x31,
        clear_ignored_x: false,
    };
    let base = lift_case(case);
    let loaded = base.blocks[0].ops[0].kind.dests()[0];
    let zero = base.blocks[0]
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => Some(dst),
            _ => None,
        })
        .unwrap();
    let output = base.blocks[0]
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::VBroadcast { dst, .. } => Some(dst),
            _ => None,
        })
        .unwrap();
    let first_scalar = base.blocks[0]
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::VExtractLane { dst, .. } => Some(dst),
            _ => None,
        })
        .unwrap();
    let mut malformed = Vec::new();

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded immediate", 6, 0x01),
        ("encoded map", 1, 0x01),
        ("encoded prefix", 2, 0x02),
        ("encoded W", 2, 0x80),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut register_source = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    replace_instruction_bytes(&mut register_source, &bytes);
    malformed.push(("register-source provenance", register_source));

    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing unaligned load hint", missing_hint));
    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(("aligned load hint", wrong_hint));
    let mut wrong_load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_load_width));
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let first_extract = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    for (name, field) in [
        ("extract destination", 0u8),
        ("extract source", 1),
        ("extract lane", 2),
        ("extract element", 3),
        ("extract sign", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } = &mut function.blocks[0].ops[first_extract].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *vec = ymm(11),
                2 => *lane ^= 1,
                3 => *elem = VecElementType::I32,
                4 => *sign = SignExtend::Sign,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let zero_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Mov { .. }))
        .unwrap();
    let mut wrong_zero_value = base.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut wrong_zero_value.blocks[0].ops[zero_index].kind
    {
        *value = 1;
    }
    malformed.push(("nonzero scalar", wrong_zero_value));
    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[zero_index].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("zero scalar width", wrong_zero_width));

    let broadcast_index = zero_index + 1;
    for (name, field) in [
        ("broadcast destination", 0u8),
        ("broadcast scalar", 1),
        ("broadcast element", 2),
        ("broadcast lanes", 3),
    ] {
        let mut function = base.clone();
        if let OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[broadcast_index].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *scalar = loaded,
                2 => *elem = VecElementType::I32,
                3 => *lanes = 3,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let first_insert = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .unwrap();
    for (name, field) in [
        ("insert destination", 0u8),
        ("insert vector", 1),
        ("insert scalar", 2),
        ("insert lane", 3),
        ("insert element", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } = &mut function.blocks[0].ops[first_insert].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *vec = loaded,
                2 => *scalar = zero,
                3 => *lane ^= 1,
                4 => *elem = VecElementType::I32,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let last = base.blocks[0].ops.len() - 1;
    for (name, field) in [
        ("final destination", 0u8),
        ("final source", 1),
        ("final width", 2),
    ] {
        let mut function = base.clone();
        if let OpKind::VMov { dst, src, width } = &mut function.blocks[0].ops[last].kind {
            match field {
                0 => *dst = ymm(8),
                1 => *src = loaded,
                2 => *width = VecWidth::V128,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));
    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: ymm(4),
            src: output,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("output escapes sequence", external_use));
    let mut duplicate_definition = base;
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFB),
        PC + 1,
        OpKind::VExtractLane {
            dst: first_scalar,
            vec: ymm(2),
            lane: 0,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        },
    ));
    malformed.push(("scalar defined twice", duplicate_definition));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn insert_graph_fail_closed_for_copy_extract_insert_and_ignored_immediate_fields() {
    let case = CrossLaneMemoryCase {
        operation: CrossLane::InsertI128,
        destination: 15,
        source1: 15,
        base: 14,
        immediate: 0xA4,
        clear_ignored_x: true,
    };
    let base = lift_case(case);
    let loaded = base.blocks[0].ops[0].kind.dests()[0];
    let raw = base.blocks[0].ops[1].kind.dests()[0];
    let mut malformed = Vec::new();

    for (name, field) in [
        ("copy destination", 0u8),
        ("copy first source", 1),
        ("copy second source", 2),
        ("copy width", 3),
    ] {
        let mut function = base.clone();
        if let OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        } = &mut function.blocks[0].ops[1].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *src1 = ymm(14),
                2 => *src2 = ymm(14),
                3 => *width = VecWidth::V128,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let mut wrong_extract_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut wrong_extract_source.blocks[0].ops[2].kind {
        *vec = ymm(14);
    }
    malformed.push(("insert extract source", wrong_extract_source));
    let mut wrong_insert_output = base.clone();
    if let OpKind::VInsertLane { dst, .. } = &mut wrong_insert_output.blocks[0].ops[3].kind {
        *dst = loaded;
    }
    malformed.push(("insert output", wrong_insert_output));
    let mut wrong_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_insert_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }
    malformed.push(("insert lane selected by imm8[0]", wrong_insert_lane));
    let mut wrong_final_source = base.clone();
    if let OpKind::VMov { src, .. } = &mut wrong_final_source.blocks[0].ops[6].kind {
        *src = loaded;
    }
    malformed.push(("insert final source", wrong_final_source));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
    assert!(matches!(
        base.blocks[0].ops[6].kind,
        OpKind::VMov { src, .. } if src == raw
    ));

    for immediate in u8::MIN..=u8::MAX {
        let current = CrossLaneMemoryCase { immediate, ..case };
        let function = optimize(lift_case(current), OptLevel::O2);
        let (code, _, sequence) = lower(&function, current);
        assert_eq!(sequence.encoding.immediate, immediate);
        assert_eq!(
            sequence.encoding.register_instruction.as_slice()[5],
            immediate
        );
        assert!(
            code.windows(6)
                .any(|window| window == current.emitted_bytes()),
            "{current:?}"
        );
    }
}
