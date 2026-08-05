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
    case: PackedAbsMemoryCase,
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

fn selected_cases() -> [PackedAbsMemoryCase; 8] {
    [
        PackedAbsMemoryCase {
            elem: VecElementType::I8,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
            w: false,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I16,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
            w: false,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            w: true,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V128,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            w: false,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V256,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
            w: true,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I8,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            w: true,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I16,
            width: VecWidth::V512,
            destination: 9,
            form: SourceForm::Vector,
            control: MaskControl::None,
            w: false,
        },
    ]
}

#[test]
fn native_packed_abs_matches_interpreter_helpers_faults_and_mask_suppression() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed-abs memory differential: host lacks AVX-512F/BW");
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
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal + 3);
            let bytes = memory_bytes(case, ordinal + 3);
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

            let active_mask = if case.mask() == 0 {
                u64::MAX
            } else {
                initial.masks[usize::from(case.mask())]
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
                fail_address: None,
                calls: 0,
                addresses: [0; 64],
            };
            let mut registers = guest_regs(&initial);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            let expected_addresses: Vec<u64> = if case.broadcast() {
                vec![0x2000]
            } else {
                (0..case.width.lanes(case.elem))
                    .filter(|lane| active_mask & (1u64 << lane) != 0)
                    .map(|lane| 0x2000 + u64::from(lane) * u64::from(case.elem.bytes()))
                    .collect()
            };
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: active source addresses"
            );
            successes += 1;

            let first_active = (0..case.width.lanes(case.elem))
                .find(|lane| active_mask & (1u64 << lane) != 0)
                .expect("selected mask has an active lane");
            let fail_address = if case.broadcast() {
                0x2000
            } else {
                0x2000 + u64::from(first_active) * u64::from(case.elem.bytes())
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                lane_bytes: case.elem.bytes() as usize,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 64],
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
                    lane_bytes: case.elem.bytes() as usize,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 64],
                };
                let mut registers = guest_regs(&suppressed_initial);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert!(successes >= 8);
    assert_eq!(successes, faults);
    assert!(suppressions >= 4);
}
