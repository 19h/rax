//! Native replay coverage for register-destination legacy MMX/SSE scalar
//! extracts.
//! Encoding and zero-extension semantics follow Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2A, `EXTRACTPS` and `PEXTRB/D/Q/W`.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, uses_x86_native_mmx_excluding,
    uses_x86_native_vectors_excluding, uses_x86_x87_tag_state_excluding,
    x86_native_mmx_features_supported_excluding, x86_native_mmx_pairs_valid_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE4E0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    ExtractPs,
    PextrB,
    PextrD,
    PextrQ,
    PextrWMap1Mmx,
    PextrWMap1Xmm,
    PextrWMap3,
}

impl Family {
    const ALL: [Self; 7] = [
        Self::ExtractPs,
        Self::PextrB,
        Self::PextrD,
        Self::PextrQ,
        Self::PextrWMap1Mmx,
        Self::PextrWMap1Xmm,
        Self::PextrWMap3,
    ];

    fn map1(self) -> bool {
        matches!(self, Self::PextrWMap1Mmx | Self::PextrWMap1Xmm)
    }

    fn mmx(self) -> bool {
        self == Self::PextrWMap1Mmx
    }

    fn requires_sse41(self) -> bool {
        matches!(
            self,
            Self::ExtractPs | Self::PextrB | Self::PextrD | Self::PextrQ | Self::PextrWMap3
        )
    }

    fn scalar_bytes(self) -> usize {
        match self {
            Self::PextrB => 1,
            Self::PextrWMap1Mmx | Self::PextrWMap1Xmm | Self::PextrWMap3 => 2,
            Self::ExtractPs | Self::PextrD => 4,
            Self::PextrQ => 8,
        }
    }

    fn lane_mask(self) -> u8 {
        match self {
            Self::PextrB => 0x0F,
            Self::PextrWMap1Mmx => 0x03,
            Self::PextrWMap1Xmm | Self::PextrWMap3 => 0x07,
            Self::ExtractPs | Self::PextrD => 0x03,
            Self::PextrQ => 0x01,
        }
    }

    fn rex_images(self) -> Vec<Option<u8>> {
        match self {
            Self::PextrD => [None].into_iter().chain((0x40..=0x47).map(Some)).collect(),
            Self::PextrQ => (0x48..=0x4F).map(Some).collect(),
            _ => [None].into_iter().chain((0x40..=0x4F).map(Some)).collect(),
        }
    }
}

fn encoding(family: Family, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if !family.mmx() {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    if family.map1() {
        bytes.extend([0x0F, 0xC5, modrm, immediate]);
    } else {
        let opcode = match family {
            Family::PextrB => 0x14,
            Family::PextrWMap3 => 0x15,
            Family::PextrD | Family::PextrQ => 0x16,
            Family::ExtractPs => 0x17,
            Family::PextrWMap1Mmx | Family::PextrWMap1Xmm => unreachable!(),
        };
        bytes.extend([0x0F, 0x3A, opcode, modrm, immediate]);
    }
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
        X86InstructionBytes::new(bytes).expect("legacy scalar-extract provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn expected_replay_bytes(bytes: &[u8]) -> Vec<u8> {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let replay = instruction
        .legacy_register_scalar_extract_replay()
        .expect("validated legacy scalar extract");
    if !matches!(replay.destination, 4 | 5) {
        return bytes.to_vec();
    }

    let rewritten = instruction
        .legacy_scalar_extract_with_destination_rax()
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
        replay.destination * 8,
    ]);
    if replay.destination == 5 {
        expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
    }
    expected.extend_from_slice(&[0x59, 0x58]);
    expected
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
fn feature_requirements_keep_mmx_independent_and_select_exact_xmm_features() {
    let excluded = std::collections::HashMap::new();
    for family in Family::ALL {
        let rex = if family == Family::PextrQ {
            Some(0x4F)
        } else {
            Some(0x47)
        };
        let bytes = encoding(family, rex, 0xCA, 0xA5);
        let function = function(&bytes, OptLevel::O2, false);
        assert!(is_native_clobber_safe(&function), "{family:?}");
        assert_eq!(
            uses_x86_native_mmx_excluding(&function, &excluded),
            family.mmx(),
            "{family:?}"
        );
        assert_eq!(
            uses_x86_x87_tag_state_excluding(&function, &excluded),
            family.mmx(),
            "{family:?}"
        );
        assert_eq!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            !family.mmx(),
            "{family:?}"
        );
        assert!(
            x86_native_mmx_pairs_valid_excluding(&function, &excluded),
            "{family:?}"
        );
        assert!(
            x86_native_mmx_features_supported_excluding(&function, &excluded),
            "{family:?}"
        );

        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        let mut expected = X86NativeReplayFeatureRequirements::default();
        if !family.mmx() {
            expected.any = true;
            expected.all_spans_support_avx_ymm16 = true;
            expected.needs_avx = true;
            expected.needs_sse41 = family.requires_sse41();
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));
        }
        assert_eq!(requirements, expected, "{family:?}");

        #[cfg(target_arch = "x86_64")]
        if !family.mmx() {
            let supported = std::is_x86_feature_detected!("avx")
                && (!family.requires_sse41() || std::is_x86_feature_detected!("sse4.1"));
            assert_eq!(requirements.x86_host_supported(), supported, "{family:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                supported,
                "{family:?}"
            );
        }

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{family:?}"
        );
    }
}

#[test]
fn all_19584_o0_o1_o2_rex_register_graphs_admit_and_emit_exact_replay() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for family in Family::ALL {
        for rex in family.rex_images() {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(family, rex, modrm, 0xA5);
                for level in LEVELS {
                    let function = function(&bytes, level, false);
                    let excluded = std::collections::HashMap::new();
                    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                    assert!(
                        x86_native_mmx_pairs_valid_excluding(&function, &excluded),
                        "{level:?} {bytes:02X?}"
                    );
                    assert_eq!(
                        uses_x86_native_mmx_excluding(&function, &excluded),
                        family.mmx(),
                        "{level:?} {bytes:02X?}"
                    );
                    assert_eq!(
                        uses_x86_native_vectors_excluding(&function, &excluded),
                        !family.mmx(),
                        "{level:?} {bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    if !family.mmx() {
                        lowerer.set_avx_ymm16_vector_state(true);
                    }
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
    assert_eq!(lowered, (5 * 17 + 9 + 8) * 64 * LEVELS.len());
}

#[test]
fn admission_rejects_malformed_provenance_and_accepts_canonicalized_non_memory_prefixes() {
    for (index, family) in Family::ALL.into_iter().enumerate() {
        let rex = if family == Family::PextrQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        // Use guest RSP as the destination so this test exercises the exact
        // replay-only frontier. Ordinary MMX PEXTRW destinations already have
        // a complete semantic native lowering and do not require provenance.
        let modrm = if family.map1() { 0xE2 } else { 0xD4 };
        let bytes = encoding(family, rex, modrm, 0xA5);
        let baseline = function(&bytes, OptLevel::O0, false);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{family:?} missing");

        let mismatch = Family::ALL[(index + 1) % Family::ALL.len()];
        let mismatch_rex = if mismatch == Family::PextrQ {
            Some(0x48)
        } else {
            Some(0x40)
        };
        for metadata in [
            encoding(mismatch, mismatch_rex, modrm, 0xA5),
            encoding(family, rex, 0xD3, 0xA5),
            encoding(family, rex, modrm, 0xA6),
            encoding(family, rex, 0x0A, 0xA5),
            {
                let mut duplicate = vec![0x67, 0x67];
                duplicate.extend(&bytes);
                duplicate
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
        }

        let mut prefixed = vec![0x67];
        prefixed.extend(&bytes);
        let canonicalized = function(&prefixed, OptLevel::O0, false);
        assert!(is_native_clobber_safe(&canonicalized), "{family:?}");
        assert_eq!(
            x86_native_replay_feature_requirements(&canonicalized, &Default::default()).any,
            !family.mmx(),
            "{family:?}"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtractCase {
    family: Family,
    destination: u8,
    source: u8,
    immediate: u8,
    rex: Option<u8>,
}

impl ExtractCase {
    fn bytes(self) -> Vec<u8> {
        let modrm = if self.family.map1() {
            0xC0 | ((self.destination & 7) << 3) | (self.source & 7)
        } else {
            0xC0 | ((self.source & 7) << 3) | (self.destination & 7)
        };
        encoding(self.family, self.rex, modrm, self.immediate)
    }
}

fn cases() -> Vec<ExtractCase> {
    let destinations = [4, 5, 0, 15, 8, 7, 1, 14, 2, 13, 3, 12, 6, 11, 9, 10];
    let sources = [15, 0, 8, 7, 1, 14, 2, 13, 3, 12, 4, 11, 5, 10, 6, 9];
    let mut cases = Vec::new();
    for (family_index, family) in Family::ALL.into_iter().enumerate() {
        for immediate in u8::MIN..=u8::MAX {
            let ordinal = family_index * 256 + usize::from(immediate);
            let destination = destinations[ordinal % destinations.len()];
            let source = if family.mmx() {
                sources[(ordinal * 3 + 1) % sources.len()] & 7
            } else {
                sources[(ordinal * 3 + 1) % sources.len()]
            };
            let (reg, rm) = if family.map1() {
                (destination, source)
            } else {
                (source, destination)
            };
            let mut rex = ((reg >= 8) as u8) << 2 | ((rm >= 8) as u8);
            if family.mmx() && ordinal & 2 != 0 {
                rex |= 0x01; // ignored REX.B cannot extend an MM register
            }
            if family == Family::PextrQ
                || (!matches!(family, Family::PextrD | Family::PextrQ) && ordinal & 1 != 0)
            {
                rex |= 0x08;
            }
            if ordinal & 4 != 0 {
                rex |= 0x02; // ignored REX.X
            }
            let rex = if rex == 0 && ordinal & 8 == 0 {
                None
            } else {
                Some(0x40 | rex)
            };
            cases.push(ExtractCase {
                family,
                destination,
                source,
                immediate,
                rex,
            });
        }
    }
    cases
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(case: ExtractCase, ordinal: usize) -> ExtractState {
    ExtractState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        mm: std::array::from_fn(|register| {
            0xA5A5_5A5A_6996_9669u64
                .rotate_left(((register * 9 + ordinal + usize::from(case.immediate)) & 63) as u32)
        }),
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + ordinal) as u32)
        }),
        rflags: 0x2 | 0x0CD5,
        ac_flag: (ordinal & 1) as u64,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal & 3],
        x87_tag_word: [0xFFFFu64, 0xA5A5, 0x0000, 0x6996][ordinal & 3],
    }
}

fn architectural_expected(case: ExtractCase, initial: &ExtractState) -> ExtractState {
    let source = if case.family.mmx() {
        initial.mm[usize::from(case.source)].to_le_bytes().to_vec()
    } else {
        let vector = initial.vectors[usize::from(case.source)];
        [vector[0].to_le_bytes(), vector[1].to_le_bytes()].concat()
    };
    let width = case.family.scalar_bytes();
    let lane = usize::from(case.immediate & case.family.lane_mask());
    let mut scalar = [0u8; 8];
    scalar[..width].copy_from_slice(&source[lane * width..lane * width + width]);

    let mut expected = initial.clone();
    expected.gprs[usize::from(case.destination)] = u64::from_le_bytes(scalar);
    if case.family.mmx() {
        expected.x87_tag_word = 0;
    }
    expected
}

fn interpret(case: ExtractCase, initial: &ExtractState, level: OptLevel) -> ExtractState {
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
    ExtractState {
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

#[test]
fn interpreter_matches_intel_o0_o1_o2_equations_for_all_immediates_and_full_state() {
    let cases = cases();
    assert_eq!(cases.len(), Family::ALL.len() * 256);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
            assert_eq!(
                interpret(case, &initial, level),
                expected,
                "{level:?} {case:?} {:02X?}",
                case.bytes()
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: ExtractCase, initial: &ExtractState, level: OptLevel) -> ExtractState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case.bytes();
    let function = function(&bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    if !case.family.mmx() {
        lowerer.set_avx_ymm16_vector_state(true);
    }
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    assert_replay_emitted(&code, &bytes);
    let exec = ExecMem::new(&code).expect("map legacy scalar-extract replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: if case.family.mmx() {
            0
        } else {
            X86_VECTOR_STATE_YMM16
        },
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        mmx_active: u64::from(case.family.mmx()),
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
    ExtractState {
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
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_SCALAR_EXTRACT_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ExtractCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
            assert_eq!(
                interpret(case, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {:02X?}",
                case.bytes()
            );
            assert_eq!(
                execute_native(case, &initial, level),
                expected,
                "native {level:?} {case:?} {:02X?}",
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
        .expect("run isolated native legacy scalar-extract differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), Family::ALL.len() * 256);
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
        "isolated native legacy scalar-extract failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_all_immediates_rex_stack_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("sse4.1") {
        eprintln!("skipping native legacy scalar-extract differential: host lacks AVX/SSE4.1");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_scalar_extract_replay::\
         replay_matches_intel_o0_o1_o2_all_immediates_rex_stack_and_full_state",
    );
}
