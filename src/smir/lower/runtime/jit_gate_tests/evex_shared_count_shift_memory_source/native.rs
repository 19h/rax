//! Native x86-64 differential, helper-call, and precise-fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

const SENTINEL_EXIT_PC: u64 = 0xA11C_E55E_D15C_A4D1;

fn memory_words(bytes: &[u8; 16]) -> [u64; 8] {
    let mut words = [0xCCDD_EEFF_0011_2233; 8];
    words[0] = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    words[1] = u64::from_le_bytes(bytes[8..].try_into().unwrap());
    words
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

fn selected_cases() -> [ShiftMemoryCase; 12] {
    [
        ShiftMemoryCase {
            kind: ShiftKind::ALL[0],
            width: VecWidth::V128,
            destination: 0,
            source: 0,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[1],
            width: VecWidth::V256,
            destination: 9,
            source: 10,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[2],
            width: VecWidth::V512,
            destination: 17,
            source: 18,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[3],
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[4],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[5],
            width: VecWidth::V512,
            destination: 20,
            source: 21,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[6],
            width: VecWidth::V128,
            destination: 23,
            source: 22,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[7],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[8],
            width: VecWidth::V512,
            destination: 20,
            source: 21,
            control: MaskControl::Zero,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[9],
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[10],
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            control: MaskControl::Merge,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[11],
            width: VecWidth::V512,
            destination: 31,
            source: 30,
            control: MaskControl::Zero,
        },
    ]
}

#[test]
fn native_shared_count_shifts_match_interpreter_and_issue_one_mem128_helper() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native shared-count shift differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let expected_executions = cases.len() * LEVELS.len();

    let counts = [0u64, 1, 15, 16, 31, 32, 63, 64, 65, u64::MAX];
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut mask_zero_checks = 0usize;
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
            let bytes = memory_bytes(
                counts[ordinal % counts.len()],
                0xD00D_F00D_CAFE_BABEu64.rotate_left(ordinal as u32),
            );
            let expected = interpret(&function, &initial, &bytes);
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
            assert_eq!(context.last_size, 16, "{level:?} {case:?}");
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
            assert_eq!(context.last_size, 16, "{level:?} {case:?}: fault size");
            faults += 1;

            if case.mask() != 0 {
                let mut mask_zero = initial.clone();
                mask_zero.masks[usize::from(case.mask())] = 0;
                let expected = interpret(&function, &mask_zero, &bytes);
                let mut context = VectorMemoryContext {
                    value,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&mask_zero);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 1, "{level:?} {case:?}: mask-zero calls");
                assert_eq!(context.last_size, 16, "{level:?} {case:?}: mask-zero size");

                let mut context = VectorMemoryContext {
                    value,
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&mask_zero);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected_fault = registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected_fault.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected_fault,
                    "{level:?} {case:?}: mask-zero fault"
                );
                assert_eq!(
                    context.calls, 1,
                    "{level:?} {case:?}: mask-zero fault calls"
                );
                mask_zero_checks += 1;
            }
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    assert!(mask_zero_checks > 0);
}

#[test]
fn native_apx_address_guard_precedes_mem128_helper_and_destination_commit() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native APX shared-count shift: host lacks AVX-512F/BW");
        return;
    }
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[11],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        control: MaskControl::None,
    };
    let apx_bytes = memory_encoding(
        case.kind,
        case.width,
        case.destination,
        case.source,
        case.mask(),
        case.zeroing(),
        true,
        true,
    );
    let function = optimize(lift_bytes(&apx_bytes), OptLevel::O2);
    let (code, entry) = lower(&function, case);
    let exec = ExecMem::new(&code).expect("map APX-address shared-count shift");
    let mut initial = initial_state(case, 41);
    initial.gpr[16] = 0x1000;
    initial.gpr[17] = (0x2000 - initial.gpr[16] - case.compressed_displacement() as u64) / 2;
    let bytes = memory_bytes(7, 0xFFFF_FFFF_FFFF_FFFF);
    let expected = interpret(&optimize(lift_case(case), OptLevel::O2), &initial, &bytes);
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
    assert_eq!(enabled_context.last_size, 16);
}
