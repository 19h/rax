//! Independent Type-E6 semantics, optimizer parity, suppression, and faults.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticState {
    pub(super) gpr: [u64; 32],
    pub(super) vectors: [[u64; 16]; 32],
    pub(super) masks: [u64; 8],
    pub(super) rflags: u64,
    pub(super) mxcsr: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticOutcome {
    pub(super) state: SemanticState,
    pub(super) memory: [u8; 64],
}

pub(super) fn lane_mask(lanes: usize) -> u64 {
    if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    }
}

pub(super) fn vector_bytes(words: &[u64; 16]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn set_vector_bytes(words: &mut [u64; 16], bytes: &[u8; 64]) {
    for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(8)) {
        *word = u64::from_le_bytes(chunk.try_into().unwrap());
    }
}

fn width_mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn seed_source_lanes(case: NarrowMemoryCase, ordinal: usize, words: &mut [u64; 16]) {
    let src_bytes = case.operation.src_elem.bytes() as usize;
    let src_bits = case.operation.src_elem.bytes() * 8;
    let dst_bits = case.operation.dst_elem.bytes() * 8;
    let src_mask = width_mask(src_bits);
    let dst_mask = width_mask(dst_bits);
    let sign = 1u64 << (src_bits - 1);
    let values = [
        0,
        1,
        dst_mask,
        dst_mask + 1,
        (1u64 << (dst_bits - 1)) - 1,
        1u64 << (dst_bits - 1),
        sign - 1,
        sign,
        sign | 1,
        src_mask - 1,
        src_mask,
        0x0123_4567_89AB_CDEF & src_mask,
    ];
    let mut bytes = vector_bytes(words);
    for lane in 0..case.lanes() {
        let value = values[(lane + ordinal) % values.len()] & src_mask;
        let offset = lane * src_bytes;
        bytes[offset..offset + src_bytes].copy_from_slice(&value.to_le_bytes()[..src_bytes]);
    }
    set_vector_bytes(words, &bytes);
}

pub(super) fn initial_state(
    case: NarrowMemoryCase,
    ordinal: usize,
    address: u64,
    writemask: u64,
) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64
                    .rotate_left(((register * 11 + word * 17 + ordinal) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x8040_2010_0804_0201)
                    ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
            })
        }),
        masks: std::array::from_fn(|index| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((index * 7 + ordinal) as u32)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x195)) & 0x8D5),
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
    };
    state.gpr[2] = address;
    seed_source_lanes(case, ordinal, &mut state.vectors[usize::from(case.source)]);
    if let Some(mask) = case.writemask {
        state.masks[usize::from(mask)] = writemask;
    }
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|index| {
        (index as u8)
            .wrapping_mul(0x3D)
            .wrapping_add((ordinal as u8).wrapping_mul(0x17))
            .wrapping_add(0x29)
    })
}

pub(super) fn narrowed_lane_value(
    case: NarrowMemoryCase,
    initial: &SemanticState,
    lane: usize,
) -> u64 {
    let source = vector_bytes(&initial.vectors[usize::from(case.source)]);
    let src_bytes = case.operation.src_elem.bytes() as usize;
    let offset = lane * src_bytes;
    let mut raw_bytes = [0u8; 8];
    raw_bytes[..src_bytes].copy_from_slice(&source[offset..offset + src_bytes]);
    let raw = u64::from_le_bytes(raw_bytes);
    let dst_bits = case.operation.dst_elem.bytes() * 8;
    let dst_mask = width_mask(dst_bits);
    match case.operation.mode {
        X86NarrowMode::Truncate => raw & dst_mask,
        X86NarrowMode::UnsignedSaturate => raw.min(dst_mask),
        X86NarrowMode::SignedSaturate => {
            let signed = match src_bytes {
                2 => i128::from(raw as u16 as i16),
                4 => i128::from(raw as u32 as i32),
                8 => i128::from(raw as i64),
                _ => unreachable!(),
            };
            let low = -(1i128 << (dst_bits - 1));
            let high = (1i128 << (dst_bits - 1)) - 1;
            signed.clamp(low, high) as u64 & dst_mask
        }
    }
}

pub(super) fn manual(
    case: NarrowMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticOutcome {
    let mut result_memory = *memory;
    let control = case.writemask.map_or_else(
        || lane_mask(case.lanes()),
        |mask| initial.masks[usize::from(mask)] & lane_mask(case.lanes()),
    );
    for lane in 0..case.lanes() {
        if control & (1u64 << lane) == 0 {
            continue;
        }
        let value = narrowed_lane_value(case, initial, lane);
        let offset = lane * case.lane_bytes();
        result_memory[offset..offset + case.lane_bytes()]
            .copy_from_slice(&value.to_le_bytes()[..case.lane_bytes()]);
    }
    SemanticOutcome {
        state: initial.clone(),
        memory: result_memory,
    }
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
    memory: &mut FlatMemory,
) -> (BlockResult, SemanticState) {
    let mut context = context(initial);
    let result = SmirInterpreter::new().execute_block(&mut context, memory, &function.blocks[0]);
    (result, state(&context))
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
) -> SemanticOutcome {
    let mut memory = FlatMemory::new(0x5000);
    memory.load(MEMORY_ADDRESS as usize, bytes);
    let (result, state) = execute(function, initial, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    let mut observed = [0u8; 64];
    memory.read(MEMORY_ADDRESS, &mut observed).unwrap();
    SemanticOutcome {
        state,
        memory: observed,
    }
}

#[test]
fn all_108_integer_narrow_cells_match_manual_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let mut mask =
            0xA55A_6996_F00F_3CC3u64.rotate_left((ordinal & 63) as u32) & lane_mask(case.lanes());
        if mask == 0 {
            mask = 1;
        }
        let initial = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
        let memory = memory_bytes(ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret_success(&function, &initial, &memory);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 108 * LEVELS.len());
}

#[test]
fn every_k1_to_k7_observes_exactly_the_architectural_narrow_lane_bits() {
    let mut comparisons = 0usize;
    for operation in NarrowOperation::all() {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for mask in 1..=7u8 {
                let case = NarrowMemoryCase {
                    operation,
                    width,
                    source: 16 + mask,
                    writemask: Some(mask),
                };
                let lanes = case.lanes();
                let active = 0xD6A5_3C69_F00F_5AA5u64
                    .rotate_left(u32::from(mask * 7) + u32::from(case.ll()))
                    & lane_mask(lanes);
                let initial = initial_state(
                    case,
                    usize::from(mask),
                    MEMORY_ADDRESS,
                    active | !lane_mask(lanes),
                );
                let memory = memory_bytes(usize::from(mask));
                let function = optimize(lift_case(case), OptLevel::O2);
                assert_eq!(
                    interpret_success(&function, &initial, &memory),
                    manual(case, &initial, &memory),
                    "{case:?}"
                );
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 18 * 3 * 7);
}

#[test]
fn empty_and_out_of_range_narrow_masks_suppress_every_memory_access() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.writemask.is_some())
        .collect();
    assert_eq!(cases.len(), 54);
    let mut empty_suppressions = 0usize;
    let mut high_suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS, 0);
            let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS, 0);
            let (result, actual) = execute(&function, &initial, &mut unmapped);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: empty mask");
            empty_suppressions += 1;

            let initial = initial_state(
                case,
                ordinal ^ 0x55,
                MEMORY_ADDRESS,
                !lane_mask(case.lanes()),
            );
            let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS, 0);
            let (result, actual) = execute(&function, &initial, &mut unmapped);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: high-only mask");
            high_suppressions += 1;
        }
    }
    assert_eq!(empty_suppressions, 54 * LEVELS.len());
    assert_eq!(high_suppressions, 54 * LEVELS.len());
}

#[test]
fn later_fixed_lane_fault_preserves_prior_writes_and_all_architectural_state() {
    let operations = [
        NarrowOperation {
            src_elem: VecElementType::I64,
            dst_elem: VecElementType::I32,
            mode: X86NarrowMode::Truncate,
        },
        NarrowOperation {
            src_elem: VecElementType::I32,
            dst_elem: VecElementType::I8,
            mode: X86NarrowMode::SignedSaturate,
        },
        NarrowOperation {
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            mode: X86NarrowMode::UnsignedSaturate,
        },
    ];
    for (ordinal, operation) in operations.into_iter().enumerate() {
        let case = NarrowMemoryCase {
            operation,
            width: VecWidth::V512,
            source: 17,
            writemask: (ordinal != 0).then_some(3),
        };
        let control = if case.writemask.is_some() {
            1 | (1u64 << (case.lanes() - 1))
        } else {
            u64::MAX
        };
        let source = initial_state(case, 114 + ordinal, MEMORY_ADDRESS, control);
        let first = narrowed_lane_value(case, &source, 0);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut partial = FlatMemory::new((MEMORY_ADDRESS + case.lane_bytes() as u64) as usize);
            partial.load(
                MEMORY_ADDRESS as usize,
                &memory_bytes(114 + ordinal)[..case.lane_bytes()],
            );
            let (result, actual) = execute(&function, &source, &mut partial);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, source, "{level:?} {case:?}: register state");
            let mut committed = vec![0u8; case.lane_bytes()];
            partial.read(MEMORY_ADDRESS, &mut committed).unwrap();
            assert_eq!(
                committed,
                first.to_le_bytes()[..case.lane_bytes()],
                "{level:?} {case:?}: first write"
            );
        }
    }
}
