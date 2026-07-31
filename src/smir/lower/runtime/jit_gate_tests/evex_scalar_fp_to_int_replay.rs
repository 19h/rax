//! Native replay coverage for EVEX scalar floating-point-to-integer conversion.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2D79;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SourceFormat {
    F32,
    F64,
    F16,
}

impl SourceFormat {
    const ALL: [Self; 3] = [Self::F32, Self::F64, Self::F16];

    fn fields(self) -> (u8, u8, bool) {
        match self {
            Self::F32 => (1, 2, false),
            Self::F64 => (1, 3, false),
            Self::F16 => (5, 2, true),
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn patterns(self) -> &'static [u64] {
        match self {
            Self::F32 => &[
                0x0000_0000,
                0x8000_0000,
                0x0000_0001,
                0x007F_FFFF,
                0x3F00_0000,
                0xBF00_0000,
                0x3FC0_0000,
                0xBFC0_0000,
                0x4020_0000,
                0xC020_0000,
                0x4EFF_FFFF,
                0x4F00_0000,
                0xCF00_0000,
                0x4F7F_FFFF,
                0x4F80_0000,
                0x5F00_0000,
                0xDF00_0000,
                0x5F80_0000,
                0x7F7F_FFFF,
                0x7F80_0000,
                0xFF80_0000,
                0x7FC1_2345,
                0xFFC1_2345,
                0x7F81_2345,
            ],
            Self::F64 => &[
                0x0000_0000_0000_0000,
                0x8000_0000_0000_0000,
                0x0000_0000_0000_0001,
                0x000F_FFFF_FFFF_FFFF,
                0x3FE0_0000_0000_0000,
                0xBFE0_0000_0000_0000,
                0x3FF8_0000_0000_0000,
                0xBFF8_0000_0000_0000,
                0x4004_0000_0000_0000,
                0xC004_0000_0000_0000,
                0x41DF_FFFF_FFC0_0000,
                0x41E0_0000_0000_0000,
                0xC1E0_0000_0000_0000,
                0x41EF_FFFF_FFE0_0000,
                0x41F0_0000_0000_0000,
                0x43DF_FFFF_FFFF_FFFF,
                0x43E0_0000_0000_0000,
                0xC3E0_0000_0000_0000,
                0x43EF_FFFF_FFFF_FFFF,
                0x43F0_0000_0000_0000,
                0x7FEF_FFFF_FFFF_FFFF,
                0x7FF0_0000_0000_0000,
                0xFFF0_0000_0000_0000,
                0x7FF8_1234_5678_9ABC,
                0x7FF0_1234_5678_9ABC,
            ],
            Self::F16 => &[
                0x0000, 0x8000, 0x0001, 0x03FF, 0x3800, 0xB800, 0x3E00, 0xBE00, 0x4100, 0xC100,
                0x7BFF, 0xFBFF, 0x7C00, 0xFC00, 0x7E55, 0x7D55,
            ],
        }
    }
}

fn encoding(
    format: SourceFormat,
    signed: bool,
    truncate: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    source: u8,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 16 && source < 32);
    let (map, pp, _) = format.fields();
    let opcode = match (signed, truncate) {
        (true, false) => 0x2D,
        (true, true) => 0x2C,
        (false, false) => 0x79,
        (false, true) => 0x78,
    };
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
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
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x08,
        opcode,
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
fn replay_feature_aggregation_requires_fp16_only_for_binary16_sources() {
    for format in SourceFormat::ALL {
        let bytes = encoding(format, false, false, true, 3, true, 15, 31);
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
fn replay_admits_and_emits_336_o0_o2_safe_semantic_shapes_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let destinations = [0u8, 3, 6, 7, 8, 9, 12, 15];
    let sources = [0u8, 7, 8, 15, 16, 23, 24, 31];
    let mut lowered = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in SourceFormat::ALL {
            for signed in [false, true] {
                for truncate in [false, true] {
                    for w in [false, true] {
                        for ll in 0..=3 {
                            for embedded_control in [false, true] {
                                if !valid_control(ll, embedded_control) {
                                    continue;
                                }
                                let destination = destinations[lowered % destinations.len()];
                                let source = sources[(lowered * 5 + 1) % sources.len()];
                                let bytes = encoding(
                                    format,
                                    signed,
                                    truncate,
                                    w,
                                    ll,
                                    embedded_control,
                                    destination,
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
    }
    assert_eq!(lowered, 336);

    let replay_only = encoding(SourceFormat::F16, false, true, true, 2, true, 15, 31);
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

    for destination in [4, 5] {
        for format in SourceFormat::ALL {
            let bytes = encoding(format, false, true, true, 2, true, destination, 31);
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
fn initial_state(
    format: SourceFormat,
    source: u8,
    source_bits: u64,
    mxcsr: u32,
) -> ConversionState {
    let mut vectors: [[u64; 8]; 32] = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    match format {
        SourceFormat::F16 => {
            vectors[source as usize][0] =
                (vectors[source as usize][0] & !0xFFFF) | (source_bits & 0xFFFF);
        }
        SourceFormat::F32 => {
            vectors[source as usize][0] =
                (vectors[source as usize][0] & !0xFFFF_FFFF) | (source_bits & 0xFFFF_FFFF);
        }
        SourceFormat::F64 => vectors[source as usize][0] = source_bits,
    }
    ConversionState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
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
    let exec = ExecMem::new(&code).expect("map EVEX scalar FP-to-int replay");
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
const CHILD_RANGE_ENV: &str = "RAX_SCALAR_FP_TO_INT_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    format: SourceFormat,
    signed: bool,
    truncate: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    source: u8,
    source_bits: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let destinations = [0u8, 3, 6, 7, 8, 9, 12, 15];
    let sources = [0u8, 7, 8, 15, 16, 23, 24, 31];
    let mut cases = Vec::new();
    let mut seen =
        std::collections::BTreeMap::<SourceFormat, std::collections::BTreeSet<u64>>::new();
    let mut shape = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in SourceFormat::ALL {
            if format == SourceFormat::F16 && !has_fp16 {
                continue;
            }
            for signed in [false, true] {
                for truncate in [false, true] {
                    for w in [false, true] {
                        for ll in 0..=3 {
                            for embedded_control in [false, true] {
                                if !valid_control(ll, embedded_control) {
                                    continue;
                                }
                                for sample in 0..2usize {
                                    let patterns = format.patterns();
                                    let source_bits =
                                        patterns[(shape * 2 + sample) % patterns.len()];
                                    seen.entry(format).or_default().insert(source_bits);
                                    let prior_status = [0, 1 << 1, 1 << 3, (1 << 1) | (1 << 3)]
                                        [(shape + sample) & 3];
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
                                        truncate,
                                        w,
                                        ll,
                                        embedded_control,
                                        destination: destinations
                                            [(shape + sample * 3) % destinations.len()],
                                        source: sources[(shape * 5 + sample) % sources.len()],
                                        source_bits,
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
    }

    for format in SourceFormat::ALL {
        if format != SourceFormat::F16 || has_fp16 {
            assert_eq!(
                seen.get(&format).map_or(0, std::collections::BTreeSet::len),
                format.patterns().len(),
                "{format:?} source-pattern coverage"
            );
        }
    }
    let expected = if has_fp16 { 672 } else { 448 };
    assert_eq!(cases.len(), expected);
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_case_matrix_covers_every_available_format_and_source_pattern() {
    let cases = native_cases();
    assert!(cases.iter().any(|case| case.format == SourceFormat::F32));
    assert!(cases.iter().any(|case| case.format == SourceFormat::F64));
    assert_eq!(
        cases.iter().any(|case| case.format == SourceFormat::F16),
        std::is_x86_feature_detected!("avx512fp16")
    );
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
            case.truncate,
            case.w,
            case.ll,
            case.embedded_control,
            case.destination,
            case.source,
        );
        let initial = initial_state(case.format, case.source, case.source_bits, case.mxcsr);
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
        .expect("run isolated native scalar FP-to-int differential")
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
        case.truncate,
        case.w,
        case.ll,
        case.embedded_control,
        case.destination,
        case.source,
    );
    panic!(
        "isolated native scalar FP-to-int failure at case {start}/{}: \
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
        eprintln!("skipping native EVEX scalar FP-to-int differential: host lacks AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::evex_scalar_fp_to_int_replay::\
         replay_matches_o0_o2_interpretation_for_formats_controls_boundaries_and_mxcsr",
    );
}
