//! Native x86-64 differential, helper-call, and precise Type E4 fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, manual, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16};

struct VectorMemoryContext {
    value: [u64; 8],
    ok: bool,
    calls: usize,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = address;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if !context.ok || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX || size != 16
    {
        return 0;
    }
    state.vector_scratch = [0; 8];
    state.vector_scratch[..2].copy_from_slice(&context.value[..2]);
    1
}

fn memory_words(bytes: &[u8; 16]) -> [u64; 8] {
    let mut words = [0u64; 8];
    for word in 0..2usize {
        words[word] = u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap());
    }
    words
}

fn guest_regs(initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        vector_active: X86_VECTOR_STATE_K16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
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
    case: FourDotProductMemoryCase,
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

fn selected_cases() -> [FourDotProductMemoryCase; 6] {
    [
        FourDotProductMemoryCase {
            saturating: false,
            destination: 17,
            source_index: 20,
            control: MaskControl::None,
        },
        FourDotProductMemoryCase {
            saturating: true,
            destination: 20,
            source_index: 23,
            control: MaskControl::None,
        },
        FourDotProductMemoryCase {
            saturating: false,
            destination: 30,
            source_index: 28,
            control: MaskControl::Merge,
        },
        FourDotProductMemoryCase {
            saturating: true,
            destination: 4,
            source_index: 7,
            control: MaskControl::Zero,
        },
        FourDotProductMemoryCase {
            saturating: false,
            destination: 1,
            source_index: 0,
            control: MaskControl::Zero,
        },
        FourDotProductMemoryCase {
            saturating: true,
            destination: 31,
            source_index: 31,
            control: MaskControl::Merge,
        },
    ]
}

#[test]
fn native_four_dot_product_matches_interpreter_and_preserves_helper_frontiers() {
    if !std::is_x86_feature_detected!("avx512f") || !x86_host_has_avx5124vnniw() {
        eprintln!("skipping native AVX512_4VNNIW differential: host lacks AVX-512F/4VNNIW");
        return;
    }

    let cases = selected_cases();
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut initial = initial_state(case, ordinal);
            if case.control != MaskControl::None {
                initial.masks[1] = 0xA55A;
            }
            let bytes = memory_bytes(ordinal);
            let expected = interpret(&function, &initial, &bytes);
            let words = memory_words(&bytes);
            let mut context = VectorMemoryContext {
                value: words,
                ok: true,
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
            let mut expected_scratch = [0u64; 8];
            expected_scratch[..2].copy_from_slice(&words[..2]);
            assert_eq!(registers.vector_scratch, expected_scratch);
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, 16, "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: words,
                ok: false,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&initial);
            let initial_scratch = registers.vector_scratch;
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &initial, level, case);
            assert_eq!(registers.exit_pc, PC, "{level:?} {case:?}");
            assert_eq!(registers.vector_scratch, initial_scratch);
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(context.last_size, 16, "{level:?} {case:?}");
            faults += 1;

            if case.control != MaskControl::None {
                let mut inactive = initial_state(case, ordinal + 0x40);
                inactive.masks[1] = 1 << 16;
                let expected = manual(case, &inactive, &bytes);
                let mut context = VectorMemoryContext {
                    value: words,
                    ok: false,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&inactive);
                let initial_scratch = registers.vector_scratch;
                let initial_exit_pc = registers.exit_pc;
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(registers.exit_pc, initial_exit_pc, "{level:?} {case:?}");
                assert_eq!(registers.vector_scratch, initial_scratch);
                assert_eq!(context.calls, 0, "{level:?} {case:?}");
                suppressions += 1;
            }
        }
    }
    assert_eq!(successes, cases.len() * 2);
    assert_eq!(faults, cases.len() * 2);
    assert_eq!(suppressions, 4 * 2);
}
