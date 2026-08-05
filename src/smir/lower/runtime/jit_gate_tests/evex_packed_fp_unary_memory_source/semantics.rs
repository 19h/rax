//! Interpreter, optimizer, and Type E2/E4 fault-suppression coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticState {
    pub(super) gpr: [u64; 32],
    pub(super) vectors: [[u64; 16]; 32],
    pub(super) masks: [u64; 8],
    pub(super) rflags: u64,
    pub(super) mxcsr: u32,
}

fn lane_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::F16 => 0xFFFF,
        VecElementType::F32 => 0xFFFF_FFFF,
        VecElementType::F64 => u64::MAX,
        _ => unreachable!("packed unary floating-point element"),
    }
}

fn boundary_value(elem: VecElementType, lane: usize, ordinal: usize) -> u64 {
    let selector = (lane + ordinal) % 16;
    match elem {
        VecElementType::F16 => [
            0x0000, 0x8000, 0x3C00, 0xBC00, 0x0001, 0x03FF, 0x0400, 0x7BFF, 0x7C00, 0xFC00, 0x7E15,
            0x7D01, 0x4000, 0x4400, 0x3555, 0xB555,
        ][selector],
        VecElementType::F32 => [
            0x0000_0000,
            0x8000_0000,
            0x3F80_0000,
            0xBF80_0000,
            0x0000_0001,
            0x007F_FFFF,
            0x0080_0000,
            0x7F7F_FFFF,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC1_2345,
            0x7F81_2345,
            0x4000_0000,
            0x4080_0000,
            0x3EAA_AAAB,
            0xBEAA_AAAB,
        ][selector],
        VecElementType::F64 => [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0xBFF0_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x000F_FFFF_FFFF_FFFF,
            0x0010_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF8_1234_5678_9ABC,
            0x7FF0_1234_5678_9ABC,
            0x4000_0000_0000_0000,
            0x4010_0000_0000_0000,
            0x3FD5_5555_5555_5555,
            0xBFD5_5555_5555_5555,
        ][selector],
        _ => unreachable!("packed unary floating-point element"),
    }
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    let bits = usize::try_from(elem.bytes()).unwrap() * 8;
    let lanes_per_word = 64 / bits;
    let shift = (lane % lanes_per_word) * bits;
    (vector[lane / lanes_per_word] >> shift) & lane_mask(elem)
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let bits = usize::try_from(elem.bytes()).unwrap() * 8;
    let lanes_per_word = 64 / bits;
    let shift = (lane % lanes_per_word) * bits;
    let mask = lane_mask(elem) << shift;
    let word = &mut vector[lane / lanes_per_word];
    *word = (*word & !mask) | ((value & lane_mask(elem)) << shift);
}

pub(super) fn source_bytes(case: PackedUnaryMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.width.lanes(case.elem()) as usize;
    for lane in 0..lanes {
        let value = boundary_value(case.elem(), lane, ordinal);
        let lane_bytes = case.elem().bytes() as usize;
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn source_vector(case: PackedUnaryMemoryCase, bytes: &[u8; 64]) -> [u64; 16] {
    let mut source = [0u64; 16];
    let lanes = case.width.lanes(case.elem()) as usize;
    let lane_bytes = case.elem().bytes() as usize;
    for lane in 0..lanes {
        let memory_lane = if case.broadcast() { 0 } else { lane };
        let offset = memory_lane * lane_bytes;
        let mut value = [0u8; 8];
        value[..lane_bytes].copy_from_slice(&bytes[offset..offset + lane_bytes]);
        set_lane(&mut source, lane, case.elem(), u64::from_le_bytes(value));
    }
    source
}

pub(super) fn initial_state(
    case: PackedUnaryMemoryCase,
    ordinal: usize,
    bytes: &[u8; 64],
) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x8000_0001_7FFF_FFFFu64.rotate_left((register * 11 + word * 17 + ordinal) as u32)
                    ^ ((register as u64) << 56)
                    ^ (word as u64).wrapping_mul(0x1020_4081_0204_0810)
            })
        }),
        masks: [
            u64::MAX,
            0x8000_0000_0000_0001,
            0x5AA5_C33C_0FF0_6969,
            0xF0F0_F0F0_9696_6996,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80
            | if ordinal & 1 != 0 { 1 << 6 } else { 0 }
            | if ordinal & 2 != 0 { 1 << 15 } else { 0 },
    };
    state.gpr[3] = MEMORY_ADDRESS;
    state.vectors[usize::from(case.scratch())] = source_vector(case, bytes);
    state
}

fn context(initial: &SemanticState) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        x86.xmm = initial.vectors;
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn state(context: &SmirContext) -> SemanticState {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    SemanticState {
        gpr: x86.gpr,
        vectors: x86.xmm,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

fn execute(
    function: &SmirFunction,
    initial: &SemanticState,
    mut memory: FlatMemory,
) -> (BlockResult, SemanticState) {
    let mut context = context(initial);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    (result, state(&context))
}

pub(super) fn interpret_mapped(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
    case: PackedUnaryMemoryCase,
) -> SemanticState {
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.memory_width().bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(MEMORY_ADDRESS as usize, &bytes[..size]);
    let (result, state) = execute(function, initial, memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state
}

fn reference_state(
    case: PackedUnaryMemoryCase,
    level: OptLevel,
    initial: &SemanticState,
) -> SemanticState {
    let function = optimize(lift_bytes(&register_encoding(case, case.scratch())), level);
    let (result, state) = execute(&function, initial, FlatMemory::new(1));
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state
}

#[test]
fn all_378_packed_unary_memory_cells_match_register_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 378);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = source_bytes(case, ordinal);
        let initial = initial_state(case, ordinal, &bytes);
        for level in LEVELS {
            let memory_function = optimize(lift_case(case), level);
            let expected = reference_state(case, level, &initial);
            let actual = interpret_mapped(&memory_function, &initial, &bytes, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 378 * LEVELS.len());
}

#[test]
fn empty_masks_suppress_every_e2_e4_memory_access_and_active_faults_do_not_commit() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.control != MaskControl::None)
        .collect();
    assert_eq!(cases.len(), 252);
    let mut suppressions = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = source_bytes(case, ordinal);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);

            let mut empty = initial_state(case, ordinal, &bytes);
            empty.masks[usize::from(case.mask())] = 0xFFFF_FFFF_0000_0000;
            let expected = reference_state(case, level, &empty);
            let (result, actual) = execute(&function, &empty, FlatMemory::new(0x1000));
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, expected, "{level:?} {case:?}");
            suppressions += 1;

            let mut active = initial_state(case, ordinal, &bytes);
            active.masks[usize::from(case.mask())] = 1;
            let (result, actual) = execute(&function, &active, FlatMemory::new(0x1000));
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, active, "{level:?} {case:?}: fault committed state");
            faults += 1;
        }
    }
    assert_eq!(suppressions, 252 * LEVELS.len());
    assert_eq!(faults, suppressions);
}

#[test]
fn fp16_sqrt_broadcast_normalizes_any_active_lane_to_predicate_bit_zero() {
    let mut executions = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = PackedUnaryMemoryCase {
                operation: PackedUnaryOperation::SqrtF16,
                width,
                destination: 17,
                form: SourceForm::Broadcast,
                control,
            };
            let bytes = source_bytes(case, 12);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_state(case, 12, &bytes);
                // Lane 0 is inactive while lane 1 is active. PredLoad observes
                // only predicate bit 0, so the aggregate must first normalize
                // this nonzero architectural mask to exactly 1.
                initial.masks[usize::from(case.mask())] = 1 << 1;
                let expected = reference_state(case, level, &initial);
                let actual = interpret_mapped(&function, &initial, &bytes, case);
                assert_eq!(actual, expected, "{level:?} {case:?}");

                let (result, faulted) = execute(&function, &initial, FlatMemory::new(0x1000));
                assert!(
                    matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                    ),
                    "{level:?} {case:?}: {result:?}"
                );
                assert_eq!(
                    faulted, initial,
                    "{level:?} {case:?}: fault committed state"
                );
                executions += 1;
            }
        }
    }
    assert_eq!(executions, 18);
}

#[test]
fn inactive_cross_boundary_lanes_suppress_faults_and_later_active_faults_are_noncommitting() {
    for (ordinal, operation) in [
        PackedUnaryOperation::SqrtF16,
        PackedUnaryOperation::SqrtF32,
        PackedUnaryOperation::SqrtF64,
        PackedUnaryOperation::GetExpF16,
        PackedUnaryOperation::Recip14F32,
        PackedUnaryOperation::Rsqrt14F64,
    ]
    .into_iter()
    .enumerate()
    {
        let case = PackedUnaryMemoryCase {
            operation,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        };
        let bytes = source_bytes(case, ordinal + 11);
        let lane_bytes = case.elem().bytes() as u64;
        let base = 0x1000 - lane_bytes;
        for level in LEVELS {
            let function = optimize(lift_case(case), level);

            let mut suppressed = initial_state(case, ordinal + 11, &bytes);
            suppressed.gpr[3] = base;
            suppressed.masks[usize::from(case.mask())] = 1;
            let mut reference = suppressed.clone();
            reference.gpr[3] = MEMORY_ADDRESS;
            let expected = reference_state(case, level, &reference);
            let mut memory = FlatMemory::new(0x1000);
            memory.load(
                base as usize,
                &bytes[..usize::try_from(lane_bytes).unwrap()],
            );
            let (result, actual) = execute(&function, &suppressed, memory);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            let mut expected_at_base = expected;
            expected_at_base.gpr[3] = base;
            assert_eq!(actual, expected_at_base, "{level:?} {case:?}");

            let mut faulting = suppressed.clone();
            faulting.masks[usize::from(case.mask())] = 3;
            let mut memory = FlatMemory::new(0x1000);
            memory.load(
                base as usize,
                &bytes[..usize::try_from(lane_bytes).unwrap()],
            );
            let (result, actual) = execute(&function, &faulting, memory);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                actual, faulting,
                "{level:?} {case:?}: earlier lane committed before later fault"
            );
        }
    }
}

#[test]
fn packed_sqrt_memory_matches_register_for_all_mxcsr_rounding_modes() {
    let mut comparisons = 0usize;
    for operation in [
        PackedUnaryOperation::SqrtF16,
        PackedUnaryOperation::SqrtF32,
        PackedUnaryOperation::SqrtF64,
    ] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            for control in MaskControl::ALL {
                let case = PackedUnaryMemoryCase {
                    operation,
                    width: VecWidth::V512,
                    destination: 17,
                    form,
                    control,
                };
                // Lane 0 is +2.0 in every precision, so round-down and
                // round-up differ by one ULP for its inexact square root.
                let bytes = source_bytes(case, 12);
                let mut outputs = [0u64; 4];
                for rounding_control in 0..4u32 {
                    let mut initial = initial_state(case, 12, &bytes);
                    if case.mask() != 0 {
                        initial.masks[usize::from(case.mask())] = 1;
                    }
                    initial.mxcsr = (0x1F80 & !(0x3F | (3 << 13))) | (rounding_control << 13);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let expected = reference_state(case, level, &initial);
                        let actual = interpret_mapped(&function, &initial, &bytes, case);
                        assert_eq!(actual, expected, "{level:?} RC={rounding_control} {case:?}");
                        assert_ne!(
                            actual.mxcsr & (1 << 5),
                            0,
                            "{level:?} RC={rounding_control} {case:?}: missing precision status"
                        );
                        if level == OptLevel::O2 {
                            outputs[rounding_control as usize] = get_lane(
                                &actual.vectors[usize::from(case.destination)],
                                0,
                                case.elem(),
                            );
                        }
                        comparisons += 1;
                    }
                }
                assert_ne!(
                    outputs[1], outputs[2],
                    "{case:?}: RC did not affect sqrt(+2)"
                );
            }
        }
    }
    assert_eq!(comparisons, 216);
}

#[test]
fn packed_sqrt_memory_unmasked_invalid_is_precise_and_noncommitting() {
    let mut traps = 0usize;
    for operation in [
        PackedUnaryOperation::SqrtF16,
        PackedUnaryOperation::SqrtF32,
        PackedUnaryOperation::SqrtF64,
    ] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            for control in [MaskControl::None, MaskControl::Merge] {
                let case = PackedUnaryMemoryCase {
                    operation,
                    width: VecWidth::V128,
                    destination: 0,
                    form,
                    control,
                };
                let (positive, negative) = match case.elem() {
                    VecElementType::F16 => (0x4400u64, 0xBC00u64),
                    VecElementType::F32 => (0x4080_0000, 0xBF80_0000),
                    VecElementType::F64 => (0x4010_0000_0000_0000, 0xBFF0_0000_0000_0000),
                    _ => unreachable!(),
                };
                let mut bytes = [0u8; 64];
                let lane_bytes = case.elem().bytes() as usize;
                for lane in 0..case.width.lanes(case.elem()) as usize {
                    let offset = lane * lane_bytes;
                    bytes[offset..offset + lane_bytes]
                        .copy_from_slice(&positive.to_le_bytes()[..lane_bytes]);
                }
                bytes[..lane_bytes].copy_from_slice(&negative.to_le_bytes()[..lane_bytes]);

                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_state(case, 31, &bytes);
                    if case.mask() != 0 {
                        initial.masks[usize::from(case.mask())] = 1;
                    }
                    initial.mxcsr = (0x1F80 & !(1 << 7)) & !0x3F;
                    let mut memory = FlatMemory::new(0x4000);
                    let size = if case.broadcast() {
                        case.memory_width().bytes()
                    } else {
                        case.width.bytes()
                    } as usize;
                    memory.load(MEMORY_ADDRESS as usize, &bytes[..size]);
                    let (result, actual) = execute(&function, &initial, memory);
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let mut expected = initial.clone();
                    expected.mxcsr |= 1;
                    assert_eq!(actual, expected, "{level:?} {case:?}: #XM committed state");
                    traps += 1;
                }
            }
        }
    }
    assert_eq!(traps, 36);
}

#[test]
fn source_patterns_cover_zero_subnormal_normal_infinity_qnan_and_snan() {
    for operation in PackedUnaryOperation::ALL {
        let case = PackedUnaryMemoryCase {
            operation,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        };
        let lanes = case.width.lanes(case.elem()) as usize;
        let values: Vec<u64> = [0, 8]
            .into_iter()
            .flat_map(|ordinal| {
                let bytes = source_bytes(case, ordinal);
                let source = source_vector(case, &bytes);
                (0..lanes)
                    .map(|lane| get_lane(&source, lane, case.elem()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(values.contains(&0), "{operation:?}");
        assert!(values.contains(&1), "{operation:?}: minimum subnormal");
        match case.elem() {
            VecElementType::F16 => {
                for value in [0x0400, 0x7C00, 0x7E15, 0x7D01] {
                    assert!(values.contains(&value), "{operation:?}: {value:#x}");
                }
            }
            VecElementType::F32 => {
                for value in [0x0080_0000, 0x7F80_0000, 0x7FC1_2345, 0x7F81_2345] {
                    assert!(values.contains(&value), "{operation:?}: {value:#x}");
                }
            }
            VecElementType::F64 => {
                for value in [
                    0x0010_0000_0000_0000,
                    0x7FF0_0000_0000_0000,
                    0x7FF8_1234_5678_9ABC,
                    0x7FF0_1234_5678_9ABC,
                ] {
                    assert!(values.contains(&value), "{operation:?}: {value:#x}");
                }
            }
            _ => unreachable!(),
        }
    }
}
