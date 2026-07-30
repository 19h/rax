//! Native replay coverage for defined register-only VEX scalar flag compares.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2F2E;
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    F32,
    F64,
}

impl Format {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        u8::from(self == Self::F64)
    }

    fn bit_mask(self) -> u64 {
        match self {
            Self::F32 => u64::from(u32::MAX),
            Self::F64 => u64::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4 { w: bool, ignored_x_clear: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompareCase {
    format: Format,
    opcode: u8,
    form: VexForm,
    first: u8,
    second: u8,
}

fn encoding(case: CompareCase) -> Vec<u8> {
    let CompareCase {
        format,
        opcode,
        form,
        first,
        second,
    } = case;
    assert!(matches!(opcode, 0x2E | 0x2F));
    assert!(first < 16 && second < 16);
    let modrm = 0xC0 | ((first & 7) << 3) | (second & 7);
    match form {
        VexForm::C5 => {
            assert!(second < 8, "C5 has no VEX.B extension");
            vec![
                0xC5,
                (if first < 8 { 0x80 } else { 0 }) | 0x78 | format.pp(),
                opcode,
                modrm,
            ]
        }
        VexForm::C4 { w, ignored_x_clear } => {
            let mut p0 = 0xE1;
            if first >= 8 {
                p0 &= !0x80;
            }
            if ignored_x_clear {
                p0 &= !0x40;
            }
            if second >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (u8::from(w) << 7) | 0x78 | format.pp(),
                opcode,
                modrm,
            ]
        }
    }
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
fn replay_features_select_avx_and_the_ymm16_state_boundary() {
    for case in [
        CompareCase {
            format: Format::F32,
            opcode: 0x2F,
            form: VexForm::C5,
            first: 1,
            second: 2,
        },
        CompareCase {
            format: Format::F64,
            opcode: 0x2E,
            form: VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            first: 9,
            second: 10,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_f16c, "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
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
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx"),
            "{case:?}"
        );
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
fn replay_emission_covers_all_4608_defined_register_images_at_o0_and_o2() {
    let levels = [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ];
    let mut images = 0usize;

    for encoded_r in [false, true] {
        for pp in 0u8..=1 {
            for opcode in [0x2Eu8, 0x2F] {
                for modrm in 0xC0u8..=0xFF {
                    let bytes = [0xC5, (u8::from(encoded_r) << 7) | 0x78 | pp, opcode, modrm];
                    for level in levels {
                        assert_admitted_and_emitted(&bytes, level);
                    }
                    images += 1;
                }
            }
        }
    }

    for extension_bits in (0u8..8).map(|value| value << 5) {
        for w in [false, true] {
            for pp in 0u8..=1 {
                for opcode in [0x2Eu8, 0x2F] {
                    for modrm in 0xC0u8..=0xFF {
                        let bytes = [
                            0xC4,
                            extension_bits | 1,
                            (u8::from(w) << 7) | 0x78 | pp,
                            opcode,
                            modrm,
                        ];
                        for level in levels {
                            assert_admitted_and_emitted(&bytes, level);
                        }
                        images += 1;
                    }
                }
            }
        }
    }

    assert_eq!(images, 4_608);
}

#[test]
fn replay_fails_closed_without_exact_defined_source_provenance() {
    let bytes = encoding(CompareCase {
        format: Format::F64,
        opcode: 0x2F,
        form: VexForm::C4 {
            w: true,
            ignored_x_clear: true,
        },
        first: 9,
        second: 10,
    });
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    for invalid in [
        {
            let mut value = bytes.clone();
            value[2] &= !0x08; // VEX.vvvv != 1111b
            value
        },
        {
            let mut value = bytes.clone();
            value[1] = (value[1] & 0xE0) | 2; // map 0F38
            value
        },
        {
            let mut value = bytes.clone();
            value[4] &= 0x3F; // memory source
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

    let mut l1 = bytes.clone();
    l1[2] |= 0x04;
    let l1_function = function(&l1);
    assert!(is_native_clobber_safe(&l1_function));
    let spans = crate::smir::ir::x86_native_replay_spans(
        &l1_function.blocks[0],
        &l1_function.x86_instruction_bytes,
    );
    assert_eq!(
        spans
            .get(&0)
            .expect("canonical VEX flag-compare span")
            .instruction,
        crate::smir::ir::X86InstructionBytes::new(&bytes).unwrap()
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompareState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(
    format: Format,
    first_register: usize,
    first: u64,
    second_register: usize,
    second: u64,
    mxcsr: u32,
) -> CompareState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        })
    });
    let mask = format.bit_mask();
    vectors[first_register][0] = (vectors[first_register][0] & !mask) | (first & mask);
    vectors[second_register][0] = (vectors[second_register][0] & !mask) | (second & mask);
    CompareState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
        }),
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_9696,
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
    initial: &CompareState,
    level: crate::smir::optimize::OptLevel,
) -> CompareState {
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
    context.flags.materialize_all();

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    CompareState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_matches_primary_flag_truth_table_and_nan_status_policy() {
    let cases = [
        (0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000, 1 << 6, 0),
        (0x3FF0_0000_0000_0000, 0x4000_0000_0000_0000, 1, 0),
        (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000, 0, 0),
        (
            0x7FF8_0000_0000_0001,
            0x3FF0_0000_0000_0000,
            (1 << 6) | (1 << 2) | 1,
            1,
        ),
        (
            0x7FF0_0000_0000_0001,
            0x3FF0_0000_0000_0000,
            (1 << 6) | (1 << 2) | 1,
            1,
        ),
    ];
    for opcode in [0x2Eu8, 0x2F] {
        for (first, second, expected_flags, signaling_status) in cases {
            let bytes = encoding(CompareCase {
                format: Format::F64,
                opcode,
                form: VexForm::C4 {
                    w: true,
                    ignored_x_clear: true,
                },
                first: 9,
                second: 10,
            });
            let initial = initial_state(Format::F64, 9, first, 10, second, 0x1F80);
            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let actual = interpret(&bytes, &initial, level);
                assert_eq!(
                    actual.rflags & STATUS_FLAGS,
                    expected_flags,
                    "{level:?} opcode={opcode:#04X} first={first:#018X}"
                );
                let expected_invalid = if first == 0x7FF8_0000_0000_0001 {
                    u32::from(opcode == 0x2F)
                } else {
                    signaling_status
                };
                assert_eq!(
                    actual.mxcsr & 1,
                    expected_invalid,
                    "{level:?} opcode={opcode:#04X} first={first:#018X}"
                );
                assert_eq!(actual.gprs, initial.gprs);
                assert_eq!(actual.vectors, initial.vectors);
                assert_eq!(actual.masks, initial.masks);
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &CompareState,
    level: crate::smir::optimize::OptLevel,
) -> CompareState {
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
    let exec = ExecMem::new(&code).expect("map VEX scalar flag-compare replay");
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
    CompareState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP_FLAG_COMPARE_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    instruction: CompareCase,
    first: u64,
    second: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn values(format: Format) -> [(u64, u64); 10] {
    match format {
        Format::F32 => [
            (0x3F80_0000, 0x3F80_0000),
            (0x3F80_0000, 0x4000_0000),
            (0x4000_0000, 0x3F80_0000),
            (0x0000_0000, 0x8000_0000),
            (0x7FC0_0001, 0x3F80_0000),
            (0x7F80_0001, 0x3F80_0000),
            (0x0000_0001, 0x0000_0000),
            (0x8000_0001, 0x8000_0000),
            (0x7F80_0000, 0x7F80_0000),
            (0xBF80_0000, 0xBF80_0000),
        ],
        Format::F64 => [
            (0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x3FF0_0000_0000_0000, 0x4000_0000_0000_0000),
            (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
            (0x7FF8_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x7FF0_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x0000_0000_0000_0001, 0x0000_0000_0000_0000),
            (0x8000_0000_0000_0001, 0x8000_0000_0000_0000),
            (0x7FF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (0xBFF0_0000_0000_0000, 0xBFF0_0000_0000_0000),
        ],
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let forms = [
        (VexForm::C5, 1, 2),
        (VexForm::C5, 9, 2),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            1,
            2,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            9,
            10,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: false,
            },
            15,
            15,
        ),
    ];
    let mut cases = Vec::new();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in Format::ALL {
            for opcode in [0x2E, 0x2F] {
                for (form, first_register, second_register) in forms {
                    for (ordinal, (first, mut second)) in values(format).into_iter().enumerate() {
                        if first_register == second_register {
                            second = first;
                        }
                        let prior_status = 1 << (ordinal % 6);
                        let rc = ((ordinal as u32) & 3) << 13;
                        let daz_ftz = if ordinal & 1 == 0 {
                            0
                        } else {
                            (1 << 6) | (1 << 15)
                        };
                        cases.push(NativeCase {
                            level,
                            instruction: CompareCase {
                                format,
                                opcode,
                                form,
                                first: first_register,
                                second: second_register,
                            },
                            first,
                            second,
                            mxcsr: 0x1F80 | prior_status | rc | daz_ftz,
                        });
                    }
                }
            }
        }
    }
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
        let bytes = encoding(case.instruction);
        let initial = initial_state(
            case.instruction.format,
            usize::from(case.instruction.first),
            case.first,
            usize::from(case.instruction.second),
            case.second,
            case.mxcsr,
        );
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
        .expect("run isolated native VEX scalar flag-compare differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 400);
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
    let bytes = encoding(case.instruction);
    panic!(
        "isolated native VEX scalar flag-compare failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_flags_nan_daz_aliases_wig_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar flag-compare differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp_flag_compare_replay::\
         replay_matches_o0_o2_interpretation_for_flags_nan_daz_aliases_wig_and_full_state",
    );
}
