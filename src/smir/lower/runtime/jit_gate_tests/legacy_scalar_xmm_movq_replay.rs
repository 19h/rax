//! Native replay coverage for register-only legacy scalar-XMM MOVQ. Semantics
//! follow Intel SDM Order No. 325383-092US (June 2026), Vol. 2B, `MOVQ`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xD67E;
const LEVELS: [crate::smir::optimize::OptLevel; 3] = [
    crate::smir::optimize::OptLevel::O0,
    crate::smir::optimize::OptLevel::O1,
    crate::smir::optimize::OptLevel::O2,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    RmDestination,
    RegDestination,
}

impl Direction {
    const ALL: [Self; 2] = [Self::RmDestination, Self::RegDestination];

    fn prefix(self) -> u8 {
        match self {
            Self::RmDestination => 0x66,
            Self::RegDestination => 0xF3,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::RmDestination => 0xD6,
            Self::RegDestination => 0x7E,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MovqCase {
    direction: Direction,
    rex: Option<u8>,
    modrm: u8,
}

impl MovqCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.direction.prefix()];
        bytes.extend(self.rex);
        bytes.extend([0x0F, self.direction.opcode(), self.modrm]);
        bytes
    }

    fn registers(self) -> (u8, u8) {
        let rex = self.rex.unwrap_or(0);
        let reg = ((rex & 0x04) << 1) | ((self.modrm >> 3) & 7);
        let rm = ((rex & 0x01) << 3) | (self.modrm & 7);
        match self.direction {
            Direction::RmDestination => (rm, reg),
            Direction::RegDestination => (reg, rm),
        }
    }
}

fn exhaustive_cases() -> Vec<MovqCase> {
    let mut cases = Vec::with_capacity(2 * 17 * 64);
    for direction in Direction::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                cases.push(MovqCase {
                    direction,
                    rex,
                    modrm,
                });
            }
        }
    }
    assert_eq!(cases.len(), 2_176);
    cases
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

#[test]
fn replay_features_select_only_the_avx_ymm16_state_bridge_for_all_encodings() {
    let excluded = std::collections::HashMap::new();
    for case in exhaustive_cases() {
        let bytes = case.bytes();
        let function = function(&bytes);
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(is_native_clobber_safe(&function), "{case:?} {bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            "{case:?} {bytes:02X?}"
        );
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(
            requirements.all_spans_support_avx_ymm16,
            "{case:?} {bytes:02X?}"
        );
        assert!(requirements.needs_avx, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx2, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_sse3, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_ssse3, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_sse41, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx512bw, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx512vl, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx512dq, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx512fp16, "{case:?} {bytes:02X?}");
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            "{case:?} {bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx"),
            "{case:?} {bytes:02X?}"
        );
    }
}

fn assert_replay_emitted(code: &[u8], source: &[u8], label: &str) {
    assert!(
        code.windows(source.len()).any(|window| window == source),
        "{label}: source instruction {source:02X?} absent from {code:02X?}"
    );
}

#[test]
fn replay_emits_every_exact_source_instruction_at_o0_o1_o2() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for case in exhaustive_cases() {
        let bytes = case.bytes();
        for level in LEVELS {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            assert_replay_emitted(&code, &bytes, &format!("{level:?} {case:?}"));
            lowered += 1;
        }
    }
    assert_eq!(lowered, 2_176 * LEVELS.len());
}

#[test]
fn replay_feature_gate_fails_closed_without_provenance_or_exact_graph() {
    let case = MovqCase {
        direction: Direction::RegDestination,
        rex: Some(0x45),
        modrm: 0xD1,
    };
    let bytes = case.bytes();
    let baseline = function(&bytes);

    let mut missing_provenance = baseline.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut malformed_graph = baseline;
    malformed_graph.blocks[0].ops.pop();

    for malformed in [missing_provenance, malformed_graph] {
        assert!(!is_native_clobber_safe(&malformed));
        assert!(
            !x86_native_replay_feature_requirements(&malformed, &std::collections::HashMap::new(),)
                .any
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MovqState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(ordinal: usize) -> MovqState {
    MovqState {
        gprs: std::array::from_fn(|register| {
            0x89AB_CDEF_0123_4567u64.rotate_left((register * 9) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xC33C_F00F_6996_A55Au64
                    .rotate_left(((ordinal * 5 + register * 13 + word * 19) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        mm: std::array::from_fn(|register| {
            0xA5A5_5A5A_6996_9669u64.rotate_left(((register * 11 + ordinal * 7) & 63) as u32)
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
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][ordinal % 4],
    }
}

fn architectural_expected(case: MovqCase, initial: &MovqState) -> MovqState {
    let mut expected = initial.clone();
    let (destination, source) = case.registers();
    expected.vectors[usize::from(destination)][0] = initial.vectors[usize::from(source)][0];
    expected.vectors[usize::from(destination)][1] = 0;
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
    initial: &MovqState,
    level: crate::smir::optimize::OptLevel,
) -> MovqState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
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
    MovqState {
        gprs: x86.gpr,
        vectors,
        mm: x86.mm,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
        x87_tag_word: u64::from(x86.x87.tag_word),
    }
}

#[test]
fn interpreter_matches_intel_o0_o1_o2_all_encodings_and_full_state() {
    for (ordinal, case) in exhaustive_cases().into_iter().enumerate() {
        let bytes = case.bytes();
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
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
    initial: &MovqState,
    level: crate::smir::optimize::OptLevel,
) -> MovqState {
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
    assert_replay_emitted(&code, bytes, &format!("{level:?}"));
    let exec = ExecMem::new(&code).expect("map legacy scalar-XMM MOVQ replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
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
    MovqState {
        gprs: registers.gpr,
        vectors,
        mm: registers.mm,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
        x87_tag_word: registers.x87_tag_word,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_SCALAR_XMM_MOVQ_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[MovqCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = case.bytes();
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
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
        .expect("run isolated native legacy scalar-XMM MOVQ differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("host lacks AVX; legacy scalar-XMM MOVQ native replay is gated off");
        return;
    }
    let cases = exhaustive_cases();
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
    let bytes = case.bytes();
    panic!(
        "isolated native legacy scalar-XMM MOVQ failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_all_encodings_and_full_state() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_scalar_xmm_movq_replay::\
         replay_matches_intel_o0_o1_o2_all_encodings_and_full_state",
    );
}
