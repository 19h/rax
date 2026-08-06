//! Native x86-64 differential and precise E6NF helper-frontier coverage.

use super::semantics::{SemanticState, initial_state, interpret_success, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

#[derive(Clone, Debug, PartialEq, Eq)]
enum MemoryCall {
    Load {
        address: u64,
        destination: u32,
        size: u32,
        zero_upper: u32,
    },
    Store {
        address: u64,
        source: u32,
        size: u32,
        value: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
struct MemoryContext {
    base: u64,
    bytes: [u8; 64],
    fail_call: Option<usize>,
    calls: Vec<MemoryCall>,
}

fn scratch_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn scratch_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut MemoryContext) };
    let call = context.calls.len();
    context.calls.push(MemoryCall::Load {
        address,
        destination,
        size,
        zero_upper,
    });
    assert_eq!(
        destination,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
    );
    assert!(matches!(size, 16 | 32));
    assert_eq!(zero_upper, 1);
    if context.fail_call == Some(call) {
        return 0;
    }

    let width = usize::try_from(size).unwrap();
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    assert!(offset + width <= context.bytes.len());
    let mut scratch = if zero_upper != 0 {
        [0u8; 64]
    } else {
        scratch_bytes(state.vector_scratch)
    };
    scratch[..width].copy_from_slice(&context.bytes[offset..offset + width]);
    state.vector_scratch = scratch_words(scratch);
    1
}

extern "C" fn vector_store_helper(
    state: *mut GuestRegs,
    address: u64,
    source: u32,
    size: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut MemoryContext) };
    let call = context.calls.len();
    assert_eq!(source, crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX);
    assert!(matches!(size, 1 | 2 | 4 | 8 | 16 | 32));
    let width = usize::try_from(size).unwrap();
    let value = scratch_bytes(state.vector_scratch)[..width].to_vec();
    context.calls.push(MemoryCall::Store {
        address,
        source,
        size,
        value: value.clone(),
    });
    if context.fail_call == Some(call) {
        return 0;
    }

    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    assert!(offset + width <= context.bytes.len());
    context.bytes[offset..offset + width].copy_from_slice(&value);
    1
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: initial.mxcsr,
        vector_scratch: std::array::from_fn(|word| {
            0xCCDD_EEFF_0011_2233u64 ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
        }),
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

fn bind_helpers(registers: &mut GuestRegs, context: &mut MemoryContext) {
    registers.ctx = (context as *mut MemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
    registers.vec_store_fn = vector_store_helper as *const () as usize as u64;
}

fn host_supports(case: ExtractMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (!case.needs_avx512vl() || std::is_x86_feature_detected!("avx512vl"))
        && (!case.needs_avx512dq() || std::is_x86_feature_detected!("avx512dq"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Success,
    LoadFault,
    StoreFault,
}

fn expected_calls(case: ExtractMemoryCase, scenario: Scenario, stored: &[u8]) -> Vec<MemoryCall> {
    let size = case.memory_size();
    let load = MemoryCall::Load {
        address: MEMORY_ADDRESS,
        destination: crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        size,
        zero_upper: 1,
    };
    let store = MemoryCall::Store {
        address: MEMORY_ADDRESS,
        source: crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        size,
        value: stored.to_vec(),
    };
    match (case.writemask().is_some(), scenario) {
        (true, Scenario::LoadFault) => vec![load],
        (true, _) => vec![load, store],
        (false, Scenario::LoadFault) => unreachable!("only masked chunks perform a load"),
        (false, _) => vec![store],
    }
}

fn expected_scratch(
    initial: [u64; 8],
    case: ExtractMemoryCase,
    scenario: Scenario,
    stored: &[u8],
) -> [u64; 8] {
    if scenario == Scenario::LoadFault {
        return initial;
    }
    let mut bytes = if case.writemask().is_some() {
        [0u8; 64]
    } else {
        scratch_bytes(initial)
    };
    bytes[..stored.len()].copy_from_slice(stored);
    scratch_words(bytes)
}

fn execute_scenario(
    executable: &ExecMem,
    entry: usize,
    case: ExtractMemoryCase,
    level: OptLevel,
    semantic: &SemanticState,
    initial_memory: [u8; 64],
    expected_memory: [u8; 64],
    scenario: Scenario,
) {
    let fail_call = match scenario {
        Scenario::Success => None,
        Scenario::LoadFault => Some(0),
        Scenario::StoreFault => Some(usize::from(case.writemask().is_some())),
    };
    let mut context = MemoryContext {
        base: MEMORY_ADDRESS,
        bytes: initial_memory,
        fail_call,
        calls: Vec::new(),
    };
    let mut registers = guest_regs(semantic);
    bind_helpers(&mut registers, &mut context);
    let mut expected_registers = registers;
    let size = case.memory_size() as usize;
    let stored = &expected_memory[..size];
    expected_registers.vector_scratch =
        expected_scratch(registers.vector_scratch, case, scenario, stored);
    if scenario != Scenario::Success {
        expected_registers.exit_pc = PC;
    }

    executable.run(entry, &mut registers);
    expected_registers.host_mxcsr = registers.host_mxcsr;
    assert_eq!(
        registers, expected_registers,
        "{level:?} {case:?} {scenario:?}"
    );
    assert_eq!(
        context.calls,
        expected_calls(case, scenario, stored),
        "{level:?} {case:?} {scenario:?}: helper order"
    );
    assert_eq!(
        context.bytes,
        if scenario == Scenario::Success {
            expected_memory
        } else {
            initial_memory
        },
        "{level:?} {case:?} {scenario:?}: memory commit"
    );
}

#[test]
fn native_evex_extract_matches_interpreter_and_preserves_e6nf_full_access_order() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    if cases.is_empty() {
        eprintln!(
            "skipping native EVEX extract-memory differential: host lacks required AVX-512 subsets"
        );
        return;
    }

    let supported = cases.len();
    let masked = cases
        .iter()
        .filter(|case| case.writemask().is_some())
        .count();
    let mut successes = 0usize;
    let mut load_faults = 0usize;
    let mut store_faults = 0usize;
    let mut empty_mask_executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let live = case.lanes();
        let lane_mask = (1u64 << live) - 1;
        let mut mask = 0xD6A5_3C69_F00F_5AA5u64.rotate_left((ordinal & 63) as u32) & lane_mask;
        if case.writemask().is_some() && mask == 0 {
            mask = 1;
        }
        let initial_memory = memory_bytes(ordinal);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let semantic = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
            let outcome = interpret_success(&function, &semantic, &initial_memory);
            assert_eq!(
                outcome.state, semantic,
                "{level:?} {case:?}: interpreter state"
            );

            execute_scenario(
                &executable,
                entry,
                case,
                level,
                &semantic,
                initial_memory,
                outcome.memory,
                Scenario::Success,
            );
            successes += 1;
            execute_scenario(
                &executable,
                entry,
                case,
                level,
                &semantic,
                initial_memory,
                outcome.memory,
                Scenario::StoreFault,
            );
            store_faults += 1;
            if case.writemask().is_some() {
                execute_scenario(
                    &executable,
                    entry,
                    case,
                    level,
                    &semantic,
                    initial_memory,
                    outcome.memory,
                    Scenario::LoadFault,
                );
                load_faults += 1;

                let empty = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, 0);
                let empty_outcome = interpret_success(&function, &empty, &initial_memory);
                assert_eq!(empty_outcome.state, empty, "{level:?} {case:?}: empty mask");
                for scenario in [Scenario::Success, Scenario::LoadFault, Scenario::StoreFault] {
                    execute_scenario(
                        &executable,
                        entry,
                        case,
                        level,
                        &empty,
                        initial_memory,
                        empty_outcome.memory,
                        scenario,
                    );
                    empty_mask_executions += 1;
                }
            }
        }
    }
    assert_eq!(successes, supported * 2);
    assert_eq!(store_faults, supported * 2);
    assert_eq!(load_faults, masked * 2);
    assert_eq!(empty_mask_executions, masked * 2 * 3);
}
