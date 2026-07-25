//! Native replay coverage for register-only AVX VEX scalar lane inserts.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xA11D;
const GPR_OPERANDS: [(u8, u8, u8); 12] = [
    (1, 2, 0),
    (9, 10, 3),
    (1, 1, 6),
    (9, 9, 7),
    (15, 8, 8),
    (13, 14, 9),
    (1, 2, 10),
    (9, 10, 11),
    (1, 1, 12),
    (9, 9, 13),
    (15, 8, 14),
    (13, 14, 15),
];
const COMPACT_OPERANDS: [(u8, u8, u8); 4] = [(1, 2, 0), (9, 10, 3), (1, 1, 6), (13, 14, 7)];
const VECTOR_OPERANDS: [(u8, u8, u8); 12] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (9, 10, 10),
    (15, 8, 13),
    (13, 14, 15),
    (15, 15, 15),
    (8, 15, 8),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertKind {
    Vpinsrb,
    Vpinsrw,
    Vpinsrd,
    Vpinsrq,
    Vinsertps,
}

impl InsertKind {
    const ALL: [Self; 5] = [
        Self::Vpinsrb,
        Self::Vpinsrw,
        Self::Vpinsrd,
        Self::Vpinsrq,
        Self::Vinsertps,
    ];

    fn fields(self) -> (u8, u8, WMode, bool) {
        match self {
            Self::Vpinsrb => (3, 0x20, WMode::Ignored, true),
            Self::Vpinsrw => (1, 0xC4, WMode::Ignored, true),
            Self::Vpinsrd => (3, 0x22, WMode::W0, true),
            Self::Vpinsrq => (3, 0x22, WMode::W1, true),
            Self::Vinsertps => (3, 0x21, WMode::Ignored, false),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InsertCase {
    kind: InsertKind,
    dst: u8,
    merge: u8,
    source: u8,
    imm: u8,
    wig_w: bool,
    compact: bool,
    clear_ignored_x: bool,
}

impl InsertCase {
    fn w(self) -> bool {
        match self.kind.fields().2 {
            WMode::Ignored => self.wig_w,
            WMode::W0 => false,
            WMode::W1 => true,
        }
    }
}

fn encoding(case: InsertCase) -> Vec<u8> {
    let (map, opcode, _, _) = case.kind.fields();
    assert!(case.dst < 16 && case.merge < 16 && case.source < 16);
    if case.compact {
        assert_eq!(case.kind, InsertKind::Vpinsrw);
        assert!(!case.w());
        assert!(case.source < 8);
        return vec![
            0xC5,
            (if case.dst < 8 { 0x80 } else { 0 }) | ((!case.merge & 0x0F) << 3) | 1,
            0xC4,
            0xC0 | ((case.dst & 7) << 3) | case.source,
            case.imm,
        ];
    }

    let mut p0 = 0xE0 | map;
    if case.dst >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.source >= 8 {
        p0 &= !0x20;
    }
    vec![
        0xC4,
        p0,
        (u8::from(case.w()) << 7) | ((!case.merge & 0x0F) << 3) | 1,
        opcode,
        0xC0 | ((case.dst & 7) << 3) | (case.source & 7),
        case.imm,
    ]
}

fn exhaustive_cases() -> Vec<InsertCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in InsertKind::ALL {
        for imm in u8::MIN..=u8::MAX {
            let compact = kind == InsertKind::Vpinsrw && imm & 3 == 0;
            let (dst, merge, source) = if compact {
                COMPACT_OPERANDS[(usize::from(imm) >> 2) % COMPACT_OPERANDS.len()]
            } else if kind.fields().3 {
                GPR_OPERANDS[ordinal % GPR_OPERANDS.len()]
            } else {
                VECTOR_OPERANDS[ordinal % VECTOR_OPERANDS.len()]
            };
            cases.push(InsertCase {
                kind,
                dst,
                merge,
                source,
                imm,
                wig_w: !compact && ordinal & 1 != 0,
                compact,
                clear_ignored_x: ordinal & 2 != 0,
            });
            ordinal += 1;
        }
    }
    cases
}

fn function_at(bytes: &[u8], block_id: BlockId, pc: u64) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(pc, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(block_id, pc);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, pc);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((block_id, pc), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
    function_at(bytes, BlockId(0), PC)
}

#[test]
fn replay_features_select_avx_ymm16_without_avx2_or_avx512() {
    let mut cases = Vec::new();
    for kind in InsertKind::ALL {
        cases.push(InsertCase {
            kind,
            dst: 13,
            merge: 14,
            source: 15,
            imm: 0xA5,
            wig_w: true,
            compact: false,
            clear_ignored_x: true,
        });
    }
    cases.push(InsertCase {
        kind: InsertKind::Vpinsrw,
        dst: 9,
        merge: 10,
        source: 3,
        imm: 7,
        wig_w: false,
        compact: true,
        clear_ignored_x: false,
    });

    for case in cases {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_fma4, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));
    }
}

#[test]
fn replay_admits_and_emits_152_o0_o2_kind_alias_extension_wig_and_c5_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases: Vec<_> = exhaustive_cases().into_iter().step_by(17).collect();
    assert_eq!(cases.len(), 76);
    assert!(cases.iter().any(|case| case.compact));
    let mut lowered = 0usize;
    for case in cases {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{level:?} {case:?} {bytes:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 152);

    let case = InsertCase {
        kind: InsertKind::Vinsertps,
        dst: 1,
        merge: 2,
        source: 3,
        imm: 0x5A,
        wig_w: true,
        compact: false,
        clear_ignored_x: true,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes.clone();
    let modrm = memory_bytes.len() - 2;
    memory_bytes[modrm] &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));

    for kind in [
        InsertKind::Vpinsrb,
        InsertKind::Vpinsrw,
        InsertKind::Vpinsrd,
        InsertKind::Vpinsrq,
    ] {
        for source in [4, 5] {
            let case = InsertCase {
                kind,
                dst: 1,
                merge: 2,
                source,
                imm: 0,
                wig_w: true,
                compact: false,
                clear_ignored_x: false,
            };
            let bytes = encoding(case);
            assert!(!is_native_clobber_safe(&function(&bytes)), "{case:?}");
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InsertState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> InsertState {
    InsertState {
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

fn architectural_expected(case: InsertCase, initial: &InsertState) -> InsertState {
    let merge = vector_bytes(initial.vectors[usize::from(case.merge)]);
    let mut result = [0u8; 64];
    result[..16].copy_from_slice(&merge[..16]);

    match case.kind {
        InsertKind::Vinsertps => {
            let source = vector_bytes(initial.vectors[usize::from(case.source)]);
            let source_lane = usize::from((case.imm >> 6) & 3);
            let destination_lane = usize::from((case.imm >> 4) & 3);
            result[destination_lane * 4..destination_lane * 4 + 4]
                .copy_from_slice(&source[source_lane * 4..source_lane * 4 + 4]);
            for lane in 0..4 {
                if case.imm & (1 << lane) != 0 {
                    result[lane * 4..lane * 4 + 4].fill(0);
                }
            }
        }
        InsertKind::Vpinsrb | InsertKind::Vpinsrw | InsertKind::Vpinsrd | InsertKind::Vpinsrq => {
            let (width, lane_mask) = match case.kind {
                InsertKind::Vpinsrb => (1, 0x0F),
                InsertKind::Vpinsrw => (2, 0x07),
                InsertKind::Vpinsrd => (4, 0x03),
                InsertKind::Vpinsrq => (8, 0x01),
                InsertKind::Vinsertps => unreachable!(),
            };
            let lane = usize::from(case.imm & lane_mask);
            let scalar = initial.gprs[usize::from(case.source)].to_le_bytes();
            result[lane * width..lane * width + width].copy_from_slice(&scalar[..width]);
        }
    }

    let mut expected = initial.clone();
    for (word, bytes) in result.chunks_exact(8).enumerate() {
        expected.vectors[usize::from(case.dst)][word] =
            u64::from_le_bytes(bytes.try_into().unwrap());
    }
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
    initial: &InsertState,
    level: crate::smir::optimize::OptLevel,
) -> InsertState {
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
    InsertState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_all_immediates_kinds_aliases_and_upper_zeroing() {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_280);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
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
    initial: &InsertState,
    level: crate::smir::optimize::OptLevel,
) -> InsertState {
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
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map VEX scalar-insert replay");
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
    InsertState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_INSERT_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[InsertCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, &initial, level),
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
        .expect("run isolated native VEX scalar-insert differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_280);
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
        "isolated native VEX scalar-insert failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_all_immediates_kinds_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar-insert differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_insert_replay::\
         replay_matches_intel_o0_o2_all_immediates_kinds_aliases_and_full_state",
    );
}
