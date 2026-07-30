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
    let value = match case.kind {
        ConvertKind::F32ToF64 | ConvertKind::F32ToI32Or64 | ConvertKind::F32ToI32Or64Truncate => {
            u64::from(
                [
                    0x0000_0000u32, // +0
                    0x8000_0000,    // -0
                    0x3FC0_0000,    // +1.5
                    0xBFC0_0000,    // -1.5
                    0x0000_0001,    // minimum positive subnormal
                    0x7F80_0000,    // +infinity
                    0x7FC1_2345,    // quiet NaN with payload
                    0x7F81_2345,    // signaling NaN with payload
                ][sample % 8],
            )
        }
        ConvertKind::F64ToF32 | ConvertKind::F64ToI32Or64 | ConvertKind::F64ToI32Or64Truncate => [
            0x0000_0000_0000_0000u64, // +0
            0x8000_0000_0000_0000,    // -0
            0x3FF8_0000_0000_0000,    // +1.5
            0xBFF8_0000_0000_0000,    // -1.5
            0x0000_0000_0000_0001,    // minimum positive subnormal
            0x7FF0_0000_0000_0000,    // +infinity
            0x7FF8_1234_5678_9ABC,    // quiet NaN with payload
            0x7FF0_1234_5678_9ABC,    // signaling NaN with payload
        ][sample % 8],
        ConvertKind::I32Or64ToF32 | ConvertKind::I32Or64ToF64 => {
            if case.int_width() == OpWidth::W32 {
                u64::from(
                    [
                        0u32,
                        1,
                        u32::MAX,
                        i32::MAX as u32,
                        i32::MIN as u32,
                        16_777_217,
                        (-16_777_217i32) as u32,
                        0x4000_0001,
                    ][sample % 8],
                )
            } else {
                [
                    0u64,
                    1,
                    u64::MAX,
                    i64::MAX as u64,
                    i64::MIN as u64,
                    9_007_199_254_740_993,
                    (-9_007_199_254_740_993i64) as u64,
                    0x4000_0000_0000_0001,
                ][sample % 8]
            }
        }
    };
    [value, 0, 0, 0, 0, 0, 0, 0]
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
        || !matches!(size, 4 | 8)
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
    if matches!(
        case.form,
        VexForm::C4 {
            encoded_x_clear: true,
            ..
        }
    ) && case.base & 7 == 4
        && case.base != 12
    {
        registers.gpr[12] = 0x180;
    }
    registers
}

#[cfg(target_arch = "x86_64")]
fn effective_address(registers: &GuestRegs, case: ConvertCase) -> u64 {
    let mut address = registers.gpr[usize::from(case.base)];
    if matches!(
        case.form,
        VexForm::C4 {
            encoded_x_clear: true,
            ..
        }
    ) && case.base & 7 == 4
    {
        address = address.wrapping_add(registers.gpr[12]);
    }
    address.wrapping_add(DISP as u64)
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
        &source_bytes[..case.memory_size() as usize],
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
    scratch_bytes[..case.memory_size() as usize]
        .copy_from_slice(&source_bytes[..case.memory_size() as usize]);
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
    assert_eq!(context.last_size, case.memory_size(), "{label} {case:?}");
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
            for form in VexForm::SCANNER_FORMS {
                for sample in 0..8 {
                    let destination = destinations[sample];
                    let merge = if kind.has_merge() {
                        if sample == 0 {
                            destination
                        } else {
                            destinations[(sample + 3) % destinations.len()]
                        }
                    } else {
                        0
                    };
                    let base = if form == VexForm::C5 {
                        c5_bases[sample]
                    } else {
                        c4_bases[sample]
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
                            form,
                            destination,
                            merge,
                            base,
                        },
                        sample,
                        // All SIMD exception masks remain set, as required by
                        // production native-region admission.
                        mxcsr: 0x1F80 | prior_status | rc | daz_ftz,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    assert_eq!(cases.len(), 384);
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.kind.is_fp_to_int() && case.instruction.destination == 4)
    );
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.kind.is_fp_to_int() && case.instruction.destination == 5)
    );
    assert!(cases.iter().any(|case| case.instruction.kind.has_merge()
        && case.instruction.destination == case.instruction.merge));
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 5));
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_CONVERT_MEMORY_CHILD_RANGE";

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
        let (code, entry) = lower(&function, case);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));

        let mut context = VectorMemoryContext {
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
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
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
        assert_helper_observation(&context, address, case, "success");
        successes += 1;

        let mut context = VectorMemoryContext {
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
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
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
        assert_helper_observation(&context, address, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX scalar conversion memory cases"
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
        .expect("run isolated native VEX scalar conversion memory differential")
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
        "isolated native VEX scalar conversion memory failure at case {start}/{}: \
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
        eprintln!("skipping native VEX scalar conversion memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_convert_memory_source::semantics::\
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
fn interpreter_matches_intel_rounding_indefinite_merge_zeroing_and_status_contracts() {
    for (kind, source, expected_low) in [
        (
            ConvertKind::F32ToF64,
            [u64::from(1.5f32.to_bits()), 0, 0, 0, 0, 0, 0, 0],
            1.5f64.to_bits(),
        ),
        (
            ConvertKind::F64ToF32,
            [1.5f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
            u64::from(1.5f32.to_bits()),
        ),
    ] {
        let case = ConvertCase {
            kind,
            form: VexForm::C4 {
                w: true,
                encoded_x_clear: true,
            },
            destination: 9,
            merge: 10,
            base: 11,
        };
        let (initial, actual) = interpreter_result(case, source, 0x1F80, 0);
        let destination = actual.zmm[usize::from(case.destination)];
        let merge = initial.zmm[usize::from(case.merge)];
        if kind == ConvertKind::F32ToF64 {
            assert_eq!(destination[0], expected_low, "{case:?}");
        } else {
            assert_eq!(
                destination[0] & u64::from(u32::MAX),
                expected_low,
                "{case:?}"
            );
            assert_eq!(
                destination[0] & !u64::from(u32::MAX),
                merge[0] & !u64::from(u32::MAX),
                "{case:?}"
            );
        }
        assert_eq!(destination[1], merge[1], "{case:?}");
        assert_eq!(destination[2..], [0; 6], "{case:?}");
    }

    for (rc, expected) in [2u64, 1, 2, 1].into_iter().enumerate() {
        let case = ConvertCase {
            kind: ConvertKind::F32ToI32Or64,
            form: VexForm::C5,
            destination: 4,
            merge: 0,
            base: 5,
        };
        let source = [u64::from(1.5f32.to_bits()), 0, 0, 0, 0, 0, 0, 0];
        let (initial, actual) = interpreter_result(case, source, 0x1F80 | ((rc as u32) << 13), rc);
        assert_eq!(actual.gpr[4], expected, "rc={rc}");
        assert_ne!(actual.mxcsr & (1 << 5), 0, "rc={rc}");
        assert_eq!(actual.zmm, initial.zmm, "rc={rc}");
        assert_eq!(actual.rflags, initial.rflags, "rc={rc}");
    }

    for rc in 0u32..4 {
        let case = ConvertCase {
            kind: ConvertKind::F32ToI32Or64Truncate,
            form: VexForm::C5,
            destination: 5,
            merge: 0,
            base: 4,
        };
        let source = [u64::from((-1.5f32).to_bits()), 0, 0, 0, 0, 0, 0, 0];
        let (_, actual) = interpreter_result(case, source, 0x1F80 | (rc << 13), rc as usize);
        assert_eq!(actual.gpr[5], u64::from(u32::MAX), "rc={rc}");
    }

    for source_bits in [0x7F80_0000u32, 0x7FC1_2345, 0x7F81_2345] {
        let case = ConvertCase {
            kind: ConvertKind::F32ToI32Or64,
            form: VexForm::C5,
            destination: 0,
            merge: 0,
            base: 2,
        };
        let source = [u64::from(source_bits), 0, 0, 0, 0, 0, 0, 0];
        let (_, actual) = interpreter_result(case, source, 0x1F80, source_bits as usize);
        assert_eq!(actual.gpr[0], 0x0000_0000_8000_0000);
        assert_ne!(actual.mxcsr & 1, 0);
    }

    let case = ConvertCase {
        kind: ConvertKind::I32Or64ToF32,
        form: VexForm::C4 {
            w: false,
            encoded_x_clear: false,
        },
        destination: 15,
        merge: 15,
        base: 12,
    };
    let source = [16_777_217, 0, 0, 0, 0, 0, 0, 0];
    let (initial, actual) = interpreter_result(case, source, 0x1F80 | (2 << 13), 0);
    assert_eq!(
        actual.zmm[15][0] & u64::from(u32::MAX),
        u64::from(16_777_218f32.to_bits())
    );
    assert_eq!(
        actual.zmm[15][0] & !u64::from(u32::MAX),
        initial.zmm[15][0] & !u64::from(u32::MAX)
    );
    assert_eq!(actual.zmm[15][1], initial.zmm[15][1]);
    assert_eq!(actual.zmm[15][2..], [0; 6]);
    assert_ne!(actual.mxcsr & (1 << 5), 0);
}
