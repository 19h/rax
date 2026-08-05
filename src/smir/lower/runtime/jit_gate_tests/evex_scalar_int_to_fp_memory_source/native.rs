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
        || !matches!(size, 4 | 8)
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

fn native_cases() -> Vec<(ScalarIntMemoryCase, u64, u32)> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                let positive = match format {
                    DestinationFormat::F16 => 2049,
                    DestinationFormat::F32 => 16_777_217,
                    DestinationFormat::F64 => 9_007_199_254_740_993,
                };
                let source = if signed && ordinal & 1 != 0 {
                    if w {
                        (-(positive as i64)) as u64
                    } else {
                        (-(positive as i32)) as u32 as u64
                    }
                } else {
                    positive
                };
                let case = ScalarIntMemoryCase {
                    format,
                    signed,
                    w,
                    ll: (ordinal % 3) as u8,
                    destination: [0, 17, 31][ordinal % 3],
                    merge: [1, 30, 16][ordinal % 3],
                    base: 2,
                };
                let mxcsr = (0x1F80 & !(3 << 13)) | (((ordinal & 3) as u32) << 13);
                cases.push((case, source, mxcsr));
                ordinal += 1;
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

fn assert_helper_observation(context: &VectorMemoryContext, case: ScalarIntMemoryCase) {
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
fn native_scalar_int_to_fp_memory_matches_interpretation_and_faults_noncommitting() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!(
            "skipping native EVEX scalar integer-to-FP memory differential: \
             host lacks AVX-512F/BW"
        );
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 3 * 2 * 2);
    let mut successes = 0usize;
    let mut faults = 0usize;
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
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, case);
            expected.vector_scratch = expected_scratch(source, case.memory_size());

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_observation(&context, case);
            successes += 1;

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
    assert!(successes >= 2 * 2 * 2 * 2); // F32/F64 without AVX-512FP16.
}
