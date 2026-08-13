//! Native replay coverage for register-only legacy SSE/SSE2 scalar floating-
//! point conversions.
//! Encoding, rounding, destination preservation, and SIMD exception behavior
//! follow Intel SDM Order No. 325383-092US (June 2026), Vol. 2A,
//! `CVTSD2SI` through `CVTSS2SI` (pp. 3-232--3-243) and `CVTTSD2SI` through
//! `CVTTSS2SI` (pp. 3-253--3-256).

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

const PC: u64 = 0xD9D0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    CvtSi2Ss,
    CvtSi2Sd,
    CvtSs2Si,
    CvtSd2Si,
    CvttSs2Si,
    CvttSd2Si,
    CvtSs2Sd,
    CvtSd2Ss,
}

impl Family {
    const ALL: [Self; 8] = [
        Self::CvtSi2Ss,
        Self::CvtSi2Sd,
        Self::CvtSs2Si,
        Self::CvtSd2Si,
        Self::CvttSs2Si,
        Self::CvttSd2Si,
        Self::CvtSs2Sd,
        Self::CvtSd2Ss,
    ];

    fn prefix(self) -> u8 {
        match self {
            Self::CvtSi2Ss | Self::CvtSs2Si | Self::CvttSs2Si | Self::CvtSs2Sd => 0xF3,
            Self::CvtSi2Sd | Self::CvtSd2Si | Self::CvttSd2Si | Self::CvtSd2Ss => 0xF2,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::CvtSi2Ss | Self::CvtSi2Sd => 0x2A,
            Self::CvttSs2Si | Self::CvttSd2Si => 0x2C,
            Self::CvtSs2Si | Self::CvtSd2Si => 0x2D,
            Self::CvtSs2Sd | Self::CvtSd2Ss => 0x5A,
        }
    }

    fn int_to_fp(self) -> bool {
        matches!(self, Self::CvtSi2Ss | Self::CvtSi2Sd)
    }

    fn fp_to_int(self) -> bool {
        matches!(
            self,
            Self::CvtSs2Si | Self::CvtSd2Si | Self::CvttSs2Si | Self::CvttSd2Si
        )
    }

    #[cfg(target_arch = "x86_64")]
    fn source_is_f32(self) -> bool {
        matches!(self, Self::CvtSs2Si | Self::CvttSs2Si | Self::CvtSs2Sd)
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![family.prefix()];
    bytes.extend(rex);
    bytes.extend([0x0F, family.opcode(), modrm]);
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
        X86InstructionBytes::new(bytes).expect("legacy scalar-conversion provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn expected_replay_bytes(bytes: &[u8]) -> Vec<u8> {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let replay = instruction
        .legacy_register_scalar_fp_convert_replay()
        .expect("validated legacy scalar conversion");
    if let Some(destination @ (4 | 5)) = replay.gpr_destination() {
        let rewritten = instruction
            .legacy_scalar_fp_to_int_with_destination_rax()
            .expect("stack destination must rewrite to RAX");
        let mut expected = vec![0x50, 0x51];
        expected.extend_from_slice(rewritten.as_slice());
        expected.extend_from_slice(&[
            0x48,
            0x8B,
            0x4D,
            X86_STATE_PTR_AT_RBP as u8,
            0x48,
            0x89,
            0x41,
            destination * 8,
        ]);
        if destination == 5 {
            expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
        }
        expected.extend_from_slice(&[0x59, 0x58]);
        return expected;
    }
    if let Some(source @ (4 | 5)) = replay.gpr_source() {
        let rewritten = instruction
            .legacy_scalar_int_to_fp_with_source_rax()
            .expect("stack source must rewrite to RAX");
        let mut expected = vec![
            0x50,
            0x48,
            0x8B,
            0x45,
            X86_STATE_PTR_AT_RBP as u8,
            0x48,
            0x8B,
            0x40,
            source * 8,
        ];
        expected.extend_from_slice(rewritten.as_slice());
        expected.push(0x58);
        return expected;
    }
    bytes.to_vec()
}

fn assert_replay_emitted(code: &[u8], bytes: &[u8]) {
    let expected = expected_replay_bytes(bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn feature_requirements_select_exactly_avx_ymm16_and_mxcsr_state() {
    for family in Family::ALL {
        let bytes = encoding(family, Some(0x4F), 0xEC);
        let function = function(&bytes, OptLevel::O2, false);
        let excluded = std::collections::HashMap::new();
        assert!(is_native_clobber_safe(&function), "{family:?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            "{family:?}"
        );
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements {
                any: true,
                all_spans_support_avx_ymm16: true,
                needs_avx: true,
                ..X86NativeReplayFeatureRequirements::default()
            },
            "{family:?}"
        );
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            std::is_x86_feature_detected!("avx"),
            "{family:?}"
        );

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{family:?}"
        );
    }
}

#[test]
fn all_26112_o0_o1_o2_rex_register_graphs_admit_and_emit_exact_replay() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for family in Family::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm);
                for level in LEVELS {
                    let function = function(&bytes, level, false);
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
                    assert_replay_emitted(&code, &bytes);
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, LEVELS.len() * Family::ALL.len() * 17 * 64);
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    for (index, family) in Family::ALL.into_iter().enumerate() {
        let bytes = encoding(family, Some(0x45), 0xEC);
        let baseline = function(&bytes, OptLevel::O0, false);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{family:?} missing");

        for metadata in [
            encoding(
                Family::ALL[(index + 1) % Family::ALL.len()],
                Some(0x45),
                0xEC,
            ),
            encoding(family, Some(0x45), 0xD4),
            encoding(family, Some(0x45), 0x2C),
            {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(family, None, 0xEC));
                reserved
            },
        ] {
            let mut malformed = baseline.clone();
            malformed.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&metadata).unwrap(),
            );
            assert!(
                !is_native_clobber_safe(&malformed),
                "{family:?} {metadata:02X?}"
            );
            assert!(
                !x86_native_replay_feature_requirements(
                    &malformed,
                    &std::collections::HashMap::new(),
                )
                .any,
                "{family:?} {metadata:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertCase {
    family: Family,
    rex: Option<u8>,
    modrm: u8,
    profile: usize,
}

#[cfg(target_arch = "x86_64")]
impl ConvertCase {
    fn bytes(self) -> Vec<u8> {
        encoding(self.family, self.rex, self.modrm)
    }
}

#[cfg(target_arch = "x86_64")]
fn cases() -> Vec<ConvertCase> {
    let mut cases = Vec::new();
    for (family_index, family) in Family::ALL.into_iter().enumerate() {
        let modrms: &[u8] = if family.int_to_fp() {
            &[0xC4, 0xC5, 0xCA, 0xFF]
        } else if family.fp_to_int() {
            &[0xE0, 0xE8, 0xCA, 0xFF]
        } else {
            &[0xC0, 0xCA, 0xFF]
        };
        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            for (operand_index, &modrm) in modrms.iter().enumerate() {
                cases.push(ConvertCase {
                    family,
                    rex,
                    modrm,
                    profile: family_index * 68 + rex_index * 4 + operand_index,
                });
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConvertState {
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
const INTEGER_PATTERNS: [u64; 16] = [
    0,
    1,
    u64::MAX,
    i64::MAX as u64,
    i64::MIN as u64,
    0x0000_0000_7FFF_FFFF,
    0xFFFF_FFFF_8000_0000,
    (1u64 << 24) - 1,
    1u64 << 24,
    (1u64 << 24) + 1,
    (1u64 << 53) - 1,
    1u64 << 53,
    (1u64 << 53) + 1,
    (-(1i64 << 24) - 1) as u64,
    (-(1i64 << 53) - 1) as u64,
    0x89AB_CDEF_0123_4567,
];

#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u32; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3F00_0000,
    0xBF00_0000,
    0x3FC0_0000,
    0xBFC0_0000,
    0x4EFF_FFFF,
    0x4F00_0000,
    0xCF00_0000,
    0x5F00_0000,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0x7F81_2345,
    0x0000_0001,
    0x007F_FFFF,
];

#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 20] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FE0_0000_0000_0000,
    0xBFE0_0000_0000_0000,
    0x3FF8_0000_0000_0000,
    0xBFF8_0000_0000_0000,
    0x41DF_FFFF_FFC0_0000,
    0x41E0_0000_0000_0000,
    0xC1E0_0000_0000_0000,
    0x43DF_FFFF_FFFF_FFFF,
    0x43E0_0000_0000_0000,
    0xC3E0_0000_0000_0000,
    0x43EF_FFFF_FFFF_FFFF,
    0x43F0_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_1234_5678_9ABC,
    0x7FF0_1234_5678_9ABC,
    0x0000_0000_0000_0001,
];

#[cfg(target_arch = "x86_64")]
fn initial_state(case: ConvertCase, ordinal: usize) -> ConvertState {
    let mut gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
            ^ (ordinal as u64).wrapping_mul(0x8040_2010_0804_0201)
    });
    for (register, value) in gprs.iter_mut().enumerate().take(16) {
        *value = INTEGER_PATTERNS[(register + case.profile) % INTEGER_PATTERNS.len()];
    }
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    for (register, value) in vectors.iter_mut().enumerate().take(16) {
        if case.family.source_is_f32() {
            value[0] = (value[0] & !u64::from(u32::MAX))
                | u64::from(F32_PATTERNS[(register + case.profile) % F32_PATTERNS.len()]);
        } else {
            value[0] = F64_PATTERNS[(register + case.profile) % F64_PATTERNS.len()];
        }
    }
    let rc = (case.profile & 3) as u32;
    let daz = u32::from(case.profile & 4 != 0) << 6;
    let ftz = u32::from(case.profile & 8 != 0) << 15;
    let prior_status = [0, 0x04, 0x10, 0x15][(case.profile >> 4) & 3];
    ConvertState {
        gprs,
        vectors,
        mm: std::array::from_fn(|index| {
            0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + case.profile) as u32)
        }),
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.profile) as u32)
        }),
        rflags: 0x2 | 0x8D5,
        ac_flag: (ordinal & 1) as u64,
        mxcsr: 0x1F80 | (rc << 13) | daz | ftz | prior_status,
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][case.profile & 3],
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: ConvertCase, initial: &ConvertState, level: OptLevel) -> ConvertState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let bytes = case.bytes();
    let function = function(&bytes, level, true);
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
    ConvertState {
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
fn execute_native(case: ConvertCase, initial: &ConvertState, level: OptLevel) -> ConvertState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case.bytes();
    let function = function(&bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    assert_replay_emitted(&code, &bytes);
    let exec = ExecMem::new(&code).expect("map legacy scalar-conversion replay");
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
    ConvertState {
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
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_SCALAR_FP_CONVERT_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ConvertCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let initial = initial_state(case, ordinal);
        for level in LEVELS {
            assert_eq!(
                execute_native(case, &initial, level),
                interpret(case, &initial, level),
                "native/interpreter {level:?} {case:?} {:02X?}",
                case.bytes()
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
        .expect("run isolated native legacy scalar-conversion differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
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
        "isolated native legacy scalar-conversion failure at case {start}/{}: \
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
fn replay_matches_interpretation_o0_o1_o2_for_rex_stack_rounding_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy scalar-conversion differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_scalar_fp_convert_replay::\
         replay_matches_interpretation_o0_o1_o2_for_rex_stack_rounding_and_full_state",
    );
}
