//! Independent architectural checks and native helper-boundary differential.

use super::*;

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn set_lane(bytes: &mut [u8; 64], lane: usize, elem: VecElementType, value: u64) {
    match elem {
        VecElementType::F32 => {
            bytes[lane * 4..lane * 4 + 4].copy_from_slice(&(value as u32).to_le_bytes());
        }
        VecElementType::F64 => {
            bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        _ => unreachable!("VEX square-root element"),
    }
}

fn get_lane(words: [u64; 8], lane: usize, elem: VecElementType) -> u64 {
    let bytes = words_to_bytes(words);
    match elem {
        VecElementType::F32 => u64::from(u32::from_le_bytes(
            bytes[lane * 4..lane * 4 + 4].try_into().unwrap(),
        )),
        VecElementType::F64 => {
            u64::from_le_bytes(bytes[lane * 8..lane * 8 + 8].try_into().unwrap())
        }
        _ => unreachable!("VEX square-root element"),
    }
}

fn patterned_vector(shift: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0xFEDC_BA98_7654_3210u64.rotate_left(((word * 11 + shift) % 64) as u32)
            ^ (shift as u64).wrapping_mul(0x0102_0304_0506_0708)
    })
}

fn source_value(case: SqrtMemoryCase, ordinal: usize) -> [u64; 8] {
    const F32: [u64; 12] = [
        0x4000_0000, // +2, inexact square root
        0x4080_0000, // +4
        0xBF80_0000, // -1, invalid
        0x7F81_2345, // signaling NaN, invalid
        0x0000_0001, // minimum subnormal, denormal and inexact unless DAZ
        0x8000_0000, // -0
        0x7F80_0000, // +infinity
        0x7FC1_2345, // quiet NaN
        0x4110_0000, // +9
        0x4180_0000, // +16
        0x0000_0000, // +0
        0x3F80_0000, // +1
    ];
    const F64: [u64; 10] = [
        0x4000_0000_0000_0000, // +2, inexact square root
        0x4010_0000_0000_0000, // +4
        0xBFF0_0000_0000_0000, // -1, invalid
        0x7FF0_2468_ACE0_1357, // signaling NaN, invalid
        0x0000_0000_0000_0001, // minimum subnormal, denormal
        0x8000_0000_0000_0000, // -0
        0x7FF0_0000_0000_0000, // +infinity
        0x7FF8_2468_ACE0_1357, // quiet NaN
        0x4022_0000_0000_0000, // +9
        0x4030_0000_0000_0000, // +16
    ];

    let mut bytes = [0; 64];
    let elem = case.kind.elem();
    let lanes = if case.kind.scalar() {
        1
    } else {
        case.width.lanes(elem) as usize
    };
    for lane in 0..lanes {
        let value = match elem {
            VecElementType::F32 => F32[(ordinal + lane) % F32.len()],
            VecElementType::F64 => F64[(ordinal + lane) % F64.len()],
            _ => unreachable!(),
        };
        set_lane(&mut bytes, lane, elem, value);
    }
    bytes_to_words(bytes)
}

fn full_guest_regs(case: SqrtMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        // Every SIMD exception is masked. RC, prior status, DAZ, and FTZ vary
        // independently while preserving the CPU admission invariant.
        mxcsr: 0x1F80
            | ((ordinal as u32) & 0x3F)
            | (((ordinal as u32) & 3) << 13)
            | (u32::from(ordinal & 4 != 0) << 6)
            | (u32::from(ordinal & 8 != 0) << 15),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(index * 5 + ordinal);
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

fn interpreted_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: SqrtMemoryCase,
    level: OptLevel,
) -> GuestRegs {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let source_bytes = words_to_bytes(source);
    memory.load(
        address as usize,
        &source_bytes[..case.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in expected.zmm.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    let mut scratch = [0; 64];
    scratch[..case.memory_size() as usize]
        .copy_from_slice(&source_bytes[..case.memory_size() as usize]);
    expected.vector_scratch = bytes_to_words(scratch);
    expected
}

fn interpreted_case(case: SqrtMemoryCase, source: [u64; 8], mxcsr: u32) -> (GuestRegs, GuestRegs) {
    let function = lift_case(case);
    let mut initial = full_guest_regs(case, 0);
    initial.mxcsr = mxcsr;
    let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
    let expected = interpreted_success(&function, &initial, source, address, case, OptLevel::O0);
    (initial, expected)
}

#[test]
fn memory_sqrt_matches_independent_ieee_special_rounding_daz_and_merge_oracles() {
    let packed_f32 = SqrtMemoryCase {
        kind: SqrtKind::PackedF32,
        width: VecWidth::V128,
        form: EncodingForm::C5,
        destination: 1,
        source1: 0,
        base: 3,
    };
    let mut source = [0; 8];
    source[0] = 0x4080_0000_4000_0000;
    source[1] = 0x7F81_2345_BF80_0000;
    let (initial, result) = interpreted_case(packed_f32, source, 0x1F80);
    assert_eq!(result.zmm[1][0], 0x4000_0000_3FB5_04F3);
    assert_eq!(result.zmm[1][1], 0x7FC1_2345_FFC0_0000);
    assert!(result.zmm[1][2..].iter().all(|word| *word == 0));
    assert_eq!(result.mxcsr & 0x3F, (1 << 0) | (1 << 5));
    assert_eq!(result.rflags, initial.rflags);

    let packed_f64 = SqrtMemoryCase {
        kind: SqrtKind::PackedF64,
        width: VecWidth::V128,
        form: EncodingForm::C5,
        destination: 2,
        source1: 0,
        base: 3,
    };
    source = [0; 8];
    source[0] = 0x4000_0000_0000_0000;
    source[1] = 1;
    let (initial, result) = interpreted_case(packed_f64, source, 0x1F80);
    assert_eq!(result.zmm[2][0], 0x3FF6_A09E_667F_3BCD);
    assert_eq!(result.zmm[2][1], 0x1E60_0000_0000_0000);
    assert!(result.zmm[2][2..].iter().all(|word| *word == 0));
    assert_eq!(result.mxcsr & 0x3F, (1 << 1) | (1 << 5));
    assert_eq!(result.rflags, initial.rflags);

    let scalar_f32 = SqrtMemoryCase {
        kind: SqrtKind::ScalarF32,
        width: VecWidth::V128,
        form: EncodingForm::C5,
        destination: 5,
        source1: 6,
        base: 3,
    };
    for (rounding, expected_low) in [
        (0, 0x3FB5_04F3),
        (1, 0x3FB5_04F3),
        (2, 0x3FB5_04F4),
        (3, 0x3FB5_04F3),
    ] {
        source = [u64::from(2.0f32.to_bits()), 0, 0, 0, 0, 0, 0, 0];
        let (initial, result) = interpreted_case(scalar_f32, source, 0x1F80 | (rounding << 13));
        assert_eq!(
            get_lane(result.zmm[5], 0, VecElementType::F32),
            expected_low
        );
        for lane in 1..4 {
            assert_eq!(
                get_lane(result.zmm[5], lane, VecElementType::F32),
                get_lane(initial.zmm[6], lane, VecElementType::F32),
                "RC={rounding} merge lane {lane}"
            );
        }
        assert!(result.zmm[5][2..].iter().all(|word| *word == 0));
        assert_eq!(result.mxcsr & 0x3F, 1 << 5);
        assert_eq!(result.rflags, initial.rflags);
    }

    let scalar_f64 = SqrtMemoryCase {
        kind: SqrtKind::ScalarF64,
        width: VecWidth::V128,
        form: EncodingForm::C5,
        destination: 7,
        source1: 7,
        base: 3,
    };
    source = [1, 0, 0, 0, 0, 0, 0, 0];
    let (initial, result) = interpreted_case(scalar_f64, source, 0x1F80 | (1 << 6));
    assert_eq!(result.zmm[7][0], 0, "DAZ must convert +min-subnormal to +0");
    assert_eq!(result.zmm[7][1], initial.zmm[7][1], "aliased merge lane");
    assert!(result.zmm[7][2..].iter().all(|word| *word == 0));
    assert_eq!(result.mxcsr & 0x3F, 0);
    assert_eq!(result.rflags, initial.rflags);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct SqrtMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn sqrt_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut SqrtMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8 | 16 | 32)
    {
        return 0;
    }
    let source = words_to_bytes(context.value);
    let mut scratch = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    scratch[..size as usize].copy_from_slice(&source[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_call(
    context: &SqrtMemoryContext,
    address: u64,
    case: SqrtMemoryCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, case.memory_size(), "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_sqrt_matches_interpreter_for_aliases_mxcsr_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX square-root memory differential: host lacks AVX");
        return;
    }
    let cases = semantic_cases();
    assert_eq!(cases.len(), 72);
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_value(case, ordinal);

            let mut context = SqrtMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut SqrtMemoryContext) as u64;
            registers.vec_load_fn = sqrt_load_helper as usize as u64;
            let initial = registers;
            let mut expected =
                interpreted_success(&function, &initial, source, address, case, level);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_call(&context, address, case, "success");
            successes += 1;

            let mut context = SqrtMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut SqrtMemoryContext) as u64;
            registers.vec_load_fn = sqrt_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_helper_call(&context, address, case, "fault");
            faults += 1;
        }
    }

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
