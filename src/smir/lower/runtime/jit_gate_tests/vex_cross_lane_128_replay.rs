//! Native replay coverage for register-only AVX/AVX2 VEX 128-bit cross-lane
//! operations.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xA11C;
const OPERANDS: [(u8, u8, u8); 12] = [
    (1, 2, 3),
    (9, 2, 3),
    (1, 10, 3),
    (1, 2, 11),
    (9, 10, 11),
    (15, 8, 9),
    (1, 1, 2),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 10),
    (9, 10, 9),
    (9, 9, 9),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrossLane {
    PermuteF128,
    InsertF128,
    InsertI128,
    PermuteI128,
}

impl CrossLane {
    const ALL: [Self; 4] = [
        Self::PermuteF128,
        Self::InsertF128,
        Self::InsertI128,
        Self::PermuteI128,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::PermuteF128 => 0x06,
            Self::InsertF128 => 0x18,
            Self::InsertI128 => 0x38,
            Self::PermuteI128 => 0x46,
        }
    }

    fn is_insert(self) -> bool {
        matches!(self, Self::InsertF128 | Self::InsertI128)
    }

    fn needs_avx2(self) -> bool {
        matches!(self, Self::InsertI128 | Self::PermuteI128)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrossLaneCase {
    operation: CrossLane,
    dst: u8,
    src1: u8,
    src2: u8,
    imm: u8,
    clear_ignored_x: bool,
}

impl CrossLaneCase {
    fn needs_avx2(self) -> bool {
        self.operation.needs_avx2()
    }
}

fn encoding(case: CrossLaneCase) -> [u8; 6] {
    assert!(case.dst < 16 && case.src1 < 16 && case.src2 < 16);
    let mut p0 = 0xE3;
    if case.dst >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.src2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        ((!case.src1 & 0x0F) << 3) | 0x05,
        case.operation.opcode(),
        0xC0 | ((case.dst & 7) << 3) | (case.src2 & 7),
        case.imm,
    ]
}

fn representative_cases() -> Vec<CrossLaneCase> {
    let immediates = [
        0x00, 0x01, 0x31, 0x82, 0x88, 0xFF, 0x42, 0xBD, 0x10, 0x20, 0x30, 0x80,
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for operation in CrossLane::ALL {
        for (dst, src1, src2) in OPERANDS {
            cases.push(CrossLaneCase {
                operation,
                dst,
                src1,
                src2,
                imm: immediates[ordinal % immediates.len()],
                clear_ignored_x: ordinal & 1 != 0,
            });
            ordinal += 1;
        }
    }
    cases
}

fn exhaustive_immediate_cases() -> Vec<CrossLaneCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for operation in CrossLane::ALL {
        for imm in u8::MIN..=u8::MAX {
            let (dst, src1, src2) = OPERANDS[ordinal % OPERANDS.len()];
            cases.push(CrossLaneCase {
                operation,
                dst,
                src1,
                src2,
                imm,
                clear_ignored_x: ordinal & 1 != 0,
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

fn function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
    function_at(bytes, BlockId(0), PC)
}

#[test]
fn replay_features_select_avx_ymm16_and_distinguish_avx_from_avx2_forms() {
    for operation in CrossLane::ALL {
        let case = CrossLaneCase {
            operation,
            dst: 9,
            src1: 10,
            src2: 11,
            imm: 0xA5,
            clear_ignored_x: true,
        };
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert_eq!(requirements.needs_avx2, operation.needs_avx2(), "{case:?}");
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
fn replay_admits_and_emits_96_o0_o2_family_alias_and_extension_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = representative_cases();
    assert_eq!(cases.len(), 48);
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
    assert_eq!(lowered, 96);

    let case = CrossLaneCase {
        operation: CrossLane::PermuteI128,
        dst: 1,
        src1: 2,
        src2: 3,
        imm: 0x31,
        clear_ignored_x: true,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes;
    memory_bytes[4] &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CrossLaneState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> CrossLaneState {
    CrossLaneState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
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

fn architectural_expected(case: CrossLaneCase, initial: &CrossLaneState) -> CrossLaneState {
    let source1 = initial.vectors[usize::from(case.src1)];
    let source2 = initial.vectors[usize::from(case.src2)];
    let mut expected = initial.clone();
    let destination = &mut expected.vectors[usize::from(case.dst)];
    destination.fill(0);

    if case.operation.is_insert() {
        destination[..4].copy_from_slice(&source1[..4]);
        let output_half = usize::from(case.imm & 1);
        destination[output_half * 2..output_half * 2 + 2].copy_from_slice(&source2[..2]);
        return expected;
    }

    for (output_half, control_shift, zero_bit) in [(0usize, 0u8, 3u8), (1, 4, 7)] {
        if case.imm >> zero_bit & 1 != 0 {
            continue;
        }
        let control = case.imm >> control_shift & 3;
        let source = if control < 2 { &source1 } else { &source2 };
        let source_half = usize::from(control & 1);
        destination[output_half * 2..output_half * 2 + 2]
            .copy_from_slice(&source[source_half * 2..source_half * 2 + 2]);
    }
    expected
}

fn optimized_function(
    bytes: &[u8; 6],
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
    bytes: &[u8; 6],
    initial: &CrossLaneState,
    level: crate::smir::optimize::OptLevel,
) -> CrossLaneState {
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
    CrossLaneState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_all_immediates_aliases_and_upper_zeroing() {
    let cases = exhaustive_immediate_cases();
    assert_eq!(cases.len(), 1_024);
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
    bytes: &[u8; 6],
    initial: &CrossLaneState,
    level: crate::smir::optimize::OptLevel,
) -> CrossLaneState {
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
    let exec = ExecMem::new(&code).expect("map VEX 128-bit cross-lane replay");
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
    CrossLaneState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_CROSS_LANE_128_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[CrossLaneCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        if case.needs_avx2() && !std::is_x86_feature_detected!("avx2") {
            continue;
        }
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
        .expect("run isolated native VEX 128-bit cross-lane differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = exhaustive_immediate_cases();
    assert_eq!(cases.len(), 1_024);
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
        "isolated native VEX 128-bit cross-lane failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_all_immediates_aliases_extensions_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX 128-bit cross-lane differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_cross_lane_128_replay::\
         replay_matches_intel_o0_o2_all_immediates_aliases_extensions_and_full_state",
    );
}
