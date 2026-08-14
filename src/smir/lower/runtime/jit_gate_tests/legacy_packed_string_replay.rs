//! Native replay coverage for register-only legacy SSE4.2 packed-string
//! comparisons.
//!
//! Encoding, explicit-length width selection, result destinations, status
//! flags, and legacy upper-state preservation follow Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2B, pp. 4-254--4-257 and 4-267--4-271.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, X86PackedStringKind};
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

const PC: u64 = 0x5043_4D50;
const STATUS_FLAGS: u64 = 0x08D5;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn encoding(opcode: u8, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(matches!(opcode, 0x60..=0x63));
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, opcode, modrm, immediate]);
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
        X86InstructionBytes::new(bytes).expect("legacy packed-string provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse42_and_avx_ymm16_state() {
    let bytes = encoding(0x60, Some(0x4F), 0xEC, 0xFF);
    let function = function(&bytes, OptLevel::O2, false);
    let excluded = std::collections::HashMap::new();
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(&function, &excluded));
    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    assert_eq!(
        requirements,
        X86NativeReplayFeatureRequirements {
            any: true,
            all_spans_support_avx_ymm16: true,
            needs_sse42: true,
            needs_avx: true,
            ..X86NativeReplayFeatureRequirements::default()
        }
    );
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    #[cfg(target_arch = "x86_64")]
    {
        let supported =
            std::is_x86_feature_detected!("sse4.2") && std::is_x86_feature_detected!("avx");
        assert_eq!(requirements.x86_host_supported(), supported);
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            supported
        );
    }

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
    // The AVX-YMM16 upper-clear postlude begins with PUSHFQ, PUSH RAX, then a
    // state load. Legacy PCMPxSTRx must preserve YMM[255:128].
    assert!(!suffix.starts_with(&[0x9C, 0x50, 0x48, 0x8B, 0x45]));
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
fn all_13_056_opcode_rex_register_and_o0_o1_o2_graphs_admit_and_emit_exactly() {
    let mut lowered = 0usize;
    let mut ordinal = 0usize;
    for opcode in 0x60..=0x63 {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(opcode, rex, modrm, ordinal as u8);
                for level in LEVELS {
                    assert_admitted_and_emitted(&bytes, level);
                    lowered += 1;
                }
                ordinal += 1;
            }
        }
    }
    assert_eq!(lowered, 4 * 17 * 64 * LEVELS.len());
}

#[test]
fn admission_fails_closed_for_semantic_metadata_memory_and_extra_op_mismatches() {
    let bytes = encoding(0x60, Some(0x4D), 0xCA, 0xA5);
    let baseline = function(&bytes, OptLevel::O0, false);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    for metadata in [
        encoding(0x61, Some(0x4D), 0xCA, 0xA5),
        encoding(0x60, Some(0x4D), 0xCB, 0xA5),
        encoding(0x60, Some(0x4D), 0xCA, 0xA4),
    ] {
        let mut malformed = baseline.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&metadata).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{metadata:02X?}");
    }

    for field in 0..4 {
        let mut malformed = baseline.clone();
        let operation = malformed.blocks[0]
            .ops
            .iter_mut()
            .find(|operation| matches!(operation.kind, OpKind::X86PackedStringCompare { .. }))
            .unwrap();
        let OpKind::X86PackedStringCompare {
            length_width,
            kind,
            imm,
            zero_upper,
            ..
        } = &mut operation.kind
        else {
            unreachable!()
        };
        match field {
            0 => *length_width = crate::smir::ir::types::OpWidth::W32,
            1 => *kind = X86PackedStringKind::ImplicitMask,
            2 => *imm ^= 1,
            3 => *zero_upper = true,
            _ => unreachable!(),
        }
        assert!(!is_native_clobber_safe(&malformed), "field {field}");
    }

    let memory = function(&[0x66, 0x0F, 0x3A, 0x60, 0x00, 0xA5], OptLevel::O2, false);
    assert!(!is_native_clobber_safe(&memory));
    assert!(!is_native_clobber_safe_excluding(
        &memory,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut extra = baseline;
    extra.blocks[0].ops.push(crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(1),
        PC,
        OpKind::Nop,
    ));
    assert!(!is_native_clobber_safe(&extra));
}

#[test]
fn non_memory_address_prefix_is_canonicalized_before_exact_replay() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let source = [0x67, 0x66, 0x0F, 0x3A, 0x63, 0xD1, 0x3A];
    let canonical = [0x66, 0x0F, 0x3A, 0x63, 0xD1, 0x3A];
    let function = function(&source, OptLevel::O2, false);
    assert!(is_native_clobber_safe(&function));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.lower_function(&function).unwrap();
    let code = lowerer.finalize().unwrap();
    assert_exact_replay_without_upper_clear(&code, &canonical);
    assert!(!code.windows(source.len()).any(|window| window == source));
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackedStringState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    opcode: u8,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
impl NativeCase {
    fn bytes(self) -> Vec<u8> {
        encoding(self.opcode, self.rex, self.modrm, self.immediate)
    }

    fn first_source(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        usize::from(((self.modrm >> 3) & 7) | ((rex & 4) << 1))
    }

    fn second_source(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        usize::from((self.modrm & 7) | ((rex & 1) << 3))
    }
}

#[cfg(target_arch = "x86_64")]
fn input_pair(seed: usize) -> ([u8; 16], [u8; 16]) {
    const INPUTS: [([u8; 16], [u8; 16]); 6] = [
        (*b"abc\0ABCDEFGHIJKL", *b"xbycz\0ABCDEFGHIJ"),
        (
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (
            [
                0x80, 0xFF, 0x7F, 0, 0x81, 1, 0xFE, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            ],
            [
                0x80, 0x7F, 0xFF, 1, 0x82, 2, 0xFD, 3, 4, 5, 6, 7, 8, 9, 10, 0,
            ],
        ),
        (
            [1, 0, 2, 0, 0, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0],
            [2, 0, 1, 0, 3, 0, 0, 0, 5, 0, 4, 0, 7, 0, 6, 0],
        ),
        (
            [0xFE, 0xFF, 2, 0, 0, 0, 4, 0, 6, 0, 8, 0, 10, 0, 12, 0],
            [0xFD, 0xFF, 0xFE, 0xFF, 2, 0, 0, 0, 3, 0, 5, 0, 7, 0, 9, 0],
        ),
        ([0xFF; 16], [0; 16]),
    ];
    INPUTS[seed % INPUTS.len()]
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> PackedStringState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (case.seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    let (first, second) = input_pair(case.seed);
    let first_source = case.first_source();
    let second_source = case.second_source();
    vectors[first_source][0] = u64::from_le_bytes(first[..8].try_into().unwrap());
    vectors[first_source][1] = u64::from_le_bytes(first[8..].try_into().unwrap());
    vectors[second_source][0] = u64::from_le_bytes(second[..8].try_into().unwrap());
    vectors[second_source][1] = u64::from_le_bytes(second[8..].try_into().unwrap());

    let mut gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
            ^ (case.seed as u64).wrapping_mul(0x8040_2010_0804_0201)
    });
    const LENGTHS: [(u64, u64); 8] = [
        (3, 5),
        ((-3i64) as u64, 5),
        (0x0000_0001_0000_0003, 0x0000_0001_0000_0005),
        (i64::MIN as u64, i64::MAX as u64),
        (u64::MAX, 0),
        (16, 8),
        (17, 9),
        ((-17i64) as u64, (-9i64) as u64),
    ];
    (gprs[0], gprs[2]) = LENGTHS[case.seed % LENGTHS.len()];

    PackedStringState {
        gprs,
        vectors,
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.seed) as u32)
        }),
        rflags: 0x2 | 0x0CD5,
        mxcsr: 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15),
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: NativeCase, initial: &PackedStringState) -> PackedStringState {
    let function = function(&case.bytes(), case.level, true);
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
    context.flags.materialize_all();
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    PackedStringState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &PackedStringState) -> PackedStringState {
    use crate::smir::lower::SmirLowerer;
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
    let exec = ExecMem::new(&code).expect("map legacy packed-string replay");
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
    PackedStringState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::with_capacity(4 * 17 * 64);
    let mut ordinal = 0usize;
    for opcode in 0x60..=0x63 {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                cases.push(NativeCase {
                    level: LEVELS[ordinal % LEVELS.len()],
                    opcode,
                    rex,
                    modrm,
                    immediate: ordinal as u8,
                    seed: ordinal,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_PACKED_STRING_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(start.parse().unwrap()..end.parse().unwrap())
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated legacy packed-string differential")
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for &case in &cases[range] {
        let initial = initial_state(case);
        assert_eq!(
            execute_native(case, &initial),
            interpret(case, &initial),
            "{case:?} {:02X?}",
            case.bytes()
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 4 * 17 * 64);
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
    panic!(
        "isolated legacy packed-string failure at case {start}/{}: {:?}; \
         whole status {}; singleton status {}; stdout: {}; stderr: {}",
        cases.len(),
        cases[start],
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpretation_for_all_opcode_rex_register_cells_and_full_state() {
    if !std::is_x86_feature_detected!("sse4.2") || !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy packed-string differential: host lacks SSE4.2 or AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_packed_string_replay::\
         replay_matches_interpretation_for_all_opcode_rex_register_cells_and_full_state",
    );
}
