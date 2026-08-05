//! Interpreter, optimizer, sequential-rounding, and Type E2 fault coverage.

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

fn get_f32(vector: &[u64; 16], lane: usize) -> u32 {
    ((vector[lane / 2] >> ((lane & 1) * 32)) & u64::from(u32::MAX)) as u32
}

pub(super) fn set_f32(vector: &mut [u64; 16], lane: usize, value: u32) {
    let shift = (lane & 1) * 32;
    let mask = u64::from(u32::MAX) << shift;
    vector[lane / 2] = (vector[lane / 2] & !mask) | (u64::from(value) << shift);
}

pub(super) fn initial_state(case: FourFmaMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA55A_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 5) as u32)
                ^ (register as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 7 + word * 13 + ordinal) as u32)
                    ^ ((register as u64) << 57)
                    ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
            })
        }),
        masks: [
            u64::MAX,
            if ordinal & 1 == 0 { 0xA55B } else { 0xA55A },
            0x8000_0000_0000_0001,
            0x9696_6996_A55A_3CC3,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[2] = MEMORY_ADDRESS;

    // Small integer binary32 values make every multiply-add exact. This gives
    // an independent arithmetic oracle without relying on MXCSR emulation.
    for register in 0..32usize {
        for lane in 0..16usize {
            let value = ((register + 1) * ((lane % 3) + 1) + (ordinal % 5)) as f32;
            set_f32(&mut state.vectors[register], lane, value.to_bits());
        }
    }
    // Retain nonzero state above ZMM so destination-width clearing is visible.
    for register in 0..32usize {
        for word in 8..16usize {
            state.vectors[register][word] =
                0xDEAD_0000_0000_0000 ^ ((register as u64) << 32) ^ word as u64 ^ ordinal as u64;
        }
    }
    // `case` is intentionally consumed here: alias cases are represented by
    // the same initial register image and the manual oracle snapshots it.
    let _ = case;
    state
}

pub(super) fn memory_bytes(_case: FourFmaMemoryCase, _ordinal: usize) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in [1.0f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}

fn memory_f32(memory: &[u8; 16], lane: usize) -> u32 {
    u32::from_le_bytes(memory[lane * 4..lane * 4 + 4].try_into().unwrap())
}

/// Independent finite-value oracle for the four architecturally sequential
/// binary32 fused boundaries. Runtime is O(L * 4), where L is 1 or 16 lanes,
/// with O(1) auxiliary state beyond the returned architectural snapshot.
pub(super) fn manual(
    case: FourFmaMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 16],
) -> SemanticState {
    let mut expected = initial.clone();
    let old_destination = initial.vectors[usize::from(case.destination)];
    let sources: [[u64; 16]; 4] =
        std::array::from_fn(|stage| initial.vectors[usize::from(case.source_base()) + stage]);
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let lanes = if case.scalar() { 1 } else { 16 };
    let mut destination = [0u64; 16];
    for lane in 0..lanes {
        let result = if mask & (1u64 << lane) == 0 {
            if case.zeroing() {
                0
            } else {
                get_f32(&old_destination, lane)
            }
        } else {
            let mut accumulator = f32::from_bits(get_f32(&old_destination, lane));
            for stage in 0..4usize {
                let source = f32::from_bits(get_f32(&sources[stage], lane));
                let multiplier = f32::from_bits(memory_f32(memory, stage));
                accumulator = if case.negate_product {
                    (-source).mul_add(multiplier, accumulator)
                } else {
                    source.mul_add(multiplier, accumulator)
                };
            }
            accumulator.to_bits()
        };
        set_f32(&mut destination, lane, result);
    }
    if case.scalar() {
        for lane in 1..4usize {
            set_f32(&mut destination, lane, get_f32(&old_destination, lane));
        }
    }
    expected.vectors[usize::from(case.destination)] = destination;
    expected
}

fn context(initial: &SemanticState) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        x86.xmm = initial.vectors;
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
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
    bytes: &[u8; 16],
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    memory.load(MEMORY_ADDRESS as usize, bytes);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_72_cells_match_independent_sequential_fma_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 72);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(case, ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(
                interpret(&function, &initial, &memory),
                expected,
                "{level:?} {case:?}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 72 * LEVELS.len());
}

#[test]
fn sequential_half_ulp_rounding_is_not_contracted_across_four_boundaries() {
    for form in FourFmaForm::ALL {
        let case = FourFmaMemoryCase {
            form,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: if form.scalar() { 1 } else { 2 },
            control: MaskControl::None,
        };
        let mut bytes = [0u8; 16];
        for stage in 0..4usize {
            bytes[stage * 4..stage * 4 + 4].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
        }
        for (mxcsr, expected_low) in [(0x1F80, 0x3F80_0000), (0x5F80, 0x3F80_0004)] {
            let mut initial = initial_state(case, usize::from(form.scalar()));
            initial.mxcsr = mxcsr;
            let lanes = if form.scalar() { 1 } else { 16 };
            for lane in 0..lanes {
                set_f32(
                    &mut initial.vectors[usize::from(case.destination)],
                    lane,
                    1.0f32.to_bits(),
                );
                for stage in 0..4usize {
                    set_f32(
                        &mut initial.vectors[usize::from(case.source_base()) + stage],
                        lane,
                        2.0f32.powi(-24).to_bits(),
                    );
                }
            }
            for level in LEVELS {
                let actual = interpret(&optimize(lift_case(case), level), &initial, &bytes);
                assert_eq!(
                    get_f32(&actual.vectors[usize::from(case.destination)], 0),
                    expected_low,
                    "{level:?} {case:?} mxcsr={mxcsr:#06X}"
                );
                assert_ne!(actual.mxcsr & (1 << 5), 0, "precision status");
            }
        }
    }
}

#[test]
fn special_values_daz_ftz_and_all_rounding_modes_are_optimizer_invariant() {
    let values = [
        0x0000_0000u32,
        0x8000_0000,
        0x0000_0001,
        0x007F_FFFF,
        0x0080_0000,
        0x7F7F_FFFF,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC0_0011,
        0x7F80_0022,
    ];
    let mxcsr_modes = [0x1F80, 0x3FC0, 0xDF80, 0xFFC0];
    let cases = [
        FourFmaMemoryCase {
            form: FourFmaForm::Packed,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: 2,
            control: MaskControl::Merge,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Packed,
            negate_product: true,
            destination: 20,
            source_index: 23,
            ll: 2,
            control: MaskControl::Zero,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Scalar,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: 0,
            control: MaskControl::Merge,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Scalar,
            negate_product: true,
            destination: 20,
            source_index: 23,
            ll: 2,
            control: MaskControl::Zero,
        },
    ];
    let mut comparisons = 0usize;
    for (case_index, case) in cases.into_iter().enumerate() {
        for (mode_index, mxcsr) in mxcsr_modes.into_iter().enumerate() {
            let mut initial = initial_state(case, case_index + mode_index);
            initial.mxcsr = mxcsr;
            initial.masks[1] |= 1;
            for stage in 0..4usize {
                for lane in 0..16usize {
                    set_f32(
                        &mut initial.vectors[usize::from(case.source_base()) + stage],
                        lane,
                        values[(stage * 3 + lane + mode_index) % values.len()],
                    );
                }
            }
            for lane in 0..16usize {
                set_f32(
                    &mut initial.vectors[usize::from(case.destination)],
                    lane,
                    values[(lane + 5) % values.len()],
                );
            }
            let mut memory = [0u8; 16];
            for stage in 0..4usize {
                memory[stage * 4..stage * 4 + 4].copy_from_slice(
                    &values[(stage * 2 + case_index) % values.len()].to_le_bytes(),
                );
            }
            let baseline = interpret(&lift_case(case), &initial, &memory);
            for level in LEVELS {
                let actual = interpret(&optimize(lift_case(case), level), &initial, &memory);
                assert_eq!(actual, baseline, "{level:?} {case:?} mxcsr={mxcsr:#06X}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, cases.len() * mxcsr_modes.len() * LEVELS.len());
}

#[test]
fn type_e2_uses_one_whole_tuple_access_and_faults_before_any_commit() {
    let cases = [
        FourFmaMemoryCase {
            form: FourFmaForm::Packed,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: 2,
            control: MaskControl::Merge,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Packed,
            negate_product: true,
            destination: 20,
            source_index: 23,
            ll: 2,
            control: MaskControl::Zero,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Scalar,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: 0,
            control: MaskControl::Merge,
        },
        FourFmaMemoryCase {
            form: FourFmaForm::Scalar,
            negate_product: true,
            destination: 20,
            source_index: 23,
            ll: 2,
            control: MaskControl::Zero,
        },
    ];
    let mut suppressions = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = memory_bytes(case, ordinal);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);

            let mut inactive = initial_state(case, ordinal);
            inactive.masks[1] = if case.scalar() { 1 << 1 } else { 1 << 16 };
            let expected = manual(case, &inactive, &bytes);
            let mut inactive_context = context(&inactive);
            let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
            let result = SmirInterpreter::new().execute_block(
                &mut inactive_context,
                &mut inaccessible,
                &function.blocks[0],
            );
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(state(&inactive_context), expected, "{level:?} {case:?}");
            suppressions += 1;

            let mut active = initial_state(case, ordinal + 0x20);
            active.masks[1] = if case.scalar() { 1 } else { 1 << 15 };
            let mut active_context = context(&active);
            let mut partial = FlatMemory::new((MEMORY_ADDRESS + 15) as usize);
            partial.load(MEMORY_ADDRESS as usize, &bytes[..15]);
            let result = SmirInterpreter::new().execute_block(
                &mut active_context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr,
                        write: false,
                        ..
                    }) if addr == MEMORY_ADDRESS + 16
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                state(&active_context),
                active,
                "{level:?} {case:?}: active fault committed state"
            );
            faults += 1;
        }
    }

    for form in FourFmaForm::ALL {
        let case = FourFmaMemoryCase {
            form,
            negate_product: false,
            destination: 17,
            source_index: 20,
            ll: if form.scalar() { 1 } else { 2 },
            control: MaskControl::None,
        };
        let bytes = memory_bytes(case, 0);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, 0x40);
            let mut fault_context = context(&initial);
            let mut partial = FlatMemory::new((MEMORY_ADDRESS + 15) as usize);
            partial.load(MEMORY_ADDRESS as usize, &bytes[..15]);
            let result = SmirInterpreter::new().execute_block(
                &mut fault_context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            assert_eq!(state(&fault_context), initial, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(suppressions, 4 * LEVELS.len());
    assert_eq!(faults, 6 * LEVELS.len());
}

#[test]
fn unmasked_invalid_at_first_fma_boundary_updates_only_mxcsr_status() {
    let case = FourFmaMemoryCase {
        form: FourFmaForm::Packed,
        negate_product: false,
        destination: 17,
        source_index: 20,
        ll: 2,
        control: MaskControl::None,
    };
    let mut memory = memory_bytes(case, 0);
    memory[..4].copy_from_slice(&f32::INFINITY.to_bits().to_le_bytes());
    for level in LEVELS {
        let function = optimize(lift_case(case), level);
        let mut initial = initial_state(case, 0);
        initial.mxcsr = 0x1F80 & !(1 << 7);
        set_f32(&mut initial.vectors[usize::from(case.source_base())], 0, 0);
        let mut expected = initial.clone();
        expected.mxcsr |= 1;
        let mut fault_context = context(&initial);
        let mut mapped = FlatMemory::new(0x4000);
        mapped.load(MEMORY_ADDRESS as usize, &memory);
        let result = SmirInterpreter::new().execute_block(
            &mut fault_context,
            &mut mapped,
            &function.blocks[0],
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: PC })
            ),
            "{level:?}: {result:?}"
        );
        assert_eq!(state(&fault_context), expected, "{level:?}");
    }
}
