use super::*;
use crate::smir::lower::runtime::ExecMem;

#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: u64,
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
        || !matches!(size, 2 | 4 | 8)
    {
        return 0;
    }

    let mut scratch = [0u8; 64];
    scratch[..size as usize].copy_from_slice(&context.value.to_le_bytes()[..size as usize]);
    state.vector_scratch = std::array::from_fn(|word| {
        u64::from_le_bytes(scratch[word * 8..word * 8 + 8].try_into().unwrap())
    });
    1
}

fn native_cases() -> Vec<(ScalarFpToIntMemoryCase, u64, u32)> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    let source = if signed && ordinal & 1 != 0 {
                        format.negative_two_and_half()
                    } else {
                        format.positive_two_and_half()
                    };
                    let case = ScalarFpToIntMemoryCase {
                        format,
                        signed,
                        truncate,
                        w,
                        ll: (ordinal % 3) as u8,
                        destination: [0, 1, 8, 15][ordinal % 4],
                        base: 2,
                    };
                    let mxcsr = (0x1F80 & !(3 << 13)) | (((ordinal & 3) as u32) << 13);
                    cases.push((case, source, mxcsr));
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

fn expected_scratch(source: u64, size: usize) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    bytes[..size].copy_from_slice(&source.to_le_bytes()[..size]);
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn assert_helper_observation(context: &VectorMemoryContext, case: ScalarFpToIntMemoryCase) {
    assert_eq!(context.calls, 1, "{case:?}");
    assert_eq!(context.last_addr, MEMORY_ADDRESS, "{case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{case:?}"
    );
    assert_eq!(context.last_size, case.memory_size() as u32, "{case:?}");
    assert_eq!(context.last_zero_upper, 1, "{case:?}");
}

#[test]
fn native_memory_replay_matches_interpretation_restores_xmm0_and_faults_noncommitting() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!(
            "skipping native EVEX scalar FP-to-integer memory differential: \
             host lacks AVX-512F/BW"
        );
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 3 * 2 * 2 * 2);
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut rax_results = 0usize;
    for (ordinal, (case, source, mxcsr)) in cases.into_iter().enumerate() {
        if case.format.needs_fp16() && !std::is_x86_feature_detected!("avx512fp16") {
            continue;
        }
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let mut context = VectorMemoryContext {
                value: source,
                ok: true,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = initial_registers(case, ordinal, mxcsr);
            let initial_xmm0 = registers.zmm[0];
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, case);
            expected.vector_scratch = expected_scratch(source, case.memory_size());

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(registers.zmm[0], initial_xmm0, "{level:?} {case:?}: XMM0");
            assert_helper_observation(&context, case);
            successes += 1;
            rax_results += usize::from(case.destination == 0);

            let mut fault_context = VectorMemoryContext {
                value: source ^ u64::MAX,
                ok: false,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut fault_registers = initial_registers(case, ordinal ^ 0x55, mxcsr);
            fault_registers.ctx = (&mut fault_context as *mut VectorMemoryContext) as u64;
            fault_registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: helper fault committed architectural state"
            );
            assert_helper_observation(&fault_context, case);
            faults += 1;
        }
    }
    assert_eq!(successes, faults);
    assert!(successes >= 2 * 2 * 2 * 2 * 2); // F32/F64, O0/O2.
    assert!(rax_results >= 4, "RAX result must survive XMM0 restoration");
}
