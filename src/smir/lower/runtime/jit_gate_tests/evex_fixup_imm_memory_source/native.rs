//! Native VFIXUPIMM helper differential, fault, and mask-suppression tests.

use super::semantics::{initial_registers, interpreter_success, memory_bytes, memory_value};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

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
    addresses: [u64; 32],
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
    LoadResult {
        value: match context.lane_bytes {
            4 => u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(context.value[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!("VFIXUPIMM scalar helper width"),
        },
        ok: 1,
    }
}

fn expected_vector_scratch(value: [u64; 8], width: VecWidth) -> [u64; 8] {
    let words = (width.bytes() / 8) as usize;
    std::array::from_fn(|word| if word < words { value[word] } else { 0 })
}

const CHILD_CASE_ENV: &str = "RAX_EVEX_FIXUP_IMM_MEMORY_CHILD_CASE";
const TEST_NAME: &str = concat!(
    "smir::lower::runtime::jit_gate_tests::evex_fixup_imm_memory_source::native::",
    "native_fixup_memory_matches_interpretation_faults_mxcsr_and_mask_suppression"
);

fn native_cases() -> Vec<FixupMemoryCase> {
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let selected = [
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V256,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V512,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Scalar { ll: 3 },
            control: MaskControl::None,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Scalar { ll: 2 },
            control: MaskControl::Zero,
            immediate: 0xFF,
        },
    ];

    selected
        .into_iter()
        .filter(|case| !case.needs_avx512vl() || has_vl)
        .collect()
}

fn execute_native_case(case: FixupMemoryCase, ordinal: usize) {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        let function = optimize(lift_case(case), level);
        let (code, entry) = lower(&function, case);
        let exec =
            ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
        let value = memory_value(case, ordinal);
        let bytes = memory_bytes(value);

        if case.form == SourceForm::Vector && case.control == MaskControl::None {
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
        if case.mask() != 0 {
            registers.k[usize::from(case.mask())] = if case.scalar() { 1 } else { 0x5555_5555 };
        }
        let lanes = if case.scalar() {
            1
        } else {
            case.width.lanes(case.elem)
        };
        let active_mask = if case.mask() == 0 {
            (1u64 << lanes) - 1
        } else {
            registers.k[usize::from(case.mask())]
        };
        let mut context = LaneMemoryContext {
            base: 0x2000,
            value: bytes,
            lane_bytes: case.elem.bytes() as usize,
            fail_address: None,
            calls: 0,
            addresses: [0; 32],
        };
        registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
        registers.load_fn = lane_load_helper as *const () as usize as u64;
        let mut expected = interpreter_success(&function, &registers, value, case);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(registers, expected, "{level:?} {case:?}: success");
        let expected_addresses: Vec<u64> = if case.scalar() || case.broadcast() {
            vec![0x2000]
        } else {
            (0..lanes)
                .filter(|lane| active_mask & (1 << lane) != 0)
                .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.elem.bytes()))
                .collect()
        };
        assert_eq!(
            &context.addresses[..context.calls],
            expected_addresses,
            "{level:?} {case:?}: active source addresses"
        );
        successes += 1;

        let mut registers = initial_registers(case, ordinal ^ 0x55);
        if case.mask() != 0 {
            registers.k[usize::from(case.mask())] = if case.scalar() { 1 } else { 0b1101 };
        }
        let fail_address = if case.scalar() || case.broadcast() {
            0x2000
        } else {
            0x2000 + 2 * u64::from(case.elem.bytes())
        };
        let mut context = LaneMemoryContext {
            base: 0x2000,
            value: bytes,
            lane_bytes: case.elem.bytes() as usize,
            fail_address: Some(fail_address),
            calls: 0,
            addresses: [0; 32],
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
            "{level:?} {case:?}: fault address"
        );
        faults += 1;

        if case.mask() != 0 {
            let mut registers = initial_registers(case, ordinal ^ 0xAA);
            registers.k[usize::from(case.mask())] = 0;
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
                fail_address: Some(0x2000),
                calls: 0,
                addresses: [0; 32],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, value, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected,
                "{level:?} {case:?}: all applicable lanes suppressed"
            );
            assert_eq!(context.calls, 0, "{level:?} {case:?}");
            suppressions += 1;
        }
    }
    assert_eq!(successes, 2, "{case:?}");
    assert_eq!(faults, 2, "{case:?}");
    assert_eq!(
        suppressions,
        if case.mask() == 0 { 0 } else { 2 },
        "{case:?}"
    );
}

#[test]
fn native_fixup_memory_matches_interpretation_faults_mxcsr_and_mask_suppression() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native VFIXUPIMM memory differential: host lacks AVX-512F/BW");
        return;
    }

    let cases = native_cases();
    assert!(cases.len() >= 4);
    if let Ok(value) = std::env::var(CHILD_CASE_ENV) {
        let ordinal: usize = value
            .parse()
            .unwrap_or_else(|_| panic!("invalid {CHILD_CASE_ENV}: {value}"));
        let case = *cases
            .get(ordinal)
            .unwrap_or_else(|| panic!("{CHILD_CASE_ENV} out of range: {ordinal}"));
        execute_native_case(case, ordinal);
        return;
    }

    for (ordinal, case) in cases.into_iter().enumerate() {
        let output = std::process::Command::new(
            std::env::current_exe().expect("current unit-test executable"),
        )
        .arg(TEST_NAME)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_CASE_ENV, ordinal.to_string())
        .output()
        .expect("run isolated native VFIXUPIMM memory differential");
        assert!(
            output.status.success(),
            "isolated native VFIXUPIMM failure at case {ordinal}/{}: {case:?}; status {}; \
             stdout: {}; stderr: {}",
            ordinal + 1,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
