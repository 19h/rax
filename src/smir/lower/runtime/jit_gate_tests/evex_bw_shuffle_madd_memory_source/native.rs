//! Native x86-64 differential, helper-call, and precise-fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        vector_active: 1,
        k: initial.masks,
        mxcsr: initial.mxcsr,
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
    case: BwMemoryCase,
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

fn selected_cases() -> [BwMemoryCase; 9] {
    [
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            control: MaskControl::None,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V256,
            destination: 9,
            source1: 9,
            control: MaskControl::Merge,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V128,
            destination: 31,
            source1: 30,
            control: MaskControl::Zero,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            control: MaskControl::Merge,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V256,
            destination: 25,
            source1: 25,
            control: MaskControl::Zero,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            control: MaskControl::None,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V512,
            destination: 31,
            source1: 31,
            control: MaskControl::Zero,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V256,
            destination: 17,
            source1: 18,
            control: MaskControl::None,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V128,
            destination: 9,
            source1: 14,
            control: MaskControl::Merge,
            w: false,
        },
    ]
}

#[test]
fn native_bw_shuffle_madd_memory_matches_interpreter_and_faults_before_commit() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!(
            "skipping native EVEX AVX-512BW shuffle/madd memory differential: host lacks \
             AVX-512F/BW"
        );
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let expected_level_cases = cases.len() * 2;

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut empty_mask_successes = 0usize;
    let mut empty_mask_faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal);
            let bytes = memory_bytes(ordinal);
            let expected = interpret(&function, &initial, &bytes, case);

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

            context.ok = 0;
            context.calls = 0;
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

            if case.mask() == 0 {
                continue;
            }
            let mut empty = initial.clone();
            empty.masks[usize::from(case.mask())] = 0;
            let expected = interpret(&function, &empty, &bytes, case);
            let mut context = VectorMemoryContext {
                value: memory_words(&bytes),
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&empty);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            assert_eq!(context.calls, 1, "{level:?} {case:?}: empty mask");
            empty_mask_successes += 1;

            context.ok = 0;
            context.calls = 0;
            let mut registers = guest_regs(&empty);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            let mut expected_fault = registers;
            expected_fault.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected_fault.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected_fault,
                "{level:?} {case:?}: empty-mask fault"
            );
            assert_eq!(
                context.calls, 1,
                "{level:?} {case:?}: empty-mask fault calls"
            );
            empty_mask_faults += 1;
        }
    }
    assert_eq!(successes, expected_level_cases);
    assert_eq!(successes, faults);
    assert!(empty_mask_successes > 0);
    assert_eq!(empty_mask_successes, empty_mask_faults);
}
