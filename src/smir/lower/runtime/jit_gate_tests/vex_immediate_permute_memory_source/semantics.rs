//! Independent semantic and native-execution differentials.

use super::*;

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

fn architectural_source_lane(case: ImmediatePermuteMemoryCase, lane: usize) -> usize {
    match case.operation {
        ImmediatePermute::PermilPs => {
            lane / 4 * 4 + (usize::from(case.immediate) >> ((lane % 4) * 2) & 3)
        }
        ImmediatePermute::PermilPd => lane / 2 * 2 + (usize::from(case.immediate) >> lane & 1),
        ImmediatePermute::PermQ | ImmediatePermute::PermPd => {
            usize::from(case.immediate) >> (lane * 2) & 3
        }
    }
}

/// Independent transcription of Intel immediate lane selection and VEX
/// upper-state clearing.
fn architectural_destination(case: ImmediatePermuteMemoryCase, memory: [u64; 8]) -> [u64; 8] {
    let source = words_to_bytes(memory);
    let mut result = [0u8; 64];
    let element_bytes = match case.operation {
        ImmediatePermute::PermilPs => 4,
        ImmediatePermute::PermilPd | ImmediatePermute::PermQ | ImmediatePermute::PermPd => 8,
    };
    let lanes = case.width.bytes() as usize / element_bytes;
    for lane in 0..lanes {
        let source_lane = architectural_source_lane(case, lane);
        result[lane * element_bytes..(lane + 1) * element_bytes].copy_from_slice(
            &source[source_lane * element_bytes..(source_lane + 1) * element_bytes],
        );
    }
    bytes_to_words(result)
}

#[test]
fn interpreter_matches_intel_all_256_immediates_six_shapes_and_o0_o2() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut checked = 0usize;
    for (shape_ordinal, (operation, width)) in SHAPES.into_iter().enumerate() {
        for immediate in u8::MIN..=u8::MAX {
            let destination = if immediate & 1 == 0 { 1 } else { 9 };
            let base = if immediate & 2 == 0 { 3 } else { 11 };
            let case = ImmediatePermuteMemoryCase {
                operation,
                width,
                destination,
                base,
                immediate,
                clear_ignored_x: immediate & 0x80 != 0,
            };
            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let initial = initial_vectors(shape_ordinal * 256 + usize::from(immediate));
                let memory = initial[(shape_ordinal + 17) % initial.len()];
                let expected = architectural_destination(case, memory);
                let gprs = std::array::from_fn(|index| {
                    0x1000u64 + (index as u64) * 0x101 + u64::from(immediate)
                });
                let rflags = 0x2 | (u64::from(immediate) & 0x8D5);
                let masks = std::array::from_fn(|index| {
                    0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)
                });
                let mxcsr = 0x1F80 | u32::from(immediate & 0x3F);
                let mut context = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                    x86.gpr = gprs;
                    x86.gpr[usize::from(case.base)] = 0x2000;
                    for (index, value) in initial.iter().enumerate() {
                        x86.xmm[index][..8].copy_from_slice(value);
                    }
                    x86.k = masks;
                    x86.rflags = rflags;
                    x86.mxcsr = mxcsr;
                }
                context.flags.materialized = MaterializedFlags::from_rflags(rflags);
                context.flags.lazy = None;
                let mut flat_memory = FlatMemory::new(0x10000);
                flat_memory.load(
                    0x2000 + DISP as usize,
                    &words_to_bytes(memory)[..case.width.bytes() as usize],
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
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                let mut expected_gprs = gprs;
                expected_gprs[usize::from(case.base)] = 0x2000;
                assert_eq!(x86.gpr, expected_gprs, "{level:?} {case:?}: GPRs");
                assert_eq!(
                    &x86.xmm[usize::from(destination)][..8],
                    &expected,
                    "{level:?} {case:?}: destination"
                );
                for register in 0..32 {
                    if register != usize::from(destination) {
                        assert_eq!(
                            &x86.xmm[register][..8],
                            &initial[register],
                            "{level:?} {case:?}: vector {register}"
                        );
                    }
                }
                assert_eq!(x86.k, masks, "{level:?} {case:?}: masks");
                assert_eq!(x86.rflags, rflags, "{level:?} {case:?}: RFLAGS");
                assert_eq!(x86.mxcsr, mxcsr, "{level:?} {case:?}: MXCSR");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 6 * 256 * DIFFERENTIAL_LEVELS.len());
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
    case: ImmediatePermuteMemoryCase,
    ordinal: usize,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

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
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.zmm = initial_vectors(ordinal);
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: crate::smir::lower::runtime::GuestRegs,
    case: ImmediatePermuteMemoryCase,
    memory: [u64; 8],
) -> crate::smir::lower::runtime::GuestRegs {
    registers.zmm[usize::from(case.destination)] = architectural_destination(case, memory);
    let mut scratch = [0u8; 64];
    scratch[..case.width.bytes() as usize]
        .copy_from_slice(&words_to_bytes(memory)[..case.width.bytes() as usize]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<ImmediatePermuteMemoryCase> {
    let immediates = [
        0x00, 0x01, 0x02, 0x03, 0x07, 0x0F, 0x10, 0x1B, 0x40, 0x4E, 0x80, 0xA5, 0xC3, 0xE4, 0xF0,
        0xFF,
    ];
    let operands = [(0, 0), (15, 11), (9, 3), (1, 0)];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for (operation, width) in SHAPES {
        for immediate in immediates {
            let (destination, base) = operands[ordinal % operands.len()];
            cases.push(ImmediatePermuteMemoryCase {
                operation,
                width,
                destination,
                base,
                immediate,
                clear_ignored_x: ordinal & 1 != 0,
            });
            ordinal += 1;
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_IMMEDIATE_PERMUTE_MEMORY_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ImmediatePermuteMemoryCase], range: std::ops::Range<usize>) {
    use crate::smir::lower::runtime::ExecMem;

    assert!(range.start < range.end && range.end <= cases.len());
    let expected_executions = range.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for ordinal in range {
        let case = cases[ordinal];
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
            let memory = initial_vectors(ordinal ^ 0x5A)[17];

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = expected_success(registers, case, memory);

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
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
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
        "executed {successes} successful and {faults} faulting native VEX immediate-permute memory cases"
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
        .expect("run isolated native VEX immediate-permute memory differential")
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
        "isolated native VEX immediate-permute memory failure at case {start}/{}: \
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
fn native_permutations_match_model_and_precise_noncommitting_faults() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping native VEX immediate-permute memory differential: host lacks AVX2");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_immediate_permute_memory_source::semantics::\
         native_permutations_match_model_and_precise_noncommitting_faults",
    );
}
