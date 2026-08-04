//! Native x86-64 differential, helper-call, scratch, and precise-fault tests.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ScalarMemoryContext {
    value: u64,
    ok: bool,
    calls: usize,
    last_address: u64,
    last_size: u64,
    last_signed: u64,
}

extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert!(matches!(size, 4 | 8));
    assert_eq!(signed, 0);
    context.calls += 1;
    context.last_address = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: u64::from(context.ok),
    }
}

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn scalar_value(bytes: &[u8; 64], width: MemWidth) -> u64 {
    let size = width.bytes() as usize;
    let mut value = [0u8; 8];
    value[..size].copy_from_slice(&bytes[..size]);
    u64::from_le_bytes(value)
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        vector_active: 1,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, value[..8].try_into().unwrap());
    }
    registers
}

fn assert_architectural_state(
    actual: &GuestRegs,
    expected: &SemanticState,
    level: OptLevel,
    case: ChunkShuffleMemoryCase,
) {
    assert_eq!(actual.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, vector) in expected.vectors.iter().enumerate() {
        assert_eq!(
            actual.get_zmm(index),
            <[u64; 8]>::try_from(&vector[..8]).unwrap(),
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(actual.k, expected.masks, "{level:?} {case:?}: opmasks");
    assert_eq!(actual.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(actual.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

fn selected_cases() -> [ChunkShuffleMemoryCase; 8] {
    [
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[0],
            width: VecWidth::V256,
            destination: 0,
            source1: 0,
            control: MaskControl::None,
            tuple: TupleKind::Full,
            immediate: 0x4E,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[0],
            width: VecWidth::V512,
            destination: 9,
            source1: 10,
            control: MaskControl::Merge,
            tuple: TupleKind::Broadcast,
            immediate: 0xA5,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[1],
            width: VecWidth::V256,
            destination: 17,
            source1: 17,
            control: MaskControl::Zero,
            tuple: TupleKind::Full,
            immediate: 0x03,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[1],
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            control: MaskControl::None,
            tuple: TupleKind::Broadcast,
            immediate: 0xFF,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[2],
            width: VecWidth::V512,
            destination: 0,
            source1: 1,
            control: MaskControl::Merge,
            tuple: TupleKind::Full,
            immediate: 0x1B,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[2],
            width: VecWidth::V256,
            destination: 9,
            source1: 9,
            control: MaskControl::Zero,
            tuple: TupleKind::Broadcast,
            immediate: 0xB1,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[3],
            width: VecWidth::V256,
            destination: 17,
            source1: 18,
            control: MaskControl::Merge,
            tuple: TupleKind::Full,
            immediate: 0x02,
        },
        ChunkShuffleMemoryCase {
            kind: ChunkKind::ALL[3],
            width: VecWidth::V512,
            destination: 25,
            source1: 25,
            control: MaskControl::Zero,
            tuple: TupleKind::Broadcast,
            immediate: 0x5A,
        },
    ]
}

fn run_success(
    exec: &ExecMem,
    entry: usize,
    case: ChunkShuffleMemoryCase,
    level: OptLevel,
    initial: &SemanticState,
    expected: &SemanticState,
    bytes: &[u8; 64],
) {
    match case.tuple {
        TupleKind::Full => {
            use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};
            let mut context = VectorMemoryContext {
                value: memory_words(bytes),
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(initial);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, expected, level, case);
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, 0x2000, "{level:?} {case:?}");
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
        }
        TupleKind::Broadcast => {
            let width = case.kind.memory_width();
            let mut context = ScalarMemoryContext {
                value: scalar_value(bytes, width),
                ok: true,
                calls: 0,
                last_address: 0,
                last_size: 0,
                last_signed: u64::MAX,
            };
            let mut registers = guest_regs(initial);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, expected, level, case);
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_address, 0x2000, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                u64::from(width.bytes()),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
        }
    }
}

fn run_fault(
    exec: &ExecMem,
    entry: usize,
    case: ChunkShuffleMemoryCase,
    level: OptLevel,
    initial: &SemanticState,
    bytes: &[u8; 64],
) {
    match case.tuple {
        TupleKind::Full => {
            use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};
            let mut context = VectorMemoryContext {
                value: memory_words(bytes),
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(initial);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "{level:?} {case:?}: fault calls");
        }
        TupleKind::Broadcast => {
            let width = case.kind.memory_width();
            let mut context = ScalarMemoryContext {
                value: scalar_value(bytes, width),
                ok: false,
                calls: 0,
                last_address: 0,
                last_size: 0,
                last_signed: u64::MAX,
            };
            let mut registers = guest_regs(initial);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as *const () as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "{level:?} {case:?}: fault calls");
        }
    }
}

#[test]
fn native_chunk_shuffle_matches_interpreter_restores_scratch_and_preserves_e4nf_faults() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX chunk-shuffle differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut empty_mask_successes = 0usize;
    let mut empty_mask_faults = 0usize;
    for (ordinal, case) in cases.iter().copied().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal);
            let bytes = memory_bytes(case, ordinal);
            let expected = interpret(&function, &initial, &bytes, case);
            run_success(&exec, entry, case, level, &initial, &expected, &bytes);
            run_fault(&exec, entry, case, level, &initial, &bytes);
            successes += 1;
            faults += 1;

            if case.mask() != 0 {
                let mut empty = initial;
                empty.masks[usize::from(case.mask())] = 0;
                let expected = interpret(&function, &empty, &bytes, case);
                run_success(&exec, entry, case, level, &empty, &expected, &bytes);
                run_fault(&exec, entry, case, level, &empty, &bytes);
                empty_mask_successes += 1;
                empty_mask_faults += 1;
            }
        }
    }
    assert_eq!(successes, cases.len() * 2);
    assert_eq!(faults, cases.len() * 2);
    let masked = cases.iter().filter(|case| case.mask() != 0).count();
    assert_eq!(empty_mask_successes, masked * 2);
    assert_eq!(empty_mask_faults, masked * 2);
}
