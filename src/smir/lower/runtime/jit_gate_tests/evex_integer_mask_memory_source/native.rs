//! Host-native differential and helper-fault coverage for integer mask operations.

use super::semantics::{initial_registers, interpreter_success, memory_bytes, memory_value};
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
    lane_bytes: usize,
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
    assert_eq!(size as usize, context.lane_bytes);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + context.lane_bytes <= context.value.len());
    let mut raw = [0u8; 8];
    raw[..context.lane_bytes].copy_from_slice(&context.value[offset..offset + context.lane_bytes]);
    LoadResult {
        value: u64::from_le_bytes(raw),
        ok: 1,
    }
}

fn expected_vector_scratch(value: [u64; 8], width: VecWidth) -> [u64; 8] {
    let words = (width.bytes() / 8) as usize;
    std::array::from_fn(|word| if word < words { value[word] } else { 0 })
}

fn native_supported(case: IntegerMaskMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

fn selected_case(
    name: &str,
    width: VecWidth,
    form: SourceForm,
    mask: u8,
    immediate: u8,
    destination: u8,
    source1: u8,
) -> IntegerMaskMemoryCase {
    let kind = KINDS.into_iter().find(|kind| kind.name == name).unwrap();
    IntegerMaskMemoryCase {
        kind,
        width,
        destination,
        source1,
        w: kind.w_values()[0],
        form,
        mask,
        immediate,
    }
}

#[test]
fn native_integer_mask_memory_matches_interpreter_and_is_fault_precise() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    let selected = [
        selected_case("VPCMPEQB", VecWidth::V128, SourceForm::Vector, 0, 0, 1, 17),
        selected_case(
            "VPCMPGTQ",
            VecWidth::V256,
            SourceForm::Broadcast,
            0,
            0,
            3,
            31,
        ),
        selected_case("VPCMPB", VecWidth::V512, SourceForm::Vector, 1, 3, 7, 17),
        selected_case(
            "VPCMPUD",
            VecWidth::V128,
            SourceForm::Broadcast,
            1,
            7,
            1,
            31,
        ),
        selected_case("VPTESTMW", VecWidth::V512, SourceForm::Vector, 1, 0, 7, 17),
        selected_case(
            "VPTESTNMD",
            VecWidth::V256,
            SourceForm::Broadcast,
            1,
            0,
            3,
            31,
        ),
        selected_case("VPTESTMQ", VecWidth::V512, SourceForm::Vector, 0, 0, 7, 17),
    ];
    let cases = selected
        .into_iter()
        .filter(|case| native_supported(*case))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!("skipping native integer mask memory differential: host lacks AVX-512F/BW");
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
            let value = memory_value(case, ordinal);
            let bytes = memory_bytes(value);

            if case.form == SourceForm::Vector && case.mask == 0 {
                let mut context = VectorMemoryContext {
                    value,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal);
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
                let mut registers = initial_registers(case, ordinal ^ 0x55);
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

            let mut registers = initial_registers(case, ordinal);
            if case.mask != 0 {
                registers.k[usize::from(case.mask)] = 0x5555_5555_5555_5555;
            }
            let active_mask = if case.mask == 0 {
                u64::MAX
            } else {
                registers.k[usize::from(case.mask)]
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem.bytes() as usize,
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
            let expected_addresses = if case.broadcast() {
                vec![0x2000]
            } else {
                (0..case.width.lanes(case.kind.elem))
                    .filter(|lane| active_mask & (1u64 << lane) != 0)
                    .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.kind.elem.bytes()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: ascending active source addresses"
            );
            successes += 1;

            let mut registers = initial_registers(case, ordinal ^ 0x55);
            if case.mask != 0 {
                registers.k[usize::from(case.mask)] = 0b1101;
            }
            let fail_address = if case.broadcast() {
                0x2000
            } else {
                0x2000 + 2 * u64::from(case.kind.elem.bytes())
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem.bytes() as usize,
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
                context.addresses[context.calls - 1],
                fail_address,
                "{level:?} {case:?}: exact faulting address"
            );
            faults += 1;

            if case.mask != 0 {
                let mut registers = initial_registers(case, ordinal ^ 0xAA);
                registers.k[usize::from(case.mask)] = 0;
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    lane_bytes: case.kind.elem.bytes() as usize,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 64],
                };
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected,
                    "{level:?} {case:?}: all source lanes suppressed"
                );
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert!(successes >= LEVELS.len());
    assert_eq!(successes, faults);
    assert!(suppressions >= LEVELS.len());
}
