//! Native replay coverage for register-only legacy MMX/SSE widening
//! doubleword multiply.

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

const PC: u64 = 0xD5D0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    MmxUnsigned,
    XmmUnsigned,
    XmmSigned,
}

const SHAPES: [Shape; 3] = [Shape::MmxUnsigned, Shape::XmmUnsigned, Shape::XmmSigned];

fn encoding(shape: Shape, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if shape != Shape::MmxUnsigned {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    match shape {
        Shape::MmxUnsigned | Shape::XmmUnsigned => bytes.extend([0x0F, 0xF4, modrm]),
        Shape::XmmSigned => bytes.extend([0x0F, 0x38, 0x28, modrm]),
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
        X86InstructionBytes::new(bytes).expect("legacy widening-multiply provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_keep_mmx_independent_and_select_exact_xmm_features() {
    let excluded = std::collections::HashMap::new();
    for shape in SHAPES {
        let bytes = encoding(shape, Some(0x4F), 0xCA);
        let function = function(&bytes, OptLevel::O2, false);
        assert!(is_native_clobber_safe(&function), "{shape:?}");
        assert_eq!(
            uses_x86_native_mmx_excluding(&function, &excluded),
            shape == Shape::MmxUnsigned,
            "{shape:?}"
        );
        assert_eq!(
            uses_x86_x87_tag_state_excluding(&function, &excluded),
            shape == Shape::MmxUnsigned,
            "{shape:?}"
        );
        assert_eq!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            shape != Shape::MmxUnsigned,
            "{shape:?}"
        );
        assert!(
            x86_native_mmx_pairs_valid_excluding(&function, &excluded),
            "{shape:?}"
        );
        assert!(
            x86_native_mmx_features_supported_excluding(&function, &excluded),
            "{shape:?}"
        );

        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        let mut expected = X86NativeReplayFeatureRequirements::default();
        if shape != Shape::MmxUnsigned {
            expected.any = true;
            expected.all_spans_support_avx_ymm16 = true;
            expected.needs_avx = true;
            expected.needs_sse41 = shape == Shape::XmmSigned;
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));
        }
        assert_eq!(requirements, expected, "{shape:?}");

        #[cfg(target_arch = "x86_64")]
        if shape != Shape::MmxUnsigned {
            let supported = std::is_x86_feature_detected!("avx")
                && (shape != Shape::XmmSigned || std::is_x86_feature_detected!("sse4.1"));
            assert_eq!(requirements.x86_host_supported(), supported, "{shape:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                supported,
                "{shape:?}"
            );
        }

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{shape:?}"
        );
    }
}

#[test]
fn all_9792_o0_o1_o2_rex_register_graphs_admit_and_emit_exact_source_bytes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for shape in SHAPES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(shape, rex, modrm);
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
                        shape == Shape::MmxUnsigned,
                        "{level:?} {bytes:02X?}"
                    );
                    assert_eq!(
                        uses_x86_native_vectors_excluding(&function, &excluded),
                        shape != Shape::MmxUnsigned,
                        "{level:?} {bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    if shape != Shape::MmxUnsigned {
                        lowerer.set_avx_ymm16_vector_state(true);
                    }
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
    assert_eq!(lowered, LEVELS.len() * 3 * 17 * 64);
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    for shape in SHAPES {
        let bytes = encoding(shape, Some(0x45), 0xCA);
        let baseline = function(&bytes, OptLevel::O0, false);

        let mut missing = baseline.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{shape:?} missing");

        let mismatch = match shape {
            Shape::MmxUnsigned => encoding(Shape::XmmUnsigned, Some(0x45), 0xCA),
            Shape::XmmUnsigned => encoding(Shape::XmmSigned, Some(0x45), 0xCA),
            Shape::XmmSigned => encoding(Shape::XmmUnsigned, Some(0x45), 0xCA),
        };
        for metadata in [
            mismatch,
            encoding(shape, Some(0x45), 0xD2),
            encoding(shape, Some(0x45), 0xC9),
            encoding(shape, Some(0x45), 0x0A),
            {
                let mut reserved = vec![0x67];
                reserved.extend(encoding(shape, None, 0xCA));
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
                "{shape:?} {metadata:02X?}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeCase {
    shape: Shape,
    destination: u8,
    source: u8,
    rex: Option<u8>,
    profile: usize,
}

fn case_bytes(case: NativeCase) -> Vec<u8> {
    let extension = if case.shape == Shape::MmxUnsigned {
        0
    } else {
        (if case.destination >= 8 { 0x04 } else { 0 }) | (if case.source >= 8 { 0x01 } else { 0 })
    };
    let rex = if case.shape == Shape::MmxUnsigned {
        case.rex
    } else {
        case.rex.map(|rex| (rex & 0xFA) | extension)
    };
    encoding(
        case.shape,
        rex,
        0xC0 | ((case.destination & 7) << 3) | (case.source & 7),
    )
}

fn cases() -> Vec<NativeCase> {
    let mut cases = Vec::new();
    for (operand_index, (destination, source)) in [(0, 0), (1, 2), (7, 6)].into_iter().enumerate() {
        for (rex_index, rex) in [None]
            .into_iter()
            .chain((0x40..=0x4F).map(Some))
            .enumerate()
        {
            cases.push(NativeCase {
                shape: Shape::MmxUnsigned,
                destination,
                source,
                rex,
                profile: (operand_index + rex_index) & 3,
            });
        }
    }
    for shape in [Shape::XmmUnsigned, Shape::XmmSigned] {
        for (operand_index, (destination, source)) in
            [(0, 0), (0, 1), (1, 0), (7, 7), (8, 9), (9, 8), (15, 15)]
                .into_iter()
                .enumerate()
        {
            if destination < 8 && source < 8 {
                cases.push(NativeCase {
                    shape,
                    destination,
                    source,
                    rex: None,
                    profile: operand_index & 3,
                });
            }
            for (ignored_index, ignored) in [0x00, 0x02, 0x08, 0x0A].into_iter().enumerate() {
                cases.push(NativeCase {
                    shape,
                    destination,
                    source,
                    rex: Some(0x40 | ignored),
                    profile: (operand_index + ignored_index) & 3,
                });
            }
        }
    }
    cases
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MultiplyState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(case: NativeCase, ordinal: usize) -> MultiplyState {
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
    let patterns = [
        (0xFFFF_FFFFu32, 0x8000_0000u32),
        (0x8000_0000, 0xFFFF_FFFE),
        (0x7FFF_FFFF, 3),
        (0, u32::MAX),
    ];
    let (lhs, rhs) = patterns[case.profile];
    if case.shape == Shape::MmxUnsigned {
        mm[usize::from(case.destination)] =
            (mm[usize::from(case.destination)] & !u64::from(u32::MAX)) | u64::from(lhs);
        mm[usize::from(case.source)] =
            (mm[usize::from(case.source)] & !u64::from(u32::MAX)) | u64::from(rhs);
    } else {
        let destination = &mut vectors[usize::from(case.destination)];
        destination[0] = (destination[0] & 0xFFFF_FFFF_0000_0000) | u64::from(lhs);
        destination[1] = (destination[1] & 0xFFFF_FFFF_0000_0000) | u64::from(!lhs);
        let source = &mut vectors[usize::from(case.source)];
        source[0] = (source[0] & 0xFFFF_FFFF_0000_0000) | u64::from(rhs);
        source[1] = (source[1] & 0xFFFF_FFFF_0000_0000) | u64::from(!rhs);
    }
    MultiplyState {
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
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13),
            0x1F80 | (3 << 13),
        ][case.profile],
        x87_tag_word: [0xFFFFu64, 0xA5A5, 0x0000, 0x6996][case.profile],
    }
}

fn product(lhs: u32, rhs: u32, signed: bool) -> u64 {
    if signed {
        (i64::from(lhs as i32)).wrapping_mul(i64::from(rhs as i32)) as u64
    } else {
        u64::from(lhs) * u64::from(rhs)
    }
}

fn architectural_expected(case: NativeCase, initial: &MultiplyState) -> MultiplyState {
    let mut expected = initial.clone();
    if case.shape == Shape::MmxUnsigned {
        let lhs = initial.mm[usize::from(case.destination)] as u32;
        let rhs = initial.mm[usize::from(case.source)] as u32;
        expected.mm[usize::from(case.destination)] = product(lhs, rhs, false);
        expected.x87_tag_word = 0;
    } else {
        let destination = initial.vectors[usize::from(case.destination)];
        let source = initial.vectors[usize::from(case.source)];
        expected.vectors[usize::from(case.destination)][0] = product(
            destination[0] as u32,
            source[0] as u32,
            case.shape == Shape::XmmSigned,
        );
        expected.vectors[usize::from(case.destination)][1] = product(
            destination[1] as u32,
            source[1] as u32,
            case.shape == Shape::XmmSigned,
        );
    }
    expected
}

fn interpret(case: NativeCase, initial: &MultiplyState, level: OptLevel) -> MultiplyState {
    let bytes = case_bytes(case);
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
    MultiplyState {
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
fn interpreter_matches_intel_equations_at_o0_o1_o2_for_aliases_rex_and_boundaries() {
    let cases = cases();
    assert_eq!(cases.len(), 115);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
            assert_eq!(
                interpret(case, &initial, level),
                expected,
                "{level:?} {case:?} {:02X?}",
                case_bytes(case)
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &MultiplyState, level: OptLevel) -> MultiplyState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case_bytes(case);
    let function = function(&bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    if case.shape != Shape::MmxUnsigned {
        lowerer.set_avx_ymm16_vector_state(true);
    }
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map legacy widening-multiply replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: if case.shape == Shape::MmxUnsigned {
            0
        } else {
            X86_VECTOR_STATE_YMM16
        },
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        mmx_active: u64::from(case.shape == Shape::MmxUnsigned),
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
    MultiplyState {
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
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_WIDENING_MUL_CHILD_RANGE";

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
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
            assert_eq!(
                interpret(case, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {:02X?}",
                case_bytes(case)
            );
            assert_eq!(
                execute_native(case, &initial, level),
                expected,
                "native {level:?} {case:?} {:02X?}",
                case_bytes(case)
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
        .expect("run isolated native legacy widening-multiply differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 115);
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
        "isolated native legacy widening-multiply failure at case {start}/{}: \
         {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case_bytes(case),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_intel_o0_o1_o2_equations_for_aliases_rex_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("sse4.1") {
        eprintln!("skipping native legacy widening-multiply differential: host lacks AVX/SSE4.1");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_widening_dword_multiply_replay::\
         replay_matches_intel_o0_o1_o2_equations_for_aliases_rex_and_full_state",
    );
}
