//! Interpreter, optimizer, integer-boundary, alias, and Type E4 fault coverage.

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

fn get_i16(vector: &[u64; 16], lane: usize) -> i16 {
    let shift = (lane & 3) * 16;
    ((vector[lane / 4] >> shift) as u16) as i16
}

fn set_i16(vector: &mut [u64; 16], lane: usize, value: i16) {
    let shift = (lane & 3) * 16;
    let mask = u64::from(u16::MAX) << shift;
    vector[lane / 4] = (vector[lane / 4] & !mask) | (u64::from(value as u16) << shift);
}

fn get_i32(vector: &[u64; 16], lane: usize) -> i32 {
    let shift = (lane & 1) * 32;
    ((vector[lane / 2] >> shift) as u32) as i32
}

fn set_i32(vector: &mut [u64; 16], lane: usize, value: i32) {
    let shift = (lane & 1) * 32;
    let mask = u64::from(u32::MAX) << shift;
    vector[lane / 2] = (vector[lane / 2] & !mask) | (u64::from(value as u32) << shift);
}

pub(super) fn initial_state(case: FourDotProductMemoryCase, ordinal: usize) -> SemanticState {
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
            0xF0F0_0F0_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80 ^ ((ordinal as u32 & 3) << 13),
    };
    state.gpr[2] = MEMORY_ADDRESS;

    for register in 0..32usize {
        for lane in 0..32usize {
            let value = ((register * 17 + lane * 13 + ordinal * 7) % 127) as i16 - 63;
            set_i16(&mut state.vectors[register], lane, value);
        }
        for word in 8..16usize {
            state.vectors[register][word] =
                0xDEAD_0000_0000_0000 ^ ((register as u64) << 32) ^ word as u64 ^ ordinal as u64;
        }
    }
    for lane in 0..16usize {
        let value = (lane as i32)
            .wrapping_mul(0x0102_0304)
            .wrapping_add(ordinal as i32 * 0x101);
        set_i32(
            &mut state.vectors[usize::from(case.destination)],
            lane,
            value,
        );
    }
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 16] {
    let pairs = [(3i16, -4i16), (-7, 8), (11, -13), (-17, -19)];
    let mut bytes = [0u8; 16];
    for (stage, (low, high)) in pairs.into_iter().enumerate() {
        let low = low.wrapping_add((ordinal % 5) as i16);
        let high = high.wrapping_sub((ordinal % 3) as i16);
        bytes[stage * 4..stage * 4 + 2].copy_from_slice(&low.to_le_bytes());
        bytes[stage * 4 + 2..stage * 4 + 4].copy_from_slice(&high.to_le_bytes());
    }
    bytes
}

fn memory_pair(memory: &[u8; 16], stage: usize) -> (i16, i16) {
    (
        i16::from_le_bytes(memory[stage * 4..stage * 4 + 2].try_into().unwrap()),
        i16::from_le_bytes(memory[stage * 4 + 2..stage * 4 + 4].try_into().unwrap()),
    )
}

/// Independent AVX512_4VNNIW oracle. Runtime is O(16 * 4) and auxiliary
/// space is O(1) beyond the returned architectural snapshot.
pub(super) fn manual(
    case: FourDotProductMemoryCase,
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
    let mut destination = [0u64; 16];
    for lane in 0..16usize {
        if mask & (1u64 << lane) == 0 {
            if !case.zeroing() {
                set_i32(&mut destination, lane, get_i32(&old_destination, lane));
            }
            continue;
        }

        let mut accumulator = i64::from(get_i32(&old_destination, lane));
        for stage in 0..4usize {
            let (memory_low, memory_high) = memory_pair(memory, stage);
            let source_low = get_i16(&sources[stage], lane * 2);
            let source_high = get_i16(&sources[stage], lane * 2 + 1);
            let sum = accumulator
                + i64::from(source_low) * i64::from(memory_low)
                + i64::from(source_high) * i64::from(memory_high);
            accumulator = if case.saturating {
                sum.clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            } else {
                i64::from(sum as i32)
            };
        }
        set_i32(&mut destination, lane, accumulator as i32);
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
fn all_30_cells_match_independent_stagewise_integer_oracle_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 30);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(ordinal);
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
    assert_eq!(comparisons, 30 * LEVELS.len());
}

#[test]
fn wrapping_and_per_iteration_signed_saturation_boundaries_are_exact() {
    let memory = [
        1, 0, 0, 0, // (1, 0)
        1, 0, 0, 0, // (1, 0)
        1, 0, 0, 0, // (1, 0)
        1, 0, 0, 0, // (1, 0)
    ];
    for (saturating, initial_value, stages, expected) in [
        (false, i32::MAX, [1, 0, 0, 0], i32::MIN),
        (true, i32::MAX - 5, [10, -20, 0, 0], i32::MAX - 20),
        (true, i32::MIN + 5, [-10, 20, 0, 0], i32::MIN + 20),
    ] {
        let case = FourDotProductMemoryCase {
            saturating,
            destination: 17,
            source_index: 20,
            control: MaskControl::None,
        };
        let mut initial = initial_state(case, 0);
        for lane in 0..16usize {
            set_i32(
                &mut initial.vectors[usize::from(case.destination)],
                lane,
                initial_value,
            );
            for (stage, source) in stages.into_iter().enumerate() {
                set_i16(
                    &mut initial.vectors[usize::from(case.source_base()) + stage],
                    lane * 2,
                    source,
                );
                set_i16(
                    &mut initial.vectors[usize::from(case.source_base()) + stage],
                    lane * 2 + 1,
                    0,
                );
            }
        }
        for level in LEVELS {
            let actual = interpret(&optimize(lift_case(case), level), &initial, &memory);
            for lane in 0..16usize {
                assert_eq!(
                    get_i32(&actual.vectors[usize::from(case.destination)], lane),
                    expected,
                    "{level:?} saturating={saturating} lane={lane}"
                );
            }
            assert_eq!(actual.rflags, initial.rflags);
            assert_eq!(actual.mxcsr, initial.mxcsr);
        }
    }
}

#[test]
fn destination_aliases_snapshot_the_entire_aligned_four_zmm_source_block() {
    for destination in 20..=23u8 {
        let case = FourDotProductMemoryCase {
            saturating: destination & 1 != 0,
            destination,
            source_index: 23,
            control: MaskControl::Merge,
        };
        let mut initial = initial_state(case, usize::from(destination));
        initial.masks[1] = 0xFFFF;
        let memory = memory_bytes(usize::from(destination));
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            assert_eq!(
                interpret(&optimize(lift_case(case), level), &initial, &memory),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn tuple_fault_suppression_uses_low_16_mask_bits_and_faults_before_commit() {
    let mut suppressions = 0usize;
    let mut faults = 0usize;
    for control in [MaskControl::Merge, MaskControl::Zero] {
        let case = FourDotProductMemoryCase {
            saturating: control == MaskControl::Zero,
            destination: 17,
            source_index: 20,
            control,
        };
        let bytes = memory_bytes(0);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);

            let mut inactive = initial_state(case, 0);
            inactive.masks[1] = 1 << 16;
            let expected = manual(case, &inactive, &bytes);
            let mut inactive_context = context(&inactive);
            let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
            let result = SmirInterpreter::new().execute_block(
                &mut inactive_context,
                &mut inaccessible,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::Return { .. })
            ));
            assert_eq!(state(&inactive_context), expected, "{level:?} {case:?}");
            suppressions += 1;

            let mut active = initial_state(case, 1);
            active.masks[1] = 1 << 15;
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
            assert_eq!(state(&active_context), active, "{level:?} {case:?}");
            faults += 1;
        }
    }

    let unmasked = FourDotProductMemoryCase {
        saturating: false,
        destination: 17,
        source_index: 20,
        control: MaskControl::None,
    };
    let bytes = memory_bytes(1);
    for level in LEVELS {
        let function = optimize(lift_case(unmasked), level);
        let initial = initial_state(unmasked, 2);
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
        assert_eq!(state(&fault_context), initial, "{level:?}");
        faults += 1;
    }
    assert_eq!(suppressions, 2 * LEVELS.len());
    assert_eq!(faults, 3 * LEVELS.len());
}
