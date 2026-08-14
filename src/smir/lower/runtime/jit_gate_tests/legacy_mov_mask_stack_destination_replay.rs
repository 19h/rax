//! Native replay coverage for legacy MOVMSKPS/MOVMSKPD targeting guest RSP or
//! RBP.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5050;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskKind {
    Movmskps,
    Movmskpd,
}

impl MaskKind {
    const ALL: [Self; 2] = [Self::Movmskps, Self::Movmskpd];

    fn element_bytes(self) -> usize {
        match self {
            Self::Movmskps => 4,
            Self::Movmskpd => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaskCase {
    kind: MaskKind,
    rex: Option<u8>,
    destination: u8,
    rm: u8,
}

impl MaskCase {
    fn source(self) -> u8 {
        (self.rex.unwrap_or(0) & 1) << 3 | self.rm
    }
}

fn encoding(case: MaskCase) -> Vec<u8> {
    assert!(matches!(case.destination, 4 | 5));
    assert!(case.rm < 8);
    let mut bytes = Vec::with_capacity(5);
    if case.kind == MaskKind::Movmskpd {
        bytes.push(0x66);
    }
    if let Some(rex) = case.rex {
        bytes.push(rex);
    }
    bytes.extend_from_slice(&[0x0F, 0x50, 0xC0 | (case.destination << 3) | case.rm]);
    bytes
}

fn exhaustive_cases() -> Vec<MaskCase> {
    let mut cases = Vec::with_capacity(288);
    for kind in MaskKind::ALL {
        for destination in [4, 5] {
            for rm in 0..8 {
                cases.push(MaskCase {
                    kind,
                    rex: None,
                    destination,
                    rm,
                });
            }
        }
        for rex in [0x40, 0x41, 0x42, 0x43, 0x48, 0x49, 0x4A, 0x4B] {
            for destination in [4, 5] {
                for rm in 0..8 {
                    cases.push(MaskCase {
                        kind,
                        rex: Some(rex),
                        destination,
                        rm,
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), 288);
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

fn expected_replay_bytes(case: MaskCase, bytes: &[u8]) -> Vec<u8> {
    let mut rewritten = bytes.to_vec();
    *rewritten.last_mut().unwrap() &= !0x38;

    let mut expected = vec![0x50, 0x51];
    expected.extend_from_slice(&rewritten);
    expected.extend_from_slice(&[
        0x48,
        0x8B,
        0x4D,
        X86_STATE_PTR_AT_RBP as u8,
        0x48,
        0x89,
        0x41,
        case.destination * 8,
    ]);
    if case.destination == 5 {
        expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
    }
    expected.extend_from_slice(&[0x59, 0x58]);
    expected
}

fn assert_replay_emitted(code: &[u8], case: MaskCase, bytes: &[u8]) {
    let expected = expected_replay_bytes(case, bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?} source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn replay_features_select_avx_ymm16_bridge_without_avx512() {
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
        assert!(!requirements.needs_ssse3, "{case:?}");
        assert!(!requirements.needs_sse41, "{case:?}");
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
fn replay_emits_exact_flag_neutral_state_commits_for_all_864_optimized_cases() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for case in exhaustive_cases() {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
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
            assert_replay_emitted(&code, case, &bytes);
            lowered += 1;
        }
    }
    assert_eq!(lowered, 864);
}

#[test]
fn replay_fails_closed_without_matching_bytes_and_semantic_graph() {
    let case = MaskCase {
        kind: MaskKind::Movmskpd,
        rex: Some(0x4B),
        destination: 5,
        rm: 7,
    };
    let bytes = encoding(case);
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();

    let mut ordinary_bytes = bytes.clone();
    *ordinary_bytes.last_mut().unwrap() &= !0x38;
    let mut ordinary_metadata = base.clone();
    ordinary_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&ordinary_bytes).unwrap(),
    );

    let mut memory_bytes = bytes;
    *memory_bytes.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = base.clone();
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );

    let mut wrong_lanes = base.clone();
    let OpKind::X86MovMask { lanes, .. } = &mut wrong_lanes.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *lanes = 3;

    let mut missing_hint = base;
    missing_hint.blocks[0].ops[0].x86_hint = None;

    for nonmatching in [
        missing,
        ordinary_metadata,
        memory_metadata,
        wrong_lanes,
        missing_hint,
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
struct MaskState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> MaskState {
    MaskState {
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn vector_bytes(vector: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (word, value) in vector.into_iter().enumerate() {
        bytes[word * 8..word * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn architectural_expected(case: MaskCase, initial: &MaskState) -> MaskState {
    let source = vector_bytes(initial.vectors[usize::from(case.source())]);
    let element_bytes = case.kind.element_bytes();
    let lanes = 16 / element_bytes;
    let mut result = 0u64;
    for lane in 0..lanes {
        let sign = source[lane * element_bytes + element_bytes - 1] >> 7;
        result |= u64::from(sign) << lane;
    }

    let mut expected = initial.clone();
    expected.gprs[usize::from(case.destination)] = result;
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
    initial: &MaskState,
    level: crate::smir::optimize::OptLevel,
) -> MaskState {
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
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
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
    MaskState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o1_o2_all_288_encodings_and_full_state() {
    for (ordinal, case) in exhaustive_cases().into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    case: MaskCase,
    initial: &MaskState,
    level: crate::smir::optimize::OptLevel,
) -> MaskState {
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
    assert_replay_emitted(&code, case, bytes);
    let exec = ExecMem::new(&code).expect("map legacy MOVMSK stack-destination replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
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
    MaskState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_MOV_MASK_STACK_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[MaskCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, case, &initial, level),
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
        .expect("run isolated native legacy MOVMSK stack-destination differential")
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
        "isolated native legacy MOVMSK stack-destination failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_all_288_encodings_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy MOVMSK stack differential: host lacks AVX bridge");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_mov_mask_stack_destination_replay::\
         replay_matches_intel_o0_o1_o2_all_288_encodings_and_full_state",
    );
}
