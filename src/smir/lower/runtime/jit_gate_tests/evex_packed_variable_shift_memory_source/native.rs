//! Native x86-64 differential, helper-call, and precise-fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
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
            2 => u16::from_le_bytes(context.value[offset..offset + 2].try_into().unwrap()) as u64,
            4 => u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(context.value[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!("packed variable-shift scalar helper width"),
        },
        ok: 1,
    }
}

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: SENTINEL_EXIT_PC,
        vector_active: X86_VECTOR_STATE_K64,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, value[..8].try_into().unwrap());
    }
    registers
}

fn assert_architectural_state(
    actual: &GuestRegs,
    expected: &SemanticState,
    level: OptLevel,
    case: ShiftMemoryCase,
) {
    assert_eq!(actual.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, vector) in expected.vectors.iter().enumerate() {
        assert_eq!(
            actual.get_zmm(index),
            <[u64; 8]>::try_from(&vector[..8]).unwrap(),
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(actual.k, expected.masks, "{level:?} {case:?}: opmasks");
    assert_eq!(actual.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(actual.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

fn selected_cases() -> [ShiftMemoryCase; 15] {
    [
        ShiftMemoryCase {
            kind: ShiftKind::ALL[0],
            width: VecWidth::V128,
            destination: 0,
            source: 3,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[1],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[2],
            width: VecWidth::V512,
            destination: 20,
            source: 21,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[3],
            width: VecWidth::V512,
            destination: 0,
            source: 0,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[3],
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[4],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[4],
            width: VecWidth::V512,
            destination: 20,
            source: 21,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[5],
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[5],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[6],
            width: VecWidth::V512,
            destination: 20,
            source: 21,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[6],
            width: VecWidth::V128,
            destination: 23,
            source: 22,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[7],
            width: VecWidth::V256,
            destination: 0,
            source: 3,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[7],
            width: VecWidth::V512,
            destination: 17,
            source: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[8],
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[8],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
    ]
}

#[test]
fn native_variable_shifts_match_interpreter_helpers_faults_and_mask_suppression() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed variable-shift differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let expected_suppressions = cases.iter().filter(|case| case.mask() != 0).count() * LEVELS.len();
    let expected_executions = cases.len() * LEVELS.len();

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut initial = initial_state(case, ordinal);
            if case.mask() != 0 {
                let lanes = case.width.lanes(case.kind.elem);
                initial.masks[usize::from(case.mask())] = 1 | (1u64 << (lanes - 1));
            }
            let bytes = memory_bytes(case, ordinal);
            let expected = interpret(&function, &initial, &bytes, case);

            if case.form == SourceForm::Vector && case.control == MaskControl::None {
                let value = memory_words(&bytes);
                let mut context = VectorMemoryContext {
                    value,
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
                assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC, "{level:?} {case:?}");
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
                let mut registers = guest_regs(&initial);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected_fault = registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected_fault.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected_fault, "{level:?} {case:?}: fault");
                assert_eq!(context.calls, 1, "{level:?} {case:?}: fault calls");
                faults += 1;
                continue;
            }

            let lane_count = case.width.lanes(case.kind.elem);
            let active_mask = if case.mask() == 0 {
                (1u64 << lane_count) - 1
            } else {
                initial.masks[usize::from(case.mask())] & ((1u64 << lane_count) - 1)
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem.bytes() as usize,
                fail_address: None,
                calls: 0,
                addresses: [0; 32],
            };
            let mut registers = guest_regs(&initial);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC, "{level:?} {case:?}");
            let expected_addresses: Vec<u64> = if case.broadcast() {
                vec![0x2000]
            } else {
                (0..lane_count)
                    .filter(|lane| active_mask & (1u64 << lane) != 0)
                    .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.kind.elem.bytes()))
                    .collect()
            };
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: active source addresses"
            );
            successes += 1;

            let last_active = (0..lane_count)
                .rev()
                .find(|lane| active_mask & (1u64 << lane) != 0)
                .expect("selected mask has an active lane");
            let fail_address = if case.broadcast() {
                0x2000
            } else {
                0x2000 + u64::from(last_active) * u64::from(case.kind.elem.bytes())
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem.bytes() as usize,
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
            assert_eq!(
                context.addresses[context.calls - 1],
                fail_address,
                "{level:?} {case:?}: fault address"
            );
            faults += 1;

            if case.mask() != 0 {
                let mut suppressed_initial = initial.clone();
                suppressed_initial.masks[usize::from(case.mask())] = 0;
                let expected = interpret(&function, &suppressed_initial, &bytes, case);
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    lane_bytes: case.kind.elem.bytes() as usize,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 32],
                };
                let mut registers = guest_regs(&suppressed_initial);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(registers.exit_pc, SENTINEL_EXIT_PC, "{level:?} {case:?}");
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    assert_eq!(suppressions, expected_suppressions);
}

#[test]
fn native_apx_address_guard_is_dynamic_and_precedes_memory_and_destination_commit() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native APX-address variable shift: host lacks AVX-512F/BW");
        return;
    }
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[8],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    let apx_bytes = memory_encoding(
        case.kind,
        case.width,
        case.destination,
        case.source,
        case.mask(),
        case.zeroing(),
        case.broadcast(),
        true,
        true,
    );
    let function = optimize(lift_bytes(&apx_bytes), OptLevel::O2);
    let (code, entry) = lower(&function, case);
    let exec = ExecMem::new(&code).expect("map APX-address variable shift");
    let mut initial = initial_state(case, 41);
    initial.gpr[16] = 0x1000;
    initial.gpr[17] = (0x2000 - initial.gpr[16] - case.compressed_displacement() as u64) / 2;
    let bytes = memory_bytes(case, 41);
    let expected = interpret(
        &optimize(lift_case(case), OptLevel::O2),
        &initial,
        &bytes,
        case,
    );
    let value = memory_words(&bytes);

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
