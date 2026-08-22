//! Interpreter, optimizer, classification, masking, DAZ, and fault semantics.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

fn element_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::F16 => 0xFFFF,
        VecElementType::F32 => 0xFFFF_FFFF,
        VecElementType::F64 => u64::MAX,
        _ => unreachable!("VFPCLASS element"),
    }
}

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    let bytes = elem.bytes() as usize;
    let word = lane * bytes / 8;
    let shift = (lane * bytes % 8) * 8;
    let mask = element_mask(elem);
    words[word] = (words[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn get_lane(words: &[u64; 8], elem: VecElementType, lane: usize) -> u64 {
    let bytes = elem.bytes() as usize;
    let word = lane * bytes / 8;
    let shift = (lane * bytes % 8) * 8;
    (words[word] >> shift) & element_mask(elem)
}

fn patterns(elem: VecElementType) -> &'static [u64] {
    const F16: [u64; 12] = [
        0x7E01, // quiet NaN
        0x0000, // +0
        0x8000, // -0
        0x7C00, // +infinity
        0xFC00, // -infinity
        0x0001, // positive denormal
        0xBC00, // negative finite
        0x7D01, // signaling NaN
        0x3C00, // positive finite (unselected)
        0x8001, // negative denormal
        0xFE01, // negative quiet NaN
        0xFD01, // negative signaling NaN
    ];
    const F32: [u64; 12] = [
        0x7FC0_0001,
        0x0000_0000,
        0x8000_0000,
        0x7F80_0000,
        0xFF80_0000,
        0x0000_0001,
        0xBF80_0000,
        0x7F80_0001,
        0x3F80_0000,
        0x8000_0001,
        0xFFC0_0001,
        0xFF80_0001,
    ];
    const F64: [u64; 12] = [
        0x7FF8_0000_0000_0001,
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x0000_0000_0000_0001,
        0xBFF0_0000_0000_0000,
        0x7FF0_0000_0000_0001,
        0x3FF0_0000_0000_0000,
        0x8000_0000_0000_0001,
        0xFFF8_0000_0000_0001,
        0xFFF0_0000_0000_0001,
    ];
    match elem {
        VecElementType::F16 => &F16,
        VecElementType::F32 => &F32,
        VecElementType::F64 => &F64,
        _ => unreachable!("VFPCLASS element"),
    }
}

pub(super) fn memory_value(case: FpClassMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = [0u64; 8];
    let source = patterns(case.elem);
    let lanes = if case.scalar() || case.broadcast() {
        1
    } else {
        case.width.lanes(case.elem) as usize
    };
    for lane in 0..lanes {
        set_lane(
            &mut value,
            case.elem,
            lane,
            source[(lane * 5 + ordinal) % source.len()],
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

pub(super) fn initial_registers(case: FpClassMemoryCase, ordinal: usize, daz: bool) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            [
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 3) as u32),
                0xFEDC_BA98_7654_3210u64.rotate_right((register * 5) as u32),
                register as u64,
                !(register as u64),
                0x5555_5555_5555_5555,
                0xAAAA_AAAA_AAAA_AAAA,
                0xDEAD_BEEF_CAFE_BABE,
                0x1020_4081_0204_0810,
            ]
        }),
        k: std::array::from_fn(|index| {
            if index == 1 {
                0xA5A5_A5A5_A5A5_A5A5
            } else {
                0xF0F0_0000_0000_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        // Retain pre-existing status flags and a non-default rounding field;
        // VFPCLASS must change neither. DAZ is bit 6.
        mxcsr: 0x1F80
            | ((ordinal as u32) & 0x3F)
            | (((ordinal as u32) & 3) << 13)
            | (u32::from(daz) << 6),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        vector_active: X86_VECTOR_STATE_K64,
        ..GuestRegs::default()
    };
    registers.gpr[2] = MEMORY_ADDRESS;
    registers.k[usize::from(case.destination)] ^= (ordinal as u64) << 17;
    registers
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
    case: FpClassMemoryCase,
) -> GuestRegs {
    let mut context = context_from(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = memory_bytes(value);
    memory.load(
        MEMORY_ADDRESS as usize,
        &bytes[..case.memory_size() as usize],
    );
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

fn class_selector(mut value: u64, elem: VecElementType, daz: bool) -> u8 {
    let (sign, exponent, mantissa, quiet) = match elem {
        VecElementType::F16 => (0x8000, 0x7C00, 0x03FF, 0x0200),
        VecElementType::F32 => (0x8000_0000, 0x7F80_0000, 0x007F_FFFF, 0x0040_0000),
        VecElementType::F64 => (
            0x8000_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x000F_FFFF_FFFF_FFFF,
            0x0008_0000_0000_0000,
        ),
        _ => unreachable!("VFPCLASS element"),
    };
    value &= element_mask(elem);
    if elem != VecElementType::F16 && daz && value & exponent == 0 && value & mantissa != 0 {
        value &= sign;
    }
    let negative = value & sign != 0;
    let exponent_value = value & exponent;
    let mantissa_value = value & mantissa;
    let zero = exponent_value == 0 && mantissa_value == 0;
    let mut classes = 0u8;
    if exponent_value == exponent {
        if mantissa_value == 0 {
            classes |= if negative { 1 << 4 } else { 1 << 3 };
        } else if value & quiet != 0 {
            classes |= 1 << 0;
        } else {
            classes |= 1 << 7;
        }
    } else if exponent_value == 0 {
        if mantissa_value == 0 {
            classes |= if negative { 1 << 2 } else { 1 << 1 };
        } else {
            classes |= 1 << 5;
        }
    }
    // Intel defines "negative finite" independently from the denormal
    // category: a negative denormal therefore selects both imm8[5] and
    // imm8[6]. DAZ converts binary32/binary64 denormals to signed zero before
    // this predicate; FP16 classification is always raw.
    if negative && exponent_value != exponent && !zero {
        classes |= 1 << 6;
    }
    classes
}

pub(super) fn expected_mask(
    case: FpClassMemoryCase,
    initial: &GuestRegs,
    memory: &[u64; 8],
) -> u64 {
    let lanes = if case.scalar() {
        1
    } else {
        case.width.lanes(case.elem) as usize
    };
    let active = if case.mask == 0 {
        u64::MAX
    } else {
        initial.k[usize::from(case.mask)]
    };
    let daz = initial.mxcsr & (1 << 6) != 0;
    let mut result = 0u64;
    for lane in 0..lanes {
        if active & (1u64 << lane) == 0 {
            continue;
        }
        let source_lane = if case.scalar() || case.broadcast() {
            0
        } else {
            lane
        };
        let value = get_lane(memory, case.elem, source_lane);
        let selected = class_selector(value, case.elem, daz);
        result |= u64::from(case.immediate & selected != 0) << lane;
    }
    result
}

fn semantic_cases() -> Vec<FpClassMemoryCase> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for width in [VecWidth::V128, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for mask in [0, 1] {
                    for immediate in [0, 1, 2, 4, 8, 16, 32, 64, 128, 0xFF] {
                        cases.push(FpClassMemoryCase {
                            elem,
                            width,
                            destination: 7,
                            form,
                            mask,
                            immediate,
                        });
                    }
                }
            }
        }
        for ll in [0, 3] {
            for mask in [0, 1] {
                for immediate in [0, 1, 2, 4, 8, 16, 32, 64, 128, 0xFF] {
                    cases.push(FpClassMemoryCase {
                        elem,
                        width: VecWidth::V128,
                        destination: 7,
                        form: SourceForm::Scalar { ll },
                        mask,
                        immediate,
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), 360);
    cases
}

#[test]
fn independent_oracle_matches_primary_class_matrix_and_daz_transform() {
    let raw = [1, 2, 4, 8, 16, 32, 64, 128, 0, 32 | 64, 1, 128];
    let daz = [1, 2, 4, 8, 16, 2, 64, 128, 0, 4, 1, 128];
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        assert_eq!(
            patterns(elem)
                .iter()
                .map(|value| class_selector(*value, elem, false))
                .collect::<Vec<_>>(),
            raw,
            "{elem:?} raw classifier"
        );
        assert_eq!(
            patterns(elem)
                .iter()
                .map(|value| class_selector(*value, elem, true))
                .collect::<Vec<_>>(),
            if elem == VecElementType::F16 {
                raw.to_vec()
            } else {
                daz.to_vec()
            },
            "{elem:?} DAZ classifier"
        );
    }
}

#[test]
fn all_360_semantic_shapes_match_independent_oracle_with_and_without_daz() {
    let mut comparisons = 0usize;
    for (ordinal, case) in semantic_cases().into_iter().enumerate() {
        let value = memory_value(case, ordinal);
        for daz in [false, true] {
            let initial = initial_registers(case, ordinal, daz);
            let mut expected = initial;
            expected.k[usize::from(case.destination)] = expected_mask(case, &initial, &value);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let actual = interpreter_success(&function, &initial, value, case);
                assert_eq!(actual, expected, "{level:?} {case:?} DAZ={daz}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 360 * 2 * LEVELS.len());
}

#[test]
fn destination_mask_alias_reads_old_k_and_scalar_observes_only_bit_zero() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for scalar in [false, true] {
            let case = FpClassMemoryCase {
                elem,
                width: VecWidth::V128,
                destination: 1,
                form: if scalar {
                    SourceForm::Scalar { ll: 3 }
                } else {
                    SourceForm::Broadcast
                },
                mask: 1,
                immediate: 0xFF,
            };
            let value = memory_value(case, 0);
            for old_mask in [0, 1, 2, 0xFFFF_FFFF_FFFF_FFFF] {
                let mut initial = initial_registers(case, usize::from(scalar), false);
                initial.k[1] = old_mask;
                let actual = interpreter_success(&lift_case(case), &initial, value, case);
                assert_eq!(actual.k[1], expected_mask(case, &initial, &value));
                let lane_mask = if scalar {
                    1
                } else {
                    (1u64 << case.width.lanes(case.elem)) - 1
                };
                assert_eq!(actual.k[1] & !lane_mask, 0, "{case:?} old={old_mask:#x}");
            }
        }
    }
}

#[test]
fn constant_false_classification_retains_precise_faults_and_mask_suppression() {
    let mut checked = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for form in [
            SourceForm::Vector,
            SourceForm::Broadcast,
            SourceForm::Scalar { ll: 3 },
        ] {
            let case = FpClassMemoryCase {
                elem,
                width: VecWidth::V512,
                destination: 7,
                form,
                mask: 1,
                immediate: 0,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);

                let mut active = initial_registers(case, checked, true);
                active.gpr[2] = 0x20_000;
                active.k[1] = 1;
                active.k[7] = 0x0123_4567_89AB_CDEF;
                let mut context = context_from(&active);
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
                assert_eq!(x86.k[7], active.k[7], "{level:?} {case:?}");
                assert_eq!(x86.rflags, active.rflags, "{level:?} {case:?}");
                assert_eq!(x86.mxcsr, active.mxcsr, "{level:?} {case:?}");

                let mut inactive = active;
                inactive.k[1] = if case.scalar() { 2 } else { 0 };
                let mut context = context_from(&inactive);
                let mut memory = FlatMemory::new(0x1000);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    &function.blocks[0],
                );
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                assert_eq!(x86.k[7], 0, "{level:?} {case:?}");
                assert_eq!(x86.rflags, inactive.rflags);
                assert_eq!(x86.mxcsr, inactive.mxcsr);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 3 * LEVELS.len());
}

#[test]
fn later_active_lane_faults_do_not_commit_after_earlier_successful_loads() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        let case = FpClassMemoryCase {
            elem,
            width: VecWidth::V512,
            destination: 7,
            form: SourceForm::Vector,
            mask: 1,
            immediate: 0xFF,
        };
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut initial = initial_registers(case, elem.bytes() as usize, false);
            let lane_bytes = u64::from(elem.bytes());
            initial.gpr[2] = 0x1000 - 2 * lane_bytes;
            initial.k[1] = 0b0101;
            initial.k[7] = 0xA55A_3CC3_F00F_9696;
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
                    BlockResult::Exit(ExitReason::MemoryFault { addr, .. })
                        if addr == 0x1000
                ),
                "{level:?} {case:?}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.k[7], initial.k[7], "{level:?} {case:?}");
            assert_eq!(x86.rflags, initial.rflags);
            assert_eq!(x86.mxcsr, initial.mxcsr);
        }
    }
}
