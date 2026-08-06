//! Native x86-64 differential, fixed helper order, and precise-fault coverage.

use super::semantics::{
    SemanticState, initial_state, interpret_success, lane_mask, memory_bytes, narrowed_lane_value,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

#[derive(Clone, Debug)]
struct NarrowStoreContext {
    base: u64,
    bytes: [u8; 64],
    fail_call: Option<usize>,
    calls: Vec<(u64, u64, u64)>,
    commits: Vec<(u64, u64, u64)>,
}

extern "C" fn narrow_store_helper(
    context: *mut NarrowStoreContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    let call = context.calls.len();
    let width = usize::try_from(size).unwrap();
    assert!(matches!(width, 1 | 2 | 4));
    let value = value
        & match width {
            1 => u64::from(u8::MAX),
            2 => u64::from(u16::MAX),
            4 => u64::from(u32::MAX),
            _ => unreachable!(),
        };
    context.calls.push((address, size, value));
    if context.fail_call == Some(call) {
        return 0;
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
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

fn bind_helper(registers: &mut GuestRegs, context: &mut NarrowStoreContext) {
    registers.ctx = (context as *mut NarrowStoreContext) as u64;
    registers.store_fn = narrow_store_helper as *const () as usize as u64;
}

fn expected_calls(case: NarrowMemoryCase, initial: &SemanticState) -> Vec<(u64, u64, u64)> {
    let control = case.writemask.map_or_else(
        || lane_mask(case.lanes()),
        |mask| initial.masks[usize::from(mask)] & lane_mask(case.lanes()),
    );
    let mut calls = Vec::new();
    for lane in 0..case.lanes() {
        if control & (1u64 << lane) == 0 {
            continue;
        }
        calls.push((
            MEMORY_ADDRESS + (lane * case.lane_bytes()) as u64,
            case.lane_bytes() as u64,
            narrowed_lane_value(case, initial, lane),
        ));
    }
    calls
}

fn host_supports(case: NarrowMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

#[test]
fn native_integer_narrow_matches_interpreter_helper_order_faults_and_suppression() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    if cases.is_empty() {
        eprintln!(
            "skipping native integer-narrow differential: host lacks required AVX-512 subsets"
        );
        return;
    }

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let mut mask =
            0xD6A5_3C69_F00F_5AA5u64.rotate_left((ordinal & 63) as u32) & lane_mask(case.lanes());
        if case.writemask.is_some() && mask.count_ones() < 2 {
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
            let calls = expected_calls(case, &semantic);
            assert!(!calls.is_empty(), "{case:?}");

            let mut context = NarrowStoreContext {
                base: MEMORY_ADDRESS,
                bytes,
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
            let mut context = NarrowStoreContext {
                base: MEMORY_ADDRESS,
                bytes,
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
            for (address, size, value) in &calls[..fail_call] {
                let offset = usize::try_from(*address - MEMORY_ADDRESS).unwrap();
                let width = usize::try_from(*size).unwrap();
                expected_bytes[offset..offset + width]
                    .copy_from_slice(&value.to_le_bytes()[..width]);
            }
            assert_eq!(
                context.bytes, expected_bytes,
                "{level:?} {case:?}: fault memory"
            );
            faults += 1;

            if case.writemask.is_some() {
                let suppressed = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, 0);
                let expected_outcome = interpret_success(&function, &suppressed, &bytes);
                let mut context = NarrowStoreContext {
                    base: MEMORY_ADDRESS,
                    bytes,
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
