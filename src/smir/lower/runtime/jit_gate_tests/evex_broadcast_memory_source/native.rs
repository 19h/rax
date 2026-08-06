//! Host-native differential, helper ordering, mask suppression, and faults.

use super::semantics::{
    bytes_to_words, expected_destination, initial_registers, interpret_success, memory_bytes,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

struct VectorMemoryContext {
    value: [u8; 64],
    ok: bool,
    calls: usize,
    last_addr: u64,
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
    context.last_addr = address;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if !context.ok
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 1 | 2 | 4 | 8 | 16 | 32)
        || zero_upper != 1
    {
        return 0;
    }
    let mut scratch = [0u8; 64];
    scratch[..size as usize].copy_from_slice(&context.value[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

fn selected_cases() -> [BroadcastMemoryCase; 8] {
    [
        BroadcastMemoryCase {
            shape: SHAPES[28],
            destination: 17,
            base: 2,
            control: MaskControl::Merge,
        },
        BroadcastMemoryCase {
            shape: SHAPES[31],
            destination: 17,
            base: 2,
            control: MaskControl::Zero,
        },
        BroadcastMemoryCase {
            shape: SHAPES[2],
            destination: 17,
            base: 2,
            control: MaskControl::None,
        },
        BroadcastMemoryCase {
            shape: SHAPES[4],
            destination: 17,
            base: 2,
            control: MaskControl::Merge,
        },
        BroadcastMemoryCase {
            shape: SHAPES[8],
            destination: 17,
            base: 2,
            control: MaskControl::Zero,
        },
        BroadcastMemoryCase {
            shape: SHAPES[11],
            destination: 17,
            base: 2,
            control: MaskControl::Merge,
        },
        BroadcastMemoryCase {
            shape: SHAPES[27],
            destination: 17,
            base: 2,
            control: MaskControl::Zero,
        },
        BroadcastMemoryCase {
            shape: SHAPES[19],
            destination: 17,
            base: 2,
            control: MaskControl::None,
        },
    ]
}

fn native_supported(case: BroadcastMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx")
        && std::is_x86_feature_detected!("avx512f")
        && (!case.shape.needs_avx512bw || std::is_x86_feature_detected!("avx512bw"))
        && (!case.shape.needs_avx512dq || std::is_x86_feature_detected!("avx512dq"))
        && (case.shape.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

fn expected_vector_scratch(case: BroadcastMemoryCase, memory: &[u8; 64]) -> [u64; 8] {
    let mut scratch = [0u8; 64];
    let size = case.shape.memory_size() as usize;
    scratch[..size].copy_from_slice(&memory[..size]);
    bytes_to_words(scratch)
}

#[test]
fn native_broadcasts_match_interpreter_and_preserve_type_e6_fault_suppression() {
    let cases = selected_cases()
        .into_iter()
        .filter(|case| native_supported(*case))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!("skipping native EVEX broadcast differential: host lacks AVX-512 support");
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let lanes = case.shape.destination_lanes();
        let lane_mask = if lanes == 64 {
            u64::MAX
        } else {
            (1u64 << lanes) - 1
        };
        let active_mask = if case.mask() == 0 {
            lane_mask
        } else {
            (0xA5A5_A5A5_A5A5_A5A5u64 | 1) & lane_mask
        };
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory = memory_bytes(case, ordinal + 3);

            let mut context = VectorMemoryContext {
                value: memory,
                ok: true,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = initial_registers(case, ordinal + 3, active_mask);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = interpret_success(&function, &registers, &memory, case);
            expected.vector_scratch = expected_vector_scratch(case, &memory);
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.shape.memory_size(),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = VectorMemoryContext {
                value: memory,
                ok: false,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut fault_registers = initial_registers(case, ordinal + 9, active_mask);
            fault_registers.ctx = (&mut fault_context as *mut VectorMemoryContext) as u64;
            fault_registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected_fault = fault_registers;
            expected_fault.exit_pc = PC;
            exec.run(entry, &mut fault_registers);
            expected_fault.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, expected_fault,
                "{level:?} {case:?}: fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault calls");
            assert_eq!(
                fault_context.last_size,
                case.shape.memory_size(),
                "{level:?} {case:?}: fault tuple size"
            );
            faults += 1;

            if case.mask() != 0 {
                let mut masks = vec![0];
                if lanes < 64 {
                    masks.push(1u64 << lanes);
                }
                for suppressed_mask in masks {
                    let mut suppressed_context = VectorMemoryContext {
                        value: memory,
                        ok: false,
                        calls: 0,
                        last_addr: 0,
                        last_index: 0,
                        last_size: 0,
                        last_zero_upper: 0,
                    };
                    let mut suppressed = initial_registers(case, ordinal + 15, suppressed_mask);
                    suppressed.ctx = (&mut suppressed_context as *mut VectorMemoryContext) as u64;
                    suppressed.vec_load_fn = vector_load_helper as usize as u64;
                    let mut expected = interpret_success(&function, &suppressed, &memory, case);
                    assert_eq!(
                        expected.zmm[usize::from(case.destination)],
                        expected_destination(case, &suppressed, &memory),
                        "{level:?} {case:?}: oracle"
                    );
                    exec.run(entry, &mut suppressed);
                    expected.host_mxcsr = suppressed.host_mxcsr;
                    assert_eq!(
                        suppressed, expected,
                        "{level:?} {case:?}: mask={suppressed_mask:#x}"
                    );
                    assert_eq!(
                        suppressed_context.calls, 0,
                        "{level:?} {case:?}: suppressed helper"
                    );
                    suppressions += 1;
                }
            }
        }
    }
    assert!(successes >= 2);
    assert_eq!(faults, successes);
    assert!(suppressions >= 2);
}
