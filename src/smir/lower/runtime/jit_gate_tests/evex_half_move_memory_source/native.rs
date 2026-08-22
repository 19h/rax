//! Native x86-64 differential and precise helper-fault coverage.

use super::semantics::{initial_state, manual_destination};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[derive(Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

fn guest_regs(case: HalfMoveCase, ordinal: usize) -> GuestRegs {
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

fn expected_success(mut registers: GuestRegs, case: HalfMoveCase, memory: u64) -> GuestRegs {
    let source = registers.zmm[usize::from(case.source1)];
    let mut source_wide = [0u64; 16];
    source_wide[..8].copy_from_slice(&source);
    let destination = manual_destination(case, &source_wide, memory);
    registers.set_zmm(
        usize::from(case.destination),
        destination[..8].try_into().unwrap(),
    );
    registers
}

#[test]
fn native_half_moves_match_manual_model_and_fault_without_commit() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX half-move memory differential: host lacks AVX-512F/BW");
        return;
    }

    let cases = representative_cases();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory =
                0xFEDC_BA98_7654_3210u64 ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);

            let mut context = ScalarMemoryContext {
                value: memory,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = guest_regs(case, ordinal);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected = expected_success(registers, case, memory);

            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(context.last_size, 8, "{level:?} {case:?}");
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = ScalarMemoryContext {
                value: memory ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = guest_regs(case, ordinal ^ 0x55);
            fault_registers.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            executable.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: source fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault");
            assert_eq!(
                fault_context.last_addr, MEMORY_ADDRESS,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_size, 8, "{level:?} {case:?}");
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, 16 * 2);
    assert_eq!(faults, successes);
}
