//! Native x86-64 differential, helper-call, and precise-fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
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
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 16],
}

extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size, 4);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + 4 <= context.value.len());
    LoadResult {
        value: u64::from(u32::from_le_bytes(
            context.value[offset..offset + 4].try_into().unwrap(),
        )),
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
    case: Bf16MemoryCase,
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

fn selected_cases() -> [Bf16MemoryCase; 12] {
    [
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V512,
            destination: 17,
            source1: 0,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V512,
            destination: 25,
            source1: 0,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V256,
            destination: 9,
            source1: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V512,
            destination: 17,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V128,
            destination: 1,
            source1: 1,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ]
}

fn applicable_e4_mask(case: Bf16MemoryCase, state: &SemanticState) -> u64 {
    let lanes = case.width.lanes(VecElementType::F32);
    let lane_mask = (1u64 << lanes) - 1;
    if case.mask() == 0 {
        lane_mask
    } else {
        state.masks[usize::from(case.mask())] & lane_mask
    }
}

fn scalar_addresses(case: Bf16MemoryCase, state: &SemanticState) -> Vec<u64> {
    assert_ne!(case.kind, Bf16Kind::ConvertTwo);
    let active = applicable_e4_mask(case, state);
    if case.broadcast() {
        return (active != 0).then_some(0x2000).into_iter().collect();
    }
    (0..case.width.lanes(VecElementType::F32))
        .filter(|lane| active & (1u64 << lane) != 0)
        .map(|lane| 0x2000 + u64::from(lane) * 4)
        .collect()
}

#[test]
fn native_bf16_memory_matches_interpreter_helpers_e4_and_e4nf_faults() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512bf16")
    {
        eprintln!("skipping native AVX512_BF16 differential: host lacks AVX-512F/BW/BF16");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let expected_successes = cases.len() * 2;

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut e4_suppressions = 0usize;
    let mut e4nf_empty_faults = 0usize;
    let mut late_faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal);
            let bytes = memory_bytes(case, ordinal);
            let expected = interpret(&function, &initial, &bytes, case);
            let vector_replay =
                !case.broadcast() && (case.kind == Bf16Kind::ConvertTwo || case.mask() == 0);

            if vector_replay {
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
            } else {
                let expected_addresses = if case.kind == Bf16Kind::ConvertTwo {
                    vec![0x2000]
                } else {
                    scalar_addresses(case, &initial)
                };
                assert!(!expected_addresses.is_empty(), "{level:?} {case:?}");
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    fail_address: None,
                    calls: 0,
                    addresses: [0; 16],
                };
                let mut registers = guest_regs(&initial);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(
                    &context.addresses[..context.calls],
                    expected_addresses,
                    "{level:?} {case:?}: active source addresses"
                );
                successes += 1;

                let fail_index = expected_addresses.len() / 2;
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    fail_address: Some(expected_addresses[fail_index]),
                    calls: 0,
                    addresses: [0; 16],
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
                    &context.addresses[..context.calls],
                    &expected_addresses[..=fail_index],
                    "{level:?} {case:?}: accesses through fault"
                );
                faults += 1;
                late_faults += usize::from(fail_index != 0);
            }

            if case.mask() == 0 {
                continue;
            }
            let mut empty = initial.clone();
            empty.masks[usize::from(case.mask())] = 0;
            if case.kind != Bf16Kind::ConvertTwo {
                let expected = interpret(&function, &empty, &bytes, case);
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 16],
                };
                let mut registers = guest_regs(&empty);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 0, "{level:?} {case:?}: Type E4 empty mask");
                e4_suppressions += 1;
            } else if case.broadcast() {
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 16],
                };
                let mut registers = guest_regs(&empty);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                let mut expected_fault = registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected_fault.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected_fault,
                    "{level:?} {case:?}: Type E4NF empty-mask fault"
                );
                assert_eq!(context.calls, 1, "{level:?} {case:?}: Type E4NF access");
                e4nf_empty_faults += 1;
            } else {
                let value = memory_words(&bytes);
                let mut context = VectorMemoryContext {
                    value,
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&empty);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected_fault = registers;
                expected_fault.exit_pc = PC;
                exec.run(entry, &mut registers);
                expected_fault.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected_fault,
                    "{level:?} {case:?}: Type E4NF empty-mask fault"
                );
                assert_eq!(context.calls, 1, "{level:?} {case:?}: Type E4NF access");
                e4nf_empty_faults += 1;
            }
        }
    }
    assert_eq!(successes, expected_successes);
    assert_eq!(successes, faults);
    assert!(e4_suppressions >= 4);
    assert!(e4nf_empty_faults >= 4);
    assert!(late_faults >= 2);
}
