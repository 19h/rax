//! Native replay coverage for register-only EVEX FP32/FP64 conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A20;
const CONTROLS: [(u8, bool); 7] = [
    (0, false),
    (1, false),
    (2, false),
    (0, true),
    (1, true),
    (2, true),
    (3, true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConvertKind {
    Widen,
    Narrow,
}

impl ConvertKind {
    const ALL: [Self; 2] = [Self::Widen, Self::Narrow];

    fn p1(self) -> u8 {
        match self {
            Self::Widen => 0x7C,
            Self::Narrow => 0xFD,
        }
    }
}

fn encoding(
    kind: ConvertKind,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        kind.p1(),
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | 0x08
            | mask,
        0x5A,
        0xC0 | ((destination & 7) << 3) | (source & 7),
    ]
}

fn function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
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
fn replay_feature_aggregation_requires_bw_and_exact_vl_without_dq_or_fp16() {
    for kind in ConvertKind::ALL {
        for (ll, embedded_control) in CONTROLS {
            let bytes = encoding(kind, ll, embedded_control, 17, 18, 1, false);
            let function = function(&bytes);
            let needs_vl = !embedded_control && ll != 2;
            let actual = x86_native_replay_feature_requirements(
                &function,
                &std::collections::HashMap::new(),
            );
            assert!(actual.any, "{bytes:02X?}");
            assert!(actual.needs_avx512bw, "{bytes:02X?}");
            assert_eq!(actual.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert!(!actual.needs_avx512dq, "{bytes:02X?}");
            assert!(!actual.needs_avx512fp16, "{bytes:02X?}");
            assert!(!actual.needs_avx512cd, "{bytes:02X?}");
            assert!(!actual.needs_gfni, "{bytes:02X?}");
            assert!(!actual.needs_avx512vp2intersect, "{bytes:02X?}");
            assert!(!actual.needs_pclmulqdq, "{bytes:02X?}");
            assert!(!actual.needs_vpclmulqdq, "{bytes:02X?}");

            let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
            assert_eq!(
                x86_native_replay_feature_requirements(&function, &excluded),
                X86NativeReplayFeatureRequirements::default(),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn replay_admits_and_emits_420_optimized_legal_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [(1u8, 2u8), (9, 10), (17, 18), (25, 26), (31, 31), (2, 2)];
    let masks = [(0u8, false), (1, false), (1, true), (2, false), (3, true)];
    let mut admitted = 0usize;
    let mut missing_provenance_checked = false;
    let mut memory_metadata_checked = false;

    for kind in ConvertKind::ALL {
        for (ll, embedded_control) in CONTROLS {
            let needs_vl = !embedded_control && ll != 2;
            for (destination, source) in operands {
                for (mask, zeroing) in masks {
                    let bytes = encoding(
                        kind,
                        ll,
                        embedded_control,
                        destination,
                        source,
                        mask,
                        zeroing,
                    );
                    let mut function = function(&bytes);
                    if !missing_provenance_checked {
                        let mut missing = function.clone();
                        missing.x86_instruction_bytes.clear();
                        crate::smir::optimize::optimize_function(
                            &mut missing,
                            crate::smir::optimize::OptLevel::O2,
                        );
                        assert!(!is_native_clobber_safe(&missing));
                        missing_provenance_checked = true;
                    }
                    if !memory_metadata_checked {
                        let mut memory = bytes;
                        memory[5] &= 0x3F;
                        let mut malformed = function.clone();
                        malformed.x86_instruction_bytes.insert(
                            (BlockId(0), PC),
                            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
                        );
                        assert!(!is_native_clobber_safe(&malformed));
                        memory_metadata_checked = true;
                    }

                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        "{bytes:02X?}"
                    );

                    #[cfg(target_arch = "x86_64")]
                    let expected_features = std::is_x86_feature_detected!("avx512f")
                        && std::is_x86_feature_detected!("avx512bw")
                        && (!needs_vl || std::is_x86_feature_detected!("avx512vl"));
                    #[cfg(not(target_arch = "x86_64"))]
                    let expected_features = false;
                    assert_eq!(
                        x86_native_vector_features_supported_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        expected_features,
                        "{bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&function)
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    assert!(
                        code.windows(bytes.len()).any(|window| window == bytes),
                        "{bytes:02X?}"
                    );
                    admitted += 1;
                }
            }
        }
    }

    assert!(missing_provenance_checked && memory_metadata_checked);
    assert_eq!(admitted, 420);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConvertState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u32; 20] = [
    0x0000_0000,
    0x8000_0000,
    0x0000_0001,
    0x007F_FFFF,
    0x0080_0000,
    0x3F80_0000,
    0x3F80_0001,
    0x3FFF_FFFF,
    0x4000_0000,
    0xBF80_0000,
    0x7F7F_FFFF,
    0xFF7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0xFFC1_2345,
    0x7F81_2345,
    0xFF81_2345,
    0x3EAA_AAAB,
    0xBEAA_AAAB,
];

#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 24] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x000F_FFFF_FFFF_FFFF,
    0x0010_0000_0000_0000,
    0x3690_0000_0000_0000,
    0x36A0_0000_0000_0000,
    0x380F_FFFF_FFFF_FFFF,
    0x3810_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0x3FF0_0000_0FFF_FFFF,
    0x3FF0_0000_1000_0000,
    0x3FF0_0000_1000_0001,
    0xBFF0_0000_1000_0000,
    0x47EF_FFFF_E000_0000,
    0x47EF_FFFF_F000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0xFFEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_1234_5678_9ABC,
    0xFFF8_1234_5678_9ABC,
    0x7FF0_1234_5678_9ABC,
    0xFFF0_1234_5678_9ABC,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(kind: ConvertKind, register: usize, profile: usize) -> [u64; 8] {
    match kind {
        ConvertKind::Widen => {
            let lanes: [u32; 16] = std::array::from_fn(|lane| {
                F32_PATTERNS[(lane + register * 7 + profile * 3) % F32_PATTERNS.len()]
            });
            std::array::from_fn(|lane| {
                u64::from(lanes[lane * 2]) | (u64::from(lanes[lane * 2 + 1]) << 32)
            })
        }
        ConvertKind::Narrow => std::array::from_fn(|lane| {
            F64_PATTERNS[(lane + register * 7 + profile * 3) % F64_PATTERNS.len()]
        }),
    }
}

#[cfg(target_arch = "x86_64")]
fn initial_state(kind: ConvertKind, profile: usize, mxcsr: u32) -> ConvertState {
    ConvertState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(kind, register, profile)),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA5,
            0,
            u64::MAX,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
fn interpret(
    bytes: &[u8; 6],
    initial: &ConvertState,
    level: crate::smir::optimize::OptLevel,
) -> ConvertState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
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
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
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
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &ConvertState,
    level: crate::smir::optimize::OptLevel,
) -> ConvertState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX FP32/FP64 conversion replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: 1,
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
    ConvertState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_FP32_FP64_CONVERT_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeProfile {
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
}

#[cfg(target_arch = "x86_64")]
const NATIVE_PROFILES: [NativeProfile; 10] = [
    NativeProfile {
        destination: 1,
        source: 2,
        mask: 0,
        zeroing: false,
    },
    NativeProfile {
        destination: 9,
        source: 10,
        mask: 1,
        zeroing: false,
    },
    NativeProfile {
        destination: 17,
        source: 18,
        mask: 1,
        zeroing: true,
    },
    NativeProfile {
        destination: 25,
        source: 26,
        mask: 2,
        zeroing: false,
    },
    NativeProfile {
        destination: 30,
        source: 31,
        mask: 3,
        zeroing: true,
    },
    NativeProfile {
        destination: 31,
        source: 31,
        mask: 1,
        zeroing: false,
    },
    NativeProfile {
        destination: 2,
        source: 2,
        mask: 3,
        zeroing: true,
    },
    NativeProfile {
        destination: 0,
        source: 16,
        mask: 1,
        zeroing: true,
    },
    NativeProfile {
        destination: 16,
        source: 0,
        mask: 1,
        zeroing: false,
    },
    NativeProfile {
        destination: 29,
        source: 30,
        mask: 2,
        zeroing: true,
    },
];

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    kind: ConvertKind,
    ll: u8,
    embedded_control: bool,
    profile: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let mut cases = Vec::new();
    let mut available_controls = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for kind in ConvertKind::ALL {
            for (ll, embedded_control) in CONTROLS {
                if !embedded_control && ll != 2 && !has_vl {
                    continue;
                }
                available_controls += 1;
                for profile in 0..NATIVE_PROFILES.len() {
                    let prior_status = 1 << (profile % 6);
                    let rc = ((profile as u32) & 3) << 13;
                    let daz = if profile & 1 == 0 { 0 } else { 1 << 6 };
                    let ftz = if profile & 2 == 0 { 0 } else { 1 << 15 };
                    cases.push(NativeCase {
                        level,
                        kind,
                        ll,
                        embedded_control,
                        profile,
                        mxcsr: 0x1F80 | prior_status | rc | daz | ftz,
                    });
                }
            }
        }
    }

    assert!(
        available_controls > 0,
        "feature-selected conversion controls"
    );
    assert_eq!(cases.len(), available_controls * NATIVE_PROFILES.len());
    cases
}

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
        let profile = NATIVE_PROFILES[case.profile];
        let bytes = encoding(
            case.kind,
            case.ll,
            case.embedded_control,
            profile.destination,
            profile.source,
            profile.mask,
            profile.zeroing,
        );
        let initial = initial_state(case.kind, case.profile, case.mxcsr);
        assert_eq!(
            execute_native(&bytes, &initial, case.level),
            interpret(&bytes, &initial, case.level),
            "{case:?} profile={profile:?} bytes={bytes:02X?}"
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
        .expect("run isolated native FP32/FP64 conversion differential")
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

    // Raw source replay can terminate the child with SIGILL before Rust can
    // report assertion context. Bisect child ranges in O(log N) launches and
    // report the exact guest encoding without killing the parent test binary.
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
    let profile = NATIVE_PROFILES[case.profile];
    let bytes = encoding(
        case.kind,
        case.ll,
        case.embedded_control,
        profile.destination,
        profile.source,
        profile.mask,
        profile.zeroing,
    );
    panic!(
        "isolated native FP32/FP64 conversion failure at case {start}/{}: \
         {case:?} profile={profile:?} {bytes:02X?}; whole status {}; \
         singleton status {}; singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_o0_o2_interpretation_for_controls_masks_aliases_mxcsr_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native FP32/FP64 conversion differential: host lacks AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::evex_fp32_fp64_convert_replay::\
         replay_matches_o0_o2_interpretation_for_controls_masks_aliases_mxcsr_and_full_state",
    );
}
