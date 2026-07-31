//! Interpreter, optimizer, and Type E4NF precise-fault coverage.

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

pub(super) fn initial_state(case: IntegerPackMemoryCase, ordinal: usize) -> SemanticState {
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
    for lane in 0..case.width.lanes(case.kind.src_elem) as usize {
        let (source1, _) = boundary_operands(case.kind.src_elem, lane, ordinal);
        set_lane(
            &mut state.vectors[usize::from(case.source1)],
            lane,
            case.kind.src_elem,
            source1,
        );
    }
    state
}

pub(super) fn memory_bytes(case: IntegerPackMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.width.lanes(case.kind.src_elem) as usize;
    for lane in 0..lanes {
        let (_, value) = boundary_operands(case.kind.src_elem, lane, ordinal);
        let lane_bytes = case.kind.src_elem.bytes() as usize;
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn lane_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I8 => 0xFF,
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        _ => unreachable!("EVEX integer-pack element"),
    }
}

fn boundary_operands(elem: VecElementType, lane: usize, ordinal: usize) -> (u64, u64) {
    let values: &[i64] = match elem {
        VecElementType::I16 => &[-32_768, -129, -128, -1, 0, 127, 128, 32_767],
        VecElementType::I32 => &[
            i32::MIN as i64,
            -32_769,
            -32_768,
            -1,
            0,
            32_767,
            32_768,
            i32::MAX as i64,
        ],
        _ => unreachable!("EVEX integer-pack source element"),
    };
    let left = values[(lane + ordinal) % values.len()] as u64 & lane_mask(elem);
    let right = values[(values.len() - 1 - lane % values.len() + ordinal) % values.len()] as u64
        & lane_mask(elem);
    (left, right)
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

fn memory_lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    let lane_bytes = elem.bytes() as usize;
    let offset = lane * lane_bytes;
    let mut value = [0u8; 8];
    value[..lane_bytes].copy_from_slice(&bytes[offset..offset + lane_bytes]);
    u64::from_le_bytes(value)
}

fn signed_value(value: u64, elem: VecElementType) -> i64 {
    let bits = u32::from(elem.bytes()) * 8;
    let shift = 64 - bits;
    ((value << shift) as i64) >> shift
}

fn saturate(kind: PackKind, source: u64) -> u64 {
    let value = signed_value(source, kind.src_elem);
    let dst_bits = u32::from(kind.dst_elem.bytes()) * 8;
    let (minimum, maximum) = if kind.to_unsigned {
        (0, (1i64 << dst_bits) - 1)
    } else {
        (-(1i64 << (dst_bits - 1)), (1i64 << (dst_bits - 1)) - 1)
    };
    value.clamp(minimum, maximum) as u64 & lane_mask(kind.dst_elem)
}

fn manual(
    case: IntegerPackMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let src_elem = case.kind.src_elem;
    let dst_elem = case.kind.dst_elem;
    let src_lanes = case.width.lanes(src_elem) as usize;
    let dst_lanes = case.width.lanes(dst_elem) as usize;
    let block_lanes = 16 / src_elem.bytes() as usize;
    let source1 = initial.vectors[usize::from(case.source1)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let mut packed = [0u64; 16];

    for block_base in (0..src_lanes).step_by(block_lanes) {
        let output_base = block_base * 2;
        for lane_in_block in 0..block_lanes {
            let source_lane = block_base + lane_in_block;
            set_lane(
                &mut packed,
                output_base + lane_in_block,
                dst_elem,
                saturate(case.kind, get_lane(&source1, source_lane, src_elem)),
            );
            set_lane(
                &mut packed,
                output_base + block_lanes + lane_in_block,
                dst_elem,
                saturate(
                    case.kind,
                    memory_lane(
                        memory,
                        if case.broadcast() { 0 } else { source_lane },
                        src_elem,
                    ),
                ),
            );
        }
    }

    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..dst_lanes {
        let value = if mask & (1u64 << lane) != 0 {
            get_lane(&packed, lane, dst_elem)
        } else if case.zeroing() {
            0
        } else {
            get_lane(&old_destination, lane, dst_elem)
        };
        set_lane(destination, lane, dst_elem, value);
    }
    for word in usize::try_from(case.width.bytes() / 8).unwrap()..destination.len() {
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
    memory_bytes: &[u8; 64],
    case: IntegerPackMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        4
    } else {
        case.width.bytes()
    } as usize;
    memory.load(0x2000, &memory_bytes[..size]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_216_integer_pack_cells_match_manual_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 216);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(case, ordinal);
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
fn empty_masks_do_not_suppress_type_e4nf_accesses_and_faults_do_not_commit() {
    for case in [
        IntegerPackMemoryCase {
            kind: PackKind::ALL[1],
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            wig_w: true,
        },
        IntegerPackMemoryCase {
            kind: PackKind::ALL[3],
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            wig_w: false,
        },
    ] {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut empty = initial_state(case, 0);
            empty.masks[usize::from(case.mask())] = 0;
            let mut fault_context = context(&empty);
            let mut unmapped = FlatMemory::new(0x1000);
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
                empty,
                "{level:?} {case:?}: fault committed architectural state"
            );
        }
    }
}
