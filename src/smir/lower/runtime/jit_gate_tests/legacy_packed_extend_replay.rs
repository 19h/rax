//! Native replay coverage for register-only legacy SSE4.1 packed
//! sign/zero-extension moves.

use super::*;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, is_native_clobber_safe, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE420;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operation {
    opcode: u8,
    source_bits: u8,
    destination_bits: u8,
    signed: bool,
}

const OPERATIONS: [Operation; 12] = [
    Operation {
        opcode: 0x20,
        source_bits: 8,
        destination_bits: 16,
        signed: true,
    },
    Operation {
        opcode: 0x21,
        source_bits: 8,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        opcode: 0x22,
        source_bits: 8,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x23,
        source_bits: 16,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        opcode: 0x24,
        source_bits: 16,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x25,
        source_bits: 32,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x30,
        source_bits: 8,
        destination_bits: 16,
        signed: false,
    },
    Operation {
        opcode: 0x31,
        source_bits: 8,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        opcode: 0x32,
        source_bits: 8,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        opcode: 0x33,
        source_bits: 16,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        opcode: 0x34,
        source_bits: 16,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        opcode: 0x35,
        source_bits: 32,
        destination_bits: 64,
        signed: false,
    },
];

fn encoding(opcode: u8, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x38, opcode, modrm]);
    bytes
}

fn canonical_encoding(
    operation: Operation,
    destination: u8,
    source: u8,
    ignored_rex_bits: u8,
    omit_rex: bool,
) -> Vec<u8> {
    assert!(destination < 16 && source < 16);
    assert_eq!(ignored_rex_bits & !0x0A, 0);
    let extension = if destination >= 8 { 0x04 } else { 0 } | if source >= 8 { 0x01 } else { 0 };
    assert!(!omit_rex || extension == 0);
    encoding(
        operation.opcode,
        (!omit_rex).then_some(0x40 | extension | ignored_rex_bits),
        0xC0 | ((destination & 7) << 3) | (source & 7),
    )
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
        X86InstructionBytes::new(bytes).expect("legacy packed-extension provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse41_avx_and_the_ymm16_bridge_only() {
    let bytes = canonical_encoding(OPERATIONS[11], 9, 10, 0x0A, false);
    let function = function(&bytes, OptLevel::O2, false);
    let excluded = std::collections::HashMap::new();
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(&function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_sse41 = true;
    expected.needs_avx = true;
    assert_eq!(requirements, expected);

    #[cfg(target_arch = "x86_64")]
    {
        let supported =
            std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("avx");
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

#[test]
fn all_39_168_o0_o1_o2_rex_register_graphs_admit_and_emit_exact_source_bytes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for operation in OPERATIONS {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(operation.opcode, rex, modrm);
                for level in LEVELS {
                    let function = function(&bytes, level, false);
                    let excluded = std::collections::HashMap::new();
                    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(&function, &excluded),
                        "{level:?} {bytes:02X?}"
                    );
                    assert!(
                        x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
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
    assert_eq!(lowered, LEVELS.len() * 12 * 17 * 64);
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_reserved_provenance() {
    let bytes = canonical_encoding(OPERATIONS[0], 9, 10, 0, false);
    let baseline = function(&bytes, OptLevel::O0, false);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    for metadata in [
        encoding(0x21, Some(0x45), 0xCA),
        encoding(0x20, Some(0x41), 0xCA),
        encoding(0x20, Some(0x44), 0xCA),
        encoding(0x20, Some(0x45), 0x0A),
        vec![0x67, 0x66, 0x45, 0x0F, 0x38, 0x20, 0xCA],
    ] {
        let mut malformed = baseline.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&metadata).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{metadata:02X?}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeCase {
    operation: Operation,
    destination: u8,
    source: u8,
    ignored_rex_bits: u8,
    omit_rex: bool,
    profile: usize,
}

fn cases() -> Vec<NativeCase> {
    const OPERANDS: [(u8, u8); 8] = [
        (1, 2),
        (9, 10),
        (1, 1),
        (9, 9),
        (1, 9),
        (9, 1),
        (0, 15),
        (15, 0),
    ];
    const IGNORED_REX: [u8; 4] = [0, 0x02, 0x08, 0x0A];
    let mut cases = Vec::new();
    for operation in OPERATIONS {
        for (destination, source) in OPERANDS {
            for (profile, ignored_rex_bits) in IGNORED_REX.into_iter().enumerate() {
                cases.push(NativeCase {
                    operation,
                    destination,
                    source,
                    ignored_rex_bits,
                    omit_rex: profile == 0 && destination < 8 && source < 8,
                    profile,
                });
            }
        }
    }
    cases
}

fn case_bytes(case: NativeCase) -> Vec<u8> {
    canonical_encoding(
        case.operation,
        case.destination,
        case.source,
        case.ignored_rex_bits,
        case.omit_rex,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtendState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
}

const BYTE_PATTERNS: [u64; 10] = [0, 1, 0x7E, 0x7F, 0x80, 0x81, 0xFE, 0xFF, 0x55, 0xAA];
const WORD_PATTERNS: [u64; 10] = [
    0, 1, 0x7FFE, 0x7FFF, 0x8000, 0x8001, 0xFFFE, 0xFFFF, 0x5555, 0xAAAA,
];
const DWORD_PATTERNS: [u64; 10] = [
    0,
    1,
    0x7FFF_FFFE,
    0x7FFF_FFFF,
    0x8000_0000,
    0x8000_0001,
    0xFFFF_FFFE,
    0xFFFF_FFFF,
    0x5555_5555,
    0xAAAA_AAAA,
];

fn lane_pattern(source_bits: u8, lane: usize, profile: usize) -> u64 {
    let index = (lane * 3 + profile * 7) % BYTE_PATTERNS.len();
    match source_bits {
        8 => BYTE_PATTERNS[index],
        16 => WORD_PATTERNS[index],
        32 => DWORD_PATTERNS[index],
        _ => unreachable!(),
    }
}

fn insert_lane(vector: &mut [u64; 8], lane: usize, bits: u8, value: u64) {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    debug_assert!(shift + usize::from(bits) <= 64);
    vector[word] = (vector[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn extract_lane(vector: &[u64; 8], lane: usize, bits: u8) -> u64 {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    debug_assert!(shift + usize::from(bits) <= 64);
    (vector[word] >> shift) & mask
}

fn initial_state(case: NativeCase, ordinal: usize) -> ExtendState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (ordinal as u64).rotate_left((word * 7) as u32)
        })
    });
    for lane in 0..(128 / usize::from(case.operation.source_bits)) {
        insert_lane(
            &mut vectors[usize::from(case.source)],
            lane,
            case.operation.source_bits,
            lane_pattern(case.operation.source_bits, lane, case.profile),
        );
    }
    ExtendState {
        gprs: std::array::from_fn(|register| {
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            u64::MAX,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        ac_flag: (ordinal & 1) as u64,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][case.profile],
    }
}

fn architectural_expected(case: NativeCase, initial: &ExtendState) -> ExtendState {
    let mut expected = initial.clone();
    let source = initial.vectors[usize::from(case.source)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    destination[0] = 0;
    destination[1] = 0;
    let lanes = 128 / usize::from(case.operation.destination_bits);
    for lane in 0..lanes {
        let raw = extract_lane(&source, lane, case.operation.source_bits);
        let extended = if case.operation.signed {
            let shift = 64 - u32::from(case.operation.source_bits);
            (((raw << shift) as i64) >> shift) as u64
        } else {
            raw
        };
        insert_lane(destination, lane, case.operation.destination_bits, extended);
    }
    expected
}

fn interpret(case: NativeCase, initial: &ExtendState, level: OptLevel) -> ExtendState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let bytes = case_bytes(case);
    let function = function(&bytes, level, true);
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
    ExtendState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_equations_at_o0_o1_o2_for_all_shapes_aliases_and_boundaries() {
    let cases = cases();
    assert_eq!(cases.len(), 384);
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
fn execute_native(case: NativeCase, initial: &ExtendState, level: OptLevel) -> ExtendState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case_bytes(case);
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
    let exec = ExecMem::new(&code).expect("map legacy packed-extension replay");
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
    ExtendState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_PACKED_EXTEND_CHILD_RANGE";

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
        .expect("run isolated native legacy packed-extension differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 384);
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
        "isolated native legacy packed-extension failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_equations_for_all_shapes_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("sse4.1") || !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy packed-extension differential: host lacks SSE4.1/AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_packed_extend_replay::\
         replay_matches_intel_o0_o1_o2_equations_for_all_shapes_aliases_and_full_state",
    );
}
