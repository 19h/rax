//! Native x86-64 store differential and precise helper-fault coverage.

use super::semantics::{expected_store_value, initial_state};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16};

#[derive(Default)]
struct StoreMemoryContext {
    memory: u64,
    ok: u64,
    calls: u64,
    commits: u64,
    last_addr: u64,
    last_value: u64,
    last_size: u64,
}

extern "C" fn scalar_store_helper(
    context: *mut StoreMemoryContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_value = value;
    context.last_size = size;
    if context.ok == 0 || size != 8 {
        return 0;
    }
    context.memory = value;
    context.commits += 1;
    1
}

fn guest_regs(case: HalfMoveStoreCase, ordinal: usize) -> GuestRegs {
    let initial = initial_state(case, ordinal);
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_K16,
        mxcsr: initial.mxcsr,
        vector_scratch: std::array::from_fn(|word| {
            0xCCDD_EEFF_0011_2233u64 ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
        }),
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

#[test]
fn native_stores_match_manual_model_and_fault_without_guest_commit() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX half-move store differential: host lacks AVX-512F/BW");
        return;
    }

    let cases = representative_store_cases();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_store_case(case), level);
            let (code, entry) = lower_store(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let expected_value = expected_store_value(&initial_state(case, ordinal), case);

            let memory_before =
                0xA55A_6996_F00F_3CC3u64 ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);
            let mut context = StoreMemoryContext {
                memory: memory_before,
                ok: 1,
                ..StoreMemoryContext::default()
            };
            let mut registers = guest_regs(case, ordinal);
            registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
            registers.store_fn = scalar_store_helper as usize as u64;
            let mut expected = registers;

            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success state");
            assert_eq!(context.calls, 1, "{level:?} {case:?}: success calls");
            assert_eq!(context.commits, 1, "{level:?} {case:?}: success commit");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(context.last_size, 8, "{level:?} {case:?}");
            assert_eq!(context.last_value, expected_value, "{level:?} {case:?}");
            assert_eq!(context.memory, expected_value, "{level:?} {case:?}");
            successes += 1;

            let fault_memory = memory_before ^ u64::MAX;
            let mut fault_context = StoreMemoryContext {
                memory: fault_memory,
                ok: 0,
                ..StoreMemoryContext::default()
            };
            let mut fault_registers = guest_regs(case, ordinal);
            fault_registers.ctx = (&mut fault_context as *mut StoreMemoryContext) as u64;
            fault_registers.store_fn = scalar_store_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            executable.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed guest state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault calls");
            assert_eq!(fault_context.commits, 0, "{level:?} {case:?}: fault commit");
            assert_eq!(
                fault_context.last_addr, MEMORY_ADDRESS,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_size, 8, "{level:?} {case:?}");
            assert_eq!(
                fault_context.last_value, expected_value,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.memory, fault_memory, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, 24 * 2);
    assert_eq!(faults, successes);
}
