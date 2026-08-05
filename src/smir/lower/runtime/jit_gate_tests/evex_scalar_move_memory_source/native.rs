use super::*;
use crate::smir::lower::runtime::ExecMem;

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ScalarMemoryContext {
    bytes: [u8; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
    last_store_value: u64,
}

impl ScalarMemoryContext {
    fn new(bytes: [u8; 8], ok: bool) -> Self {
        Self {
            bytes,
            ok: u64::from(ok),
            calls: 0,
            last_addr: 0,
            last_size: 0,
            last_signed: 0,
            last_store_value: 0,
        }
    }
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
        value: u64::from_le_bytes(context.bytes),
        ok: context.ok,
    }
}

extern "C" fn scalar_store_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_store_value = value;
    if context.ok == 0 {
        return 0;
    }
    let size = usize::try_from(size).unwrap();
    context.bytes[..size].copy_from_slice(&value.to_le_bytes()[..size]);
    1
}

fn bind_helpers(registers: &mut GuestRegs, context: &mut ScalarMemoryContext) {
    registers.ctx = (context as *mut ScalarMemoryContext) as u64;
    registers.load_fn = scalar_load_helper as usize as u64;
    registers.store_fn = scalar_store_helper as usize as u64;
}

#[test]
fn native_scalar_moves_match_independent_oracle_faults_and_mask_suppression() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX scalar move memory differential: host lacks AVX-512F/BW");
        return;
    }

    let fp16 = std::is_x86_feature_detected!("avx512fp16");
    let supported_formats = 2 + usize::from(fp16);
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        if case.format == ScalarFormat::F16 && !fp16 {
            continue;
        }
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory_before = (0xA1B2_C3D4_E5F6_0718u64
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081))
            .to_le_bytes();

            let mut context = ScalarMemoryContext::new(memory_before, true);
            let mut registers = initial_registers(case, ordinal, true);
            bind_helpers(&mut registers, &mut context);
            let mut expected = independent_success_oracle(&registers, memory_before, case, true).0;
            let expected_memory =
                independent_success_oracle(&registers, memory_before, case, true).1;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}: success calls");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(context.bytes, expected_memory, "{level:?} {case:?}");
            match case.direction {
                Direction::Load => assert_eq!(context.last_signed, 0, "{level:?} {case:?}"),
                Direction::Store => assert_eq!(
                    context.last_store_value & case.format.scalar_mask(),
                    expected_memory
                        .iter()
                        .take(case.format.memory_size())
                        .enumerate()
                        .fold(0u64, |value, (index, byte)| {
                            value | (u64::from(*byte) << (index * 8))
                        }),
                    "{level:?} {case:?}"
                ),
            }
            successes += 1;

            let fault_memory = 0x55AA_33CC_0FF0_9696u64.to_le_bytes();
            let mut fault_context = ScalarMemoryContext::new(fault_memory, false);
            let mut fault_registers = initial_registers(case, ordinal ^ 0x55, true);
            bind_helpers(&mut fault_registers, &mut fault_context);
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed architectural state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault calls");
            assert_eq!(fault_context.bytes, fault_memory, "{level:?} {case:?}");
            assert_eq!(
                fault_context.last_addr, MEMORY_ADDRESS,
                "{level:?} {case:?}"
            );
            assert_eq!(
                fault_context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            faults += 1;

            if case.control != MaskControl::None {
                let suppressed_memory = 0x0F1E_2D3C_4B5A_6978u64.to_le_bytes();
                let mut suppressed_context = ScalarMemoryContext::new(suppressed_memory, false);
                let mut suppressed = initial_registers(case, ordinal ^ 0xAA, false);
                bind_helpers(&mut suppressed, &mut suppressed_context);
                let mut suppressed_expected =
                    independent_success_oracle(&suppressed, suppressed_memory, case, false).0;

                exec.run(entry, &mut suppressed);
                suppressed_expected.host_mxcsr = suppressed.host_mxcsr;
                assert_eq!(
                    suppressed, suppressed_expected,
                    "{level:?} {case:?}: suppressed access"
                );
                assert_eq!(
                    suppressed_context.calls, 0,
                    "{level:?} {case:?}: suppressed helper call"
                );
                assert_eq!(suppressed_context.bytes, suppressed_memory);
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, supported_formats * 15 * 2);
    assert_eq!(faults, successes);
    assert_eq!(suppressions, supported_formats * 9 * 2);
}
