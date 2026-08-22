use super::*;
use crate::smir::lower::runtime::ExecMem;

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[derive(Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
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
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

fn native_cases() -> Vec<ScalarUnaryMemoryCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for operation in UnaryOperation::ALL {
        for format in ScalarFormat::ALL {
            for control in MaskControl::ALL {
                cases.push(ScalarUnaryMemoryCase {
                    operation,
                    format,
                    destination: [0, 17, 31][ordinal % 3],
                    merge: [1, 17, 30][ordinal % 3],
                    ll: (ordinal % 3) as u8,
                    control,
                    // M=0 makes inexact REDUCE/RNDSCALE cases exercise the
                    // unsuppressed precision path on native hardware. RC is
                    // still selected dynamically from MXCSR.
                    immediate: 0x04,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

#[test]
fn native_scalar_unary_memory_matches_interpretation_faults_and_mask_suppression() {
    if !std::is_x86_feature_detected!("avx")
        || !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
    {
        eprintln!(
            "skipping native scalar EVEX floating-point unary differential: \
             host lacks AVX/AVX-512F/BW"
        );
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 4 * 3 * 3);
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        if case.format == ScalarFormat::F16 && !std::is_x86_feature_detected!("avx512fp16") {
            continue;
        }
        if case.needs_dq() && !std::is_x86_feature_detected!("avx512dq") {
            continue;
        }
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_bits(case.format, ordinal);

            let mut context = ScalarMemoryContext {
                value: source,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = initial_registers(case, ordinal, true);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = ScalarMemoryContext {
                value: source ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = initial_registers(case, ordinal ^ 0x55, true);
            fault_registers.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: source fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault");
            assert_eq!(
                fault_context.last_addr, MEMORY_ADDRESS,
                "{level:?} {case:?}: fault"
            );
            assert_eq!(
                fault_context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}: fault"
            );
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}: fault");
            faults += 1;

            if case.control != MaskControl::None {
                let mut suppressed_context = ScalarMemoryContext {
                    value: source ^ u64::MAX,
                    ok: 0,
                    ..ScalarMemoryContext::default()
                };
                let mut suppressed = initial_registers(case, ordinal ^ 0xAA, false);
                suppressed.ctx = (&mut suppressed_context as *mut ScalarMemoryContext) as u64;
                suppressed.load_fn = scalar_load_helper as usize as u64;
                let mut suppressed_expected =
                    interpreter_success(&function, &suppressed, source, case);

                exec.run(entry, &mut suppressed);
                suppressed_expected.host_mxcsr = suppressed.host_mxcsr;
                assert_eq!(
                    suppressed, suppressed_expected,
                    "{level:?} {case:?}: inactive memory access"
                );
                assert_eq!(
                    suppressed_context.calls, 0,
                    "{level:?} {case:?}: inactive mask called helper"
                );
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, faults);
    assert!(successes >= 3 * 2 * 3 * 2); // non-DQ classic F32/F64 families.
    assert!(suppressions >= 3 * 2 * 2 * 2);
}
