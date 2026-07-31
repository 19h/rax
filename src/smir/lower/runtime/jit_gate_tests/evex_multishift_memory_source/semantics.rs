//! Raw-bit interpreter, optimizer, and Intel Type E4NF coverage.

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

fn get_byte(vector: &[u64; 16], lane: usize) -> u8 {
    (vector[lane / 8] >> ((lane % 8) * 8)) as u8
}

fn set_byte(vector: &mut [u64; 16], lane: usize, value: u8) {
    let shift = (lane % 8) * 8;
    let word = &mut vector[lane / 8];
    *word = (*word & !(0xFFu64 << shift)) | (u64::from(value) << shift);
}

pub(super) fn initial_state(case: MultiShiftMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 11 + word * 17 + ordinal) as u32)
                    ^ ((register as u64) << 56)
                    ^ (word as u64).wrapping_mul(0x1020_4081_0204_0810)
            })
        }),
        masks: [
            u64::MAX,
            0xA55A_3CC3_F00F_9696,
            0x5AA5_C33C_0FF0_6969,
            0x9696_6996_A55A_3CC3,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[2] = 0x2000;

    let boundary_controls = [
        0u8, 1, 7, 8, 15, 31, 56, 57, 60, 63, 64, 127, 128, 191, 192, 255,
    ];
    let controls = &mut state.vectors[usize::from(case.control_register)];
    for lane in 0..case.width.bytes() as usize {
        set_byte(
            controls,
            lane,
            boundary_controls[(lane + ordinal) % boundary_controls.len()]
                .wrapping_add((lane as u8).rotate_left((ordinal & 7) as u32)),
        );
    }
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|byte| {
        (byte as u8)
            .wrapping_mul(29)
            .wrapping_add((ordinal as u8).rotate_left((byte & 7) as u32))
            ^ 0xA5
    })
}

fn memory_qword(case: MultiShiftMemoryCase, memory: &[u8; 64], qword: usize) -> u64 {
    let source_qword = if case.broadcast() { 0 } else { qword };
    let offset = source_qword * 8;
    u64::from_le_bytes(memory[offset..offset + 8].try_into().unwrap())
}

pub(super) fn manual(
    case: MultiShiftMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let controls = initial.vectors[usize::from(case.control_register)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];

    for qword in 0..case.width.bytes() as usize / 8 {
        let source = memory_qword(case, memory, qword);
        for byte in 0..8 {
            let lane = qword * 8 + byte;
            let value = if mask & (1u64 << lane) != 0 {
                let shift = u32::from(get_byte(&controls, lane) & 63);
                (source.rotate_right(shift) & 0xFF) as u8
            } else if case.zeroing() {
                0
            } else {
                get_byte(&old_destination, lane)
            };
            set_byte(destination, lane, value);
        }
    }
    for word in case.width.bytes() as usize / 8..destination.len() {
        destination[word] = 0;
    }
    expected
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

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
    case: MultiShiftMemoryCase,
) -> SemanticState {
    let tuple_bytes = if case.broadcast() {
        8
    } else {
        case.width.bytes() as usize
    };
    let mut context = context(initial);
    let mut memory = FlatMemory::with_base(0x2000, tuple_bytes);
    memory.load(0, &bytes[..tuple_bytes]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_54_raw_bit_cells_match_manual_circular_windows_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 54 * LEVELS.len());
}

#[test]
fn low_six_control_bits_select_exact_wrapping_windows() {
    let case = MultiShiftMemoryCase {
        width: VecWidth::V128,
        destination: 1,
        control_register: 2,
        form: SourceForm::Broadcast,
        mask_control: MaskControl::None,
    };
    let controls = [0u8, 1, 7, 8, 15, 31, 56, 57, 60, 63, 64, 127, 255];
    let expected_bytes = [
        0xEFu8, 0xF7, 0x9B, 0xCD, 0x57, 0xCF, 0x01, 0x80, 0xF0, 0xDE, 0xEF, 0xDE, 0xDE,
    ];
    let mut initial = initial_state(case, 0);
    for (lane, control) in controls.into_iter().enumerate() {
        set_byte(
            &mut initial.vectors[usize::from(case.control_register)],
            lane,
            control,
        );
    }
    let mut memory = [0u8; 64];
    memory[..8].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    for level in LEVELS {
        let function = optimize(lift_case(case), level);
        let actual = interpret(&function, &initial, &memory, case);
        for (lane, expected) in expected_bytes.into_iter().enumerate() {
            assert_eq!(
                get_byte(&actual.vectors[usize::from(case.destination)], lane),
                expected,
                "{level:?}: control={} lane={lane}",
                controls[lane]
            );
        }
    }
}

#[test]
fn empty_masks_still_read_and_fault_before_architectural_commit() {
    let mut successes = 0usize;
    let mut faults = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            for mask_control in [MaskControl::Merge, MaskControl::Zero] {
                let case = MultiShiftMemoryCase {
                    width,
                    destination: 17,
                    control_register: 18,
                    form,
                    mask_control,
                };
                for level in [OptLevel::O0, OptLevel::O2] {
                    let function = optimize(lift_case(case), level);
                    assert!(
                        !function.blocks[0]
                            .ops
                            .iter()
                            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
                        "{level:?} {case:?}"
                    );
                    let mut initial = initial_state(case, successes);
                    initial.masks[usize::from(case.mask())] = 0;
                    let bytes = memory_bytes(successes);
                    let expected = manual(case, &initial, &bytes);
                    assert_eq!(
                        interpret(&function, &initial, &bytes, case),
                        expected,
                        "{level:?} {case:?}: mapped empty mask"
                    );
                    successes += 1;

                    let mut fault_context = context(&initial);
                    let mut unmapped = FlatMemory::with_base(0x2000, 0);
                    let result = SmirInterpreter::new().execute_block(
                        &mut fault_context,
                        &mut unmapped,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    assert_eq!(
                        state(&fault_context),
                        initial,
                        "{level:?} {case:?}: fault committed state"
                    );
                    faults += 1;
                }
            }
        }
    }
    assert_eq!(successes, 24);
    assert_eq!(faults, successes);
}

#[test]
fn full_vector_tuples_fault_on_an_eight_byte_partial_mapping_without_commit() {
    let mut faults = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let case = MultiShiftMemoryCase {
            width,
            destination: 17,
            control_register: 18,
            form: SourceForm::Vector,
            mask_control: MaskControl::None,
        };
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, faults);
            let bytes = memory_bytes(faults);
            let mut partial = FlatMemory::with_base(0x2000, 8);
            partial.load(0, &bytes[..8]);
            let mut fault_context = context(&initial);
            let result = SmirInterpreter::new().execute_block(
                &mut fault_context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(state(&fault_context), initial, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(faults, 6);
}

#[test]
fn broadcasts_read_exactly_one_qword_at_the_mapping_boundary() {
    let mut successes = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for mask_control in MaskControl::ALL {
            let case = MultiShiftMemoryCase {
                width,
                destination: 17,
                control_register: 18,
                form: SourceForm::Broadcast,
                mask_control,
            };
            let initial = initial_state(case, successes);
            let bytes = memory_bytes(successes);
            let expected = manual(case, &initial, &bytes);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                assert_eq!(
                    interpret(&function, &initial, &bytes, case),
                    expected,
                    "{level:?} {case:?}"
                );
                successes += 1;
            }
        }
    }
    assert_eq!(successes, 27);
}
