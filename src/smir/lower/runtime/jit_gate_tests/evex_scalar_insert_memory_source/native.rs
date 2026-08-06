//! Native x86-64 differential and precise helper-fault coverage.

use super::semantics::{initial_state, manual_destination};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoadCall {
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
}

#[derive(Clone, Debug)]
struct MemoryContext {
    scalar: u64,
    fail: bool,
    calls: Vec<LoadCall>,
}

fn scratch_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn scratch_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut MemoryContext) };
    context.calls.push(LoadCall {
        address,
        destination,
        size,
        zero_upper,
    });
    assert_eq!(
        destination,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
    );
    assert!(matches!(size, 1 | 2 | 4 | 8));
    assert_eq!(zero_upper, 1);
    if context.fail {
        return 0;
    }
    let width = size as usize;
    let mut scratch = if zero_upper != 0 {
        [0u8; 64]
    } else {
        scratch_bytes(state.vector_scratch)
    };
    scratch[..width].copy_from_slice(&context.scalar.to_le_bytes()[..width]);
    state.vector_scratch = scratch_words(scratch);
    1
}

fn guest_regs(case: InsertCase, ordinal: usize) -> GuestRegs {
    let initial = initial_state(case, ordinal);
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_K64,
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

fn host_supports(case: InsertCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && (!case.shape.needs_avx512dq() || std::is_x86_feature_detected!("avx512dq"))
}

fn expected_success(mut registers: GuestRegs, case: InsertCase, scalar: u64) -> GuestRegs {
    let source = registers.zmm[usize::from(case.source1)];
    let mut source_wide = [0u64; 16];
    source_wide[..8].copy_from_slice(&source);
    let destination = manual_destination(case, &source_wide, scalar);
    registers.set_zmm(
        usize::from(case.destination),
        destination[..8].try_into().unwrap(),
    );
    let mut scratch = [0u8; 64];
    let width = case.shape.kind.memory_width().bytes() as usize;
    scratch[..width].copy_from_slice(&scalar.to_le_bytes()[..width]);
    registers.vector_scratch = scratch_words(scratch);
    registers
}

fn execute(
    executable: &ExecMem,
    entry: usize,
    case: InsertCase,
    level: OptLevel,
    ordinal: usize,
    fail: bool,
) {
    let scalar = 0xFEDC_BA98_7654_3210u64 ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);
    let mut context = MemoryContext {
        scalar,
        fail,
        calls: Vec::new(),
    };
    let mut registers = guest_regs(case, ordinal);
    registers.ctx = (&mut context as *mut MemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
    let mut expected = if fail {
        let mut expected = registers;
        expected.exit_pc = PC;
        expected
    } else {
        expected_success(registers, case, scalar)
    };

    executable.run(entry, &mut registers);
    expected.host_mxcsr = registers.host_mxcsr;
    assert_eq!(registers, expected, "{level:?} {case:?} fail={fail}");
    assert_eq!(
        context.calls,
        vec![LoadCall {
            address: MEMORY_ADDRESS,
            destination: crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
            size: case.shape.kind.memory_width().bytes(),
            zero_upper: 1,
        }],
        "{level:?} {case:?} fail={fail}"
    );
}

#[test]
fn native_evex_scalar_inserts_match_manual_model_and_fault_noncommitting() {
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    if cases.is_empty() {
        eprintln!(
            "skipping native EVEX scalar-insert differential: host lacks required AVX-512 subsets"
        );
        return;
    }

    let supported = cases.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            execute(&executable, entry, case, level, ordinal, false);
            successes += 1;
            execute(&executable, entry, case, level, ordinal ^ 0x55, true);
            faults += 1;
        }
    }
    assert_eq!(successes, supported * 2);
    assert_eq!(faults, supported * 2);
}
