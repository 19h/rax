//! Native x86-64 differential, helper ordering, and precise-fault coverage.

use super::semantics::{
    bytes_to_words, initial_registers, interpret_success, memory_bytes, words_to_bytes,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ScalarMemoryContext {
    base: u64,
    value: [u8; 64],
    lane_bytes: usize,
    fail_call: Option<usize>,
    calls: usize,
    addresses: [u64; 64],
    sizes: [u64; 64],
}

extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size as usize, context.lane_bytes);
    assert_eq!(signed, 0);
    let call = context.calls;
    context.addresses[call] = address;
    context.sizes[call] = size;
    context.calls += 1;
    if context.fail_call == Some(call) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + context.lane_bytes <= context.value.len());
    let mut value = [0u8; 8];
    value[..context.lane_bytes]
        .copy_from_slice(&context.value[offset..offset + context.lane_bytes]);
    LoadResult {
        value: u64::from_le_bytes(value),
        ok: 1,
    }
}

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
        || !matches!(size, 4 | 8 | 16 | 32 | 64)
        || zero_upper != 1
    {
        return 0;
    }
    let mut scratch = [0u8; 64];
    scratch[..size as usize].copy_from_slice(&context.value[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

fn selected_cases() -> [ConvertCase; 9] {
    [
        ConvertCase {
            spec: SPECS[0],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[1],
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[2],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[4],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        ConvertCase {
            spec: SPECS[5],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[8],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        ConvertCase {
            spec: SPECS[10],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[12],
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[12],
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
    ]
}

fn expected_scalar_addresses(case: ConvertCase, mask: u64) -> Vec<u64> {
    if case.broadcast() {
        if case.mask() == 0 || mask & ((1u64 << case.lanes()) - 1) != 0 {
            vec![MEMORY_ADDRESS]
        } else {
            Vec::new()
        }
    } else {
        (0..case.lanes())
            .filter(|lane| mask & (1u64 << lane) != 0)
            .map(|lane| {
                MEMORY_ADDRESS + u64::from(lane) * u64::from(case.spec.source_elem().bytes())
            })
            .collect()
    }
}

fn expected_vector_scratch(case: ConvertCase, memory: &[u8; 64]) -> [u64; 8] {
    let mut scratch = [0u8; 64];
    let size = case.memory_size() as usize;
    scratch[..size].copy_from_slice(&memory[..size]);
    bytes_to_words(scratch)
}

#[test]
fn native_fp16_conversions_match_interpreter_helpers_faults_and_mask_suppression() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!("skipping native packed FP16 conversion differential: host lacks F/BW/VL/FP16");
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    let mut reconstructed_broadcasts = 0usize;
    for (ordinal, case) in selected_cases().into_iter().enumerate() {
        let lane_mask = (1u64 << case.lanes()) - 1;
        let mask = if case.mask() == 0 {
            lane_mask
        } else {
            (0xA5A5_A5A5 ^ ordinal as u64) & lane_mask
        };
        assert!(case.mask() == 0 || mask != 0);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory = memory_bytes(case, ordinal + 3);
            let initial = initial_registers(case, ordinal + 3, 0x1F80, mask);

            if case.form == SourceForm::Vector && case.control == MaskControl::None {
                let mut context = VectorMemoryContext {
                    value: memory,
                    ok: true,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial;
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
                assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
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
                let mut fault_registers = initial;
                fault_registers.ctx = (&mut fault_context as *mut VectorMemoryContext) as u64;
                fault_registers.vec_load_fn = vector_load_helper as usize as u64;
                let mut expected_fault = fault_registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut fault_registers);
                expected_fault.host_mxcsr = fault_registers.host_mxcsr;
                assert_eq!(fault_registers, expected_fault, "{level:?} {case:?}: fault");
                assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault calls");
                faults += 1;
                continue;
            }

            let expected_addresses = expected_scalar_addresses(case, mask);
            let mut context = ScalarMemoryContext {
                base: MEMORY_ADDRESS,
                value: memory,
                lane_bytes: case.spec.source_elem().bytes() as usize,
                fail_call: None,
                calls: 0,
                addresses: [0; 64],
                sizes: [0; 64],
            };
            let mut registers = initial;
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected = interpret_success(&function, &registers, &memory, case);
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: helper addresses"
            );
            assert!(
                context.sizes[..context.calls]
                    .iter()
                    .all(|size| *size == u64::from(case.spec.source_elem().bytes())),
                "{level:?} {case:?}: helper sizes"
            );
            successes += 1;
            reconstructed_broadcasts += usize::from(case.spec == SPECS[4] && case.broadcast());

            let fail_call = expected_addresses.len() / 2;
            let mut fault_context = ScalarMemoryContext {
                base: MEMORY_ADDRESS,
                value: memory,
                lane_bytes: case.spec.source_elem().bytes() as usize,
                fail_call: Some(fail_call),
                calls: 0,
                addresses: [0; 64],
                sizes: [0; 64],
            };
            let mut fault_registers = initial;
            fault_registers.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected_fault = fault_registers;
            expected_fault.exit_pc = PC;
            exec.run(entry, &mut fault_registers);
            expected_fault.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(fault_registers, expected_fault, "{level:?} {case:?}: fault");
            assert_eq!(
                &fault_context.addresses[..fault_context.calls],
                &expected_addresses[..=fail_call],
                "{level:?} {case:?}: calls through fault"
            );
            faults += 1;

            if case.mask() != 0 {
                let empty = initial_registers(case, ordinal + 9, 0x1F80, 0);
                let mut suppressed_context = ScalarMemoryContext {
                    base: MEMORY_ADDRESS,
                    value: memory,
                    lane_bytes: case.spec.source_elem().bytes() as usize,
                    fail_call: Some(0),
                    calls: 0,
                    addresses: [0; 64],
                    sizes: [0; 64],
                };
                let mut suppressed = empty;
                suppressed.ctx = (&mut suppressed_context as *mut ScalarMemoryContext) as u64;
                suppressed.load_fn = scalar_load_helper as usize as u64;
                let mut expected = interpret_success(&function, &suppressed, &memory, case);
                exec.run(entry, &mut suppressed);
                expected.host_mxcsr = suppressed.host_mxcsr;
                assert_eq!(suppressed, expected, "{level:?} {case:?}: suppression");
                assert_eq!(suppressed_context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, faults);
    assert_eq!(successes, selected_cases().len() * 2);
    assert!(suppressions >= 6);
    assert!(reconstructed_broadcasts >= 2);
    assert_eq!(
        bytes_to_words(words_to_bytes([0x0123_4567_89AB_CDEF; 8])),
        [0x0123_4567_89AB_CDEF; 8]
    );
}
