//! Native replay coverage for register-only legacy SSE4.1 `INSERTPS`.
//!
//! Encoding, imm8 controls, absence of SIMD floating-point exceptions, and
//! legacy upper-lane preservation follow Intel SDM Order No. 325383-092US
//! (June 2026), Vol. 2A, pp. 3-461--3-463.

use super::*;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x0B0A_0910;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn encoding(rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, 0x21, modrm, immediate]);
    bytes
}

fn function(bytes: &[u8], level: OptLevel, halt: bool) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(if halt {
        Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        }
    } else {
        Terminator::Return { values: Vec::new() }
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy INSERTPS provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse41_avx_ymm16_state() {
    let bytes = encoding(Some(0x4F), 0xEC, 0xFD);
    let function = function(&bytes, OptLevel::O2, false);
    let excluded = std::collections::HashMap::new();
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(&function, &excluded));
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements {
            any: true,
            all_spans_support_avx_ymm16: true,
            needs_sse41: true,
            needs_avx: true,
            ..X86NativeReplayFeatureRequirements::default()
        }
    );
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(&function, &excluded),
        std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("avx")
    );

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

fn assert_exact_replay_without_upper_clear(code: &[u8], bytes: &[u8]) {
    let positions: Vec<_> = code
        .windows(bytes.len())
        .enumerate()
        .filter_map(|(position, window)| (window == bytes).then_some(position))
        .collect();
    assert_eq!(positions.len(), 1, "source={bytes:02X?}");
    let suffix = &code[positions[0] + bytes.len()..];
    // emit_avx_ymm16_state_backed_upper_clear begins with pushfq, push rax,
    // mov rax,[rbp+state]. Legacy INSERTPS preserves YMM[255:128].
    assert!(
        !suffix.starts_with(&[0x9C, 0x50, 0x48, 0x8B, 0x45]),
        "source={bytes:02X?}"
    );
}

fn assert_admitted_and_emitted(bytes: &[u8], level: OptLevel) {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = function(bytes, level, false);
    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_exact_replay_without_upper_clear(&code, bytes);
}

#[test]
fn all_4032_immediate_rex_register_and_o0_o1_o2_shapes_admit_and_emit_exactly() {
    let mut lowered = 0usize;

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(Some(0x4F), 0xCA, immediate);
        for level in LEVELS {
            assert_admitted_and_emitted(&bytes, level);
            lowered += 1;
        }
    }

    for (rex_index, rex) in [None]
        .into_iter()
        .chain((0x40..=0x4F).map(Some))
        .enumerate()
    {
        for modrm in 0xC0..=0xFF {
            let immediate = (rex_index * 64 + usize::from(modrm)) as u8;
            let bytes = encoding(rex, modrm, immediate);
            for level in LEVELS {
                assert_admitted_and_emitted(&bytes, level);
                lowered += 1;
            }
        }
    }
    assert_eq!(lowered, LEVELS.len() * (256 + 17 * 64));
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_prefixed_provenance() {
    let bytes = encoding(Some(0x45), 0xEC, 0xA5);
    let baseline = function(&bytes, OptLevel::O0, false);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing), "missing provenance");

    for metadata in [
        encoding(Some(0x45), 0xD4, 0xA5),
        encoding(Some(0x45), 0x2C, 0xA5),
        encoding(Some(0x45), 0xEC, 0xA4),
        {
            let mut prefixed = vec![0x67];
            prefixed.extend(encoding(None, 0xEC, 0xA5));
            prefixed
        },
    ] {
        let mut malformed = baseline.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&metadata).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{metadata:02X?}");
        assert!(
            !x86_native_replay_feature_requirements(&malformed, &std::collections::HashMap::new(),)
                .any,
            "{metadata:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
impl NativeCase {
    fn bytes(self) -> Vec<u8> {
        encoding(self.rex, self.modrm, self.immediate)
    }

    fn destination(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        usize::from(((self.modrm >> 3) & 7) | ((rex & 4) << 1))
    }

    fn source(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        usize::from((self.modrm & 7) | ((rex & 1) << 3))
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct InsertpsState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> InsertpsState {
    InsertpsState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
                ^ (case.seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                    ^ (case.seed as u64).wrapping_mul(0x8040_2010_0804_0201)
            })
        }),
        mm: std::array::from_fn(|index| {
            0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + case.seed) as u32)
        }),
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.seed) as u32)
        }),
        rflags: 0x2 | 0x8D5,
        ac_flag: (case.seed & 1) as u64,
        mxcsr: 0x1F80 | (1 << (case.seed % 6)) | (((case.seed / 3) as u32 & 3) << 13),
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][case.seed & 3],
    }
}

#[cfg(target_arch = "x86_64")]
fn i32_lane(vector: &[u64; 8], lane: usize) -> u32 {
    (vector[lane / 2] >> ((lane % 2) * 32)) as u32
}

#[cfg(target_arch = "x86_64")]
fn set_i32_lane(vector: &mut [u64; 8], lane: usize, value: u32) {
    let shift = (lane % 2) * 32;
    let mask = u64::from(u32::MAX) << shift;
    vector[lane / 2] = (vector[lane / 2] & !mask) | (u64::from(value) << shift);
}

#[cfg(target_arch = "x86_64")]
fn expected_state(case: NativeCase, initial: &InsertpsState) -> InsertpsState {
    let mut expected = initial.clone();
    let source_lane = usize::from(case.immediate >> 6);
    let destination_lane = usize::from((case.immediate >> 4) & 3);
    let selected = i32_lane(&initial.vectors[case.source()], source_lane);
    set_i32_lane(
        &mut expected.vectors[case.destination()],
        destination_lane,
        selected,
    );
    for lane in 0..4 {
        if case.immediate & (1 << lane) != 0 {
            set_i32_lane(&mut expected.vectors[case.destination()], lane, 0);
        }
    }
    expected
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: NativeCase, initial: &InsertpsState) -> InsertpsState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let bytes = case.bytes();
    let function = function(&bytes, case.level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        x86.mm = initial.mm;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.x87.tag_word = initial.x87_tag_word as u16;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.materialized.ac = initial.ac_flag != 0;
    context.flags.lazy = None;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    InsertpsState {
        gprs: x86.gpr,
        vectors,
        mm: x86.mm,
        masks: x86.k,
        rflags: x86.rflags,
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
        x87_tag_word: u64::from(x86.x87.tag_word),
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &InsertpsState) -> InsertpsState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case.bytes();
    let function = function(&bytes, case.level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_exact_replay_without_upper_clear(&code, &bytes);
    let exec = ExecMem::new(&code).expect("map legacy INSERTPS replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        x87_tag_word: initial.x87_tag_word,
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
    InsertpsState {
        gprs: registers.gpr,
        vectors,
        mm: registers.mm,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
        x87_tag_word: registers.x87_tag_word,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::with_capacity(13_056);
    let mut ordinal = 0usize;
    for (level_index, level) in LEVELS.into_iter().enumerate() {
        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            for immediate in u8::MIN..=u8::MAX {
                let register_pair =
                    (usize::from(immediate) + 13 * rex_index + 29 * level_index) & 0x3F;
                cases.push(NativeCase {
                    level,
                    rex,
                    modrm: 0xC0 | register_pair as u8,
                    immediate,
                    seed: ordinal,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_INSERTPS_CHILD_RANGE";

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
    for case in &cases[range] {
        let initial = initial_state(*case);
        let expected = expected_state(*case, &initial);
        assert_eq!(
            interpret(*case, &initial),
            expected,
            "{case:?}: interpreter"
        );
        assert_eq!(
            execute_native(*case, &initial),
            expected,
            "{case:?}: native"
        );
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
        .expect("run isolated native legacy INSERTPS differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 13_056);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    // Exact source-byte replay can terminate a child with SIGILL before Rust
    // reports assertion context. Bisect in O(log N) launches and report the
    // exact case while preserving the parent test process.
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
        "isolated native legacy INSERTPS failure at case {start}/{}: \
         {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case.bytes(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn all_13056_native_cases_match_o0_o1_o2_interpretation_equation_and_full_state() {
    if !std::is_x86_feature_detected!("sse4.1") || !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy INSERTPS differential: host lacks SSE4.1 or AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_insertps_replay::\
         all_13056_native_cases_match_o0_o1_o2_interpretation_equation_and_full_state",
    );
}
