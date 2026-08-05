//! Host-native differential, helper ordering, mask suppression, and fault precision.

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

fn native_supported(case: FpClassMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (case.elem == VecElementType::F16 || std::is_x86_feature_detected!("avx512dq"))
        && (case.elem != VecElementType::F16 || std::is_x86_feature_detected!("avx512fp16"))
        && (!case.needs_avx512vl() || std::is_x86_feature_detected!("avx512vl"))
}

fn selected_cases() -> [FpClassMemoryCase; 7] {
    [
        FpClassMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Vector,
            mask: 0,
            immediate: 0xFF,
        },
        FpClassMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V512,
            destination: 7,
            form: SourceForm::Vector,
            mask: 1,
            immediate: 0xA5,
        },
        FpClassMemoryCase {
            elem: VecElementType::F16,
            width: VecWidth::V512,
            destination: 7,
            form: SourceForm::Vector,
            mask: 1,
            immediate: 0x5A,
        },
        FpClassMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V256,
            destination: 3,
            form: SourceForm::Broadcast,
            mask: 0,
            immediate: 0xFF,
        },
        FpClassMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Broadcast,
            mask: 1,
            immediate: 0x21,
        },
        FpClassMemoryCase {
            elem: VecElementType::F16,
            width: VecWidth::V128,
            destination: 7,
            form: SourceForm::Scalar { ll: 3 },
            mask: 1,
            immediate: 0xFF,
        },
        FpClassMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Scalar { ll: 0 },
            mask: 0,
            immediate: 0x60,
        },
    ]
}

#[test]
fn native_fp_class_memory_matches_interpreter_and_is_fault_precise() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    let cases = selected_cases()
        .into_iter()
        .filter(|case| native_supported(*case))
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!(
            "skipping native VFPCLASS memory differential: host lacks required AVX-512 features"
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
            let value = memory_value(case, ordinal);

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
                let mut registers = initial_registers(case, ordinal, ordinal & 1 != 0);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);
                expected.vector_scratch = expected_vector_scratch(value, case.width);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: success");
                assert_eq!(context.calls, 1);
                assert_eq!(context.last_addr, MEMORY_ADDRESS);
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
                );
                assert_eq!(context.last_size, case.width.bytes());
                assert_eq!(context.last_zero_upper, 1);
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
                let mut registers = initial_registers(case, ordinal ^ 0x55, true);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: fault");
                assert_eq!(context.calls, 1);
                faults += 1;
                continue;
            }

            let bytes = memory_bytes(value);
            let mut registers = initial_registers(case, ordinal, ordinal & 1 != 0);
            if case.mask != 0 {
                registers.k[usize::from(case.mask)] = if case.scalar() { 1 } else { 0b1101 };
            }
            let active = if case.mask == 0 {
                u64::MAX
            } else {
                registers.k[usize::from(case.mask)]
            };
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
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
            let expected_addresses = if case.scalar() || case.broadcast() {
                vec![MEMORY_ADDRESS]
            } else {
                (0..case.width.lanes(case.elem))
                    .filter(|lane| active & (1u64 << lane) != 0)
                    .map(|lane| MEMORY_ADDRESS + u64::from(lane) * u64::from(case.elem.bytes()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: ascending active source addresses"
            );
            successes += 1;

            let mut registers = initial_registers(case, ordinal ^ 0x55, true);
            if case.mask != 0 {
                registers.k[usize::from(case.mask)] = if case.scalar() { 1 } else { 0b1101 };
            }
            let fail_address = if case.scalar() || case.broadcast() {
                MEMORY_ADDRESS
            } else {
                MEMORY_ADDRESS + 2 * u64::from(case.elem.bytes())
            };
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
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
            assert_eq!(context.addresses[context.calls - 1], fail_address);
            faults += 1;

            if case.mask != 0 {
                let mut registers = initial_registers(case, ordinal ^ 0xAA, false);
                registers.k[usize::from(case.mask)] = if case.scalar() { 2 } else { 0 };
                let mut context = LaneMemoryContext {
                    base: MEMORY_ADDRESS,
                    value: bytes,
                    lane_bytes: case.elem.bytes() as usize,
                    fail_address: Some(MEMORY_ADDRESS),
                    calls: 0,
                    addresses: [0; 64],
                };
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);
                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: suppression");
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert!(successes >= LEVELS.len());
    assert_eq!(successes, faults);
    assert!(suppressions >= LEVELS.len());
}
