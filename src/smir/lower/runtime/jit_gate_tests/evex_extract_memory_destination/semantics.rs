//! Independent E6NF/E9NF semantics, optimizer parity, and fault frontiers.

use super::*;
use crate::smir::TrapKind;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::types::{AtomicOp, FenceKind, GuestAddr, MemoryOrder};

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

fn vector_bytes(words: &[u64; 16]) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn initial_state(
    case: ExtractMemoryCase,
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
    if let Some(mask) = case.writemask() {
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

fn lane_mask(lanes: usize) -> u64 {
    (1u64 << lanes) - 1
}

fn manual(case: ExtractMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticOutcome {
    let source = vector_bytes(&initial.vectors[usize::from(case.source())]);
    let elem_bytes = case.elem().bytes() as usize;
    let mut result_memory = *memory;
    match case {
        ExtractMemoryCase::Scalar { .. } => {
            let offset = case.selected_first_lane() * elem_bytes;
            result_memory[..elem_bytes].copy_from_slice(&source[offset..offset + elem_bytes]);
        }
        ExtractMemoryCase::Chunk { writemask, .. } => {
            let control = writemask.map_or_else(
                || lane_mask(case.lanes()),
                |mask| initial.masks[usize::from(mask)] & lane_mask(case.lanes()),
            );
            for lane in 0..case.lanes() {
                if control & (1u64 << lane) == 0 {
                    continue;
                }
                let source_offset = (case.selected_first_lane() + lane) * elem_bytes;
                let destination_offset = lane * elem_bytes;
                result_memory[destination_offset..destination_offset + elem_bytes]
                    .copy_from_slice(&source[source_offset..source_offset + elem_bytes]);
            }
        }
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
    memory: &mut dyn SmirMemory,
) -> (BlockResult, SemanticState) {
    // A normal x86 Return terminator reads the return address at RSP. Replace
    // it for instruction-semantic tests so the recorder observes only memory
    // accesses made by the extraction instruction itself.
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
    bytes: &[u8; 64],
) -> SemanticOutcome {
    let mut memory = FlatMemory::new(0x5000);
    memory.load(MEMORY_ADDRESS as usize, bytes);
    let (result, state) = execute(function, initial, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut observed = [0u8; 64];
    memory.read(MEMORY_ADDRESS, &mut observed).unwrap();
    SemanticOutcome {
        state,
        memory: observed,
    }
}

#[test]
fn all_32_evex_extract_cells_match_manual_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 32);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let mask = 0xA55A_6996_F00F_3CC3u64.rotate_left((ordinal & 63) as u32);
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
    assert_eq!(comparisons, 32 * LEVELS.len());
}

#[test]
fn every_k1_to_k7_and_every_chunk_immediate_selector_obey_exact_lane_granularity() {
    let mut comparisons = 0usize;
    for (shape_index, shape) in CHUNK_SHAPES.into_iter().enumerate() {
        let chunks = shape.source_width.bytes() / shape.chunk_width().bytes();
        for mask in 1..=7u8 {
            for immediate in 0..chunks as u8 {
                let case = ExtractMemoryCase::Chunk {
                    shape,
                    source: 16 + mask,
                    writemask: Some(mask),
                    immediate: immediate | 0xFC,
                };
                let active = 0xD6A5_3C69_F00F_5AA5u64
                    .rotate_left(u32::from(mask * 7) + u32::from(immediate));
                let initial = initial_state(
                    case,
                    shape_index + usize::from(mask),
                    MEMORY_ADDRESS,
                    active | !lane_mask(case.lanes()),
                );
                let memory = memory_bytes(shape_index + usize::from(mask));
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
    let selector_cells: usize = CHUNK_SHAPES
        .into_iter()
        .map(|shape| (shape.source_width.bytes() / shape.chunk_width().bytes()) as usize)
        .sum();
    assert_eq!(comparisons, 7 * selector_cells);
}

struct RecordingMemory {
    inner: FlatMemory,
    reads: Vec<(GuestAddr, usize)>,
    writes: Vec<(GuestAddr, usize)>,
    fault_reads: bool,
    fault_writes: bool,
}

impl RecordingMemory {
    fn new(bytes: &[u8; 64], fault_reads: bool, fault_writes: bool) -> Self {
        let mut inner = FlatMemory::with_base(MEMORY_ADDRESS, 64);
        inner.load(0, bytes);
        Self {
            inner,
            reads: Vec::new(),
            writes: Vec::new(),
            fault_reads,
            fault_writes,
        }
    }
}

impl SmirMemory for RecordingMemory {
    fn read(&mut self, addr: GuestAddr, buf: &mut [u8]) -> Result<(), MemoryError> {
        self.reads.push((addr, buf.len()));
        if self.fault_reads {
            Err(MemoryError::PageFault {
                addr,
                write: false,
                user: true,
            })
        } else {
            self.inner.read(addr, buf)
        }
    }

    fn write(&mut self, addr: GuestAddr, data: &[u8]) -> Result<(), MemoryError> {
        self.writes.push((addr, data.len()));
        if self.fault_writes {
            Err(MemoryError::PageFault {
                addr,
                write: true,
                user: true,
            })
        } else {
            self.inner.write(addr, data)
        }
    }

    fn atomic_load(
        &mut self,
        addr: GuestAddr,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.inner.atomic_load(addr, size, order)
    }

    fn atomic_store(
        &mut self,
        addr: GuestAddr,
        value: u64,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<(), MemoryError> {
        self.inner.atomic_store(addr, value, size, order)
    }

    fn compare_and_swap(
        &mut self,
        addr: GuestAddr,
        expected: u64,
        new: u64,
        size: MemWidth,
        success_order: MemoryOrder,
        failure_order: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        self.inner
            .compare_and_swap(addr, expected, new, size, success_order, failure_order)
    }

    fn atomic_rmw(
        &mut self,
        addr: GuestAddr,
        op: AtomicOp,
        operand: u64,
        size: MemWidth,
        order: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.inner.atomic_rmw(addr, op, operand, size, order)
    }

    fn load_exclusive(&mut self, addr: GuestAddr, size: MemWidth) -> Result<u64, MemoryError> {
        self.inner.load_exclusive(addr, size)
    }

    fn store_exclusive(
        &mut self,
        addr: GuestAddr,
        value: u64,
        size: MemWidth,
    ) -> Result<bool, MemoryError> {
        self.inner.store_exclusive(addr, value, size)
    }

    fn clear_exclusive(&mut self) {
        self.inner.clear_exclusive();
    }

    fn fence(&mut self, kind: FenceKind) {
        self.inner.fence(kind);
    }

    fn probe(&self, addr: GuestAddr, size: usize, write: bool) -> Result<(), MemoryError> {
        if (write && self.fault_writes) || (!write && self.fault_reads) {
            Err(MemoryError::PageFault {
                addr,
                write,
                user: true,
            })
        } else {
            self.inner.probe(addr, size, write)
        }
    }
}

#[test]
fn e6nf_empty_and_out_of_range_masks_still_issue_one_full_read_and_write() {
    let shapes = [
        CHUNK_SHAPES[0],
        CHUNK_SHAPES[1],
        CHUNK_SHAPES[8],
        CHUNK_SHAPES[11],
    ];
    let mut checks = 0usize;
    for shape in shapes {
        for mask_value in [
            0,
            !lane_mask(shape.chunk_width().lanes(shape.elem()) as usize),
        ] {
            let case = ExtractMemoryCase::Chunk {
                shape,
                source: 31,
                writemask: Some(7),
                immediate: 0xFF,
            };
            let initial = initial_state(case, checks, MEMORY_ADDRESS, mask_value);
            let bytes = memory_bytes(checks);
            let function = optimize(lift_case(case), OptLevel::O2);
            let mut memory = RecordingMemory::new(&bytes, false, false);
            let (result, actual_state) = execute(&function, &initial, &mut memory);
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            assert_eq!(actual_state, initial, "{case:?}");
            assert_eq!(
                memory.reads,
                vec![(MEMORY_ADDRESS, case.memory_size() as usize)]
            );
            assert_eq!(
                memory.writes,
                vec![(MEMORY_ADDRESS, case.memory_size() as usize)]
            );
            let mut observed = [0u8; 64];
            memory.inner.read(MEMORY_ADDRESS, &mut observed).unwrap();
            assert_eq!(observed, bytes, "{case:?}");
            checks += 1;
        }
    }
    assert_eq!(checks, 8);
}

#[test]
fn e6nf_read_and_write_faults_are_ordered_and_noncommitting() {
    let case = ExtractMemoryCase::Chunk {
        shape: CHUNK_SHAPES[11],
        source: 31,
        writemask: Some(7),
        immediate: 0xFF,
    };
    let initial = initial_state(case, 9, MEMORY_ADDRESS, 0);
    let bytes = memory_bytes(9);
    let function = optimize(lift_case(case), OptLevel::O2);

    let mut read_fault = RecordingMemory::new(&bytes, true, false);
    let (result, actual_state) = execute(&function, &initial, &mut read_fault);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: MEMORY_ADDRESS,
            write: false
        })
    ));
    assert_eq!(actual_state, initial);
    assert_eq!(read_fault.reads, vec![(MEMORY_ADDRESS, 32)]);
    assert!(read_fault.writes.is_empty());

    let mut write_fault = RecordingMemory::new(&bytes, false, true);
    let (result, actual_state) = execute(&function, &initial, &mut write_fault);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: MEMORY_ADDRESS,
            write: true
        })
    ));
    assert_eq!(actual_state, initial);
    assert_eq!(write_fault.reads, vec![(MEMORY_ADDRESS, 32)]);
    assert_eq!(write_fault.writes, vec![(MEMORY_ADDRESS, 32)]);
    let mut observed = [0u8; 64];
    write_fault
        .inner
        .read(MEMORY_ADDRESS, &mut observed)
        .unwrap();
    assert_eq!(observed, bytes);
}

#[test]
fn e9nf_scalar_and_unmasked_e6nf_chunk_fault_before_any_state_commit() {
    let cases = [
        ExtractMemoryCase::Scalar {
            shape: SCALAR_SHAPES[5],
            source: 31,
            immediate: 0xFF,
        },
        ExtractMemoryCase::Chunk {
            shape: CHUNK_SHAPES[8],
            source: 31,
            writemask: None,
            immediate: 0xFF,
        },
    ];
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal, MEMORY_ADDRESS, u64::MAX);
        let bytes = memory_bytes(ordinal);
        let function = optimize(lift_case(case), OptLevel::O2);
        let mut memory = RecordingMemory::new(&bytes, false, true);
        let (result, actual_state) = execute(&function, &initial, &mut memory);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::MemoryFault {
                addr: MEMORY_ADDRESS,
                write: true
            })
        ));
        assert_eq!(actual_state, initial, "{case:?}");
        assert!(memory.reads.is_empty(), "{case:?}");
        assert_eq!(
            memory.writes,
            vec![(MEMORY_ADDRESS, case.memory_size() as usize)]
        );
    }
}
