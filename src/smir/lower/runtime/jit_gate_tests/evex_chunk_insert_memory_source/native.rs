//! Native x86-64 differential, helper-call, scratch, and precise-fault tests.

use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};
use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
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
    case: ChunkInsertMemoryCase,
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

fn selected_cases() -> Vec<ChunkInsertMemoryCase> {
    let mut cases = Vec::new();
    for (ordinal, (kind, width)) in shape_cases().into_iter().enumerate() {
        for control in MaskControl::ALL {
            cases.push(ChunkInsertMemoryCase {
                kind,
                width,
                destination: [0, 9, 17, 25][ordinal % 4],
                source1: [0, 10, 17, 26]
                    [(ordinal + usize::from(!matches!(control, MaskControl::None))) % 4],
                control,
                immediate: [0x00, 0x4E, 0xA5, 0xFF][(ordinal + control as usize) % 4],
            });
        }
    }
    cases
}

fn run_success(
    exec: &ExecMem,
    entry: usize,
    case: ChunkInsertMemoryCase,
    level: OptLevel,
    initial: &SemanticState,
    expected: &SemanticState,
    bytes: &[u8; 64],
) {
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
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{level:?} {case:?}"
    );
    assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
}

fn run_fault(
    exec: &ExecMem,
    entry: usize,
    case: ChunkInsertMemoryCase,
    level: OptLevel,
    initial: &SemanticState,
    bytes: &[u8; 64],
) {
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
    assert_eq!(
        context.last_addr, 0x2000,
        "{level:?} {case:?}: fault address"
    );
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{level:?} {case:?}: fault scratch"
    );
    assert_eq!(
        context.last_size,
        case.memory_size(),
        "{level:?} {case:?}: fault tuple size"
    );
    assert_eq!(
        context.last_zero_upper, 1,
        "{level:?} {case:?}: fault zero-upper policy"
    );
}

#[test]
fn native_chunk_insert_matches_interpreter_restores_scratch_and_preserves_e6nf_faults() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX chunk-insert memory differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_dq = std::is_x86_feature_detected!("avx512dq");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .filter(|case| !case.kind.needs_dq() || has_dq)
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
