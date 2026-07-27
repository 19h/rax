//! Native replay coverage for register-destination F16C VEX `VCVTPS2PH`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xF32_2F16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn l(self) -> u8 {
        u8::from(self == Self::V256)
    }

    fn result_qwords(self) -> usize {
        match self {
            Self::V128 => 1,
            Self::V256 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NarrowCase {
    width: Width,
    destination: u8,
    source: u8,
    ignored_x_clear: bool,
    immediate: u8,
}

fn encoding(case: NarrowCase) -> [u8; 6] {
    let NarrowCase {
        width,
        destination,
        source,
        ignored_x_clear,
        immediate,
    } = case;
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE3;
    if source >= 8 {
        p0 &= !0x80;
    }
    if ignored_x_clear {
        p0 &= !0x40;
    }
    if destination >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (width.l() << 2),
        0x1D,
        0xC0 | ((source & 7) << 3) | (destination & 7),
        immediate,
    ]
}

fn cases() -> Vec<NarrowCase> {
    const IMMEDIATES: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE,
        0xFF,
    ];
    let mut cases = Vec::new();
    for width in [Width::V128, Width::V256] {
        for ignored_x_clear in [false, true] {
            for (destination, source) in [(1, 2), (9, 10), (1, 1), (9, 9), (15, 15), (1, 9), (9, 1)]
            {
                for immediate in IMMEDIATES {
                    cases.push(NarrowCase {
                        width,
                        destination,
                        source,
                        ignored_x_clear,
                        immediate,
                    });
                }
            }
        }
    }
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
fn replay_features_select_avx_f16c_and_the_ymm16_boundary() {
    for case in [
        NarrowCase {
            width: Width::V128,
            destination: 1,
            source: 2,
            ignored_x_clear: false,
            immediate: 0,
        },
        NarrowCase {
            width: Width::V256,
            destination: 9,
            source: 10,
            ignored_x_clear: true,
            immediate: 0xFF,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(requirements.needs_f16c, "{case:?}");
        assert!(requirements.needs_vex_fp16_narrow, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{case:?}"
        );

        #[cfg(target_arch = "x86_64")]
        {
            let mut expected =
                std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("f16c");
            #[cfg(target_os = "macos")]
            {
                expected &= !running_under_rosetta();
            }
            assert_eq!(requirements.x86_host_supported(), expected, "{case:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(
                    &function,
                    &std::collections::HashMap::new()
                ),
                expected,
                "{case:?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_host_gate_conjoins_avx_and_f16c() {
    let host_avx = std::is_x86_feature_detected!("avx");
    let host_f16c = std::is_x86_feature_detected!("f16c");
    for needs_avx in [false, true] {
        for needs_f16c in [false, true] {
            let requirements = X86NativeReplayFeatureRequirements {
                needs_avx,
                needs_f16c,
                ..X86NativeReplayFeatureRequirements::default()
            };
            assert_eq!(
                requirements.x86_host_supported(),
                (!needs_avx || host_avx) && (!needs_f16c || host_f16c)
            );
        }
    }
}

fn assert_admitted_and_emitted(bytes: &[u8], level: crate::smir::optimize::OptLevel) {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut function = function(bytes);
    crate::smir::optimize::optimize_function(&mut function, level);
    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
    assert!(
        x86_native_vector_uses_avx_ymm16_only_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
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
}

#[test]
fn replay_emission_covers_prefix_modrm_and_full_immediate_fields_at_o0_and_o2() {
    let levels = [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ];
    let mut trials = 0usize;

    // Cartesian coverage of all eight VEX.R/X/B images, both vector lengths,
    // all 64 register ModR/M images, and representative immediate boundaries.
    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 3 {
            continue;
        }
        for p1 in [0x79u8, 0x7D] {
            for modrm in 0xC0u8..=0xFF {
                for immediate in [0x00u8, 0x07, 0xF8, 0xFF] {
                    let bytes = [0xC4, p0, p1, 0x1D, modrm, immediate];
                    for level in levels {
                        assert_admitted_and_emitted(&bytes, level);
                        trials += 1;
                    }
                }
            }
        }
    }

    // Every immediate byte is independently admitted at both optimizer levels;
    // together the loops cover the complete field product without lowering all
    // 262,144 byte images redundantly.
    for immediate in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0x03, 0x7D, 0x1D, 0xFF, immediate];
        for level in levels {
            assert_admitted_and_emitted(&bytes, level);
            trials += 1;
        }
    }

    assert_eq!(trials, 8_704);
}

#[test]
fn replay_survives_aliases_extensions_and_fails_closed_without_exact_provenance() {
    for case in cases() {
        let bytes = encoding(case);
        assert_admitted_and_emitted(&bytes, crate::smir::optimize::OptLevel::O2);
    }

    let bytes = encoding(NarrowCase {
        width: Width::V256,
        destination: 9,
        source: 10,
        ignored_x_clear: true,
        immediate: 0xFF,
    });
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    for invalid in [
        {
            let mut value = bytes;
            value[4] &= 0x3F; // memory destination
            value
        },
        {
            let mut value = bytes;
            value[2] |= 0x80; // W1
            value
        },
        {
            let mut value = bytes;
            value[2] &= !0x08; // VEX.vvvv != 1111b
            value
        },
        {
            let mut value = bytes;
            value[1] = (value[1] & 0xE0) | 2; // map 0F38
            value
        },
    ] {
        let mut malformed = base.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&invalid).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{invalid:02X?}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NarrowState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const F32_PATTERNS: [u32; 20] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0xC000_0000,
    0x0000_0001,
    0x007F_FFFF,
    0x0080_0000,
    0x3300_0000,
    0x3380_0000,
    0x3F80_1000,
    0xBF80_1000,
    0x477F_E000,
    0x477F_F000,
    0x7F7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0xFFC1_2345,
    0x7F81_2345,
    0xFF81_2345,
];

fn patterned_vector(register: usize, profile: usize) -> [u64; 8] {
    let lanes: [u32; 16] = std::array::from_fn(|lane| {
        F32_PATTERNS[(lane + register * 7 + profile * 3) % F32_PATTERNS.len()]
    });
    std::array::from_fn(|word| u64::from(lanes[word * 2]) | (u64::from(lanes[word * 2 + 1]) << 32))
}

fn initial_state(profile: usize) -> NarrowState {
    let prior_status = (profile as u32).rotate_left(3) & 0x3F;
    let rc = ((profile as u32 >> 1) & 3) << 13;
    let daz = if profile & 1 == 0 { 0 } else { 1 << 6 };
    let ftz = if profile & 2 == 0 { 0 } else { 1 << 15 };
    NarrowState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(register, profile)),
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
        // Every SIMD exception remains masked at the native boundary.
        mxcsr: 0x1F80 | prior_status | rc | daz | ftz,
    }
}

fn set_f32_lanes(state: &mut NarrowState, register: u8, lanes: [u32; 8]) {
    let mut bytes = [0u8; 64];
    for (lane, value) in lanes.into_iter().enumerate() {
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    state.vectors[usize::from(register)] = std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    });
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
    initial: &NarrowState,
    level: crate::smir::optimize::OptLevel,
) -> NarrowState {
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
    NarrowState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_preserves_full_state_and_applies_vex_result_zeroing() {
    let cases = cases();
    assert_eq!(cases.len(), 448);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let o0 = interpret(&bytes, &initial, crate::smir::optimize::OptLevel::O0);
        let o2 = interpret(&bytes, &initial, crate::smir::optimize::OptLevel::O2);
        assert_eq!(o2, o0, "{case:?} {bytes:02X?}");
        assert_eq!(o0.gprs, initial.gprs, "{case:?}");
        assert_eq!(o0.masks, initial.masks, "{case:?}");
        assert_eq!(o0.rflags, initial.rflags, "{case:?}");
        for register in 0..32 {
            if register != usize::from(case.destination) {
                assert_eq!(
                    o0.vectors[register], initial.vectors[register],
                    "{case:?} register={register}"
                );
            }
        }
        assert!(
            o0.vectors[usize::from(case.destination)][case.width.result_qwords()..]
                .iter()
                .all(|word| *word == 0),
            "{case:?} {bytes:02X?}"
        );
    }
}

#[test]
fn interpreter_matches_exact_binary16_results_status_and_ftz_rule() {
    let source = [
        0x0000_0001,
        0x8000_0000,
        0x3F80_0000,
        (1.0f32 + 2.0f32.powi(-11)).to_bits(),
        2.0f32.powi(-24).to_bits(),
        2.0f32.powi(-25).to_bits(),
        f32::MAX.to_bits(),
        0x7F80_0001,
    ];
    let expected = [0x3C00_3C00_8000_0000, 0x7E00_7C00_0000_0001];

    for width in [Width::V128, Width::V256] {
        let case = NarrowCase {
            width,
            destination: 9,
            source: 10,
            ignored_x_clear: true,
            immediate: 0,
        };
        let bytes = encoding(case);
        let mut initial = initial_state(0);
        set_f32_lanes(&mut initial, 10, source);
        initial.mxcsr = 0x1F80 | (1 << 15);

        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let actual = interpret(&bytes, &initial, level);
            assert_eq!(
                &actual.vectors[9][..width.result_qwords()],
                &expected[..width.result_qwords()],
                "{level:?} {width:?}"
            );
            assert!(
                actual.vectors[9][width.result_qwords()..]
                    .iter()
                    .all(|word| *word == 0),
                "{level:?} {width:?}"
            );
            let expected_status = if width == Width::V128 { 0x32 } else { 0x3B };
            assert_eq!(actual.mxcsr, initial.mxcsr | expected_status);
        }
    }
}

#[test]
fn interpreter_dynamic_rounding_honors_daz_and_ignores_ftz() {
    let case = NarrowCase {
        width: Width::V128,
        destination: 1,
        source: 2,
        ignored_x_clear: true,
        immediate: 0xFF,
    };
    let bytes = encoding(case);
    let midpoint = 1.0f32 + 2.0f32.powi(-11);
    let mut initial = initial_state(0);
    set_f32_lanes(
        &mut initial,
        2,
        [
            1,
            2.0f32.powi(-24).to_bits(),
            midpoint.to_bits(),
            (-midpoint).to_bits(),
            0,
            0,
            0,
            0,
        ],
    );
    initial.mxcsr = 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15);

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let actual = interpret(&bytes, &initial, level);
        assert_eq!(actual.vectors[1][0], 0xBC00_3C01_0001_0000, "{level:?}");
        assert!(actual.vectors[1][1..].iter().all(|word| *word == 0));
        assert_eq!(actual.mxcsr, initial.mxcsr | (1 << 5));
    }
}

#[test]
fn interpreter_all_256_immediates_reduce_to_the_architectural_low_three_bits() {
    let initial = initial_state(17);
    let mut references = Vec::new();
    for immediate in 0u8..8 {
        let bytes = encoding(NarrowCase {
            width: Width::V256,
            destination: 9,
            source: 10,
            ignored_x_clear: true,
            immediate,
        });
        references.push(interpret(
            &bytes,
            &initial,
            crate::smir::optimize::OptLevel::O0,
        ));
    }

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(NarrowCase {
            width: Width::V256,
            destination: 9,
            source: 10,
            ignored_x_clear: true,
            immediate,
        });
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                references[usize::from(immediate & 7)],
                "{level:?} immediate={immediate:#04X}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &NarrowState,
    level: crate::smir::optimize::OptLevel,
) -> NarrowState {
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
    let exec = ExecMem::new(&code).expect("map VEX FP16 narrowing replay");
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
    NarrowState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP16_NARROW_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[NarrowCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                execute_native(&bytes, &initial, level),
                interpret(&bytes, &initial, level),
                "{level:?} {case:?} {bytes:02X?}"
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
        .expect("run isolated native VEX FP16 narrowing differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
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
    let bytes = encoding(case);
    panic!(
        "isolated native VEX FP16 narrowing failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_formats_controls_aliases_mxcsr_and_full_state() {
    #[cfg(target_os = "macos")]
    if running_under_rosetta() {
        eprintln!(
            "skipping native VEX FP16 narrowing differential: Rosetta applies MXCSR.FTZ \
             even though VCVTPS2PH must retain FP16 denormal results"
        );
        return;
    }
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("f16c") {
        eprintln!("skipping native VEX FP16 narrowing differential: host lacks AVX/F16C");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp16_narrow_replay::\
         replay_matches_o0_o2_interpretation_for_formats_controls_aliases_mxcsr_and_full_state",
    );
}
