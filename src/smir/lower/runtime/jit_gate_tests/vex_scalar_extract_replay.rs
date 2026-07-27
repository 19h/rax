//! Native replay coverage for register-destination AVX VEX scalar lane
//! extracts.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xE17A;
const GPRS: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GprField {
    Reg,
    Rm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtractKind {
    Vextractps,
    Vpextrb,
    Vpextrd,
    Vpextrq,
    VpextrwMap1,
    VpextrwMap3,
}

impl ExtractKind {
    const ALL: [Self; 6] = [
        Self::Vextractps,
        Self::Vpextrb,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VpextrwMap1,
        Self::VpextrwMap3,
    ];

    fn fields(self) -> (u8, u8, WMode, GprField, usize, u8) {
        match self {
            Self::Vextractps => (3, 0x17, WMode::Ignored, GprField::Rm, 4, 0x03),
            Self::Vpextrb => (3, 0x14, WMode::Ignored, GprField::Rm, 1, 0x0F),
            Self::Vpextrd => (3, 0x16, WMode::W0, GprField::Rm, 4, 0x03),
            Self::Vpextrq => (3, 0x16, WMode::W1, GprField::Rm, 8, 0x01),
            Self::VpextrwMap1 => (1, 0xC5, WMode::Ignored, GprField::Reg, 2, 0x07),
            Self::VpextrwMap3 => (3, 0x15, WMode::Ignored, GprField::Rm, 2, 0x07),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtractCase {
    kind: ExtractKind,
    destination: u8,
    source: u8,
    immediate: u8,
    wig_w: bool,
    compact: bool,
    ignored_x: bool,
}

impl ExtractCase {
    fn w(self) -> bool {
        match self.kind.fields().2 {
            WMode::Ignored => self.wig_w,
            WMode::W0 => false,
            WMode::W1 => true,
        }
    }
}

fn encoding(case: ExtractCase) -> Vec<u8> {
    let (map, opcode, _, gpr_field, _, _) = case.kind.fields();
    assert!(case.destination < 16 && case.source < 16);
    if case.compact {
        assert_eq!(case.kind, ExtractKind::VpextrwMap1);
        assert!(case.source < 8);
        return vec![
            0xC5,
            (if case.destination < 8 { 0x80 } else { 0 }) | 0x79,
            0xC5,
            0xC0 | ((case.destination & 7) << 3) | case.source,
            case.immediate,
        ];
    }

    let (reg, rm) = match gpr_field {
        GprField::Reg => (case.destination, case.source),
        GprField::Rm => (case.source, case.destination),
    };
    let mut p0 = 0xE0 | map;
    if reg >= 8 {
        p0 &= !0x80;
    }
    if case.ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    vec![
        0xC4,
        p0,
        0x79 | (u8::from(case.w()) << 7),
        opcode,
        0xC0 | ((reg & 7) << 3) | (rm & 7),
        case.immediate,
    ]
}

fn exhaustive_cases() -> Vec<ExtractCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in ExtractKind::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let compact = kind == ExtractKind::VpextrwMap1 && immediate & 3 == 0;
            let destination = GPRS[(ordinal * 5 + 3) % GPRS.len()];
            let source = if compact {
                ((ordinal * 5 + 3) % 8) as u8
            } else {
                ((ordinal * 7 + 3) % 16) as u8
            };
            cases.push(ExtractCase {
                kind,
                destination,
                source,
                immediate,
                wig_w: ordinal & 1 != 0,
                compact,
                ignored_x: ordinal & 2 != 0,
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

fn expected_replay_bytes(case: ExtractCase, bytes: &[u8]) -> Vec<u8> {
    if !matches!(case.destination, 4 | 5) {
        return bytes.to_vec();
    }

    let mut rewritten = bytes.to_vec();
    if rewritten[0] == 0xC5 {
        rewritten[1] |= 0x80;
        rewritten[3] &= !0x38;
    } else if rewritten[1] & 0x1F == 1 {
        rewritten[1] |= 0x80;
        rewritten[4] &= !0x38;
    } else {
        rewritten[1] |= 0x20;
        rewritten[4] &= !0x07;
    }

    let mut expected = vec![0x50, 0x51];
    expected.extend_from_slice(&rewritten);
    expected.extend_from_slice(&[
        0x48,
        0x8B,
        0x4D,
        X86_STATE_PTR_AT_RBP as u8,
        0x48,
        0x89,
        0x41,
        case.destination * 8,
    ]);
    if case.destination == 5 {
        expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
    }
    expected.extend_from_slice(&[0x59, 0x58]);
    expected
}

fn assert_replay_emitted(code: &[u8], case: ExtractCase, bytes: &[u8]) {
    let expected = expected_replay_bytes(case, bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?} source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn replay_features_select_avx_ymm16_without_avx2_or_avx512() {
    let mut cases = Vec::new();
    for kind in ExtractKind::ALL {
        cases.push(ExtractCase {
            kind,
            destination: 14,
            source: 15,
            immediate: 0xFF,
            wig_w: true,
            compact: false,
            ignored_x: true,
        });
    }
    cases.push(ExtractCase {
        kind: ExtractKind::VpextrwMap1,
        destination: 14,
        source: 7,
        immediate: 0xFF,
        wig_w: false,
        compact: true,
        ignored_x: false,
    });
    cases.push(ExtractCase {
        kind: ExtractKind::Vextractps,
        destination: 4,
        source: 15,
        immediate: 0xFF,
        wig_w: true,
        compact: false,
        ignored_x: true,
    });
    cases.push(ExtractCase {
        kind: ExtractKind::VpextrwMap1,
        destination: 5,
        source: 7,
        immediate: 0xFF,
        wig_w: false,
        compact: true,
        ignored_x: false,
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
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            "{case:?}"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx"),
            "{case:?}"
        );
    }
}

#[test]
fn replay_admits_and_emits_182_o0_o2_kind_extension_wig_and_c5_samples() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases: Vec<_> = exhaustive_cases().into_iter().step_by(17).collect();
    assert_eq!(cases.len(), 91);
    assert!(cases.iter().any(|case| case.compact));
    assert!(cases.iter().any(|case| case.source >= 8));
    assert!(cases.iter().any(|case| case.destination >= 8));
    assert!(cases.iter().any(|case| matches!(case.destination, 4 | 5)));
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
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            assert_replay_emitted(&code, case, &bytes);
            lowered += 1;
        }
    }
    assert_eq!(lowered, 182);
}

#[test]
fn replay_fails_closed_without_exact_register_provenance() {
    let case = ExtractCase {
        kind: ExtractKind::Vextractps,
        destination: 9,
        source: 11,
        immediate: 0xFF,
        wig_w: true,
        compact: false,
        ignored_x: true,
    };
    let bytes = encoding(case);
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();

    let modrm_index = bytes.len() - 2;
    let mut memory = bytes.clone();
    memory[modrm_index] &= 0x3F;
    let mut memory_metadata = base.clone();
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );

    let mut nonreserved_vvvv = bytes.clone();
    nonreserved_vvvv[2] &= !0x08;
    let mut vvvv_metadata = base.clone();
    vvvv_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&nonreserved_vvvv).unwrap(),
    );

    let mut wrong_l = bytes.clone();
    wrong_l[2] |= 0x04;
    let mut l_metadata = base;
    l_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&wrong_l).unwrap(),
    );

    for nonmatching in [missing, memory_metadata, vvvv_metadata, l_metadata] {
        assert!(!is_native_clobber_safe(&nonmatching));
        assert!(
            !x86_native_replay_feature_requirements(
                &nonmatching,
                &std::collections::HashMap::new()
            )
            .any
        );
    }
}

#[test]
fn rsp_rbp_destinations_emit_a_flag_neutral_state_commit_for_every_shape() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    for destination in [4, 5] {
        for kind in ExtractKind::ALL {
            let wig_values: &[bool] = if kind.fields().2 == WMode::Ignored {
                &[false, true]
            } else {
                &[false]
            };
            let compact_values: &[bool] = if kind == ExtractKind::VpextrwMap1 {
                &[false, true]
            } else {
                &[false]
            };
            for &wig_w in wig_values {
                for &compact in compact_values {
                    let case = ExtractCase {
                        kind,
                        destination,
                        source: if compact { 7 } else { 15 },
                        immediate: 0xFF,
                        wig_w,
                        compact,
                        ignored_x: true,
                    };
                    let bytes = encoding(case);
                    for level in [
                        crate::smir::optimize::OptLevel::O0,
                        crate::smir::optimize::OptLevel::O2,
                    ] {
                        let mut function = function(&bytes);
                        crate::smir::optimize::optimize_function(&mut function, level);
                        assert!(is_native_clobber_safe(&function), "{level:?} {case:?}");
                        let mut lowerer = X86_64Lowerer::new();
                        lowerer.set_avx_ymm16_vector_state(true);
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                        assert_replay_emitted(&code, case, &bytes);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> ExtractState {
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

fn architectural_expected(case: ExtractCase, initial: &ExtractState) -> ExtractState {
    let source = vector_bytes(initial.vectors[usize::from(case.source)]);
    let (_, _, _, _, width, lane_mask) = case.kind.fields();
    let lane = usize::from(case.immediate & lane_mask);
    let mut scalar = [0u8; 8];
    scalar[..width].copy_from_slice(&source[lane * width..lane * width + width]);

    let mut expected = initial.clone();
    expected.gprs[usize::from(case.destination)] = u64::from_le_bytes(scalar);
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
    initial: &ExtractState,
    level: crate::smir::optimize::OptLevel,
) -> ExtractState {
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
    ExtractState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_all_immediates_kinds_extensions_and_zero_extension() {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
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
    case: ExtractCase,
    initial: &ExtractState,
    level: crate::smir::optimize::OptLevel,
) -> ExtractState {
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
    assert_replay_emitted(&code, case, bytes);
    let exec = ExecMem::new(&code).expect("map VEX scalar-extract replay");
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
    ExtractState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_EXTRACT_CHILD_RANGE";

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
                execute_native(&bytes, case, &initial, level),
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
        .expect("run isolated native VEX scalar-extract differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
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
        "isolated native VEX scalar-extract failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_all_immediates_kinds_extensions_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar-extract differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_extract_replay::\
         replay_matches_intel_o0_o2_all_immediates_kinds_extensions_and_full_state",
    );
}
