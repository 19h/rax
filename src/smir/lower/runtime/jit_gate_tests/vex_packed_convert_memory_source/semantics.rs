//! Native/interpreter differential and independent semantic checks.

use super::*;

#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const NATIVE_SAMPLE_COUNT: usize = 16;

#[cfg(target_arch = "x86_64")]
fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn source_words(case: ConvertCase, sample: usize) -> [u64; 8] {
    const F32_VALUES: [u32; 16] = [
        0x0000_0000, // +0
        0x8000_0000, // -0
        0x3FC0_0000, // +1.5
        0xBFC0_0000, // -1.5
        0x0000_0001, // minimum positive subnormal
        0x007F_FFFF, // maximum positive subnormal
        0x0080_0000, // minimum positive normal
        0x7F7F_FFFF, // maximum finite
        0x7F80_0000, // +infinity
        0xFF80_0000, // -infinity
        0x7FC1_2345, // quiet NaN with payload
        0x7F81_2345, // signaling NaN with payload
        0x4F00_0000, // +2^31
        0xCF00_0000, // -2^31
        0x3F00_0000, // +0.5
        0xBF00_0000, // -0.5
    ];
    const F64_VALUES: [u64; 16] = [
        0x0000_0000_0000_0000, // +0
        0x8000_0000_0000_0000, // -0
        0x3FF8_0000_0000_0000, // +1.5
        0xBFF8_0000_0000_0000, // -1.5
        0x0000_0000_0000_0001, // minimum positive subnormal
        0x000F_FFFF_FFFF_FFFF, // maximum positive subnormal
        0x0010_0000_0000_0000, // minimum positive normal
        0x7FEF_FFFF_FFFF_FFFF, // maximum finite
        0x7FF0_0000_0000_0000, // +infinity
        0xFFF0_0000_0000_0000, // -infinity
        0x7FF8_1234_5678_9ABC, // quiet NaN with payload
        0x7FF0_1234_5678_9ABC, // signaling NaN with payload
        0x41E0_0000_0000_0000, // +2^31
        0xC1E0_0000_0000_0000, // -2^31
        0x47EF_FFFF_E000_0000, // maximum finite f32 represented exactly
        0x36A0_0000_0000_0000, // minimum positive f32 subnormal represented exactly
    ];
    const I32_VALUES: [u32; 16] = [
        0,
        1,
        u32::MAX,
        i32::MAX as u32,
        i32::MIN as u32,
        16_777_217,
        (-16_777_217i32) as u32,
        0x4000_0001,
        16_777_215,
        16_777_216,
        16_777_218,
        (-16_777_215i32) as u32,
        (-16_777_216i32) as u32,
        (-16_777_218i32) as u32,
        0x5555_5555,
        0xAAAA_AAAA,
    ];

    let mut bytes = [0u8; 64];
    let lanes = usize::from(case.lanes());
    for lane in 0..lanes {
        let ordinal = sample + lane;
        match case.kind.source_elem() {
            VecElementType::F32 => {
                bytes[lane * 4..(lane + 1) * 4]
                    .copy_from_slice(&F32_VALUES[ordinal % F32_VALUES.len()].to_le_bytes());
            }
            VecElementType::F64 => {
                bytes[lane * 8..(lane + 1) * 8]
                    .copy_from_slice(&F64_VALUES[ordinal % F64_VALUES.len()].to_le_bytes());
            }
            VecElementType::I32 => {
                bytes[lane * 4..(lane + 1) * 4]
                    .copy_from_slice(&I32_VALUES[ordinal % I32_VALUES.len()].to_le_bytes());
            }
            other => unreachable!("packed conversion source element {other:?}"),
        }
    }
    bytes_to_words(bytes)
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 8 | 16 | 32)
    {
        return 0;
    }

    let mut destination_bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    let source_bytes = words_to_bytes(context.value);
    destination_bytes[..size as usize].copy_from_slice(&source_bytes[..size as usize]);
    state.vector_scratch = bytes_to_words(destination_bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ConvertCase, seed: usize, mxcsr: u32) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn effective_address(registers: &GuestRegs, case: ConvertCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64)
}

#[cfg(target_arch = "x86_64")]
fn interpreted_expected(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: ConvertCase,
) -> GuestRegs {
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
        &source_bytes[..case.source_width().bytes() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

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
    let mut scratch_bytes = [0u8; 64];
    let memory_size = case.source_width().bytes() as usize;
    scratch_bytes[..memory_size].copy_from_slice(&source_bytes[..memory_size]);
    expected.vector_scratch = bytes_to_words(scratch_bytes);
    expected
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: ConvertCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(
        context.last_size,
        case.source_width().bytes(),
        "{label} {case:?}"
    );
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: ConvertCase,
    sample: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let destinations = [0u8, 4, 5, 9, 15, 7, 1, 12];
    let c5_bases = [0u8, 2, 4, 5, 7, 1, 3, 6];
    let c4_bases = [11u8, 12, 4, 5, 14, 0, 2, 15];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in DIFFERENTIAL_LEVELS {
        for kind in ConvertKind::ALL {
            for width in [VecWidth::V128, VecWidth::V256] {
                for form in VexForm::ALL {
                    for sample in 0..NATIVE_SAMPLE_COUNT {
                        let base = if form == VexForm::C5 {
                            c5_bases[sample % c5_bases.len()]
                        } else {
                            c4_bases[sample % c4_bases.len()]
                        };
                        let prior_status = 1u32 << (ordinal % 6);
                        let rc = ((sample & 3) as u32) << 13;
                        let daz_ftz = if ordinal & 1 == 0 {
                            0
                        } else {
                            (1 << 6) | (1 << 15)
                        };
                        cases.push(NativeCase {
                            level,
                            instruction: ConvertCase {
                                kind,
                                width,
                                form,
                                destination: destinations[sample % destinations.len()],
                                base,
                            },
                            sample,
                            // Production native-vector admission requires all
                            // six SIMD exception-mask bits to remain set.
                            mxcsr: 0x1F80 | prior_status | rc | daz_ftz,
                        });
                        ordinal += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        cases.len(),
        DIFFERENTIAL_LEVELS.len()
            * ConvertKind::ALL.len()
            * 2
            * VexForm::ALL.len()
            * NATIVE_SAMPLE_COUNT
    );
    assert!(cases.iter().any(|case| case.instruction.destination == 4));
    assert!(cases.iter().any(|case| case.instruction.destination == 5));
    assert!(cases.iter().any(|case| case.instruction.destination == 15));
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 12));
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PACKED_CONVERT_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    let executions = range.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for native_case in &cases[range] {
        let case = native_case.instruction;
        let source = source_words(case, native_case.sample);
        let function = optimize(lift_case(case), native_case.level);
        let (code, entry) = lower(&function, case, native_case.level);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));

        let mut memory_context = VectorMemoryContext {
            value: source,
            ok: 1,
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
        };
        let mut registers = full_guest_regs(case, native_case.sample, native_case.mxcsr);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut memory_context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let initial = registers;
        let mut expected = interpreted_expected(&function, &initial, source, address, case);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_helper_observation(&memory_context, address, case, "success");
        successes += 1;

        let mut memory_context = VectorMemoryContext {
            value: source,
            ok: 0,
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
        };
        let mut registers = full_guest_regs(
            case,
            native_case.sample ^ 0x55,
            native_case.mxcsr ^ (1 << 5),
        );
        let address = effective_address(&registers, case);
        registers.ctx = (&mut memory_context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let mut expected = registers;
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_helper_observation(&memory_context, address, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX packed conversion memory cases"
    );
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX packed-conversion memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        let marker = format!(
            "executed {} successful and {} faulting native VEX packed conversion memory cases",
            cases.len(),
            cases.len()
        );
        assert!(
            String::from_utf8_lossy(&whole.stdout).contains(&marker)
                || String::from_utf8_lossy(&whole.stderr).contains(&marker),
            "isolated native differential child did not execute the requested range; \
             stdout: {}; stderr: {}",
            String::from_utf8_lossy(&whole.stdout),
            String::from_utf8_lossy(&whole.stderr),
        );
        return;
    }

    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = case.instruction.bytes();
    panic!(
        "isolated native VEX packed conversion memory failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_conversions_match_o0_o2_interpreter_and_fault_without_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed conversion memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_packed_convert_memory_source::semantics::\
         native_memory_conversions_match_o0_o2_interpreter_and_fault_without_commit",
    );
}

#[cfg(target_arch = "x86_64")]
fn interpreter_result(
    case: ConvertCase,
    source: [u64; 8],
    mxcsr: u32,
    seed: usize,
) -> (GuestRegs, GuestRegs) {
    let function = optimize(lift_case(case), OptLevel::O2);
    let initial = full_guest_regs(case, seed, mxcsr);
    let address = effective_address(&initial, case);
    let expected = interpreted_expected(&function, &initial, source, address, case);
    (initial, expected)
}

#[cfg(target_arch = "x86_64")]
#[test]
fn interpreter_matches_rounding_indefinite_lane_and_upper_zero_contracts() {
    for (rc, expected) in [16_777_216u32, 16_777_216, 16_777_218, 16_777_216]
        .into_iter()
        .enumerate()
    {
        let case = ConvertCase {
            kind: ConvertKind::I32ToF32,
            width: VecWidth::V128,
            form: VexForm::C5,
            destination: 9,
            base: 4,
        };
        let mut source_bytes = [0u8; 64];
        for lane in 0..4 {
            source_bytes[lane * 4..(lane + 1) * 4].copy_from_slice(&16_777_217i32.to_le_bytes());
        }
        let (initial, actual) = interpreter_result(
            case,
            bytes_to_words(source_bytes),
            0x1F80 | ((rc as u32) << 13),
            rc,
        );
        assert_eq!(
            actual.zmm[9][0] as u32,
            (expected as f32).to_bits(),
            "rc={rc}"
        );
        assert_ne!(actual.mxcsr & (1 << 5), 0, "rc={rc}");
        assert_eq!(actual.zmm[9][2..], [0; 6], "rc={rc}");
        assert_eq!(actual.rflags, initial.rflags, "rc={rc}");
    }

    for (kind, rc, expected) in [
        (ConvertKind::F32ToI32, 0, 2u32),
        (ConvertKind::F32ToI32, 1, 1),
        (ConvertKind::F32ToI32, 2, 2),
        (ConvertKind::F32ToI32, 3, 1),
        (ConvertKind::F32ToI32Truncate, 0, 1),
        (ConvertKind::F32ToI32Truncate, 2, 1),
    ] {
        let case = ConvertCase {
            kind,
            width: VecWidth::V128,
            form: VexForm::C4W1,
            destination: 15,
            base: 12,
        };
        let mut source_bytes = [0u8; 64];
        for lane in 0..4 {
            source_bytes[lane * 4..(lane + 1) * 4].copy_from_slice(&1.5f32.to_bits().to_le_bytes());
        }
        let (_, actual) = interpreter_result(
            case,
            bytes_to_words(source_bytes),
            0x1F80 | (rc << 13),
            rc as usize,
        );
        for lane in 0..4 {
            let offset = lane * 4;
            let destination = words_to_bytes(actual.zmm[15]);
            assert_eq!(
                u32::from_le_bytes(destination[offset..offset + 4].try_into().unwrap()),
                expected,
                "{kind:?} rc={rc} lane={lane}"
            );
        }
        assert_eq!(actual.zmm[15][2..], [0; 6], "{kind:?} rc={rc}");
    }

    for bits in [0x7F80_0000u32, 0x7FC1_2345, 0x7F81_2345] {
        let case = ConvertCase {
            kind: ConvertKind::F32ToI32,
            width: VecWidth::V128,
            form: VexForm::C5,
            destination: 5,
            base: 2,
        };
        let mut source_bytes = [0u8; 64];
        for lane in 0..4 {
            source_bytes[lane * 4..(lane + 1) * 4].copy_from_slice(&bits.to_le_bytes());
        }
        let (_, actual) =
            interpreter_result(case, bytes_to_words(source_bytes), 0x1F80, bits as usize);
        let destination = words_to_bytes(actual.zmm[5]);
        for lane in 0..4 {
            assert_eq!(
                u32::from_le_bytes(destination[lane * 4..lane * 4 + 4].try_into().unwrap()),
                0x8000_0000,
                "bits={bits:08X} lane={lane}"
            );
        }
        assert_ne!(actual.mxcsr & 1, 0, "bits={bits:08X}");
    }

    let widening = ConvertCase {
        kind: ConvertKind::F32ToF64,
        width: VecWidth::V256,
        form: VexForm::C4W0,
        destination: 9,
        base: 11,
    };
    let mut source_bytes = [0u8; 64];
    for (lane, value) in [0.0f32, -0.0, 1.5, f32::INFINITY].into_iter().enumerate() {
        source_bytes[lane * 4..(lane + 1) * 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    let (_, actual) = interpreter_result(widening, bytes_to_words(source_bytes), 0x1F80, 0);
    assert_eq!(
        actual.zmm[9][..4],
        [
            0.0f64.to_bits(),
            (-0.0f64).to_bits(),
            1.5f64.to_bits(),
            f64::INFINITY.to_bits(),
        ]
    );
    assert_eq!(actual.zmm[9][4..], [0; 4]);
}
