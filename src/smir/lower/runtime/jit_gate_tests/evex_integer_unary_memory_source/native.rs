//! Native x86-64 differential, helper-order, and precise-fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16, X86_VECTOR_STATE_K64};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct LaneMemoryContext {
    base: u64,
    value: [u8; 64],
    lane_bytes: usize,
    fail_call: Option<usize>,
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
    let call = context.calls;
    context.addresses[call] = address;
    context.calls += 1;
    if context.fail_call == Some(call) {
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

fn guest_regs(initial: &SemanticState, case: IntegerUnaryMemoryCase) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        vector_active: if case.uses_k16_opmasks() {
            X86_VECTOR_STATE_K16
        } else {
            X86_VECTOR_STATE_K64
        },
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
    case: IntegerUnaryMemoryCase,
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

fn selected_cases() -> [IntegerUnaryMemoryCase; 8] {
    [
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::ConflictD,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::ConflictQ,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::LeadingZerosD,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::LeadingZerosQ,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntB,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntW,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntD,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntQ,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
    ]
}

fn host_supports(case: IntegerUnaryMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
        && (!case.operation.needs_cd() || std::is_x86_feature_detected!("avx512cd"))
        && (!case.operation.needs_bitalg() || std::is_x86_feature_detected!("avx512bitalg"))
        && (!case.operation.needs_vpopcntdq() || std::is_x86_feature_detected!("avx512vpopcntdq"))
        && (case.uses_k16_opmasks() || std::is_x86_feature_detected!("avx512bw"))
}

fn expected_addresses(case: IntegerUnaryMemoryCase, mask: u64) -> Vec<u64> {
    let lanes = case.width.lanes(case.elem()) as usize;
    if case.control == MaskControl::None {
        return vec![MEMORY_ADDRESS];
    }
    if matches!(
        case.operation,
        IntegerUnaryOperation::ConflictD | IntegerUnaryOperation::ConflictQ
    ) {
        let highest = (0..lanes)
            .rev()
            .find(|lane| mask & (1u64 << lane) != 0)
            .expect("selected conflict mask has an active destination");
        return (0..=highest)
            .map(|lane| {
                MEMORY_ADDRESS
                    + if case.broadcast() {
                        0
                    } else {
                        lane as u64 * u64::from(case.elem().bytes())
                    }
            })
            .collect();
    }
    (0..lanes)
        .filter(|lane| mask & (1u64 << lane) != 0)
        .map(|lane| {
            MEMORY_ADDRESS
                + if case.broadcast() {
                    0
                } else {
                    lane as u64 * u64::from(case.elem().bytes())
                }
        })
        .collect()
}

#[test]
fn native_integer_unary_matches_interpreter_helper_order_faults_and_suppression() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    if cases.is_empty() {
        eprintln!(
            "skipping native integer-unary memory differential: host lacks required AVX-512 subsets"
        );
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal + 23);
            let bytes = memory_bytes(case, ordinal + 23);
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
                let mut registers = guest_regs(&initial, case);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
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
                let mut registers = guest_regs(&initial, case);
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
            let addresses = expected_addresses(case, active_mask);
            assert!(!addresses.is_empty(), "{case:?}");
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                value: bytes,
                lane_bytes: case.elem().bytes() as usize,
                fail_call: None,
                calls: 0,
                addresses: [0; 64],
            };
            let mut registers = guest_regs(&initial, case);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            assert_eq!(
                &context.addresses[..context.calls],
                addresses,
                "{level:?} {case:?}: helper address order"
            );
            successes += 1;

            let fail_call = addresses.len() / 2;
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                value: bytes,
                lane_bytes: case.elem().bytes() as usize,
                fail_call: Some(fail_call),
                calls: 0,
                addresses: [0; 64],
            };
            let mut registers = guest_regs(&initial, case);
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected_fault = registers;
            expected_fault.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected_fault.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected_fault, "{level:?} {case:?}: fault");
            assert_eq!(
                context.calls,
                fail_call + 1,
                "{level:?} {case:?}: fault calls"
            );
            assert_eq!(
                &context.addresses[..context.calls],
                &addresses[..=fail_call],
                "{level:?} {case:?}: fault address order"
            );
            faults += 1;

            if case.mask() != 0 {
                let mut suppressed_initial = initial.clone();
                suppressed_initial.masks[usize::from(case.mask())] = 0;
                let expected = interpret(&function, &suppressed_initial, &bytes, case);
                let mut context = LaneMemoryContext {
                    base: MEMORY_ADDRESS,
                    value: bytes,
                    lane_bytes: case.elem().bytes() as usize,
                    fail_call: Some(0),
                    calls: 0,
                    addresses: [0; 64],
                };
                let mut registers = guest_regs(&suppressed_initial, case);
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert!(successes > 0);
    assert_eq!(successes, faults);
    assert!(suppressions > 0);
    assert!(suppressions <= successes);
}
