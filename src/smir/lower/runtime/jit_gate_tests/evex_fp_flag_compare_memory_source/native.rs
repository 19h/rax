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

#[derive(Clone, Copy)]
enum Relation {
    Less,
    Equal,
    Greater,
}

fn finite_values(format: Format, relation: Relation) -> (u64, u64) {
    let (one, two, three) = match format {
        Format::F16 => (0x3C00, 0x4000, 0x4200),
        Format::F32 => (0x3F80_0000, 0x4000_0000, 0x4040_0000),
        Format::F64 => (
            0x3FF0_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x4008_0000_0000_0000,
        ),
    };
    match relation {
        Relation::Less => (one, two),
        Relation::Equal => (two, two),
        Relation::Greater => (three, two),
    }
}

fn host_supports(format: Format) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (format != Format::F16 || std::is_x86_feature_detected!("avx512fp16"))
}

fn assert_helper(context: &ScalarMemoryContext, case: Case, label: impl std::fmt::Debug) {
    assert_eq!(context.calls, 1, "{label:?} {case:?}");
    assert_eq!(context.last_addr, MEMORY_ADDRESS, "{label:?} {case:?}");
    assert_eq!(
        context.last_size,
        case.format.memory_size() as u64,
        "{label:?} {case:?}"
    );
    assert_eq!(context.last_signed, 0, "{label:?} {case:?}");
}

#[test]
fn native_finite_results_match_interpreter_and_faults_are_precise_noncommitting() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX VCOMI/VUCOMI memory differential: host lacks AVX-512F/BW");
        return;
    }

    let relations = [Relation::Less, Relation::Equal, Relation::Greater];
    let mut successes = 0usize;
    let mut faults = 0usize;
    for format in Format::ALL {
        if !host_supports(format) {
            continue;
        }
        for signaling in [false, true] {
            for source1 in [1, 9, 17, 30] {
                for ll in 0..=2 {
                    let case = Case {
                        format,
                        signaling,
                        source1,
                        ll,
                    };
                    let relation = relations[(successes + faults) % relations.len()];
                    let (first, second) = finite_values(format, relation);
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = optimize(lift_case(case), level);
                        let (code, entry) = lower(&function, case);
                        let exec = ExecMem::new(&code)
                            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

                        let mut context = ScalarMemoryContext {
                            value: second,
                            ok: 1,
                            ..ScalarMemoryContext::default()
                        };
                        let mut registers = initial_registers(case, successes);
                        set_source1(&mut registers, case, first);
                        registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                        registers.load_fn = scalar_load_helper as usize as u64;
                        let mut expected = interpreter_success(&function, &registers, second, case);

                        exec.run(entry, &mut registers);
                        expected.host_mxcsr = registers.host_mxcsr;
                        assert_eq!(registers, expected, "{level:?} {case:?}: success");
                        assert_helper(&context, case, (level, "success"));
                        successes += 1;

                        let mut fault_context = ScalarMemoryContext {
                            value: second ^ u64::MAX,
                            ok: 0,
                            ..ScalarMemoryContext::default()
                        };
                        let mut fault_registers = initial_registers(case, faults ^ 0x55);
                        set_source1(&mut fault_registers, case, first);
                        fault_registers.ctx =
                            (&mut fault_context as *mut ScalarMemoryContext) as u64;
                        fault_registers.load_fn = scalar_load_helper as usize as u64;
                        let mut fault_expected = fault_registers;
                        fault_expected.exit_pc = PC;

                        exec.run(entry, &mut fault_registers);
                        fault_expected.host_mxcsr = fault_registers.host_mxcsr;
                        assert_eq!(
                            fault_registers, fault_expected,
                            "{level:?} {case:?}: source fault committed state"
                        );
                        assert_helper(&fault_context, case, (level, "fault"));
                        faults += 1;
                    }
                }
            }
        }
    }
    assert_eq!(successes, faults);
    assert!(successes >= 48 * 2);
}

#[test]
fn native_masked_nan_status_and_flags_match_interpreter() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX VCOMI/VUCOMI NaN differential: host lacks AVX-512F/BW");
        return;
    }

    let mut executions = 0usize;
    for format in Format::ALL {
        if !host_supports(format) {
            continue;
        }
        let one = finite_values(format, Relation::Equal).0;
        let qnan = match format {
            Format::F16 => 0x7E11,
            Format::F32 => 0x7FC0_0011,
            Format::F64 => 0x7FF8_0000_0000_0011,
        };
        let snan = match format {
            Format::F16 => 0x7C11,
            Format::F32 => 0x7F80_0011,
            Format::F64 => 0x7FF0_0000_0000_0011,
        };
        for signaling in [false, true] {
            for first in [qnan, snan] {
                let case = Case {
                    format,
                    signaling,
                    source1: 17,
                    ll: 1,
                };
                let function = optimize(lift_case(case), OptLevel::O2);
                let (code, entry) = lower(&function, case);
                let exec = ExecMem::new(&code).unwrap();
                let mut context = ScalarMemoryContext {
                    value: one,
                    ok: 1,
                    ..ScalarMemoryContext::default()
                };
                let mut registers = initial_registers(case, executions);
                set_source1(&mut registers, case, first);
                registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                registers.load_fn = scalar_load_helper as usize as u64;
                let mut expected = interpreter_success(&function, &registers, one, case);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{case:?} first={first:#018X}");
                assert_helper(&context, case, "NaN");
                executions += 1;
            }
        }
    }
    assert!(executions >= 8);
}
