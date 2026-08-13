//! Native replay coverage for register-only legacy MMX/SSE packed floating-
//! point conversions.

use super::*;
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

const PC: u64 = 0xD7D0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Cvtpi2ps,
    Cvttps2pi,
    Cvtps2pi,
    Cvtps2pd,
}

impl Kind {
    const ALL: [Self; 4] = [
        Self::Cvtpi2ps,
        Self::Cvttps2pi,
        Self::Cvtps2pi,
        Self::Cvtps2pd,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::Cvtpi2ps => 0x2A,
            Self::Cvttps2pi => 0x2C,
            Self::Cvtps2pi => 0x2D,
            Self::Cvtps2pd => 0x5A,
        }
    }

    fn touches_mmx(self) -> bool {
        self != Self::Cvtps2pd
    }
}

fn encoding(kind: Kind, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    bytes.extend(rex);
    bytes.extend([0x0F, kind.opcode(), modrm]);
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
        X86InstructionBytes::new(bytes).expect("legacy packed-conversion provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_vector_mxcsr_and_independent_mmx_state() {
    let excluded = std::collections::HashMap::new();
    for kind in Kind::ALL {
        let bytes = encoding(kind, Some(0x4F), 0xCA);
        let function = function(&bytes, OptLevel::O2, false);
        assert!(is_native_clobber_safe(&function), "{kind:?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            "{kind:?}"
        );
        assert_eq!(
            uses_x86_native_mmx_excluding(&function, &excluded),
            kind.touches_mmx(),
            "{kind:?}"
        );
        assert_eq!(
            uses_x86_x87_tag_state_excluding(&function, &excluded),
            kind.touches_mmx(),
            "{kind:?}"
        );
        assert!(
            x86_native_mmx_pairs_valid_excluding(&function, &excluded),
            "{kind:?}"
        );
        assert!(
            x86_native_mmx_features_supported_excluding(&function, &excluded),
            "{kind:?}"
        );

        let expected = X86NativeReplayFeatureRequirements {
            any: true,
            all_spans_support_avx_ymm16: true,
            needs_avx: true,
            ..X86NativeReplayFeatureRequirements::default()
        };
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            expected,
            "{kind:?}"
        );
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            std::is_x86_feature_detected!("avx"),
            "{kind:?}"
        );

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{kind:?}"
        );
    }
}

#[test]
fn all_13056_o0_o1_o2_rex_register_graphs_admit_and_emit_exact_source_bytes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for kind in Kind::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(kind, rex, modrm);
                for level in LEVELS {
                    let function = function(&bytes, level, false);
                    let excluded = std::collections::HashMap::new();
                    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                    assert!(
                        x86_native_mmx_pairs_valid_excluding(&function, &excluded),
                        "{level:?} {bytes:02X?}"
                    );
                    assert!(
                        uses_x86_native_vectors_excluding(&function, &excluded),
                        "{level:?} {bytes:02X?}"
                    );
                    assert_eq!(
                        uses_x86_native_mmx_excluding(&function, &excluded),
                        kind.touches_mmx(),
                        "{level:?} {bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer.set_avx_ymm16_vector_state(true);
                    lowerer
                        .lower_function(&function)
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    assert!(
                        code.windows(bytes.len()).any(|window| window == bytes),
                        "{level:?} {bytes:02X?}"
                    );
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, LEVELS.len() * Kind::ALL.len() * 17 * 64);
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    for (index, kind) in Kind::ALL.into_iter().enumerate() {
        let bytes = encoding(kind, Some(0x45), 0xCA);
        let baseline = function(&bytes, OptLevel::O0, false);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{kind:?} missing");

        let mismatch = encoding(Kind::ALL[(index + 1) % Kind::ALL.len()], Some(0x45), 0xCA);
        for metadata in [
            mismatch,
            encoding(kind, Some(0x45), 0xD2),
            encoding(kind, Some(0x45), 0xC9),
            encoding(kind, Some(0x45), 0x0A),
            {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(kind, None, 0xCA));
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
                "{kind:?} {metadata:02X?}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertCase {
    kind: Kind,
    rex: Option<u8>,
    modrm: u8,
    profile: usize,
}

impl ConvertCase {
    fn destination(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        let reg = (self.modrm >> 3) & 7;
        usize::from(match self.kind {
            Kind::Cvtpi2ps | Kind::Cvtps2pd => reg | ((rex & 0x04) << 1),
            Kind::Cvttps2pi | Kind::Cvtps2pi => reg,
        })
    }

    fn source(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        let rm = self.modrm & 7;
        usize::from(match self.kind {
            Kind::Cvtpi2ps => rm,
            Kind::Cvttps2pi | Kind::Cvtps2pi | Kind::Cvtps2pd => rm | ((rex & 0x01) << 3),
        })
    }

    fn bytes(self) -> Vec<u8> {
        encoding(self.kind, self.rex, self.modrm)
    }
}

fn cases() -> Vec<ConvertCase> {
    let mut cases = Vec::new();
    for (kind_index, kind) in Kind::ALL.into_iter().enumerate() {
        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            for (operand_index, modrm) in [0xC0, 0xCA, 0xFF].into_iter().enumerate() {
                cases.push(ConvertCase {
                    kind,
                    rex,
                    modrm,
                    profile: kind_index * 51 + rex_index * 3 + operand_index,
                });
            }
        }
    }
    cases
}

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

const INTEGER_PAIRS: [(i32, i32); 8] = [
    (0, -1),
    (1, i32::MIN),
    (16_777_217, -16_777_217),
    (i32::MAX, -2_147_483_647),
    (16_777_216, -16_777_216),
    (i32::MAX, i32::MIN),
    (16_777_217, 1),
    (-2_147_483_647, 0),
];

const F32_PAIRS: [(u32, u32); 10] = [
    (0x3FC0_0000, 0xBFC0_0000), // +1.5, -1.5
    (0x4020_0000, 0xC020_0000), // +2.5, -2.5
    (0x0000_0000, 0x8000_0000), // +0, -0
    (0x4EFF_FFFF, 0xCF00_0000), // largest in-range positive, -2^31
    (0x4F00_0000, 0xCF00_0001), // positive and negative overflow
    (0x7F80_0000, 0xFF80_0000), // infinities
    (0x7FC1_2345, 0xFFC1_2345), // quiet NaNs
    (0x7F81_2345, 0xFF81_2345), // signaling NaNs
    (0x0000_0001, 0x8000_0001), // minimum subnormals
    (0x3F80_0000, 0xC040_0000), // +1, -3
];

fn initial_state(case: ConvertCase, ordinal: usize) -> ConvertState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (ordinal as u64).rotate_left((word * 7) as u32)
        })
    });
    let mut mm = std::array::from_fn(|register| {
        0x8000_0001_FFFF_FFFFu64.rotate_left((register * 7 + case.profile * 11) as u32)
            ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
    });
    if case.kind == Kind::Cvtpi2ps {
        for (register, value) in mm.iter_mut().enumerate() {
            let (low, high) = INTEGER_PAIRS[(register + case.profile) % INTEGER_PAIRS.len()];
            *value = u64::from(low as u32) | (u64::from(high as u32) << 32);
        }
    } else {
        for (register, value) in vectors.iter_mut().enumerate() {
            let (low, high) = F32_PAIRS[(register + case.profile) % F32_PAIRS.len()];
            value[0] = u64::from(low) | (u64::from(high) << 32);
        }
    }
    let rc = (case.profile & 3) as u32;
    let daz = u32::from(case.profile & 4 != 0) << 6;
    let ftz = u32::from(case.profile & 8 != 0) << 15;
    let prior_status = [0, 0x04, 0x10, 0x15][(case.profile >> 4) & 3];
    ConvertState {
        gprs: std::array::from_fn(|register| {
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
        mm,
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.profile) as u32)
        }),
        rflags: 0x2 | 0x8D5,
        ac_flag: (ordinal & 1) as u64,
        mxcsr: 0x1F80 | (rc << 13) | daz | ftz | prior_status,
        x87_tag_word: [0xFFFFu64, 0xA5A5, 0x0000, 0x6996][case.profile & 3],
    }
}

fn oracle_i32_to_f32(value: i32, rc: u32) -> (u32, u32) {
    let exact = match value {
        0 => Some(0x0000_0000),
        1 => Some(0x3F80_0000),
        -1 => Some(0xBF80_0000),
        16_777_216 => Some(0x4B80_0000),
        -16_777_216 => Some(0xCB80_0000),
        i32::MIN => Some(0xCF00_0000),
        _ => None,
    };
    if let Some(bits) = exact {
        return (bits, 0);
    }

    let bits = match value {
        16_777_217 => [0x4B80_0000, 0x4B80_0000, 0x4B80_0001, 0x4B80_0000][rc as usize],
        -16_777_217 => [0xCB80_0000, 0xCB80_0001, 0xCB80_0000, 0xCB80_0000][rc as usize],
        i32::MAX => [0x4F00_0000, 0x4EFF_FFFF, 0x4F00_0000, 0x4EFF_FFFF][rc as usize],
        -2_147_483_647 => [0xCF00_0000, 0xCF00_0000, 0xCEFF_FFFF, 0xCEFF_FFFF][rc as usize],
        _ => unreachable!("integer corpus is confined to audited conversion boundaries"),
    };
    (bits, 1 << 5)
}

fn oracle_f32_to_i32(raw: u32, truncate: bool, mxcsr: u32) -> (u32, u32) {
    let sign = raw & 0x8000_0000;
    let exponent = raw >> 23 & 0xFF;
    let fraction = raw & 0x007F_FFFF;
    let denormal = exponent == 0 && fraction != 0;
    let adjusted = if denormal && mxcsr & (1 << 6) != 0 {
        sign
    } else {
        raw
    };
    let value = f64::from(f32::from_bits(adjusted));
    let rc = (mxcsr >> 13) & 3;
    let rounded = if truncate {
        value.trunc()
    } else {
        match rc {
            0 => value.round_ties_even(),
            1 => value.floor(),
            2 => value.ceil(),
            3 => value.trunc(),
            _ => unreachable!(),
        }
    };
    if !value.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return (0x8000_0000, 1);
    }
    let status = if rounded != value { 1 << 5 } else { 0 };
    (rounded as i32 as u32, status)
}

fn oracle_f32_to_f64(raw: u32, mxcsr: u32) -> (u64, u32) {
    let sign32 = raw & 0x8000_0000;
    let exponent = raw >> 23 & 0xFF;
    let fraction = raw & 0x007F_FFFF;
    let denormal = exponent == 0 && fraction != 0;
    if denormal && mxcsr & (1 << 6) != 0 {
        return ((u64::from(sign32)) << 32, 0);
    }
    if exponent == 0xFF && fraction != 0 {
        let signaling = fraction & 0x0040_0000 == 0;
        let payload = (u64::from(fraction) << 29) | if signaling { 1 << 51 } else { 0 };
        return (
            (u64::from(sign32) << 32) | 0x7FF0_0000_0000_0000 | payload,
            u32::from(signaling),
        );
    }
    (
        f64::from(f32::from_bits(raw)).to_bits(),
        if denormal { 1 << 1 } else { 0 },
    )
}

fn architectural_expected(case: ConvertCase, initial: &ConvertState) -> ConvertState {
    let mut expected = initial.clone();
    let source = case.source();
    let destination = case.destination();
    match case.kind {
        Kind::Cvtpi2ps => {
            let input = initial.mm[source];
            let rc = (initial.mxcsr >> 13) & 3;
            let (low, low_status) = oracle_i32_to_f32(input as u32 as i32, rc);
            let (high, high_status) = oracle_i32_to_f32((input >> 32) as u32 as i32, rc);
            expected.vectors[destination][0] = u64::from(low) | (u64::from(high) << 32);
            expected.mxcsr |= low_status | high_status;
        }
        Kind::Cvttps2pi | Kind::Cvtps2pi => {
            let input = initial.vectors[source][0];
            let (low, low_status) =
                oracle_f32_to_i32(input as u32, case.kind == Kind::Cvttps2pi, initial.mxcsr);
            let (high, high_status) = oracle_f32_to_i32(
                (input >> 32) as u32,
                case.kind == Kind::Cvttps2pi,
                initial.mxcsr,
            );
            expected.mm[destination] = u64::from(low) | (u64::from(high) << 32);
            expected.mxcsr |= low_status | high_status;
        }
        Kind::Cvtps2pd => {
            let input = initial.vectors[source][0];
            let (low, low_status) = oracle_f32_to_f64(input as u32, initial.mxcsr);
            let (high, high_status) = oracle_f32_to_f64((input >> 32) as u32, initial.mxcsr);
            expected.vectors[destination][0] = low;
            expected.vectors[destination][1] = high;
            expected.mxcsr |= low_status | high_status;
        }
    }
    if case.kind.touches_mmx() {
        expected.x87_tag_word = 0;
    }
    expected
}

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

#[test]
fn interpreter_matches_primary_spec_oracle_at_o0_o1_o2_for_all_state_edges() {
    let cases = cases();
    assert_eq!(cases.len(), Kind::ALL.len() * 17 * 3);
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
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map legacy packed-conversion replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        mmx_active: u64::from(case.kind.touches_mmx()),
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
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_PACKED_FP_CONVERT_CHILD_RANGE";

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
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
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
        .expect("run isolated native legacy packed-conversion differential")
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
        "isolated native legacy packed-conversion failure at case {start}/{}: \
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
fn replay_matches_primary_spec_o0_o1_o2_for_rex_rounding_exceptions_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy packed-conversion differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_packed_fp_convert_replay::\
         replay_matches_primary_spec_o0_o1_o2_for_rex_rounding_exceptions_and_full_state",
    );
}
