//! Native replay coverage for register-only legacy SSE4.1 `DPPS` and `DPPD`.
//!
//! Encoding, staged MXCSR behavior, implementation-dependent horizontal NaN
//! selection, and legacy upper-lane preservation follow Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2A, pp. 3-277--3-281.

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

const PC: u64 = 0x0B0A_0900;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotKind {
    Dpps,
    Dppd,
}

impl DotKind {
    const ALL: [Self; 2] = [Self::Dpps, Self::Dppd];

    fn opcode(self) -> u8 {
        match self {
            Self::Dpps => 0x40,
            Self::Dppd => 0x41,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn lanes(self) -> usize {
        match self {
            Self::Dpps => 4,
            Self::Dppd => 2,
        }
    }
}

fn encoding(kind: DotKind, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, kind.opcode(), modrm, immediate]);
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
        X86InstructionBytes::new(bytes).expect("legacy dot-product provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse41_avx_ymm16_and_mxcsr_state() {
    for kind in DotKind::ALL {
        let bytes = encoding(kind, Some(0x4F), 0xEC, 0xFD);
        let function = function(&bytes, OptLevel::O2, false);
        let excluded = std::collections::HashMap::new();
        assert!(is_native_clobber_safe(&function), "{kind:?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            "{kind:?}"
        );
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements {
                any: true,
                all_spans_support_avx_ymm16: true,
                needs_sse41: true,
                needs_avx: true,
                ..X86NativeReplayFeatureRequirements::default()
            },
            "{kind:?}"
        );
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("avx"),
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

fn assert_exact_replay_without_upper_clear(code: &[u8], bytes: &[u8]) {
    let positions: Vec<_> = code
        .windows(bytes.len())
        .enumerate()
        .filter_map(|(position, window)| (window == bytes).then_some(position))
        .collect();
    assert_eq!(positions.len(), 1, "source={bytes:02X?}");
    let suffix = &code[positions[0] + bytes.len()..];
    // emit_avx_ymm16_state_backed_upper_clear begins with pushfq, push rax,
    // mov rax,[rbp+state]. Legacy dot products preserve YMM[255:128].
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
fn all_8064_immediate_rex_register_and_o0_o1_o2_shapes_admit_and_emit_exactly() {
    let mut lowered = 0usize;

    for kind in DotKind::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let bytes = encoding(kind, Some(0x4F), 0xCA, immediate);
            for level in LEVELS {
                assert_admitted_and_emitted(&bytes, level);
                lowered += 1;
            }
        }
    }

    for (kind_index, kind) in DotKind::ALL.into_iter().enumerate() {
        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            for modrm in 0xC0..=0xFF {
                let immediate = (kind_index * 17 * 64 + rex_index * 64 + usize::from(modrm)) as u8;
                let bytes = encoding(kind, rex, modrm, immediate);
                for level in LEVELS {
                    assert_admitted_and_emitted(&bytes, level);
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, LEVELS.len() * DotKind::ALL.len() * (256 + 17 * 64));
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    for (index, kind) in DotKind::ALL.into_iter().enumerate() {
        let bytes = encoding(kind, Some(0x45), 0xEC, 0xA5);
        let baseline = function(&bytes, OptLevel::O0, false);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{kind:?} missing");

        for metadata in [
            encoding(
                DotKind::ALL[(index + 1) % DotKind::ALL.len()],
                Some(0x45),
                0xEC,
                0xA5,
            ),
            encoding(kind, Some(0x45), 0xD4, 0xA5),
            encoding(kind, Some(0x45), 0x2C, 0xA5),
            {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(kind, None, 0xEC, 0xA5));
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
            assert!(
                !x86_native_replay_feature_requirements(
                    &malformed,
                    &std::collections::HashMap::new(),
                )
                .any,
                "{kind:?} {metadata:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    kind: DotKind,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
    seed: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
impl NativeCase {
    fn bytes(self) -> Vec<u8> {
        encoding(self.kind, self.rex, self.modrm, self.immediate)
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
struct DotState {
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
fn pack_f32(values: [u32; 4]) -> [u64; 2] {
    [
        u64::from(values[0]) | (u64::from(values[1]) << 32),
        u64::from(values[2]) | (u64::from(values[3]) << 32),
    ]
}

#[cfg(target_arch = "x86_64")]
fn f32_profile(seed: usize) -> ([u64; 2], [u64; 2]) {
    let profile = seed % 10;
    let (first, second) = match profile {
        0 => (
            [0x3FC0_0000, 0xC010_0000, 0x4070_0000, 0xC090_0000],
            [0x4000_0000, 0x4040_0000, 0xBF80_0000, 0x3F00_0000],
        ),
        1 => (
            [0x3F80_0001, 0x3F7F_FFFF, 0x4000_0001, 0xBFFF_FFFF],
            [0x3F80_0001, 0x3F80_0001, 0x3F00_0001, 0x3F00_0001],
        ),
        2 => (
            [0x0000_0001, 0x8000_0001, 0x0080_0000, 0x8080_0000],
            [0x3F00_0000, 0x3F00_0000, 0x3F00_0000, 0x3F00_0000],
        ),
        3 => (
            [0x7F7F_FFFF, 0xFF7F_FFFF, 0x7F00_0000, 0xFF00_0000],
            [0x4000_0000, 0x4000_0000, 0x4080_0000, 0x4080_0000],
        ),
        4 => (
            [0x0080_0000, 0x8080_0000, 0x0100_0000, 0x8100_0000],
            [0x3F00_0000, 0x3F00_0000, 0x3E80_0000, 0x3E80_0000],
        ),
        5 => (
            [0x7F80_0000, 0xFF80_0000, 0x3F80_0000, 0xBF80_0000],
            [0x3F80_0000, 0xBF80_0000, 0x4000_0000, 0x4000_0000],
        ),
        6 => (
            [0x7FC0_0123, 0x3F80_0000, 0x4000_0000, 0x4040_0000],
            [0x3F80_0000; 4],
        ),
        7 => (
            [0x7F80_0123, 0x3F80_0000, 0x4000_0000, 0x4040_0000],
            [0x3F80_0000; 4],
        ),
        8 => (
            [0x0000_0000, 0x8000_0000, 0x3F80_0000, 0xBF80_0000],
            [0x7F80_0000, 0xFF80_0000, 0x0000_0000, 0x8000_0000],
        ),
        _ => (
            [0x3F80_0000, 0xBF80_0000, 0x4000_0000, 0xC000_0000],
            [0x3F80_0000, 0x3F80_0000, 0xBF00_0000, 0x3F00_0000],
        ),
    };
    (pack_f32(first), pack_f32(second))
}

#[cfg(target_arch = "x86_64")]
fn f64_profile(seed: usize) -> ([u64; 2], [u64; 2]) {
    match seed % 10 {
        0 => (
            [0x3FF8_0000_0000_0000, 0xC002_0000_0000_0000],
            [0x4000_0000_0000_0000, 0x4008_0000_0000_0000],
        ),
        1 => (
            [0x3FF0_0000_0000_0001, 0x3FEF_FFFF_FFFF_FFFF],
            [0x3FF0_0000_0000_0001, 0x3FF0_0000_0000_0001],
        ),
        2 => (
            [0x0000_0000_0000_0001, 0x8000_0000_0000_0001],
            [0x3FE0_0000_0000_0000; 2],
        ),
        3 => (
            [0x7FEF_FFFF_FFFF_FFFF, 0xFFEF_FFFF_FFFF_FFFF],
            [0x4000_0000_0000_0000; 2],
        ),
        4 => (
            [0x0010_0000_0000_0000, 0x8010_0000_0000_0000],
            [0x3FE0_0000_0000_0000; 2],
        ),
        5 => (
            [0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000],
            [0x3FF0_0000_0000_0000, 0xBFF0_0000_0000_0000],
        ),
        6 => (
            [0x7FF8_0000_0000_0123, 0x3FF0_0000_0000_0000],
            [0x3FF0_0000_0000_0000; 2],
        ),
        7 => (
            [0x7FF0_0000_0000_0123, 0x3FF0_0000_0000_0000],
            [0x3FF0_0000_0000_0000; 2],
        ),
        8 => (
            [0x0000_0000_0000_0000, 0x8000_0000_0000_0000],
            [0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000],
        ),
        _ => (
            [0x3FF0_0000_0000_0000, 0xBFF0_0000_0000_0000],
            [0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000],
        ),
    }
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> DotState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (case.seed as u64).wrapping_mul(0x8040_2010_0804_0201)
        })
    });
    let (first, second) = match case.kind {
        DotKind::Dpps => f32_profile(case.seed),
        DotKind::Dppd => f64_profile(case.seed),
    };
    vectors[case.destination()][..2].copy_from_slice(&first);
    if case.source() != case.destination() {
        vectors[case.source()][..2].copy_from_slice(&second);
    }

    DotState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
                ^ (case.seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
        mm: std::array::from_fn(|index| {
            0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + case.seed) as u32)
        }),
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.seed) as u32)
        }),
        rflags: 0x2 | 0x8D5,
        ac_flag: (case.seed & 1) as u64,
        mxcsr: case.mxcsr,
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][case.seed & 3],
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: NativeCase, initial: &DotState) -> DotState {
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
    DotState {
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
fn execute_native(case: NativeCase, initial: &DotState) -> DotState {
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
    let exec = ExecMem::new(&code).expect("map legacy dot-product replay");
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
    DotState {
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
fn lane_bits(state: &DotState, case: NativeCase, lane: usize) -> u64 {
    let words = state.vectors[case.destination()];
    match case.kind {
        DotKind::Dpps => (words[lane / 2] >> ((lane % 2) * 32)) & u64::from(u32::MAX),
        DotKind::Dppd => words[lane],
    }
}

#[cfg(target_arch = "x86_64")]
fn is_quiet_nan(kind: DotKind, bits: u64) -> bool {
    match kind {
        DotKind::Dpps => bits & 0x7FC0_0000 == 0x7FC0_0000,
        DotKind::Dppd => bits & 0x7FF8_0000_0000_0000 == 0x7FF8_0000_0000_0000,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_spec_equivalent(case: NativeCase, native: &DotState, interpreted: &DotState) {
    assert_eq!(native.gprs, interpreted.gprs, "{case:?}: GPRs");
    assert_eq!(native.mm, interpreted.mm, "{case:?}: MMX");
    assert_eq!(native.masks, interpreted.masks, "{case:?}: opmask");
    assert_eq!(native.rflags, interpreted.rflags, "{case:?}: RFLAGS");
    assert_eq!(native.ac_flag, interpreted.ac_flag, "{case:?}: AC");
    assert_eq!(native.mxcsr, interpreted.mxcsr, "{case:?}: MXCSR");
    assert_eq!(
        native.x87_tag_word, interpreted.x87_tag_word,
        "{case:?}: x87 tag"
    );

    let destination = case.destination();
    for register in 0..32 {
        for word in 0..8 {
            if register == destination && word < 2 {
                continue;
            }
            assert_eq!(
                native.vectors[register][word], interpreted.vectors[register][word],
                "{case:?}: vector {register} word {word}"
            );
        }
    }
    for lane in 0..case.kind.lanes() {
        let native_bits = lane_bits(native, case, lane);
        let interpreted_bits = lane_bits(interpreted, case, lane);
        if native_bits == interpreted_bits {
            continue;
        }
        assert_ne!(
            case.immediate & (1 << lane),
            0,
            "{case:?}: zero destination mask differed at lane {lane}"
        );
        assert!(
            is_quiet_nan(case.kind, native_bits) && is_quiet_nan(case.kind, interpreted_bits),
            "{case:?}: non-NaN mismatch at lane {lane}: native={native_bits:#x} \
             interpreter={interpreted_bits:#x}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::with_capacity(26_112);
    let mut ordinal = 0usize;
    for level in LEVELS {
        for (kind_index, kind) in DotKind::ALL.into_iter().enumerate() {
            for (rex_index, rex) in [None]
                .into_iter()
                .chain((0x40..=0x4F).map(Some))
                .enumerate()
            {
                for immediate in u8::MIN..=u8::MAX {
                    let register_pair =
                        (usize::from(immediate) + 13 * rex_index + 29 * kind_index) & 0x3F;
                    let prior_status = 1 << (ordinal % 6);
                    let rc = ((ordinal / 3) & 3) as u32;
                    let daz_ftz = if ordinal & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    cases.push(NativeCase {
                        level,
                        kind,
                        rex,
                        modrm: 0xC0 | register_pair as u8,
                        immediate,
                        seed: ordinal,
                        mxcsr: 0x1F80 | prior_status | (rc << 13) | daz_ftz,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_DOT_PRODUCT_CHILD_RANGE";

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
        assert_spec_equivalent(
            *case,
            &execute_native(*case, &initial),
            &interpret(*case, &initial),
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
        .expect("run isolated native legacy dot-product differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 26_112);
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
        "isolated native legacy dot-product failure at case {start}/{}: \
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
fn all_26112_native_cases_match_o0_o1_o2_interpretation_and_preserve_full_state() {
    if !std::is_x86_feature_detected!("sse4.1") || !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy dot-product differential: host lacks SSE4.1 or AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_dot_product_replay::\
         all_26112_native_cases_match_o0_o1_o2_interpretation_and_preserve_full_state",
    );
}
