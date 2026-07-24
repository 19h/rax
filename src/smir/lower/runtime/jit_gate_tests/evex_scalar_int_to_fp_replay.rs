//! Native replay coverage for EVEX scalar integer-to-floating-point conversion.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2A7B;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DestinationFormat {
    F32,
    F64,
    F16,
}

impl DestinationFormat {
    const ALL: [Self; 3] = [Self::F32, Self::F64, Self::F16];

    fn fields(self) -> (u8, u8, bool) {
        match self {
            Self::F32 => (1, 2, false),
            Self::F64 => (1, 3, false),
            Self::F16 => (5, 2, true),
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn precision(self) -> u32 {
        match self {
            Self::F16 => 11,
            Self::F32 => 24,
            Self::F64 => 53,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn patterns(self, signed: bool, w: bool) -> Vec<u64> {
        let mask = if w { u64::MAX } else { u32::MAX as u64 };
        let sign_bit = if w { 1u64 << 63 } else { 1u64 << 31 };
        let mut patterns = vec![0, 1, 2, mask, sign_bit - 1, sign_bit];
        let boundary = 1u64 << self.precision();
        if boundary <= mask {
            patterns.extend([boundary - 1, boundary, boundary + 1, boundary + 3]);
            if signed {
                patterns.extend([
                    mask.wrapping_sub(boundary).wrapping_add(1),
                    mask.wrapping_sub(boundary),
                ]);
            }
        }
        if self == Self::F16 {
            patterns.extend([65_503, 65_504, 65_519, 65_520, 70_000]);
            if signed {
                patterns.push(mask.wrapping_sub(70_000).wrapping_add(1));
            }
        }
        patterns.iter_mut().for_each(|value| *value &= mask);
        patterns.sort_unstable();
        patterns.dedup();
        patterns
    }
}

fn encoding(
    format: DestinationFormat,
    signed: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 16);
    let (map, pp, _) = format.fields();
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        (if w { 0x80 } else { 0 }) | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 },
        if signed { 0x2A } else { 0x7B },
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn valid_control(ll: u8, embedded_control: bool) -> bool {
    ll != 3 || embedded_control
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
fn replay_feature_aggregation_requires_fp16_only_for_binary16_destinations() {
    for format in DestinationFormat::ALL {
        let bytes = encoding(format, false, true, 3, true, 31, 30, 15);
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{format:?} {bytes:02X?}");
        assert!(actual.needs_avx512bw, "{format:?} {bytes:02X?}");
        assert!(!actual.needs_avx512vl, "{format:?} {bytes:02X?}");
        assert!(!actual.needs_avx512dq, "{format:?} {bytes:02X?}");
        assert_eq!(
            actual.needs_avx512fp16,
            format.fields().2,
            "{format:?} {bytes:02X?}"
        );
        assert!(!actual.needs_avx512cd, "{format:?} {bytes:02X?}");
        assert!(!actual.needs_gfni, "{format:?} {bytes:02X?}");
        assert!(!actual.needs_avx512vp2intersect, "{format:?} {bytes:02X?}");
        assert!(!actual.needs_vpclmulqdq, "{format:?} {bytes:02X?}");

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{format:?} {bytes:02X?}"
        );
    }
}

#[test]
fn replay_admits_and_emits_168_o0_o2_safe_semantic_shapes_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let destinations = [0u8, 3, 7, 8, 15, 16, 17, 24, 31];
    let merges = [0u8, 2, 8, 15, 16, 18, 23, 31];
    let sources = [0u8, 1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let mut seen_sources = std::collections::BTreeSet::new();
    let mut lowered = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in DestinationFormat::ALL {
            for signed in [false, true] {
                for w in [false, true] {
                    for ll in 0..=3 {
                        for embedded_control in [false, true] {
                            if !valid_control(ll, embedded_control) {
                                continue;
                            }
                            let destination = destinations[lowered % destinations.len()];
                            let merge = merges[(lowered * 3 + 1) % merges.len()];
                            let source = sources[(lowered * 5 + 3) % sources.len()];
                            seen_sources.insert(source);
                            let bytes = encoding(
                                format,
                                signed,
                                w,
                                ll,
                                embedded_control,
                                destination,
                                merge,
                                source,
                            );
                            let mut function = function(&bytes);
                            crate::smir::optimize::optimize_function(&mut function, level);
                            assert!(
                                is_native_clobber_safe(&function),
                                "{level:?} {format:?} {bytes:02X?}"
                            );
                            assert!(
                                uses_x86_native_vectors_excluding(
                                    &function,
                                    &std::collections::HashMap::new()
                                ),
                                "{level:?} {format:?} {bytes:02X?}"
                            );

                            #[cfg(target_arch = "x86_64")]
                            let expected_features = std::is_x86_feature_detected!("avx512f")
                                && std::is_x86_feature_detected!("avx512bw")
                                && (!format.fields().2
                                    || std::is_x86_feature_detected!("avx512fp16"));
                            #[cfg(not(target_arch = "x86_64"))]
                            let expected_features = false;
                            assert_eq!(
                                x86_native_vector_features_supported_excluding(
                                    &function,
                                    &std::collections::HashMap::new()
                                ),
                                expected_features,
                                "{level:?} {format:?} {bytes:02X?}"
                            );

                            let mut lowerer = X86_64Lowerer::new();
                            lowerer.lower_function(&function).unwrap_or_else(|error| {
                                panic!("{level:?} {format:?} {bytes:02X?}: {error:?}")
                            });
                            let code = lowerer.finalize().unwrap_or_else(|error| {
                                panic!("{level:?} {format:?} {bytes:02X?}: {error:?}")
                            });
                            assert!(
                                code.windows(bytes.len()).any(|window| window == bytes),
                                "{level:?} {format:?} {bytes:02X?}"
                            );
                            lowered += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 168);
    assert!(seen_sources.contains(&12) && seen_sources.contains(&13));

    let replay_only = encoding(DestinationFormat::F16, false, true, 3, true, 31, 30, 15);
    let mut missing = function(&replay_only);
    missing.x86_instruction_bytes.clear();
    crate::smir::optimize::optimize_function(&mut missing, crate::smir::optimize::OptLevel::O2);
    assert!(!is_native_clobber_safe(&missing), "{replay_only:02X?}");

    let mut memory = replay_only;
    memory[5] &= 0x3F;
    let mut malformed = function(&replay_only);
    malformed.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );
    assert!(!is_native_clobber_safe(&malformed), "{memory:02X?}");

    let mut fabricated_gpr_bit4 = replay_only;
    fabricated_gpr_bit4[1] &= !0x40;
    let mut malformed = function(&replay_only);
    malformed.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&fabricated_gpr_bit4).unwrap(),
    );
    assert!(
        !is_native_clobber_safe(&malformed),
        "{fabricated_gpr_bit4:02X?}"
    );

    for source in [4, 5] {
        for format in DestinationFormat::ALL {
            let bytes = encoding(format, false, true, 3, true, 31, 30, source);
            assert!(
                !is_native_clobber_safe(&function(&bytes)),
                "{format:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversionState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn initial_state(source: u8, source_value: u64, mxcsr: u32) -> ConversionState {
    let mut gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
    });
    gprs[source as usize] = source_value;
    ConversionState {
        gprs,
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                    ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
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
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
fn interpret(
    bytes: &[u8],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
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
    ConversionState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
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
    let exec = ExecMem::new(&code).expect("map EVEX scalar integer-to-FP replay");
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
    ConversionState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_SCALAR_INT_TO_FP_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    format: DestinationFormat,
    signed: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    source_value: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let destinations = [0u8, 3, 7, 8, 15, 16, 17, 24, 31];
    let merges = [0u8, 2, 8, 15, 16, 18, 23, 31];
    let sources = [0u8, 1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let mut cases = Vec::new();
    let mut next_pattern = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeMap::<
        (DestinationFormat, bool, bool),
        std::collections::BTreeSet<u64>,
    >::new();
    let mut shape = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in DestinationFormat::ALL {
            if format == DestinationFormat::F16 && !has_fp16 {
                continue;
            }
            for signed in [false, true] {
                for w in [false, true] {
                    let key = (format, signed, w);
                    let patterns = format.patterns(signed, w);
                    for ll in 0..=3 {
                        for embedded_control in [false, true] {
                            if !valid_control(ll, embedded_control) {
                                continue;
                            }
                            for sample in 0..2usize {
                                let cursor = next_pattern.entry(key).or_insert(0usize);
                                let effective = patterns[*cursor % patterns.len()];
                                *cursor += 1;
                                seen.entry(key).or_default().insert(effective);
                                let upper_noise = 0xA5A5_0000_0000_0000u64
                                    ^ ((shape as u64).wrapping_mul(0x1021) << 32)
                                    ^ ((sample as u64) << 48);
                                let source_value = if w {
                                    effective
                                } else {
                                    upper_noise | (effective & u32::MAX as u64)
                                };
                                let prior_status =
                                    [0, 1, 1 << 1, 1 << 3, 1 << 5, (1 << 1) | (1 << 3)]
                                        [(shape + sample) % 6];
                                let rc = (((shape + sample * 3) & 3) as u32) << 13;
                                let daz = if (shape + sample) & 1 == 0 { 0 } else { 1 << 6 };
                                let ftz = if (shape + sample) & 2 == 0 {
                                    0
                                } else {
                                    1 << 15
                                };
                                cases.push(NativeCase {
                                    level,
                                    format,
                                    signed,
                                    w,
                                    ll,
                                    embedded_control,
                                    destination: destinations
                                        [(shape + sample * 3) % destinations.len()],
                                    merge: merges[(shape * 3 + sample * 5) % merges.len()],
                                    source: sources[(shape * 5 + sample) % sources.len()],
                                    source_value,
                                    mxcsr: 0x1F80 | prior_status | rc | daz | ftz,
                                });
                            }
                            shape += 1;
                        }
                    }
                }
            }
        }
    }

    for format in DestinationFormat::ALL {
        if format == DestinationFormat::F16 && !has_fp16 {
            continue;
        }
        for signed in [false, true] {
            for w in [false, true] {
                let key = (format, signed, w);
                assert_eq!(
                    seen.get(&key).map_or(0, std::collections::BTreeSet::len),
                    format.patterns(signed, w).len(),
                    "{format:?} signed={signed} W{} source-pattern coverage",
                    u8::from(w)
                );
            }
        }
    }
    let expected = if has_fp16 { 336 } else { 224 };
    assert_eq!(cases.len(), expected);
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_case_matrix_covers_every_available_format_register_bank_and_boundary() {
    let cases = native_cases();
    assert!(
        cases
            .iter()
            .any(|case| case.format == DestinationFormat::F32)
    );
    assert!(
        cases
            .iter()
            .any(|case| case.format == DestinationFormat::F64)
    );
    assert_eq!(
        cases
            .iter()
            .any(|case| case.format == DestinationFormat::F16),
        std::is_x86_feature_detected!("avx512fp16")
    );
    assert!(cases.iter().any(|case| case.destination >= 16));
    assert!(cases.iter().any(|case| case.merge >= 16));
    assert!(cases.iter().any(|case| case.source == 12));
    assert!(cases.iter().any(|case| case.source == 13));
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
        let bytes = encoding(
            case.format,
            case.signed,
            case.w,
            case.ll,
            case.embedded_control,
            case.destination,
            case.merge,
            case.source,
        );
        let initial = initial_state(case.source, case.source_value, case.mxcsr);
        assert_eq!(
            execute_native(&bytes, &initial, case.level),
            interpret(&bytes, &initial, case.level),
            "{case:?} bytes={bytes:02X?}"
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
        .expect("run isolated native scalar integer-to-FP differential")
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

    // Raw source replay can terminate the child with SIGILL or SIGFPE before
    // Rust reports assertion context. Bisect in O(log N) child launches and
    // report the exact guest encoding without terminating the parent binary.
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
    let bytes = encoding(
        case.format,
        case.signed,
        case.w,
        case.ll,
        case.embedded_control,
        case.destination,
        case.merge,
        case.source,
    );
    panic!(
        "isolated native scalar integer-to-FP failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_formats_controls_boundaries_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX scalar integer-to-FP differential: host lacks AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::evex_scalar_int_to_fp_replay::\
         replay_matches_o0_o2_interpretation_for_formats_controls_boundaries_and_mxcsr",
    );
}
