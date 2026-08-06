//! Independent scalar-insert semantics, optimizer parity, and fault precision.

use super::*;
use crate::smir::TrapKind;
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

fn vector_bytes(words: &[u64; 16]) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn vector_words(bytes: &[u8; 128]) -> [u64; 16] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

pub(super) fn initial_state(case: InsertCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0202_0404_0808)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        masks: std::array::from_fn(|register| {
            0x0102_0304_0506_0708u64.rotate_left((register * 7) as u32)
        }),
        rflags: 0x2 | ((ordinal as u64).wrapping_mul(0x145) & 0x8D5),
        mxcsr: 0x1F80 | (ordinal as u32 & 0x3F),
    };
    state.gpr[2] = MEMORY_ADDRESS; // encoded [RDX]
    // Alias cases must consume the pre-instruction destination as source1.
    assert!(case.destination < 32 && case.source1 < 32);
    state
}

pub(super) fn manual_destination(case: InsertCase, source1: &[u64; 16], scalar: u64) -> [u64; 16] {
    let source = vector_bytes(source1);
    let mut result = [0u8; 128];
    result[..16].copy_from_slice(&source[..16]);
    let width = case.shape.kind.memory_width().bytes() as usize;
    let lane = usize::from(case.shape.kind.destination_lane(case.immediate));
    result[lane * width..lane * width + width].copy_from_slice(&scalar.to_le_bytes()[..width]);
    if case.shape.kind == X86ScalarInsertMemoryKind::Vinsertps {
        for lane in 0..4usize {
            if case.immediate & (1 << lane) != 0 {
                result[lane * 4..lane * 4 + 4].fill(0);
            }
        }
    }
    vector_words(&result)
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
    let mut function = function.clone();
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = context(initial);
    let result = SmirInterpreter::new().execute_block(&mut context, memory, &function.blocks[0]);
    (result, state(&context))
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &SemanticState,
    scalar: u64,
) -> SemanticState {
    let width = match function.blocks[0].ops.iter().find_map(|op| match op.kind {
        OpKind::Load { width, .. } => Some(width.bytes() as usize),
        _ => None,
    }) {
        Some(width) => width,
        None => panic!("scalar-insert graph has no Load"),
    };
    let mut memory = FlatMemory::new(0x5000);
    memory.load(MEMORY_ADDRESS as usize, &scalar.to_le_bytes()[..width]);
    let (result, state) = execute(function, initial, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    state
}

#[test]
fn every_immediate_shape_alias_and_high_register_matches_intel_semantics() {
    let mut comparisons = 0usize;
    for (shape_index, shape) in SHAPES.into_iter().enumerate() {
        for immediate in u8::MIN..=u8::MAX {
            let alias = immediate & 1 != 0;
            let destination = [0, 9, 17, 31][usize::from(immediate & 3)];
            let source1 = if alias {
                destination
            } else {
                [1, 10, 18, 30][usize::from((immediate >> 2) & 3)]
            };
            let case = InsertCase {
                shape,
                destination,
                source1,
                immediate,
            };
            let ordinal = shape_index * 256 + usize::from(immediate);
            let scalar =
                0xFEDC_BA98_7654_3210u64 ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);
            let initial = initial_state(case, ordinal);
            let mut expected = initial.clone();
            expected.vectors[usize::from(destination)] =
                manual_destination(case, &initial.vectors[usize::from(source1)], scalar);
            for level in [OptLevel::O0, OptLevel::O2] {
                let function = optimize(lift_case(case), level);
                let actual = interpret_success(&function, &initial, scalar);
                assert_eq!(actual, expected, "{level:?} {case:?}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 7 * 256 * 2);
}

#[test]
fn fully_zeroed_vinsertps_still_faults_before_any_state_commit() {
    let case = InsertCase {
        shape: SHAPES[0],
        destination: 31,
        source1: 30,
        // Every output lane is zero; Count_S/Count_D are otherwise maximal.
        immediate: 0xFF,
    };
    for level in LEVELS {
        let function = optimize(lift_case(case), level);
        let initial = initial_state(case, level as usize);
        let mut memory = FlatMemory::new(0);
        let (result, actual) = execute(&function, &initial, &mut memory);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: MEMORY_ADDRESS,
                    write: false,
                })
            ),
            "{level:?}: {result:?}"
        );
        assert_eq!(actual, initial, "{level:?}: fault must not commit state");
    }
}
