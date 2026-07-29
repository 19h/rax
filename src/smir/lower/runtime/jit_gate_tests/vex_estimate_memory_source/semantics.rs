//! Independent estimate contracts and native helper-boundary differential.

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

fn set_f32_lane(vector: &mut [u64; 8], lane: usize, value: u32) {
    let word = lane / 2;
    let shift = ((lane & 1) * 32) as u32;
    vector[word] = (vector[word] & !(u64::from(u32::MAX) << shift)) | (u64::from(value) << shift);
}

fn get_f32_lane(vector: &[u64; 8], lane: usize) -> u32 {
    let word = lane / 2;
    let shift = ((lane & 1) * 32) as u32;
    (vector[word] >> shift) as u32
}

fn active_lanes(case: EstimateMemoryCase) -> usize {
    if case.shape.scalar() {
        1
    } else {
        case.logical_width().lanes(VecElementType::F32) as usize
    }
}

fn patterned_vector(shift: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0xFEDC_BA98_7654_3210u64.rotate_left(((word * 11 + shift) % 64) as u32)
            ^ (shift as u64).wrapping_mul(0x0102_0304_0506_0708)
    })
}

fn source_value(case: EstimateMemoryCase, ordinal: usize) -> [u64; 8] {
    const INPUTS: [u32; 16] = [
        7.0f32.to_bits(),
        3.0f32.to_bits(),
        4.0f32.to_bits(),
        (-11.0f32).to_bits(),
        0,
        0x8000_0000,
        1,
        0x8000_0001,
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
        0x7FC1_2345,
        0xFF81_2345,
        f32::MAX.to_bits(),
        f32::MIN_POSITIVE.to_bits(),
        0.5f32.to_bits(),
        (-0.5f32).to_bits(),
    ];
    let mut source = [0; 8];
    for lane in 0..active_lanes(case) {
        set_f32_lane(&mut source, lane, INPUTS[(ordinal + lane) % INPUTS.len()]);
    }
    source
}

fn full_guest_regs(case: EstimateMemoryCase, ordinal: usize) -> GuestRegs {
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
        // All SIMD exceptions remain masked. Prior status, RC, DAZ, and FTZ
        // vary independently even though estimate instructions preserve/ignore
        // them.
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
    case: EstimateMemoryCase,
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

fn assert_intel_estimate(estimate: Estimate, input: u32, output: u32) {
    let sign = input & 0x8000_0000;
    let exponent = input & 0x7F80_0000;
    let fraction = input & 0x007F_FFFF;
    if exponent == 0 {
        assert_eq!(output, sign | 0x7F80_0000, "input={input:08X}");
        return;
    }
    if exponent == 0x7F80_0000 && fraction != 0 {
        assert_eq!(output, input | 0x0040_0000, "input={input:08X}");
        return;
    }
    if estimate == Estimate::ReciprocalSqrt && sign != 0 {
        assert_eq!(output, 0xFFC0_0000, "input={input:08X}");
        return;
    }
    if exponent == 0x7F80_0000 {
        assert_eq!(
            output,
            if estimate == Estimate::Reciprocal {
                sign
            } else {
                0
            },
            "input={input:08X}"
        );
        return;
    }

    let value = f64::from(f32::from_bits(input));
    let exact = if estimate == Estimate::Reciprocal {
        1.0 / value
    } else {
        1.0 / value.sqrt()
    };
    if exact.abs() < f64::from(f32::MIN_POSITIVE) {
        assert_eq!(output, sign, "tiny input={input:08X}");
        return;
    }
    let actual = f64::from(f32::from_bits(output));
    let relative_error = ((actual - exact) / exact).abs();
    assert!(
        relative_error <= INTEL_RELATIVE_ERROR_BOUND,
        "input={input:08X} output={output:08X} error={relative_error:e} \
         bound={INTEL_RELATIVE_ERROR_BOUND:e}"
    );
}

fn assert_active_estimates(case: EstimateMemoryCase, source: [u64; 8], registers: &GuestRegs) {
    let destination = &registers.zmm[usize::from(case.destination)];
    for lane in 0..active_lanes(case) {
        assert_intel_estimate(
            case.estimate,
            get_f32_lane(&source, lane),
            get_f32_lane(destination, lane),
        );
    }
}

fn mask_active_estimates(case: EstimateMemoryCase, registers: &mut GuestRegs) {
    for lane in 0..active_lanes(case) {
        set_f32_lane(&mut registers.zmm[usize::from(case.destination)], lane, 0);
    }
}

fn architectural_template(
    case: EstimateMemoryCase,
    initial: &GuestRegs,
    source: [u64; 8],
) -> GuestRegs {
    let mut expected = *initial;
    let source_bytes = words_to_bytes(source);
    let mut scratch = [0; 64];
    scratch[..case.memory_size() as usize]
        .copy_from_slice(&source_bytes[..case.memory_size() as usize]);
    expected.vector_scratch = bytes_to_words(scratch);

    let mut destination = [0; 8];
    if case.shape.scalar() {
        for lane in 1..4 {
            set_f32_lane(
                &mut destination,
                lane,
                get_f32_lane(&initial.zmm[usize::from(case.source1)], lane),
            );
        }
    }
    expected.zmm[usize::from(case.destination)] = destination;
    expected
}

#[test]
fn interpreted_memory_estimates_obey_error_special_merge_upper_clear_and_state_contracts() {
    let mut checked = 0usize;
    for estimate in Estimate::ALL {
        for shape in Shape::ALL {
            for encoded_width in [VecWidth::V128, VecWidth::V256] {
                let case = EstimateMemoryCase {
                    estimate,
                    shape,
                    encoded_width,
                    form: EncodingForm::C5,
                    destination: 5,
                    source1: 6,
                    base: 3,
                };
                for ordinal in 0..16 {
                    let function = optimize(lift_case(case), OptLevel::O0);
                    let initial = full_guest_regs(case, ordinal);
                    let source = source_value(case, ordinal);
                    let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                    let mut actual = interpreted_success(
                        &function,
                        &initial,
                        source,
                        address,
                        case,
                        OptLevel::O0,
                    );
                    assert_active_estimates(case, source, &actual);
                    mask_active_estimates(case, &mut actual);
                    assert_eq!(
                        actual,
                        architectural_template(case, &initial, source),
                        "{case:?} ordinal={ordinal}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 128);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct EstimateMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn estimate_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut EstimateMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 16 | 32)
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
    context: &EstimateMemoryContext,
    address: u64,
    case: EstimateMemoryCase,
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
fn native_memory_estimates_obey_independent_bounds_and_match_all_other_state() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX estimate memory differential: host lacks AVX");
        return;
    }
    let cases = semantic_cases();
    assert_eq!(cases.len(), 108);
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

            let mut context = EstimateMemoryContext {
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
            registers.ctx = (&mut context as *mut EstimateMemoryContext) as u64;
            registers.vec_load_fn = estimate_load_helper as usize as u64;
            let initial = registers;
            let mut expected =
                interpreted_success(&function, &initial, source, address, case, level);

            exec.run(entry, &mut registers);
            assert_active_estimates(case, source, &registers);
            assert_active_estimates(case, source, &expected);
            mask_active_estimates(case, &mut registers);
            mask_active_estimates(case, &mut expected);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_call(&context, address, case, "success");
            successes += 1;

            let mut context = EstimateMemoryContext {
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
            registers.ctx = (&mut context as *mut EstimateMemoryContext) as u64;
            registers.vec_load_fn = estimate_load_helper as usize as u64;
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
