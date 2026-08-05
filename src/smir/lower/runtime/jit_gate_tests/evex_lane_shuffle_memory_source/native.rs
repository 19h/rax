//! Native x86-64 differential, helper-call, scratch, WIG, and fault tests.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ScalarMemoryContext {
    value: u64,
    ok: bool,
    calls: usize,
    last_address: u64,
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
    assert_eq!(size, 4);
    assert_eq!(signed, 0);
    context.calls += 1;
    context.last_address = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: u64::from(context.ok),
    }
}

fn memory_words(bytes: &[u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn scalar_value(bytes: &[u8; 64]) -> u64 {
    u64::from(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
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
    case: LaneShuffleMemoryCase,
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

fn selected_cases() -> [LaneShuffleMemoryCase; 9] {
    [
        LaneShuffleMemoryCase {
            kind: ShuffleKind::Dword,
            width: VecWidth::V128,
            w: false,
            destination: 0,
            control: MaskControl::None,
            tuple: TupleKind::Full,
            immediate: 0xE4,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::Dword,
            width: VecWidth::V256,
            w: false,
            destination: 9,
            control: MaskControl::Merge,
            tuple: TupleKind::Broadcast,
            immediate: 0x1B,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::Dword,
            width: VecWidth::V512,
            w: false,
            destination: 25,
            control: MaskControl::Zero,
            tuple: TupleKind::Full,
            immediate: 0xA5,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::HighWord,
            width: VecWidth::V128,
            w: false,
            destination: 1,
            control: MaskControl::Merge,
            tuple: TupleKind::Full,
            immediate: 0x5A,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::HighWord,
            width: VecWidth::V256,
            w: true,
            destination: 17,
            control: MaskControl::Zero,
            tuple: TupleKind::Full,
            immediate: 0xC3,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::HighWord,
            width: VecWidth::V512,
            w: false,
            destination: 25,
            control: MaskControl::None,
            tuple: TupleKind::Full,
            immediate: 0x93,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::LowWord,
            width: VecWidth::V128,
            w: true,
            destination: 9,
            control: MaskControl::Zero,
            tuple: TupleKind::Full,
            immediate: 0x00,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::LowWord,
            width: VecWidth::V256,
            w: false,
            destination: 17,
            control: MaskControl::Merge,
            tuple: TupleKind::Full,
            immediate: 0xFF,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::LowWord,
            width: VecWidth::V512,
            w: true,
            destination: 25,
            control: MaskControl::None,
            tuple: TupleKind::Full,
            immediate: 0xE4,
        },
    ]
}

#[test]
fn native_lane_shuffle_matches_interpreter_restores_scratch_and_preserves_e4nf_faults() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX lane-shuffle differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let expected_runs = cases.len() * 2;
    let expected_empty_mask_runs = cases.iter().filter(|case| case.mask() != 0).count() * 2;

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut empty_mask_successes = 0usize;
    let mut empty_mask_faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal);
            let bytes = memory_bytes(case, ordinal);
            let expected = interpret(&function, &initial, &bytes, case);

            match case.tuple {
                TupleKind::Full => {
                    let mut context = VectorMemoryContext {
                        value: memory_words(&bytes),
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

                    context.ok = 0;
                    context.calls = 0;
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
                }
                TupleKind::Broadcast => {
                    let mut context = ScalarMemoryContext {
                        value: scalar_value(&bytes),
                        ok: true,
                        calls: 0,
                        last_address: 0,
                        last_size: 0,
                        last_signed: u64::MAX,
                    };
                    let mut registers = guest_regs(&initial);
                    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                    registers.load_fn = scalar_load_helper as *const () as usize as u64;
                    exec.run(entry, &mut registers);
                    assert_architectural_state(&registers, &expected, level, case);
                    assert_eq!(context.calls, 1, "{level:?} {case:?}");
                    assert_eq!(context.last_address, 0x2000, "{level:?} {case:?}");
                    assert_eq!(context.last_size, 4, "{level:?} {case:?}");
                    assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
                    successes += 1;

                    context.ok = false;
                    context.calls = 0;
                    let mut registers = guest_regs(&initial);
                    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                    registers.load_fn = scalar_load_helper as *const () as usize as u64;
                    let mut expected_fault = registers;
                    expected_fault.exit_pc = PC;
                    exec.run(entry, &mut registers);
                    expected_fault.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(registers, expected_fault, "{level:?} {case:?}: fault");
                    assert_eq!(context.calls, 1, "{level:?} {case:?}: fault calls");
                    faults += 1;
                }
            }

            if case.mask() == 0 {
                continue;
            }
            let mut empty = initial.clone();
            empty.masks[usize::from(case.mask())] = 0;
            let expected = interpret(&function, &empty, &bytes, case);
            match case.tuple {
                TupleKind::Full => {
                    let mut context = VectorMemoryContext {
                        value: memory_words(&bytes),
                        ok: 1,
                        calls: 0,
                        last_addr: 0,
                        last_index: 0,
                        last_size: 0,
                        last_zero_upper: 0,
                    };
                    let mut registers = guest_regs(&empty);
                    registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                    registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                    exec.run(entry, &mut registers);
                    assert_architectural_state(&registers, &expected, level, case);
                    assert_eq!(context.calls, 1, "{level:?} {case:?}: empty mask");
                    empty_mask_successes += 1;

                    context.ok = 0;
                    context.calls = 0;
                    let mut registers = guest_regs(&empty);
                    registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                    registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                    let mut expected_fault = registers;
                    expected_fault.exit_pc = PC;
                    exec.run(entry, &mut registers);
                    expected_fault.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(
                        registers, expected_fault,
                        "{level:?} {case:?}: empty-mask fault"
                    );
                    assert_eq!(
                        context.calls, 1,
                        "{level:?} {case:?}: empty-mask fault calls"
                    );
                    empty_mask_faults += 1;
                }
                TupleKind::Broadcast => {
                    let mut context = ScalarMemoryContext {
                        value: scalar_value(&bytes),
                        ok: true,
                        calls: 0,
                        last_address: 0,
                        last_size: 0,
                        last_signed: u64::MAX,
                    };
                    let mut registers = guest_regs(&empty);
                    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                    registers.load_fn = scalar_load_helper as *const () as usize as u64;
                    exec.run(entry, &mut registers);
                    assert_architectural_state(&registers, &expected, level, case);
                    assert_eq!(context.calls, 1, "{level:?} {case:?}: empty mask");
                    empty_mask_successes += 1;

                    context.ok = false;
                    context.calls = 0;
                    let mut registers = guest_regs(&empty);
                    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                    registers.load_fn = scalar_load_helper as *const () as usize as u64;
                    let mut expected_fault = registers;
                    expected_fault.exit_pc = PC;
                    exec.run(entry, &mut registers);
                    expected_fault.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(
                        registers, expected_fault,
                        "{level:?} {case:?}: empty-mask fault"
                    );
                    assert_eq!(
                        context.calls, 1,
                        "{level:?} {case:?}: empty-mask fault calls"
                    );
                    empty_mask_faults += 1;
                }
            }
        }
    }
    assert_eq!(successes, expected_runs);
    assert_eq!(faults, expected_runs);
    assert_eq!(empty_mask_successes, expected_empty_mask_runs);
    assert_eq!(empty_mask_faults, expected_empty_mask_runs);
}
