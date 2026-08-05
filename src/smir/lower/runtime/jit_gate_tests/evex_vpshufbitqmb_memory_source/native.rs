//! Host-native differential, helper-footprint, scratch, and fault coverage.

use super::semantics::{initial_registers, interpreter_success, memory_value};
use super::*;
use crate::smir::lower::runtime::ExecMem;

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct LaneMemoryContext {
    base: u64,
    value: [u8; 64],
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 64],
}

extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size, 1);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    LoadResult {
        value: u64::from(context.value[offset]),
        ok: 1,
    }
}

fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn expected_vector_scratch(value: [u64; 8], width: VecWidth) -> [u64; 8] {
    let words = (width.bytes() / 8) as usize;
    std::array::from_fn(|word| if word < words { value[word] } else { 0 })
}

fn native_supported(case: VpshufbitqmbMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512bitalg")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

fn selected_cases() -> [VpshufbitqmbMemoryCase; 6] {
    [
        VpshufbitqmbMemoryCase {
            width: VecWidth::V128,
            destination: 2,
            source1: 0,
            mask: 0,
        },
        VpshufbitqmbMemoryCase {
            width: VecWidth::V128,
            destination: 1,
            source1: 17,
            mask: 1,
        },
        VpshufbitqmbMemoryCase {
            width: VecWidth::V256,
            destination: 3,
            source1: 15,
            mask: 0,
        },
        VpshufbitqmbMemoryCase {
            width: VecWidth::V256,
            destination: 7,
            source1: 31,
            mask: 1,
        },
        VpshufbitqmbMemoryCase {
            width: VecWidth::V512,
            destination: 5,
            source1: 16,
            mask: 0,
        },
        VpshufbitqmbMemoryCase {
            width: VecWidth::V512,
            destination: 7,
            source1: 31,
            mask: 7,
        },
    ]
}

#[test]
fn native_vpshufbitqmb_matches_interpreter_and_is_fault_precise() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    let cases = selected_cases()
        .into_iter()
        .filter(|case| native_supported(*case))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!(
            "skipping native VPSHUFBITQMB memory differential: host lacks AVX-512F/BW/BITALG"
        );
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let value = memory_value(ordinal + 503);

            if case.mask == 0 {
                let mut context = VectorMemoryContext {
                    value,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal + 503);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);
                expected.vector_scratch = expected_vector_scratch(value, case.width);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: success");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, 0x2000, "{level:?} {case:?}");
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                    "{level:?} {case:?}"
                );
                assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
                assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
                successes += 1;

                let mut context = VectorMemoryContext {
                    value,
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal + 701);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: fault");
                assert_eq!(context.calls, 1, "{level:?} {case:?}: fault");
                faults += 1;
                continue;
            }

            let bytes = memory_bytes(value);
            let mut registers = initial_registers(case, ordinal + 503);
            registers.k[usize::from(case.mask)] = 0xA5A5_5A5A_C33C_6996;
            let active_mask = registers.k[usize::from(case.mask)];
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: None,
                calls: 0,
                addresses: [0; 64],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, value, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            let expected_addresses = (0..case.width.bytes())
                .filter(|lane| active_mask & (1u64 << lane) != 0)
                .map(|lane| 0x2000 + u64::from(lane))
                .collect::<Vec<_>>();
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: ascending active byte addresses"
            );
            successes += 1;

            let mut registers = initial_registers(case, ordinal + 701);
            registers.k[usize::from(case.mask)] = 0b1101;
            let fail_address = 0x2002;
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 64],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(
                &context.addresses[..context.calls],
                &[0x2000, fail_address],
                "{level:?} {case:?}: exact fault frontier"
            );
            faults += 1;

            let mut registers = initial_registers(case, ordinal + 809);
            registers.k[usize::from(case.mask)] = 0;
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: Some(0x2000),
                calls: 0,
                addresses: [0; 64],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, value, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: suppression");
            assert_eq!(context.calls, 0, "{level:?} {case:?}: suppression");
            suppressions += 1;
        }
    }
    assert!(successes >= LEVELS.len());
    assert_eq!(successes, faults);
    assert!(suppressions >= LEVELS.len());
}
