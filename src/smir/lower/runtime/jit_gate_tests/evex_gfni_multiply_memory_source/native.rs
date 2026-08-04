//! Native x86-64 differential, helper-call, and precise Type E4 fault coverage.

use super::semantics::{SemanticState, initial_state, interpret, memory_bytes};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K64};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

struct ByteMemoryContext {
    base: u64,
    value: [u8; 64],
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 64],
}

extern "C" fn byte_load_helper(
    context: *mut ByteMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size, 1);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset < context.value.len());
    LoadResult {
        value: u64::from(context.value[offset]),
        ok: 1,
    }
}

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
    if !context.ok
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32 | 64)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
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
        vector_active: X86_VECTOR_STATE_K64,
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
    case: GfniMultiplyCase,
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

fn selected_cases() -> [GfniMultiplyCase; 6] {
    [
        GfniMultiplyCase {
            width: VecWidth::V128,
            destination: 17,
            source1: 17,
            control: MaskControl::None,
        },
        GfniMultiplyCase {
            width: VecWidth::V512,
            destination: 9,
            source1: 30,
            control: MaskControl::None,
        },
        GfniMultiplyCase {
            width: VecWidth::V128,
            destination: 25,
            source1: 25,
            control: MaskControl::Merge,
        },
        GfniMultiplyCase {
            width: VecWidth::V256,
            destination: 17,
            source1: 18,
            control: MaskControl::Zero,
        },
        GfniMultiplyCase {
            width: VecWidth::V512,
            destination: 31,
            source1: 31,
            control: MaskControl::Merge,
        },
        GfniMultiplyCase {
            width: VecWidth::V512,
            destination: 17,
            source1: 30,
            control: MaskControl::Zero,
        },
    ]
}

#[test]
fn native_memory_matches_interpreter_active_byte_helpers_faults_and_suppression() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("gfni")
    {
        eprintln!("skipping native EVEX VGF2P8MULB: host lacks AVX-512F/BW or GFNI");
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
            let exact = sequence(&function, true).expect("native exact sequence");
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut initial = initial_state(case, ordinal);
            let bytes = memory_bytes(case, ordinal);

            if case.control == MaskControl::None {
                let value = memory_words(&bytes);
                let expected = interpret(&function, &initial, &bytes, case);
                let mut context = VectorMemoryContext {
                    value,
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
                expected_scratch[..(case.width.bytes() / 8) as usize]
                    .copy_from_slice(&value[..(case.width.bytes() / 8) as usize]);
                assert_eq!(registers.vector_scratch, expected_scratch);
                assert_eq!(context.calls, 1);
                assert_eq!(context.last_addr, 0x2000);
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
                );
                assert_eq!(context.last_size, case.width.bytes());
                assert_eq!(context.last_zero_upper, 1);
                successes += 1;

                let mut context = VectorMemoryContext {
                    value,
                    ok: false,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&initial);
                let initial_registers = registers;
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                exec.run(entry, &mut registers);
                assert_architectural_state(&registers, &initial, level, case);
                assert_eq!(registers.exit_pc, PC);
                assert_eq!(registers.vector_scratch, initial_registers.vector_scratch);
                assert_eq!(context.calls, 1);
                faults += 1;
                continue;
            }

            if ordinal & 1 != 0 {
                initial.masks[usize::from(case.mask())] = 0;
            }
            let expected = interpret(&function, &initial, &bytes, case);
            let active_lanes: Vec<_> = (0..case.width.bytes())
                .filter(|lane| initial.masks[usize::from(case.mask())] & (1u64 << lane) != 0)
                .collect();
            let mut context = ByteMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: None,
                calls: 0,
                addresses: [0; 64],
            };
            let mut registers = guest_regs(&initial);
            let initial_scratch = registers.vector_scratch;
            registers.ctx = (&mut context as *mut ByteMemoryContext) as u64;
            registers.load_fn = byte_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &expected, level, case);
            let expected_addresses: Vec<u64> = active_lanes
                .iter()
                .map(|lane| 0x2000 + u64::from(*lane))
                .collect();
            assert_eq!(&context.addresses[..context.calls], expected_addresses);
            assert_eq!(registers.vector_scratch, initial_scratch);
            successes += 1;

            if active_lanes.is_empty() {
                assert_eq!(context.calls, 0);
                suppressions += 1;
                continue;
            }
            let failing_lane = active_lanes[usize::from(active_lanes.len() > 1)];
            let fail_address = 0x2000 + u64::from(failing_lane);
            let mut context = ByteMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 64],
            };
            let mut registers = guest_regs(&initial);
            let initial_scratch = registers.vector_scratch;
            registers.ctx = (&mut context as *mut ByteMemoryContext) as u64;
            registers.load_fn = byte_load_helper as *const () as usize as u64;
            exec.run(entry, &mut registers);
            assert_architectural_state(&registers, &initial, level, case);
            assert_eq!(registers.exit_pc, PC);
            assert_eq!(registers.vector_scratch, initial_scratch);
            assert_eq!(context.addresses[context.calls - 1], fail_address);
            faults += 1;

            assert!(matches!(
                exact.encoding.replay,
                X86EvexGfniMultiplyMemoryReplay::MaskedVector { .. }
            ));
        }
    }
    assert!(successes >= 6);
    assert!(faults >= 4);
    assert!(suppressions >= 2);
}
