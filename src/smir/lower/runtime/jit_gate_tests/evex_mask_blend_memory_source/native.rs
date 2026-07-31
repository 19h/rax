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
    lane_bytes: usize,
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 64],
    sizes: [u64; 64],
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
    context.sizes[context.calls] = size;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + context.lane_bytes <= context.value.len());
    let mut value = [0u8; 8];
    value[..context.lane_bytes]
        .copy_from_slice(&context.value[offset..offset + context.lane_bytes]);
    LoadResult {
        value: u64::from_le_bytes(value),
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
    case: MaskBlendMemoryCase,
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

fn selected_cases() -> [MaskBlendMemoryCase; 10] {
    [
        MaskBlendMemoryCase {
            kind: BlendKind::PackedSingle,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedQword,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedByte,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedWord,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedDword,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedDouble,
            width: VecWidth::V128,
            destination: 1,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedSingle,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedDouble,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedDword,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedQword,
            width: VecWidth::V512,
            destination: 17,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ]
}

fn applicable_mask(case: MaskBlendMemoryCase, state: &SemanticState) -> u64 {
    if case.selector() == 0 {
        return u64::MAX;
    }
    let lanes = case.width.lanes(case.kind.elem());
    let lane_mask = if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    };
    state.masks[usize::from(case.selector())] & lane_mask
}

fn scalar_addresses(case: MaskBlendMemoryCase, state: &SemanticState) -> Vec<u64> {
    let active = applicable_mask(case, state);
    if case.broadcast() {
        return (active != 0).then_some(0x2000).into_iter().collect();
    }
    (0..case.width.lanes(case.kind.elem()))
        .filter(|lane| active & (1u64 << lane) != 0)
        .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.kind.elem().bytes()))
        .collect()
}

#[test]
fn native_mask_blends_match_interpreter_helpers_and_precise_type_e4_faults() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX mask-blend differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    let mut high_lane_guards = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut initial = initial_state(case, ordinal);
            if case.kind == BlendKind::PackedByte
                && case.width == VecWidth::V512
                && case.selector() != 0
            {
                initial.masks[usize::from(case.selector())] = 1u64 << 63;
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

            let expected_addresses = scalar_addresses(case, &initial);
            assert!(!expected_addresses.is_empty(), "{level:?} {case:?}");
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem().bytes() as usize,
                fail_address: None,
                calls: 0,
                addresses: [0; 64],
                sizes: [0; 64],
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
            assert!(
                context.sizes[..context.calls]
                    .iter()
                    .all(|size| *size == u64::from(case.kind.elem().bytes())),
                "{level:?} {case:?}: helper widths {:?}",
                &context.sizes[..context.calls]
            );
            if expected_addresses == [0x203F] {
                high_lane_guards += 1;
            }
            successes += 1;

            let fail_index = if case.broadcast() {
                0
            } else {
                expected_addresses.len() / 2
            };
            let fail_address = expected_addresses[fail_index];
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.kind.elem().bytes() as usize,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 64],
                sizes: [0; 64],
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

            if case.selector() != 0 {
                let mut empty = initial.clone();
                empty.masks[usize::from(case.selector())] = 0;
                let expected = interpret(&function, &empty, &bytes, case);
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    lane_bytes: case.kind.elem().bytes() as usize,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 64],
                    sizes: [0; 64],
                };
                let mut registers = guest_regs(&empty);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 0, "{level:?} {case:?}: empty selector");
                suppressions += 1;
            }
        }
    }
    assert!(successes >= 8);
    assert_eq!(successes, faults);
    assert!(suppressions >= 6);
    assert_eq!(high_lane_guards, 2);
}
