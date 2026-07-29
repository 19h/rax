//! Independent semantic and native-execution differentials.

use super::*;

const DEFINED_FLAG_MASK: u64 = 0x8D5;
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

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

fn initial_vectors(ordinal: usize) -> [[u64; 8]; 32] {
    std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xA55A_6996_F00F_3CC3u64
                .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
        })
    })
}

fn tested_bits(case: PackedBitTestMemoryCase) -> u64 {
    case.operation.tested_bits().unwrap_or(u64::MAX)
}

fn semantic_pair(case: PackedBitTestMemoryCase, ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let mask = tested_bits(case);
    let first_bit = 1u64 << mask.trailing_zeros();
    let remaining = mask & !first_bit;
    let (second_word, second_bit) = if remaining != 0 {
        (0, 1u64 << remaining.trailing_zeros())
    } else {
        (1, first_bit)
    };
    let words = case.width.bytes() as usize / 8;
    let mut first = [0u64; 8];
    let mut second = [0u64; 8];
    match ordinal % 8 {
        // ZF=1, CF=1.
        0 => {}
        // ZF=0, CF=1.
        1 => {
            first[0] = first_bit;
            second[0] = first_bit;
        }
        // ZF=1, CF=0.
        2 => second[0] = first_bit,
        // ZF=0, CF=0.
        3 => {
            first[0] = first_bit;
            second[0] = first_bit;
            second[second_word] |= second_bit;
        }
        // Non-sign payload bits are outside VTESTPS/VTESTPD's domain.
        4 => {
            for word in 0..words {
                first[word] = !mask;
                second[word] = !mask;
            }
        }
        // Make only the upper 128-bit half observable for 256-bit forms.
        5 => {
            let word = if words == 4 { 3 } else { 1 };
            first[word] = first_bit;
            second[word] = first_bit;
        }
        // Alternate contained and outside tested bits across qwords.
        6 => {
            for word in 0..words {
                first[word] = if word & 1 == 0 { mask } else { 0 };
                second[word] = mask;
            }
        }
        7 => {
            for word in 0..words {
                first[word] = mask.rotate_left((word * 7) as u32);
                second[word] = mask.rotate_right((word * 11) as u32);
            }
        }
        _ => unreachable!(),
    }
    (first, second)
}

/// Independent transcription of Intel's whole-vector AND/ANDN reductions and
/// complete defined-status update.
fn architectural_rflags(
    case: PackedBitTestMemoryCase,
    first: [u64; 8],
    second: [u64; 8],
    before: u64,
) -> u64 {
    let mask = tested_bits(case);
    let words = case.width.bytes() as usize / 8;
    let mut intersection = 0u64;
    let mut outside = 0u64;
    for word in 0..words {
        let first = first[word] & mask;
        let second = second[word] & mask;
        intersection |= first & second;
        outside |= second & !first;
    }
    (before & !DEFINED_FLAG_MASK) | u64::from(outside == 0) | (u64::from(intersection == 0) << 6)
}

#[test]
fn interpreter_matches_intel_truth_table_width_masks_and_o0_o2() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut checked = 0usize;
    let mut observed = [[false; 4]; 3];
    for (shape_ordinal, (operation, width, w)) in SHAPES.into_iter().enumerate() {
        for pattern in 0..64usize {
            let first_source = [0, 15, 9, 1][pattern % 4];
            let base = [3, 11, 0, 7][(pattern / 2) % 4];
            let case = PackedBitTestMemoryCase {
                operation,
                width,
                w,
                first_source,
                base,
                clear_ignored_x: pattern & 1 != 0,
            };
            let (first, second) = semantic_pair(case, pattern);
            let before = 0x2 | DEFINED_FLAG_MASK | (1 << 10);
            let expected_flags = architectural_rflags(case, first, second, before);
            let outcome = (usize::from(expected_flags & (1 << 6) != 0) << 1)
                | usize::from(expected_flags & 1 != 0);
            observed[case.operation as usize][outcome] = true;

            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let mut vectors = initial_vectors(shape_ordinal * 64 + pattern);
                vectors[usize::from(first_source)] = first;
                let gprs = std::array::from_fn(|index| {
                    0x1000u64 + (index as u64) * 0x101 + pattern as u64
                });
                let masks = std::array::from_fn(|index| {
                    0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)
                });
                let mxcsr = 0x1F80 | ((pattern as u32) & 0x3F);
                let mut context = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                    x86.gpr = gprs;
                    x86.gpr[usize::from(case.base)] = 0x2000;
                    for (index, value) in vectors.iter().enumerate() {
                        x86.xmm[index][..8].copy_from_slice(value);
                    }
                    x86.k = masks;
                    x86.rflags = before;
                    x86.mxcsr = mxcsr;
                }
                context.flags.materialized = MaterializedFlags::from_rflags(before);
                context.flags.lazy = None;
                let mut flat_memory = FlatMemory::new(0x10000);
                flat_memory.load(
                    0x2000 + DISP as usize,
                    &words_to_bytes(second)[..case.width.bytes() as usize],
                );
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut flat_memory,
                    &function.blocks[0],
                );
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                context.flags.materialize_all();
                assert_eq!(
                    context.flags.materialized.to_rflags(),
                    expected_flags,
                    "{level:?} {case:?}: RFLAGS"
                );
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                let mut expected_gprs = gprs;
                expected_gprs[usize::from(case.base)] = 0x2000;
                assert_eq!(x86.gpr, expected_gprs, "{level:?} {case:?}: GPRs");
                for (register, expected) in vectors.iter().enumerate() {
                    assert_eq!(
                        &x86.xmm[register][..8],
                        expected,
                        "{level:?} {case:?}: vector {register}"
                    );
                }
                assert_eq!(x86.k, masks, "{level:?} {case:?}: masks");
                assert_eq!(x86.mxcsr, mxcsr, "{level:?} {case:?}: MXCSR");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, SHAPES.len() * 64 * DIFFERENTIAL_LEVELS.len());
    assert_eq!(observed, [[true; 4]; 3]);
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
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    use crate::smir::lower::runtime::GuestRegs;

    let state: &mut GuestRegs = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    let mut bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    bytes[..size as usize].copy_from_slice(&words_to_bytes(context.value)[..size as usize]);
    state.vector_scratch = bytes_to_words(bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(
    case: PackedBitTestMemoryCase,
    ordinal: usize,
    first: [u64; 8],
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | DEFINED_FLAG_MASK | (1 << 10),
        ac_flag: (ordinal & 1) as u64,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.zmm = initial_vectors(ordinal);
    registers.zmm[usize::from(case.first_source)] = first;
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: crate::smir::lower::runtime::GuestRegs,
    case: PackedBitTestMemoryCase,
    first: [u64; 8],
    memory: [u64; 8],
) -> crate::smir::lower::runtime::GuestRegs {
    registers.rflags = architectural_rflags(case, first, memory, registers.rflags);
    let mut scratch = [0u8; 64];
    scratch[..case.width.bytes() as usize]
        .copy_from_slice(&words_to_bytes(memory)[..case.width.bytes() as usize]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<PackedBitTestMemoryCase> {
    let operands = [(0, 3), (15, 11), (9, 0), (1, 7)];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for (operation, width, w) in SHAPES {
        for _ in 0..16 {
            let (first_source, base) = operands[ordinal % operands.len()];
            cases.push(PackedBitTestMemoryCase {
                operation,
                width,
                w,
                first_source,
                base,
                clear_ignored_x: ordinal & 1 != 0,
            });
            ordinal += 1;
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PTEST_MEMORY_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[PackedBitTestMemoryCase], range: std::ops::Range<usize>) {
    use crate::smir::lower::runtime::ExecMem;

    assert!(range.start < range.end && range.end <= cases.len());
    let expected_executions = range.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for ordinal in range {
        let case = cases[ordinal];
        let (first, memory) = semantic_pair(case, ordinal);
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, sequence) = lower(&function);
            assert!(
                code.windows(sequence.encoding.register_instruction.as_slice().len())
                    .any(|window| window == sequence.encoding.register_instruction.as_slice()),
                "{level:?} {case:?}"
            );
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, first);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = expected_success(registers, case, first, memory);

            eprintln!("native success case {ordinal}: {level:?} {case:?}");
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, first);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            eprintln!("native fault case {ordinal}: {level:?} {case:?}");
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX packed bit-test memory cases"
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
        .expect("run isolated native VEX packed bit-test memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = semantic_cases();
    assert!(!cases.is_empty());
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
    panic!(
        "isolated native VEX packed bit-test memory failure at case {start}/{}: \
         {case:?}; whole status {}; singleton status {}; singleton stdout: {}; \
         singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_tests_match_model_and_precise_noncommitting_faults() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed bit-test memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_ptest_memory_source::semantics::\
         native_tests_match_model_and_precise_noncommitting_faults",
    );
}
