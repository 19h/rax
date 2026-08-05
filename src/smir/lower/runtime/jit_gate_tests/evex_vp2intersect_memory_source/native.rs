//! Native x86-64 differential, helper-footprint, scratch, and fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ScalarMemoryContext {
    base: u64,
    value: [u8; 64],
    elem_bytes: usize,
    ok: bool,
    calls: usize,
    last_address: u64,
    last_size: u64,
}

extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size as usize, context.elem_bytes);
    assert_eq!(signed, 0);
    context.calls += 1;
    context.last_address = address;
    context.last_size = size;
    if !context.ok {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + context.elem_bytes <= context.value.len());
    LoadResult {
        value: match context.elem_bytes {
            4 => u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(context.value[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!("VP2INTERSECT scalar helper width"),
        },
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
    case: Vp2IntersectMemoryCase,
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

fn selected_cases() -> [Vp2IntersectMemoryCase; 8] {
    [
        Vp2IntersectMemoryCase {
            width: VecWidth::V128,
            elem: VecElementType::I32,
            destination: 1,
            source1: 0,
            broadcast: false,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V128,
            elem: VecElementType::I64,
            destination: 3,
            source1: 17,
            broadcast: true,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V256,
            elem: VecElementType::I32,
            destination: 5,
            source1: 15,
            broadcast: false,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V256,
            elem: VecElementType::I64,
            destination: 7,
            source1: 31,
            broadcast: true,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I32,
            destination: 0,
            source1: 18,
            broadcast: false,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I64,
            destination: 2,
            source1: 19,
            broadcast: true,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I32,
            destination: 5,
            source1: 31,
            broadcast: true,
        },
        Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I64,
            destination: 7,
            source1: 0,
            broadcast: false,
        },
    ]
}

#[test]
fn native_vp2intersect_matches_interpreter_and_faults_before_both_k_commits() {
    use super::super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vp2intersect")
    {
        eprintln!(
            "skipping native VP2INTERSECT memory differential: host lacks AVX-512F/BW/VP2INTERSECT"
        );
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
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_state(case, ordinal + 401);
            let bytes = memory_bytes(case, &initial, ordinal + 401);
            let expected = interpret(&function, &initial, &bytes, case);

            if case.broadcast {
                let mut context = ScalarMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    elem_bytes: case.elem.bytes() as usize,
                    ok: true,
                    calls: 0,
                    last_address: 0,
                    last_size: 0,
                };
                let mut registers = guest_regs(&initial);
                registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                registers.load_fn = scalar_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &expected, level, case);
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_address, 0x2000, "{level:?} {case:?}");
                assert_eq!(
                    context.last_size,
                    u64::from(case.elem.bytes()),
                    "{level:?} {case:?}"
                );
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
            } else {
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
        }
    }
    assert!(successes >= 8);
    assert_eq!(successes, faults);
}
