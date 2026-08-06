//! Native x86-64 differential, helper-call, scratch, and precise-fault tests.

use super::semantics::{
    SemanticOutcome, SemanticState, initial_state, interpret_success, memory_bytes,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16};

struct VectorMemoryContext {
    value: [u8; 64],
    ok: bool,
    calls: usize,
    last_address: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_address = address;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if !context.ok
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 8 | 16 | 32 | 64)
        || zero_upper != 1
    {
        return 0;
    }

    let mut scratch = [0u8; 64];
    scratch[..size as usize].copy_from_slice(&context.value[..size as usize]);
    state.vector_scratch = std::array::from_fn(|word| {
        u64::from_le_bytes(scratch[word * 8..word * 8 + 8].try_into().unwrap())
    });
    1
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_K16,
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b1110_0110,
        cs_l: 1,
        apx_enabled: 1,
        ..GuestRegs::default()
    };
    for (index, vector) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, vector[..8].try_into().unwrap());
    }
    registers
}

fn expected_scratch(memory: &[u8; 64], size: usize) -> [u64; 8] {
    let mut scratch = [0u8; 64];
    scratch[..size].copy_from_slice(&memory[..size]);
    std::array::from_fn(|word| {
        u64::from_le_bytes(scratch[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn expected_guest_regs(
    initial: GuestRegs,
    outcome: &SemanticOutcome,
    memory_size: usize,
) -> GuestRegs {
    let mut expected = initial;
    expected.gpr = outcome.state.gpr;
    expected.rflags = outcome.state.rflags;
    expected.k = outcome.state.masks;
    expected.mxcsr = outcome.state.mxcsr;
    expected.vector_scratch = expected_scratch(&outcome.memory, memory_size);
    for (index, vector) in outcome.state.vectors.iter().enumerate() {
        expected.set_zmm(index, vector[..8].try_into().unwrap());
    }
    expected
}

fn bind_helper(registers: &mut GuestRegs, context: &mut VectorMemoryContext) {
    registers.ctx = (context as *mut VectorMemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
}

fn assert_helper_call(context: &VectorMemoryContext, case: DuplicateMemoryCase) {
    assert_eq!(context.calls, 1, "{case:?}");
    assert_eq!(context.last_address, MEMORY_ADDRESS, "{case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{case:?}"
    );
    assert_eq!(context.last_size, case.memory_size(), "{case:?}");
    assert_eq!(context.last_zero_upper, 1, "{case:?}");
}

fn supported(case: DuplicateMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

#[test]
fn native_duplicate_moves_match_interpreter_restore_scratch_and_preserve_nf_faults() {
    // Lower every cell even on hosts where native execution must self-skip.
    let mut compiled = 0usize;
    for case in all_cases() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let _ = lower(&function, case);
            compiled += 1;
        }
    }
    assert_eq!(compiled, 27 * 2);

    if !std::is_x86_feature_detected!("avx512f") {
        eprintln!("skipping native duplicate-move differential: host lacks AVX-512F");
        return;
    }
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| supported(*case))
        .collect();
    assert!(!cases.is_empty());

    let expected_runs = cases.len() * 2;
    let expected_masked_runs = cases.iter().filter(|case| case.mask() != 0).count() * 2;
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut empty_mask_successes = 0usize;
    let mut empty_mask_faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let lanes = case.lanes();
        let mut mask =
            0xD6A5_3C69_F00F_5AA5u64.rotate_left((ordinal & 63) as u32) & ((1u64 << lanes) - 1);
        if case.mask() != 0 {
            mask |= 1;
            mask &= !(1u64 << (lanes - 1));
        }
        let memory = memory_bytes(ordinal);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let semantic = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
            let outcome = interpret_success(&function, &semantic, &memory);
            let mut context = VectorMemoryContext {
                value: memory,
                ok: true,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&semantic);
            bind_helper(&mut registers, &mut context);
            let initial = registers;
            executable.run(entry, &mut registers);
            let mut expected = expected_guest_regs(initial, &outcome, case.memory_size() as usize);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success state");
            assert_helper_call(&context, case);
            successes += 1;

            let mut fault_context = VectorMemoryContext {
                value: memory,
                ok: false,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut fault_registers = guest_regs(&semantic);
            bind_helper(&mut fault_registers, &mut fault_context);
            let mut expected_fault = fault_registers;
            expected_fault.exit_pc = PC;
            executable.run(entry, &mut fault_registers);
            expected_fault.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, expected_fault,
                "{level:?} {case:?}: fault committed state"
            );
            assert_helper_call(&fault_context, case);
            faults += 1;

            if case.mask() == 0 {
                continue;
            }
            let empty = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, 0);
            let outcome = interpret_success(&function, &empty, &memory);
            let mut context = VectorMemoryContext {
                value: memory,
                ok: true,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&empty);
            bind_helper(&mut registers, &mut context);
            let initial = registers;
            executable.run(entry, &mut registers);
            let mut expected = expected_guest_regs(initial, &outcome, case.memory_size() as usize);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: empty mask");
            assert_helper_call(&context, case);
            empty_mask_successes += 1;

            context.ok = false;
            context.calls = 0;
            let mut registers = guest_regs(&empty);
            bind_helper(&mut registers, &mut context);
            let mut expected_fault = registers;
            expected_fault.exit_pc = PC;
            executable.run(entry, &mut registers);
            expected_fault.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected_fault,
                "{level:?} {case:?}: empty-mask fault committed state"
            );
            assert_helper_call(&context, case);
            empty_mask_faults += 1;
        }
    }
    assert_eq!(successes, expected_runs);
    assert_eq!(faults, expected_runs);
    assert_eq!(empty_mask_successes, expected_masked_runs);
    assert_eq!(empty_mask_faults, expected_masked_runs);
}
