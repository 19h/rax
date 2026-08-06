//! Native x86-64 differential, dense helper order, and precise-fault coverage.

use super::semantics::{
    SemanticState, initial_state, interpret_success, lane_mask, memory_bytes, vector_bytes,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

#[derive(Clone, Debug)]
struct DenseStoreContext {
    base: u64,
    bytes: [u8; 64],
    lane_bytes: usize,
    fail_call: Option<usize>,
    calls: Vec<(u64, u64, u64)>,
    commits: Vec<(u64, u64, u64)>,
}

extern "C" fn dense_store_helper(
    context: *mut DenseStoreContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    let call = context.calls.len();
    context.calls.push((address, size, value));
    if context.fail_call == Some(call) {
        return 0;
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    let width = usize::try_from(size).unwrap();
    assert_eq!(width, context.lane_bytes);
    assert!(offset + width <= context.bytes.len());
    context.bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    context.commits.push((address, size, value));
    1
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b1110_0110,
        cs_l: 1,
        apx_enabled: 1,
        ..GuestRegs::default()
    };
    for (index, vector) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, vector[..8].try_into().unwrap());
    }
    registers
}

fn bind_helper(registers: &mut GuestRegs, context: &mut DenseStoreContext) {
    registers.ctx = (context as *mut DenseStoreContext) as u64;
    registers.store_fn = dense_store_helper as *const () as usize as u64;
}

fn selected_values(case: CompressMemoryCase, initial: &SemanticState) -> Vec<u64> {
    let control = if case.mask() == 0 {
        lane_mask(case.lanes())
    } else {
        initial.masks[usize::from(case.mask())] & lane_mask(case.lanes())
    };
    let source = vector_bytes(&initial.vectors[usize::from(case.source)]);
    let mut values = Vec::new();
    for lane in 0..case.lanes() {
        if control & (1u64 << lane) == 0 {
            continue;
        }
        let offset = lane * case.lane_bytes();
        let mut value = [0u8; 8];
        value[..case.lane_bytes()].copy_from_slice(&source[offset..offset + case.lane_bytes()]);
        values.push(u64::from_le_bytes(value));
    }
    values
}

fn expected_calls(case: CompressMemoryCase, values: &[u64]) -> Vec<(u64, u64, u64)> {
    values
        .iter()
        .enumerate()
        .map(|(slot, value)| {
            (
                MEMORY_ADDRESS + (slot * case.lane_bytes()) as u64,
                case.lane_bytes() as u64,
                *value,
            )
        })
        .collect()
}

fn host_supports(case: CompressMemoryCase) -> bool {
    std::is_x86_feature_detected!("popcnt")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
        && (!case.operation.needs_vbmi2() || std::is_x86_feature_detected!("avx512vbmi2"))
}

#[test]
fn native_compress_matches_interpreter_dense_helper_order_faults_and_suppression() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    if cases.is_empty() {
        eprintln!(
            "skipping native packed compress differential: host lacks required AVX-512 subsets"
        );
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let mut mask =
            0xD6A5_3C69_F00F_5AA5u64.rotate_left((ordinal & 63) as u32) & lane_mask(case.lanes());
        if case.mask() != 0 && mask.count_ones() < 2 {
            mask |= 1 | (1u64 << (case.lanes() - 1));
        }
        let bytes = memory_bytes(ordinal);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let semantic = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
            let expected_outcome = interpret_success(&function, &semantic, &bytes);
            let values = selected_values(case, &semantic);
            let calls = expected_calls(case, &values);
            assert!(!calls.is_empty(), "{case:?}");

            let mut context = DenseStoreContext {
                base: MEMORY_ADDRESS,
                bytes,
                lane_bytes: case.lane_bytes(),
                fail_call: None,
                calls: Vec::new(),
                commits: Vec::new(),
            };
            let mut registers = guest_regs(&semantic);
            bind_helper(&mut registers, &mut context);
            let mut expected = registers;
            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success state");
            assert_eq!(context.bytes, expected_outcome.memory, "{level:?} {case:?}");
            assert_eq!(context.calls, calls, "{level:?} {case:?}: helper calls");
            assert_eq!(context.commits, calls, "{level:?} {case:?}: commits");
            successes += 1;

            let fail_call = calls.len() / 2;
            let mut context = DenseStoreContext {
                base: MEMORY_ADDRESS,
                bytes,
                lane_bytes: case.lane_bytes(),
                fail_call: Some(fail_call),
                calls: Vec::new(),
                commits: Vec::new(),
            };
            let mut registers = guest_regs(&semantic);
            bind_helper(&mut registers, &mut context);
            let mut expected = registers;
            expected.exit_pc = PC;
            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault state");
            assert_eq!(
                context.calls,
                calls[..=fail_call],
                "{level:?} {case:?}: calls through fault"
            );
            assert_eq!(
                context.commits,
                calls[..fail_call],
                "{level:?} {case:?}: prior commits"
            );
            let mut expected_bytes = bytes;
            for (slot, (_, _, value)) in calls[..fail_call].iter().enumerate() {
                let offset = slot * case.lane_bytes();
                expected_bytes[offset..offset + case.lane_bytes()]
                    .copy_from_slice(&value.to_le_bytes()[..case.lane_bytes()]);
            }
            assert_eq!(
                context.bytes, expected_bytes,
                "{level:?} {case:?}: fault memory"
            );
            faults += 1;

            if case.mask() != 0 {
                let suppressed = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, 0);
                let expected_outcome = interpret_success(&function, &suppressed, &bytes);
                let mut context = DenseStoreContext {
                    base: MEMORY_ADDRESS,
                    bytes,
                    lane_bytes: case.lane_bytes(),
                    fail_call: Some(0),
                    calls: Vec::new(),
                    commits: Vec::new(),
                };
                let mut registers = guest_regs(&suppressed);
                bind_helper(&mut registers, &mut context);
                let mut expected = registers;
                executable.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: suppression");
                assert_eq!(context.bytes, expected_outcome.memory, "{level:?} {case:?}");
                assert!(context.calls.is_empty(), "{level:?} {case:?}");
                assert!(context.commits.is_empty(), "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, faults);
    assert!(successes > 0);
    assert!(suppressions > 0 && suppressions < successes);
}
