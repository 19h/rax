//! Native replay coverage for register-only legacy SHA-NI instructions.

use super::*;
use crate::smir::ir::ops::X86Sha32Op;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, is_native_clobber_safe, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x5A32;
const OPERATIONS: [(X86Sha32Op, u8, u8, bool); 7] = [
    (X86Sha32Op::Sha1Nexte, 0x38, 0xC8, false),
    (X86Sha32Op::Sha1Msg1, 0x38, 0xC9, false),
    (X86Sha32Op::Sha1Msg2, 0x38, 0xCA, false),
    (X86Sha32Op::Sha256Rounds2, 0x38, 0xCB, false),
    (X86Sha32Op::Sha256Msg1, 0x38, 0xCC, false),
    (X86Sha32Op::Sha256Msg2, 0x38, 0xCD, false),
    (X86Sha32Op::Sha1Rounds4, 0x3A, 0xCC, true),
];
const INERT_PREFIXES: [Option<u8>; 4] = [None, Some(0x64), Some(0x65), Some(0x67)];

fn encoding(
    map: u8,
    opcode: u8,
    has_immediate: bool,
    inert_prefix: Option<u8>,
    rex: Option<u8>,
    modrm: u8,
    immediate: u8,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(inert_prefix);
    bytes.extend(rex);
    bytes.extend([0x0F, map, opcode, modrm]);
    if has_immediate {
        bytes.push(immediate);
    }
    bytes
}

fn canonical_encoding(
    map: u8,
    opcode: u8,
    has_immediate: bool,
    destination: u8,
    source: u8,
    immediate: u8,
    inert_prefix: Option<u8>,
    ignored_rex_bits: u8,
) -> Vec<u8> {
    assert!(destination < 16 && source < 16);
    let rex = 0x40
        | (ignored_rex_bits & 0x0A)
        | if destination >= 8 { 0x04 } else { 0 }
        | if source >= 8 { 0x01 } else { 0 };
    encoding(
        map,
        opcode,
        has_immediate,
        inert_prefix,
        Some(rex),
        0xC0 | ((destination & 7) << 3) | (source & 7),
        immediate,
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
        X86InstructionBytes::new(bytes).expect("legacy SHA provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sha_avx_and_the_ymm16_bridge_only() {
    let bytes = canonical_encoding(0x38, 0xCB, false, 9, 11, 0, Some(0x67), 0x0A);
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
    expected.needs_avx = true;
    expected.needs_sha = true;
    assert_eq!(requirements, expected);

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("sha")
    );

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn all_60_928_o0_o2_inert_rex_register_graphs_lower_to_canonical_instruction() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for inert_prefix in INERT_PREFIXES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for (_, map, opcode, has_immediate) in OPERATIONS {
                for modrm in 0xC0..=0xFF {
                    let immediate = modrm ^ rex.unwrap_or(0) ^ inert_prefix.unwrap_or(0);
                    let bytes = encoding(
                        map,
                        opcode,
                        has_immediate,
                        inert_prefix,
                        rex,
                        modrm,
                        immediate,
                    );
                    let replay_bytes =
                        encoding(map, opcode, has_immediate, None, rex, modrm, immediate);
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = function(&bytes, level, false);
                        assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                        let mut lowerer = X86_64Lowerer::new();
                        lowerer.set_avx_ymm16_vector_state(true);
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        assert!(
                            code.windows(replay_bytes.len())
                                .any(|window| window == replay_bytes),
                            "{level:?} {bytes:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 2 * 4 * 17 * 7 * 64);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShaState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    map: u8,
    opcode: u8,
    has_immediate: bool,
    destination: u8,
    source: u8,
    immediate: u8,
    inert_prefix: Option<u8>,
    ignored_rex_bits: u8,
    data_case: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const OPERANDS: [(u8, u8); 7] = [(1, 3), (9, 11), (1, 1), (15, 0), (0, 15), (8, 8), (2, 7)];
    let mut cases = Vec::new();
    for (operation_index, (_, map, opcode, has_immediate)) in OPERATIONS.into_iter().enumerate() {
        for (operand_index, (destination, source)) in OPERANDS.into_iter().enumerate() {
            for data_case in 0..4 {
                cases.push(NativeCase {
                    map,
                    opcode,
                    has_immediate,
                    destination,
                    source,
                    immediate: if has_immediate {
                        [0x00, 0x01, 0xA6, 0xFF][data_case]
                    } else {
                        0
                    },
                    inert_prefix: [None, Some(0x64), Some(0x65), Some(0x67)]
                        [(operation_index + operand_index + data_case) & 3],
                    ignored_rex_bits: ((operation_index + operand_index + data_case) as u8 & 3)
                        << 1,
                    data_case,
                });
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase, ordinal: usize) -> ShaState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (ordinal as u64).rotate_left((word * 7) as u32)
        })
    });
    let roles = [
        [0u64, 0],
        [u64::MAX, u64::MAX],
        [0x0011_2233_4455_6677, 0x8899_AABB_CCDD_EEFF],
        [0x8000_0000_0000_0001, 0x7FFF_FFFF_FFFF_FFFE],
    ];
    vectors[usize::from(case.destination)][..2].copy_from_slice(&roles[case.data_case]);
    vectors[usize::from(case.source)][..2].copy_from_slice(&[
        roles[(case.data_case + 1) & 3][1],
        roles[(case.data_case + 2) & 3][0],
    ]);
    vectors[0][..2].copy_from_slice(&[
        roles[(case.data_case + 2) & 3][1],
        roles[(case.data_case + 3) & 3][0],
    ]);
    ShaState {
        gprs: std::array::from_fn(|register| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 5) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
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
        ac_flag: (ordinal & 1) as u64,
        mxcsr: 0x1F80 | ((ordinal as u32 & 3) << 13) | (ordinal as u32 & 0x3F),
    }
}

#[cfg(target_arch = "x86_64")]
fn case_bytes(case: NativeCase) -> Vec<u8> {
    canonical_encoding(
        case.map,
        case.opcode,
        case.has_immediate,
        case.destination,
        case.source,
        case.immediate,
        case.inert_prefix,
        case.ignored_rex_bits,
    )
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: NativeCase, initial: &ShaState, level: OptLevel) -> ShaState {
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
    ShaState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &ShaState, level: OptLevel) -> ShaState {
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
    let exec = ExecMem::new(&code).expect("map legacy SHA replay");
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
    ShaState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_SHA_CHILD_RANGE";

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
        for level in [OptLevel::O0, OptLevel::O2] {
            let expected = interpret(case, &initial, level);
            assert_eq!(
                execute_native(case, &initial, level),
                expected,
                "{level:?} {case:?} {:02X?}",
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
        .expect("run isolated native legacy SHA differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
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
        "isolated native legacy SHA failure at case {start}/{}: {case:?} {:02X?}; \
         whole status {}; singleton status {}; singleton stdout: {}; singleton stderr: {}",
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
fn native_replay_matches_o0_o2_interpretation_for_all_operations_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("sha") {
        eprintln!("skipping native legacy SHA differential: host lacks AVX or SHA");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_sha_replay::\
         native_replay_matches_o0_o2_interpretation_for_all_operations_aliases_and_full_state",
    );
}
