//! Native replay coverage for register-only legacy GFNI instructions.
//! Encoding and finite-field semantics follow Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2A, `GF2P8*`.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexGfniMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xECE0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const KINDS: [X86VexGfniMemoryKind; 3] = [
    X86VexGfniMemoryKind::Multiply,
    X86VexGfniMemoryKind::Affine,
    X86VexGfniMemoryKind::AffineInverse,
];
const OPERANDS: [(u8, u8); 16] = [
    (0, 0),
    (15, 15),
    (8, 8),
    (7, 7),
    (1, 14),
    (14, 1),
    (2, 13),
    (13, 2),
    (3, 12),
    (12, 3),
    (4, 11),
    (11, 4),
    (5, 10),
    (10, 5),
    (6, 9),
    (9, 6),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GfniCase {
    kind: X86VexGfniMemoryKind,
    destination: u8,
    source: u8,
    immediate: u8,
    rex: Option<u8>,
}

impl GfniCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![0x66];
        bytes.extend(self.rex);
        let modrm = 0xC0 | ((self.destination & 7) << 3) | (self.source & 7);
        match self.kind {
            X86VexGfniMemoryKind::Multiply => bytes.extend([0x0F, 0x38, 0xCF, modrm]),
            X86VexGfniMemoryKind::Affine => {
                bytes.extend([0x0F, 0x3A, 0xCE, modrm, self.immediate]);
            }
            X86VexGfniMemoryKind::AffineInverse => {
                bytes.extend([0x0F, 0x3A, 0xCF, modrm, self.immediate]);
            }
        }
        bytes
    }
}

fn case_for(kind: X86VexGfniMemoryKind, ordinal: usize, immediate: u8) -> GfniCase {
    let (destination, source) = OPERANDS[ordinal % OPERANDS.len()];
    let mut rex = (((destination >= 8) as u8) << 2) | ((source >= 8) as u8);
    if ordinal & 1 != 0 {
        rex |= 0x08; // ignored REX.W
    }
    if ordinal & 2 != 0 {
        rex |= 0x02; // ignored REX.X
    }
    let rex = if rex == 0 && ordinal & 4 == 0 {
        None
    } else {
        Some(0x40 | rex)
    };
    GfniCase {
        kind,
        destination,
        source,
        immediate,
        rex,
    }
}

fn cases() -> Vec<GfniCase> {
    let mut cases = Vec::with_capacity(KINDS.len() * 256);
    let mut ordinal = 0usize;
    for kind in KINDS {
        for immediate in u8::MIN..=u8::MAX {
            cases.push(case_for(kind, ordinal, immediate));
            ordinal += 1;
        }
    }
    cases
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
        X86InstructionBytes::new(bytes).expect("legacy GFNI provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_replay_emitted(code: &[u8], bytes: &[u8]) {
    assert!(
        code.windows(bytes.len()).any(|window| window == bytes),
        "missing exact source replay {bytes:02X?}"
    );
}

#[test]
fn feature_requirements_select_gfni_and_avx_ymm16_state() {
    for kind in KINDS {
        let case = case_for(
            kind,
            usize::from(kind != X86VexGfniMemoryKind::Multiply),
            0xA5,
        );
        let bytes = case.bytes();
        let function = function(&bytes, OptLevel::O2, false);
        let excluded = std::collections::HashMap::new();
        assert!(is_native_clobber_safe(&function), "{case:?}");
        assert!(uses_x86_native_vectors_excluding(&function, &excluded));
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert_eq!(
            requirements,
            X86NativeReplayFeatureRequirements {
                any: true,
                all_spans_support_avx_ymm16: true,
                needs_avx: true,
                needs_gfni: true,
                ..X86NativeReplayFeatureRequirements::default()
            },
            "{case:?}"
        );

        #[cfg(target_arch = "x86_64")]
        {
            let supported =
                std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("gfni");
            assert_eq!(requirements.x86_host_supported(), supported, "{case:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                supported,
                "{case:?}"
            );
        }

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{case:?}"
        );
    }
}

#[test]
fn all_9792_kind_rex_register_and_o0_o1_o2_graphs_admit_and_emit_exact_replay() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for kind in KINDS {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let destination = ((modrm >> 3) & 7) | ((rex.unwrap_or(0) & 0x04) << 1);
                let source = (modrm & 7) | ((rex.unwrap_or(0) & 0x01) << 3);
                let case = GfniCase {
                    kind,
                    destination,
                    source,
                    immediate: 0xA5,
                    rex,
                };
                let bytes = case.bytes();
                assert_eq!(
                    bytes[bytes.len() - 1 - usize::from(kind != X86VexGfniMemoryKind::Multiply)]
                        & 0xC0,
                    0xC0
                );
                for level in LEVELS {
                    let function = function(&bytes, level, false);
                    let excluded = std::collections::HashMap::new();
                    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(&function, &excluded),
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
                    assert_replay_emitted(&code, &bytes);
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, KINDS.len() * 17 * 64 * LEVELS.len());
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    let case = GfniCase {
        kind: X86VexGfniMemoryKind::AffineInverse,
        destination: 9,
        source: 10,
        immediate: 0xA5,
        rex: Some(0x45),
    };
    let bytes = case.bytes();
    let baseline = function(&bytes, OptLevel::O0, false);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let metadata = [
        GfniCase {
            destination: 10,
            ..case
        }
        .bytes(),
        GfniCase { source: 11, ..case }.bytes(),
        GfniCase {
            immediate: 0xA4,
            ..case
        }
        .bytes(),
        GfniCase {
            kind: X86VexGfniMemoryKind::Affine,
            ..case
        }
        .bytes(),
        vec![0x66, 0x45, 0x0F, 0x3A, 0xCF, 0x0A, 0xA5],
        {
            let mut reserved = vec![0x67];
            reserved.extend(GfniCase { rex: None, ..case }.bytes());
            reserved
        },
    ];
    for bytes in metadata {
        let mut malformed = baseline.clone();
        malformed
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert!(!is_native_clobber_safe(&malformed), "{bytes:02X?}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GfniState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(ordinal: usize) -> GfniState {
    GfniState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        mm: std::array::from_fn(|register| {
            0xA5A5_5A5A_6996_9669u64.rotate_left(((register * 9 + ordinal) & 63) as u32)
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

fn gf_multiply(a: u8, b: u8) -> u8 {
    let mut product = 0u16;
    for bit in 0..8 {
        if b & (1 << bit) != 0 {
            product ^= u16::from(a) << bit;
        }
    }
    for degree in (8..=14).rev() {
        if product & (1 << degree) != 0 {
            product ^= 0x11B << (degree - 8);
        }
    }
    product as u8
}

fn gf_inverse(value: u8) -> u8 {
    let mut result = 1u8;
    let mut power = value;
    let mut exponent = 254u8;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = gf_multiply(result, power);
        }
        power = gf_multiply(power, power);
        exponent >>= 1;
    }
    result
}

fn vector_byte(vector: &[u64; 8], index: usize) -> u8 {
    (vector[index / 8] >> ((index % 8) * 8)) as u8
}

fn architectural_expected(case: GfniCase, initial: &GfniState) -> GfniState {
    let destination = initial.vectors[usize::from(case.destination)];
    let source = initial.vectors[usize::from(case.source)];
    let mut result = [0u8; 16];
    for (lane, output) in result.iter_mut().enumerate() {
        let input = vector_byte(&destination, lane);
        *output = match case.kind {
            X86VexGfniMemoryKind::Multiply => gf_multiply(input, vector_byte(&source, lane)),
            X86VexGfniMemoryKind::Affine | X86VexGfniMemoryKind::AffineInverse => {
                let input = if case.kind == X86VexGfniMemoryKind::AffineInverse {
                    gf_inverse(input)
                } else {
                    input
                };
                let qword_base = lane & !7;
                let mut transformed = 0u8;
                for bit in 0..8 {
                    let matrix_row = vector_byte(&source, qword_base + 7 - bit);
                    let parity = (matrix_row & input).count_ones() as u8 & 1;
                    transformed |= (parity ^ ((case.immediate >> bit) & 1)) << bit;
                }
                transformed
            }
        };
    }

    let mut expected = initial.clone();
    expected.vectors[usize::from(case.destination)][0] =
        u64::from_le_bytes(result[..8].try_into().unwrap());
    expected.vectors[usize::from(case.destination)][1] =
        u64::from_le_bytes(result[8..].try_into().unwrap());
    expected
}

fn interpret(case: GfniCase, initial: &GfniState, level: OptLevel) -> GfniState {
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
    GfniState {
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
fn interpreter_matches_intel_o0_o1_o2_equations_for_all_inputs_immediates_and_full_state() {
    let cases = cases();
    assert_eq!(cases.len(), KINDS.len() * 256);
    let mut seen_inputs = [[false; 256]; 3];
    let mut seen_sources = [[false; 256]; 3];
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(ordinal);
        for lane in 0..16 {
            let kind = match case.kind {
                X86VexGfniMemoryKind::Multiply => 0,
                X86VexGfniMemoryKind::Affine => 1,
                X86VexGfniMemoryKind::AffineInverse => 2,
            };
            seen_inputs[kind][usize::from(vector_byte(
                &initial.vectors[usize::from(case.destination)],
                lane,
            ))] = true;
            seen_sources[kind][usize::from(vector_byte(
                &initial.vectors[usize::from(case.source)],
                lane,
            ))] = true;
        }
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
    for (kind, seen) in seen_inputs.into_iter().enumerate() {
        assert!(
            seen.into_iter().all(std::convert::identity),
            "kind {kind} input"
        );
    }
    for (kind, seen) in seen_sources.into_iter().enumerate() {
        assert!(
            seen.into_iter().all(std::convert::identity),
            "kind {kind} source"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: GfniCase, initial: &GfniState, level: OptLevel) -> GfniState {
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
    let exec = ExecMem::new(&code).expect("map legacy GFNI replay");
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
    GfniState {
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
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_GFNI_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[GfniCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let initial = initial_state(ordinal);
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
        .expect("run isolated native legacy GFNI differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), KINDS.len() * 256);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }
    eprintln!("executing {} native legacy GFNI cases", cases.len());
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
        "isolated native legacy GFNI failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_all_inputs_immediates_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("gfni") {
        eprintln!("skipping native legacy GFNI differential: host lacks AVX/GFNI");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_gfni_replay::\
         replay_matches_intel_o0_o1_o2_all_inputs_immediates_aliases_and_full_state",
    );
}
