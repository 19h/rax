//! Native x86-64 differential, helper-call, APX, and precise-fault coverage.

use super::semantics::{initial_registers, interpreter_success, memory_bytes, memory_value};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

const SENTINEL_EXIT_PC: u64 = 0xA11C_E55E_D15C_A4D0;

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
    assert!(context.calls < context.addresses.len());
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(
        address
            .checked_sub(context.base)
            .expect("helper address precedes test memory"),
    )
    .unwrap();
    assert!(offset + context.lane_bytes <= context.value.len());
    LoadResult {
        value: match context.lane_bytes {
            4 => u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(context.value[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!("VRANGE scalar helper width"),
        },
        ok: 1,
    }
}

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn guest_regs(initial: &GuestRegs) -> GuestRegs {
    GuestRegs {
        exit_pc: SENTINEL_EXIT_PC,
        vector_active: X86_VECTOR_STATE_K64,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..*initial
    }
}

fn assert_architectural_state(
    actual: &GuestRegs,
    expected: &GuestRegs,
    level: OptLevel,
    case: RangeMemoryCase,
) {
    assert_eq!(actual.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    assert_eq!(actual.zmm, expected.zmm, "{level:?} {case:?}: vectors");
    assert_eq!(actual.k, expected.k, "{level:?} {case:?}: opmasks");
    assert_eq!(actual.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(actual.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

fn selected_cases() -> Vec<RangeMemoryCase> {
    let mut cases = Vec::new();
    let immediates = [0x00, 0x01, 0x02, 0x05, 0x0A, 0x0F];
    let mut ordinal = 0usize;
    for elem in [VecElementType::F32, VecElementType::F64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    cases.push(RangeMemoryCase {
                        elem,
                        width,
                        destination: [0, 9, 17, 20][ordinal & 3],
                        source1: [3, 10, 18, 21][ordinal & 3],
                        form,
                        control,
                        immediate: immediates[ordinal % immediates.len()],
                    });
                    ordinal += 1;
                }
            }
        }
        for control in MaskControl::ALL {
            cases.push(RangeMemoryCase {
                elem,
                width: VecWidth::V128,
                destination: [0, 17, 20][ordinal % 3],
                source1: [3, 18, 21][ordinal % 3],
                form: SourceForm::Scalar { ll: 3 },
                control,
                immediate: immediates[ordinal % immediates.len()],
            });
            ordinal += 1;
        }
    }
    cases
}

#[test]
fn native_range_matches_interpreter_helpers_faults_masks_and_mxcsr() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
    {
        eprintln!("skipping native VRANGE differential: host lacks AVX-512F/BW/DQ");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| !case.needs_avx512vl() || has_vl)
        .collect();
    assert!(!cases.is_empty());

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut initial = initial_registers(case, ordinal);
            if case.mask() != 0 {
                let lanes = if case.scalar() {
                    1
                } else {
                    case.width.lanes(case.elem)
                };
                initial.k[usize::from(case.mask())] = 1 | (1u64 << (lanes - 1));
            }
            let value = memory_value(case, ordinal);
            let bytes = memory_bytes(value);
            let expected = interpreter_success(&function, &initial, value, case);

            if !case.scalar() && !case.broadcast() && case.control == MaskControl::None {
                let mut context = VectorMemoryContext {
                    value: memory_words(&bytes),
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&initial);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC);
                assert_eq!(context.calls, 1);
                assert_eq!(context.last_addr, 0x2000);
                assert_eq!(context.last_size, case.width.bytes());
                successes += 1;

                let mut context = VectorMemoryContext {
                    value: memory_words(&bytes),
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&initial);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected_fault = registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected_fault.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected_fault, "{level:?} {case:?}: fault");
                assert_eq!(context.calls, 1);
                faults += 1;
                continue;
            }

            let lane_count = if case.scalar() {
                1
            } else {
                case.width.lanes(case.elem)
            };
            let applicable_mask = (1u64 << lane_count) - 1;
            let active_mask = if case.mask() == 0 {
                applicable_mask
            } else {
                initial.k[usize::from(case.mask())] & applicable_mask
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
                fail_address: None,
                calls: 0,
                addresses: [0; 32],
            };
            let mut registers = guest_regs(&initial);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC);
            let expected_addresses: Vec<u64> = if case.scalar() || case.broadcast() {
                vec![0x2000]
            } else {
                (0..lane_count)
                    .filter(|lane| active_mask & (1u64 << lane) != 0)
                    .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.elem.bytes()))
                    .collect()
            };
            assert_eq!(&context.addresses[..context.calls], expected_addresses);
            successes += 1;

            let last_active = (0..lane_count)
                .rev()
                .find(|lane| active_mask & (1u64 << lane) != 0)
                .expect("selected mask has an active lane");
            let fail_address = if case.scalar() || case.broadcast() {
                0x2000
            } else {
                0x2000 + u64::from(last_active) * u64::from(case.elem.bytes())
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 32],
            };
            let mut registers = guest_regs(&initial);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected_fault = registers;
            expected_fault.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected_fault.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected_fault, "{level:?} {case:?}: fault");
            assert_eq!(context.addresses[context.calls - 1], fail_address);
            faults += 1;

            if case.mask() != 0 {
                let mut suppressed_initial = initial;
                suppressed_initial.k[usize::from(case.mask())] = 0;
                let expected = interpreter_success(&function, &suppressed_initial, value, case);
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    lane_bytes: case.elem.bytes() as usize,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 32],
                };
                let mut registers = guest_regs(&suppressed_initial);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC);
                assert_eq!(context.calls, 0);
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, faults);
    assert!(successes >= 24);
    assert!(suppressions >= 12);
}

#[test]
fn native_apx_guard_precedes_range_memory_and_destination_commit() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
    {
        eprintln!("skipping native APX-address VRANGE: host lacks AVX-512F/BW/DQ");
        return;
    }
    let case = RangeMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::None,
        immediate: 0x0D,
    };
    let bytes = memory_encoding(case, 0, false, true, true);
    let function = optimize(lift_bytes(&bytes), OptLevel::O2);
    let (code, entry) = lower(&function, case);
    let exec = ExecMem::new(&code).expect("map APX-address VRANGE");
    let mut initial = initial_registers(case, 41);
    initial.gpr[16] = 0x1000;
    initial.gpr[17] = (0x2000 - initial.gpr[16] - case.compressed_displacement() as u64) / 2;
    let value = memory_value(case, 41);
    let expected = interpreter_success(
        &optimize(lift_case(case), OptLevel::O2),
        &initial,
        value,
        case,
    );

    let mut disabled_context = VectorMemoryContext {
        value,
        ok: 1,
        calls: 0,
        last_addr: 0,
        last_index: 0,
        last_size: 0,
        last_zero_upper: 0,
    };
    let mut disabled = guest_regs(&initial);
    disabled.ctx = (&mut disabled_context as *mut VectorMemoryContext) as u64;
    disabled.vec_load_fn = vector_load_helper as *const () as usize as u64;
    disabled.apx_enabled = 0;
    let mut expected_disabled = disabled;
    expected_disabled.exit_pc = PC;
    exec.run(entry, &mut disabled);
    expected_disabled.host_mxcsr = disabled.host_mxcsr;
    assert_eq!(disabled, expected_disabled);
    assert_eq!(disabled_context.calls, 0);

    let mut enabled_context = VectorMemoryContext {
        value,
        ok: 1,
        calls: 0,
        last_addr: 0,
        last_index: 0,
        last_size: 0,
        last_zero_upper: 0,
    };
    let mut enabled = guest_regs(&initial);
    enabled.ctx = (&mut enabled_context as *mut VectorMemoryContext) as u64;
    enabled.vec_load_fn = vector_load_helper as *const () as usize as u64;
    enabled.apx_enabled = 1;
    exec.run(entry, &mut enabled);
    assert_architectural_state(&enabled, &expected, OptLevel::O2, case);
    assert_eq!(enabled.exit_pc, SENTINEL_EXIT_PC);
    assert_eq!(enabled_context.calls, 1);
    assert_eq!(enabled_context.last_addr, 0x2000);
}
