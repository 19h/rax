//! Native replay coverage for AVX VEX packed bit tests.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x170E;
const DEFINED_FLAG_MASK: u64 = 0x8D5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestKind {
    Vptest,
    Vtestps,
    Vtestpd,
}

impl TestKind {
    const ALL: [Self; 3] = [Self::Vptest, Self::Vtestps, Self::Vtestpd];

    fn opcode(self) -> u8 {
        match self {
            Self::Vptest => 0x17,
            Self::Vtestps => 0x0E,
            Self::Vtestpd => 0x0F,
        }
    }

    fn valid_w_values(self) -> &'static [bool] {
        match self {
            Self::Vptest => &[false, true],
            Self::Vtestps | Self::Vtestpd => &[false],
        }
    }

    fn tested_bits_per_u64(self) -> u64 {
        match self {
            Self::Vptest => u64::MAX,
            Self::Vtestps => 0x8000_0000_8000_0000,
            Self::Vtestpd => 0x8000_0000_0000_0000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestCase {
    kind: TestKind,
    w: bool,
    wide: bool,
    ignored_x: bool,
    first: u8,
    second: u8,
}

fn encoding(case: TestCase) -> [u8; 5] {
    assert!(case.first < 16 && case.second < 16);
    assert!(case.kind == TestKind::Vptest || !case.w);
    let mut p0 = 0xE2;
    if case.first >= 8 {
        p0 &= !0x80;
    }
    if case.ignored_x {
        p0 &= !0x40;
    }
    if case.second >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(case.w) << 7) | 0x79 | (u8::from(case.wide) << 2),
        case.kind.opcode(),
        0xC0 | ((case.first & 7) << 3) | (case.second & 7),
    ]
}

fn exhaustive_cases() -> Vec<TestCase> {
    let mut cases = Vec::with_capacity(4_096);
    for kind in TestKind::ALL {
        for &w in kind.valid_w_values() {
            for wide in [false, true] {
                for ignored_x in [false, true] {
                    for first in 0..16 {
                        for second in 0..16 {
                            cases.push(TestCase {
                                kind,
                                w,
                                wide,
                                ignored_x,
                                first,
                                second,
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 4_096);
    cases
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        result.ops.iter().all(|op| op.guest_pc == PC),
        "{bytes:02X?}"
    );

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn assert_replay_emitted(code: &[u8], bytes: &[u8]) {
    let mut expected = bytes.to_vec();
    expected.extend_from_slice(&[0x9C, 0x48, 0x81, 0x24, 0x24, 0x6B, 0xF7, 0xFF, 0xFF, 0x9D]);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "missing exact packed-test replay and defined-flag fixup: \
         source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn replay_features_require_only_avx_and_select_the_ymm16_bridge() {
    for case in exhaustive_cases() {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_fma4, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            "{case:?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx"),
            "{case:?}"
        );
    }
}

#[test]
fn replay_emits_all_8192_exact_o0_o2_instruction_images() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for case in exhaustive_cases() {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            assert_replay_emitted(&code, &bytes);
            lowered += 1;
        }
    }
    assert_eq!(lowered, 8_192);
}

#[test]
fn replay_fails_closed_without_exact_register_ptest_provenance() {
    let case = TestCase {
        kind: TestKind::Vtestps,
        w: false,
        wide: true,
        ignored_x: true,
        first: 15,
        second: 14,
    };
    let bytes = encoding(case);
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();

    let mut memory = bytes;
    memory[4] &= 0x3F;
    let mut memory_metadata = base.clone();
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );

    let mut w1 = bytes;
    w1[2] |= 0x80;
    let mut w1_metadata = base.clone();
    w1_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&w1).unwrap(),
    );

    let mut nonreserved_vvvv = bytes;
    nonreserved_vvvv[2] &= !0x08;
    let mut vvvv_metadata = base.clone();
    vvvv_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&nonreserved_vvvv).unwrap(),
    );

    let mut wrong_map = bytes;
    wrong_map[1] ^= 1;
    let mut map_metadata = base;
    map_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&wrong_map).unwrap(),
    );

    for nonmatching in [
        missing,
        memory_metadata,
        w1_metadata,
        vvvv_metadata,
        map_metadata,
    ] {
        assert!(!is_native_clobber_safe(&nonmatching));
        assert!(
            !x86_native_replay_feature_requirements(
                &nonmatching,
                &std::collections::HashMap::new()
            )
            .any
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
}

fn initial_state(case: TestCase, ordinal: usize) -> TestState {
    let mut state = TestState {
        gprs: std::array::from_fn(|register| {
            0x89AB_CDEF_0123_4567u64.rotate_left((register * 9) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xC33C_F00F_6996_A55Au64
                    .rotate_left(((ordinal * 5 + register * 13 + word * 19) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            u64::MAX,
        ],
        // Set every status bit defined by (V)PTEST and one preserved status
        // bit (DF) so both clearing and non-target preservation are observed.
        rflags: 0x2 | DEFINED_FLAG_MASK | (1 << 10),
        ac_flag: (ordinal & 1) as u64,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    };

    if case.first == case.second {
        return state;
    }

    let mask = case.kind.tested_bits_per_u64();
    let bit_a = 1u64 << mask.trailing_zeros();
    let remaining = mask & !bit_a;
    let (bit_b_word, bit_b) = if remaining != 0 {
        (0, 1u64 << remaining.trailing_zeros())
    } else {
        (1, bit_a)
    };
    let active_words = if case.wide { 4 } else { 2 };
    let mut first = [0u64; 8];
    let mut second = [0u64; 8];
    match ordinal % 5 {
        // ZF=1, CF=1: the second source contributes no tested bit.
        0 => first[0] = bit_a,
        // ZF=1, CF=0: the tested source bit is outside the first operand.
        1 => second[0] = bit_a,
        // ZF=0, CF=1: every tested source bit is contained in the first.
        2 => {
            first[0] = bit_a;
            second[0] = bit_a;
        }
        // ZF=0, CF=0: one source bit is contained and another is outside.
        3 => {
            first[0] = bit_a;
            second[0] = bit_a;
            second[bit_b_word] |= bit_b;
        }
        // Non-sign payload bits must be ignored by VTESTPS/VTESTPD.
        4 => {
            for word in 0..active_words {
                first[word] = !mask;
                second[word] = !mask;
            }
        }
        _ => unreachable!(),
    }
    for word in 0..active_words {
        state.vectors[usize::from(case.first)][word] = first[word];
        state.vectors[usize::from(case.second)][word] = second[word];
    }
    state
}

fn architectural_expected(case: TestCase, initial: &TestState) -> TestState {
    let tested_bits = case.kind.tested_bits_per_u64();
    let words = if case.wide { 4 } else { 2 };
    let mut intersection = 0u64;
    let mut outside = 0u64;
    for word in 0..words {
        let first = initial.vectors[usize::from(case.first)][word] & tested_bits;
        let second = initial.vectors[usize::from(case.second)][word] & tested_bits;
        intersection |= first & second;
        outside |= second & !first;
    }

    let mut expected = initial.clone();
    expected.rflags &= !DEFINED_FLAG_MASK;
    expected.rflags |= u64::from(outside == 0);
    expected.rflags |= u64::from(intersection == 0) << 6;
    expected
}

fn optimized_function(
    bytes: &[u8],
    level: crate::smir::optimize::OptLevel,
    halt: bool,
) -> crate::smir::ir::SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn interpret(
    bytes: &[u8],
    initial: &TestState,
    level: crate::smir::optimize::OptLevel,
) -> TestState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags | (initial.ac_flag << 18);
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized =
        MaterializedFlags::from_rflags(initial.rflags | (initial.ac_flag << 18));
    context.flags.lazy = None;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    context.flags.materialize_all();

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    TestState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !DEFINED_FLAG_MASK)
            | (context.flags.materialized.to_rflags() & DEFINED_FLAG_MASK),
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_all_4096_encodings_and_full_state() {
    let mut observed_flag_outcomes = [[false; 4]; 3];
    for (ordinal, case) in exhaustive_cases().into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        let outcome = (usize::from(expected.rflags & (1 << 6) != 0) << 1)
            | usize::from(expected.rflags & 1 != 0);
        observed_flag_outcomes[case.kind as usize][outcome] = true;
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }
    assert_eq!(observed_flag_outcomes, [[true; 4]; 3]);
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &TestState,
    level: crate::smir::optimize::OptLevel,
) -> TestState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_replay_emitted(&code, bytes);
    let exec = ExecMem::new(&code).expect("map VEX packed-test replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    TestState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PTEST_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[TestCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, &initial, level),
                expected,
                "native {level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX packed-test differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = exhaustive_cases();
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
    let bytes = encoding(case);
    panic!(
        "isolated native VEX packed-test failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_all_4096_encodings_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed-test differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_ptest_replay::\
         replay_matches_intel_o0_o2_all_4096_encodings_and_full_state",
    );
}
