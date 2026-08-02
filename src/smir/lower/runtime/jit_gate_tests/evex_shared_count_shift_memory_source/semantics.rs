//! Interpreter, optimizer, Mem128, and E4NF fault coverage.

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

pub(super) fn initial_state(case: ShiftMemoryCase, ordinal: usize) -> SemanticState {
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
            0xA55A_3CC3_F00F_9696,
            0x5AA5_C33C_0FF0_6969,
            [0, u64::MAX, 0xA55A_3CC3_F00F_9696, 0x8000_0000_0000_0001][ordinal & 3],
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[0] = 0x1000;
    state.gpr[1] = (0x2000 - state.gpr[0] - case.compressed_displacement() as u64) / 2;
    if case.source == case.destination {
        state.vectors[usize::from(case.destination)] =
            std::array::from_fn(|word| 0xFFFF_0001_8000_7FFFu64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(count: u64, upper_qword: u64) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&count.to_le_bytes());
    bytes[8..].copy_from_slice(&upper_qword.to_le_bytes());
    bytes
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I16 => (vector[lane / 4] >> ((lane & 3) * 16)) & 0xFFFF,
        VecElementType::I32 => (vector[lane / 2] >> ((lane & 1) * 32)) & 0xFFFF_FFFF,
        VecElementType::I64 => vector[lane],
        _ => unreachable!("packed shared-count integer element"),
    }
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let (word, shift, mask) = match elem {
        VecElementType::I16 => (lane / 4, (lane & 3) * 16, 0xFFFFu64),
        VecElementType::I32 => (lane / 2, (lane & 1) * 32, 0xFFFF_FFFFu64),
        VecElementType::I64 => (lane, 0, u64::MAX),
        _ => unreachable!("packed shared-count integer element"),
    };
    vector[word] = (vector[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn shifted(value: u64, count: u64, elem: VecElementType, shift: ShiftOp) -> u64 {
    let bits = u64::from(elem.bytes()) * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let value = value & mask;
    if count >= bits {
        return match shift {
            ShiftOp::Lsl | ShiftOp::Lsr => 0,
            ShiftOp::Asr => {
                if value & (1u64 << (bits - 1)) != 0 {
                    mask
                } else {
                    0
                }
            }
            _ => unreachable!("packed shared-count shift operation"),
        };
    }
    let count = count as u32;
    match (elem, shift) {
        (_, ShiftOp::Lsl) => (value << count) & mask,
        (_, ShiftOp::Lsr) => value >> count,
        (VecElementType::I16, ShiftOp::Asr) => ((value as u16 as i16) >> count) as u16 as u64,
        (VecElementType::I32, ShiftOp::Asr) => ((value as u32 as i32) >> count) as u32 as u64,
        (VecElementType::I64, ShiftOp::Asr) => ((value as i64) >> count) as u64,
        _ => unreachable!("packed shared-count shift operation"),
    }
}

fn manual(case: ShiftMemoryCase, initial: &SemanticState, count: u64) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.kind.elem) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let source = initial.vectors[usize::from(case.source)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        if mask & (1u64 << lane) == 0 {
            set_lane(
                destination,
                lane,
                case.kind.elem,
                if case.zeroing() {
                    0
                } else {
                    get_lane(&old_destination, lane, case.kind.elem)
                },
            );
            continue;
        }
        let value = get_lane(&source, lane, case.kind.elem);
        set_lane(
            destination,
            lane,
            case.kind.elem,
            shifted(value, count, case.kind.elem, case.kind.shift),
        );
    }
    for word in usize::try_from(case.width.bytes() / 8).unwrap()..destination.len() {
        destination[word] = 0;
    }
    expected
}

fn context_from_state(initial: &SemanticState) -> SmirContext {
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

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    memory: &[u8; 16],
) -> SemanticState {
    let mut context = context_from_state(initial);
    let mut flat_memory = FlatMemory::new(0x4000);
    flat_memory.load(0x2000, memory);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut flat_memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

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

#[test]
fn all_324_memory_cells_match_manual_shared_count_model_at_o0_o1_o2() {
    let counts = [0u64, 1, 15, 16, 31, 32, 63, 64, 65, u64::MAX];
    let cases = all_cases();
    assert_eq!(cases.len(), 324);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let count = counts[ordinal % counts.len()];
        let upper = 0xD00D_F00D_CAFE_BABEu64.rotate_left(ordinal as u32);
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(count, upper);
        let expected = manual(case, &initial, count);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &memory);
            assert_eq!(actual, expected, "{level:?} {case:?} count={count}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 324 * LEVELS.len());
}

#[test]
fn low_qword_count_boundaries_and_ignored_upper_qword_are_exact() {
    let semantic_kinds = [
        ShiftKind::ALL[0],
        ShiftKind::ALL[2],
        ShiftKind::ALL[3],
        ShiftKind::ALL[4],
        ShiftKind::ALL[6],
        ShiftKind::ALL[7],
        ShiftKind::ALL[8],
        ShiftKind::ALL[10],
        ShiftKind::ALL[11],
    ];
    let mut comparisons = 0usize;
    for (kind_ordinal, kind) in semantic_kinds.into_iter().enumerate() {
        let bits = u64::from(kind.elem.bytes()) * 8;
        for count in [0, 1, bits - 1, bits, bits + 1, u64::MAX] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for control in MaskControl::ALL {
                    let case = ShiftMemoryCase {
                        kind,
                        width,
                        destination: 17,
                        source: 18,
                        control,
                    };
                    let mut initial = initial_state(case, kind_ordinal);
                    if case.mask() != 0 {
                        initial.masks[usize::from(case.mask())] = 0xA55A_A55A_A55A_A55A;
                    }
                    let expected = manual(case, &initial, count);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let low_upper = interpret(&function, &initial, &memory_bytes(count, 0));
                        let high_upper =
                            interpret(&function, &initial, &memory_bytes(count, u64::MAX));
                        assert_eq!(low_upper, expected, "{level:?} {case:?} count={count}");
                        assert_eq!(
                            high_upper, expected,
                            "{level:?} {case:?} count={count}: upper qword affected result"
                        );
                        comparisons += 2;
                    }
                }
            }
        }
    }
    assert_eq!(
        comparisons,
        semantic_kinds.len() * 6 * 3 * MaskControl::ALL.len() * LEVELS.len() * 2
    );
}

#[test]
fn e4nf_mask_zero_still_reads_all_16_bytes_and_faults_without_commit() {
    for kind in ShiftKind::ALL {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ShiftMemoryCase {
                kind,
                width: VecWidth::V512,
                destination: 17,
                source: 18,
                control,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_state(case, 0);
                initial.masks[usize::from(case.mask())] = 0;
                let old_destination = initial.vectors[usize::from(case.destination)];

                for memory_size in [0x100usize, 0x200F] {
                    let mut faulting = initial.clone();
                    if memory_size == 0x100 {
                        faulting.gpr[0] = 0x8000;
                        faulting.gpr[1] = 0;
                    }
                    let mut context = context_from_state(&faulting);
                    let mut memory = FlatMemory::new(memory_size);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?} memory_size={memory_size:#x}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(
                        x86.xmm[usize::from(case.destination)],
                        old_destination,
                        "{level:?} {case:?} memory_size={memory_size:#x}"
                    );
                }

                let memory = memory_bytes(3, 0xFFFF_FFFF_FFFF_FFFF);
                let expected = manual(case, &initial, 3);
                let actual = interpret(&function, &initial, &memory);
                assert_eq!(actual, expected, "{level:?} {case:?}: mask-zero success");
            }
        }
    }
}
