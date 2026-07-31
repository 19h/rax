//! Raw-bit interpreter, optimizer, and Type E4NF.nb fault coverage.

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

fn element_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I8 => 0xFF,
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        _ => unreachable!("EVEX AVX-512BW result element"),
    }
}

fn get_element(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    let bits = elem.bytes() as usize * 8;
    (vector[lane * bits / 64] >> (lane * bits % 64)) & element_mask(elem)
}

fn set_element(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let bits = elem.bytes() as usize * 8;
    let word = &mut vector[lane * bits / 64];
    let shift = lane * bits % 64;
    let mask = element_mask(elem);
    *word = (*word & !(mask << shift)) | ((value & mask) << shift);
}

pub(super) fn initial_state(case: BwMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA55A_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 5) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x8000_7FFF_FF00_01FEu64.rotate_left((register * 13 + word * 17 + ordinal) as u32)
                    ^ ((register as u64) << 57)
                    ^ (word as u64).wrapping_mul(0x8102_0408_1020_4081)
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
    state.gpr[3] = 0x2000;
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|byte| {
        (byte as u8)
            .wrapping_mul(29)
            .wrapping_add((ordinal as u8).rotate_left((byte & 7) as u32))
            ^ if byte % 5 == 0 { 0x80 } else { 0x35 }
    })
}

fn memory_element(memory: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    let bytes = elem.bytes() as usize;
    let start = lane * bytes;
    let mut value = 0u64;
    for byte in 0..bytes {
        value |= u64::from(memory[start + byte]) << (byte * 8);
    }
    value
}

fn raw_result(case: BwMemoryCase, source: &[u64; 16], memory: &[u8; 64], lane: usize) -> u64 {
    match case.kind {
        Kind::ByteShuffle => {
            let control = memory[lane];
            if control & 0x80 != 0 {
                0
            } else {
                let source_lane = (lane & !15) | usize::from(control & 15);
                get_element(source, source_lane, VecElementType::I8)
            }
        }
        Kind::MultiplyAddUnsignedBytes => {
            let source0 = get_element(source, lane * 2, VecElementType::I8) as i32;
            let source1 = get_element(source, lane * 2 + 1, VecElementType::I8) as i32;
            let memory0 = memory[lane * 2] as i8 as i32;
            let memory1 = memory[lane * 2 + 1] as i8 as i32;
            let sum = source0 * memory0 + source1 * memory1;
            sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16 as u64
        }
        Kind::MultiplyAddWords => {
            let source0 = get_element(source, lane * 2, VecElementType::I16) as u16 as i16 as i32;
            let source1 =
                get_element(source, lane * 2 + 1, VecElementType::I16) as u16 as i16 as i32;
            let memory0 =
                memory_element(memory, lane * 2, VecElementType::I16) as u16 as i16 as i32;
            let memory1 =
                memory_element(memory, lane * 2 + 1, VecElementType::I16) as u16 as i16 as i32;
            source0
                .wrapping_mul(memory0)
                .wrapping_add(source1.wrapping_mul(memory1)) as u32 as u64
        }
    }
}

fn manual(case: BwMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let result_elem = case.kind.result_elem();
    let lanes = case.width.lanes(result_elem) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let old_destination = initial.vectors[usize::from(case.destination)];
    let source = initial.vectors[usize::from(case.source1)];
    let destination = &mut expected.vectors[usize::from(case.destination)];

    for lane in 0..lanes {
        let value = if mask & (1u64 << lane) != 0 {
            raw_result(case, &source, memory, lane)
        } else if case.zeroing() {
            0
        } else {
            get_element(&old_destination, lane, result_elem)
        };
        set_element(destination, lane, result_elem, value);
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
    case: BwMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let tuple_bytes = case.width.bytes() as usize;
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

fn semantic_cases() -> Vec<BwMemoryCase> {
    let mut cases = Vec::new();
    for kind in Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (destination, source1) in [(0, 1), (9, 9), (17, 18), (31, 31)] {
                for control in MaskControl::ALL {
                    for w in [false, true] {
                        cases.push(BwMemoryCase {
                            kind,
                            width,
                            destination,
                            source1,
                            control,
                            w,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn all_216_raw_bit_cases_match_manual_lane_semantics_at_all_levels() {
    let cases = semantic_cases();
    assert_eq!(cases.len(), 216);
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
    assert_eq!(comparisons, 216 * LEVELS.len());
}

#[test]
fn empty_masks_still_fault_before_any_architectural_commit() {
    let mut faults = 0usize;
    for kind in Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                for w in [false, true] {
                    let case = BwMemoryCase {
                        kind,
                        width,
                        destination: 17,
                        source1: 18,
                        control,
                        w,
                    };
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = optimize(lift_case(case), level);
                        assert_eq!(
                            function.blocks[0]
                                .ops
                                .iter()
                                .filter(|op| matches!(op.kind, OpKind::VLoad { .. }))
                                .count(),
                            1
                        );
                        assert!(
                            !function.blocks[0]
                                .ops
                                .iter()
                                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                        );
                        let mut initial = initial_state(case, faults);
                        initial.masks[usize::from(case.mask())] = 0;
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
    }
    assert_eq!(faults, 72);
}

#[test]
fn full_mem_tuple_faults_when_only_width_minus_one_bytes_are_mapped() {
    let mut faults = 0usize;
    for kind in Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for w in [false, true] {
                let case = BwMemoryCase {
                    kind,
                    width,
                    destination: 25,
                    source1: 26,
                    control: MaskControl::None,
                    w,
                };
                for level in [OptLevel::O0, OptLevel::O2] {
                    let function = optimize(lift_case(case), level);
                    let initial = initial_state(case, faults);
                    let bytes = memory_bytes(faults);
                    let mapped = width.bytes() as usize - 1;
                    let mut partial = FlatMemory::with_base(0x2000, mapped);
                    partial.load(0, &bytes[..mapped]);
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
                    assert_eq!(
                        state(&fault_context),
                        initial,
                        "{level:?} {case:?}: partial tuple committed state"
                    );
                    faults += 1;
                }
            }
        }
    }
    assert_eq!(faults, 36);
}
