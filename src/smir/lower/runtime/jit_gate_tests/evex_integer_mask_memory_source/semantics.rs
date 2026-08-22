//! Interpreter, optimizer, masking, integer-truth-table, and fault semantics.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    let bytes = elem.bytes() as usize;
    let word = lane * bytes / 8;
    let shift = (lane * bytes % 8) * 8;
    let mask = if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    };
    words[word] = (words[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn get_lane(words: &[u64; 8], elem: VecElementType, lane: usize) -> u64 {
    let bytes = elem.bytes() as usize;
    let word = lane * bytes / 8;
    let shift = (lane * bytes % 8) * 8;
    let mask = if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    };
    (words[word] >> shift) & mask
}

fn patterns(elem: VecElementType) -> &'static [u64] {
    const I8: [u64; 10] = [0, 1, 0x7E, 0x7F, 0x80, 0x81, 0xFE, 0xFF, 0x55, 0xAA];
    const I16: [u64; 10] = [
        0, 1, 0x7FFE, 0x7FFF, 0x8000, 0x8001, 0xFFFE, 0xFFFF, 0x5555, 0xAAAA,
    ];
    const I32: [u64; 10] = [
        0,
        1,
        0x7FFF_FFFE,
        0x7FFF_FFFF,
        0x8000_0000,
        0x8000_0001,
        0xFFFF_FFFE,
        0xFFFF_FFFF,
        0x5555_5555,
        0xAAAA_AAAA,
    ];
    const I64: [u64; 10] = [
        0,
        1,
        0x7FFF_FFFF_FFFF_FFFE,
        0x7FFF_FFFF_FFFF_FFFF,
        0x8000_0000_0000_0000,
        0x8000_0000_0000_0001,
        0xFFFF_FFFF_FFFF_FFFE,
        0xFFFF_FFFF_FFFF_FFFF,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
    ];
    match elem {
        VecElementType::I8 => &I8,
        VecElementType::I16 => &I16,
        VecElementType::I32 => &I32,
        VecElementType::I64 => &I64,
        _ => unreachable!("packed integer mask element"),
    }
}

pub(super) fn initial_registers(case: IntegerMaskMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            let mut value = [0u64; 8];
            let source = patterns(case.kind.elem);
            for lane in 0..case.width.lanes(case.kind.elem) as usize {
                set_lane(
                    &mut value,
                    case.kind.elem,
                    lane,
                    source[(lane + register * 3 + ordinal) % source.len()],
                );
            }
            value
        }),
        k: std::array::from_fn(|index| {
            if index == 1 {
                0xA5A5_A5A5_A5A5_A5A5
            } else {
                0xF0F0_0000_0000_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        mxcsr: 0x1F80 | (((ordinal & 3) as u32) << 13),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        vector_active: X86_VECTOR_STATE_K64,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;
    registers
}

pub(super) fn memory_value(case: IntegerMaskMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = [0u64; 8];
    let source = patterns(case.kind.elem);
    for lane in 0..case.width.lanes(case.kind.elem) as usize {
        set_lane(
            &mut value,
            case.kind.elem,
            lane,
            source[(lane * 5 + ordinal + 1) % source.len()],
        );
    }
    value
}

pub(super) fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn context_from(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    value: [u64; 8],
    case: IntegerMaskMemoryCase,
) -> GuestRegs {
    let mut context = context_from(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = memory_bytes(value);
    memory.load(0x2000, &bytes[..case.memory_size() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut actual = *initial;
    actual.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        actual.zmm[index].copy_from_slice(&value[..8]);
    }
    actual.k = x86.k;
    actual.rflags = x86.rflags;
    actual.mxcsr = x86.mxcsr;
    actual
}

fn signed(value: u64, elem: VecElementType) -> i64 {
    let shift = 64 - elem.bytes() * 8;
    ((value << shift) as i64) >> shift
}

fn lane_result(case: IntegerMaskMemoryCase, source1: u64, source2: u64) -> bool {
    match case.kind.semantic {
        IntegerMaskSemantic::FixedCompare(VecCmpCond::Eq) => source1 == source2,
        IntegerMaskSemantic::FixedCompare(VecCmpCond::Gt) => {
            signed(source1, case.kind.elem) > signed(source2, case.kind.elem)
        }
        IntegerMaskSemantic::FixedCompare(other) => {
            unreachable!("fixed integer comparison condition {other:?}")
        }
        IntegerMaskSemantic::ImmediateCompare {
            signed: signed_comparison,
        } => {
            let equal = source1 == source2;
            let less = if signed_comparison {
                signed(source1, case.kind.elem) < signed(source2, case.kind.elem)
            } else {
                source1 < source2
            };
            match case.immediate & 7 {
                0 => equal,
                1 => less,
                2 => less || equal,
                3 => false,
                4 => !equal,
                5 => !less,
                6 => !less && !equal,
                7 => true,
                _ => unreachable!(),
            }
        }
        IntegerMaskSemantic::Test { inverted } => ((source1 & source2) == 0) == inverted,
    }
}

fn expected_mask(case: IntegerMaskMemoryCase, initial: &GuestRegs, memory: &[u64; 8]) -> u64 {
    let lanes = case.width.lanes(case.kind.elem) as usize;
    let active = if case.mask == 0 {
        u64::MAX
    } else {
        initial.k[usize::from(case.mask)]
    };
    let mut result = 0u64;
    for lane in 0..lanes {
        if active & (1u64 << lane) == 0 {
            continue;
        }
        let source1 = get_lane(
            &initial.zmm[usize::from(case.source1)],
            case.kind.elem,
            lane,
        );
        let source2 = get_lane(
            memory,
            case.kind.elem,
            if case.broadcast() { 0 } else { lane },
        );
        result |= u64::from(lane_result(case, source1, source2)) << lane;
    }
    result
}

#[test]
fn all_480_semantic_shapes_match_independent_oracle_at_o0_o1_o2() {
    let mut comparisons = 0usize;
    for (ordinal, case) in semantic_cases().into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        let value = memory_value(case, ordinal);
        let mut expected = initial;
        expected.k[usize::from(case.destination)] = expected_mask(case, &initial, &value);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, value, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 480 * LEVELS.len());
}

#[test]
fn destination_writemask_alias_reads_old_k_and_clears_every_high_result_bit() {
    let kind = KINDS
        .into_iter()
        .find(|kind| {
            matches!(
                kind.semantic,
                IntegerMaskSemantic::ImmediateCompare { signed: false }
            ) && kind.elem == VecElementType::I32
        })
        .unwrap();
    for immediate in [0, 3, 7] {
        let case = IntegerMaskMemoryCase {
            kind,
            width: VecWidth::V128,
            destination: 1,
            source1: 17,
            w: kind.w_values()[0],
            form: SourceForm::Broadcast,
            mask: 1,
            immediate,
        };
        let mut initial = initial_registers(case, usize::from(immediate));
        initial.k[1] = 0b1101 | (u64::MAX << 8);
        let memory = memory_value(case, usize::from(immediate));
        let expected = expected_mask(case, &initial, &memory);
        let actual = interpreter_success(&lift_case(case), &initial, memory, case);
        assert_eq!(actual.k[1], expected, "{case:?}");
        assert_eq!(actual.k[1] >> 4, 0, "{case:?}");
    }
}

#[test]
fn constant_predicates_retain_memory_faults_and_mask_suppression_at_all_levels() {
    let mut checked = 0usize;
    for kind in KINDS.into_iter().filter(|kind| kind.has_immediate()) {
        for immediate in [3, 7] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                if form == SourceForm::Broadcast && !kind.permits_broadcast() {
                    continue;
                }
                for mask in [0, 1] {
                    let case = IntegerMaskMemoryCase {
                        kind,
                        width: VecWidth::V512,
                        destination: 7,
                        source1: 17,
                        w: kind.w_values()[0],
                        form,
                        mask,
                        immediate,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let mut initial = initial_registers(case, checked);
                        initial.gpr[3] = 0x20_000;
                        initial.k[7] = 0x0123_4567_89AB_CDEF;
                        if mask != 0 {
                            initial.k[1] = 0;
                        }
                        let mut context = context_from(&initial);
                        let mut memory = FlatMemory::new(0x1000);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut memory,
                            &function.blocks[0],
                        );
                        if mask == 0 {
                            assert!(
                                matches!(
                                    result,
                                    BlockResult::Exit(ExitReason::MemoryFault {
                                        addr: 0x20_000,
                                        write: false,
                                        ..
                                    })
                                ),
                                "{level:?} {case:?}: {result:?}"
                            );
                            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                                unreachable!()
                            };
                            assert_eq!(x86.k[7], initial.k[7], "{level:?} {case:?}");
                        } else {
                            assert!(matches!(
                                result,
                                BlockResult::Exit(ExitReason::Return { .. })
                            ));
                            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                                unreachable!()
                            };
                            assert_eq!(x86.k[7], 0, "{level:?} {case:?}");
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 48 * LEVELS.len());
}

#[test]
fn active_lane_faults_are_precise_and_noncommitting_for_all_elements_and_operations() {
    for elem in [
        VecElementType::I8,
        VecElementType::I16,
        VecElementType::I32,
        VecElementType::I64,
    ] {
        for semantic_class in 0..2 {
            let kind = KINDS
                .into_iter()
                .find(|kind| {
                    kind.elem == elem
                        && match semantic_class {
                            0 => matches!(kind.semantic, IntegerMaskSemantic::FixedCompare(_)),
                            1 => matches!(kind.semantic, IntegerMaskSemantic::Test { .. }),
                            _ => unreachable!(),
                        }
                })
                .unwrap();
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                if form == SourceForm::Broadcast && !kind.permits_broadcast() {
                    continue;
                }
                let case = IntegerMaskMemoryCase {
                    kind,
                    width: VecWidth::V512,
                    destination: 7,
                    source1: 17,
                    w: kind.w_values()[0],
                    form,
                    mask: 1,
                    immediate: 0,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_registers(case, semantic_class);
                    initial.gpr[3] = 0x20_000;
                    initial.k[1] = 1;
                    initial.k[7] = 0x0123_4567_89AB_CDEF;
                    let mut context = context_from(&initial);
                    let mut memory = FlatMemory::new(0x1000);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault {
                                addr: 0x20_000,
                                write: false,
                                ..
                            })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(x86.k[7], initial.k[7], "{level:?} {case:?}");
                    assert_eq!(x86.rflags, initial.rflags, "{level:?} {case:?}");
                    assert_eq!(x86.mxcsr, initial.mxcsr, "{level:?} {case:?}");
                }
            }
        }
    }
}

#[test]
fn raw_lane_helpers_cover_every_integer_boundary_without_cross_lane_aliasing() {
    for elem in [
        VecElementType::I8,
        VecElementType::I16,
        VecElementType::I32,
        VecElementType::I64,
    ] {
        let mut words = [u64::MAX; 8];
        let lanes = VecWidth::V512.lanes(elem) as usize;
        for lane in 0..lanes {
            set_lane(&mut words, elem, lane, lane as u64 + 1);
        }
        for lane in 0..lanes {
            assert_eq!(get_lane(&words, elem, lane), lane as u64 + 1);
        }
    }
}
