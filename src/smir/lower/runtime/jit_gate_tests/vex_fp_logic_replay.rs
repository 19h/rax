//! Native replay coverage for register-only AVX VEX floating logical instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5457;

#[derive(Clone, Copy, Debug)]
enum EncodingForm {
    C5,
    C4W0,
    C4W1IgnoredX,
}

fn encoding(
    form: EncodingForm,
    opcode: u8,
    pp: u8,
    l: bool,
    dst: u8,
    src1: u8,
    src2: u8,
) -> Vec<u8> {
    assert!(matches!(opcode, 0x54..=0x57));
    assert!(pp <= 1);
    assert!(dst < 16 && src1 < 16 && src2 < 16);
    match form {
        EncodingForm::C5 => {
            assert!(src2 < 8);
            vec![
                0xC5,
                (if dst < 8 { 0x80 } else { 0 })
                    | ((!src1 & 0x0F) << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                opcode,
                0xC0 | ((dst & 7) << 3) | src2,
            ]
        }
        EncodingForm::C4W0 | EncodingForm::C4W1IgnoredX => {
            let mut p0 = 0xE1;
            if dst >= 8 {
                p0 &= !0x80;
            }
            if matches!(form, EncodingForm::C4W1IgnoredX) {
                p0 &= !0x40;
            }
            if src2 >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if matches!(form, EncodingForm::C4W1IgnoredX) {
                    0x80
                } else {
                    0
                }) | ((!src1 & 0x0F) << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                opcode,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
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
fn replay_feature_aggregation_uses_avx_ymm16_boundary() {
    let bytes = encoding(EncodingForm::C4W1IgnoredX, 0x57, 1, true, 15, 14, 13);
    let function = function(&bytes);
    let requirements =
        x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_avx512cd);
    assert!(!requirements.needs_gfni);
    assert!(!requirements.needs_avx512vp2intersect);
    assert!(!requirements.needs_vpclmulqdq);

    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx")
        );
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx")
        );
    }

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn replay_admits_and_emits_exact_c4_c5_bytes_at_o0_o2_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    const LOW_OPERANDS: [(u8, u8, u8); 6] = [
        (1, 2, 3),
        (9, 10, 3),
        (1, 1, 2),
        (1, 2, 1),
        (1, 2, 2),
        (1, 1, 1),
    ];
    const FULL_OPERANDS: [(u8, u8, u8); 6] = [
        (1, 2, 3),
        (9, 10, 11),
        (1, 1, 2),
        (1, 2, 1),
        (1, 2, 2),
        (1, 1, 1),
    ];
    let mut lowered = 0usize;
    for opcode in 0x54..=0x57 {
        for pp in 0..=1 {
            for l in [false, true] {
                for form in [
                    EncodingForm::C5,
                    EncodingForm::C4W0,
                    EncodingForm::C4W1IgnoredX,
                ] {
                    let operands = if matches!(form, EncodingForm::C5) {
                        LOW_OPERANDS
                    } else {
                        FULL_OPERANDS
                    };
                    for (dst, src1, src2) in operands {
                        let bytes = encoding(form, opcode, pp, l, dst, src1, src2);
                        for level in [
                            crate::smir::optimize::OptLevel::O0,
                            crate::smir::optimize::OptLevel::O2,
                        ] {
                            let mut function = function(&bytes);
                            crate::smir::optimize::optimize_function(&mut function, level);
                            assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                            assert!(
                                uses_x86_native_vectors_excluding(
                                    &function,
                                    &std::collections::HashMap::new()
                                ),
                                "{level:?} {bytes:02X?}"
                            );

                            let mut lowerer = X86_64Lowerer::new();
                            lowerer.lower_function(&function).unwrap_or_else(|error| {
                                panic!("{level:?} {bytes:02X?}: {error:?}")
                            });
                            let code = lowerer.finalize().unwrap_or_else(|error| {
                                panic!("{level:?} {bytes:02X?}: {error:?}")
                            });
                            assert!(
                                code.windows(bytes.len()).any(|window| window == bytes),
                                "{level:?} {bytes:02X?}"
                            );
                            lowered += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 576);

    let bytes = encoding(EncodingForm::C5, 0x54, 0, false, 1, 2, 3);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &missing.blocks[0],
            &missing.x86_instruction_bytes
        )
        .is_empty()
    );

    let mut memory_bytes = bytes.clone();
    *memory_bytes.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &memory_metadata.blocks[0],
            &memory_metadata.x86_instruction_bytes
        )
        .is_empty()
    );
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct LogicState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    form: EncodingForm,
    opcode: u8,
    pp: u8,
    l: bool,
    dst: u8,
    src1: u8,
    src2: u8,
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> LogicState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    vectors[usize::from(case.dst)] =
        std::array::from_fn(|word| 0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7) as u32));
    vectors[usize::from(case.src1)] =
        std::array::from_fn(|word| 0x00FF_F0F0_3333_5555u64.rotate_left((word * 9) as u32));
    vectors[usize::from(case.src2)] =
        std::array::from_fn(|word| 0xFF00_0F0F_CCCC_AAAAu64.rotate_right((word * 13) as u32));

    LogicState {
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: 0xDFC5,
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
    initial: &LogicState,
    level: crate::smir::optimize::OptLevel,
) -> LogicState {
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
    LogicState {
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
    initial: &LogicState,
    level: crate::smir::optimize::OptLevel,
) -> LogicState {
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
    let exec = ExecMem::new(&code).expect("map VEX floating logic replay");
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
    LogicState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const LOW_OPERANDS: [(u8, u8, u8); 6] = [
        (1, 2, 3),
        (9, 10, 3),
        (1, 1, 2),
        (1, 2, 1),
        (1, 2, 2),
        (1, 1, 1),
    ];
    const FULL_OPERANDS: [(u8, u8, u8); 6] = [
        (1, 2, 3),
        (9, 10, 11),
        (1, 1, 2),
        (1, 2, 1),
        (1, 2, 2),
        (1, 1, 1),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for opcode in 0x54..=0x57 {
            for pp in 0..=1 {
                for l in [false, true] {
                    for form in [
                        EncodingForm::C5,
                        EncodingForm::C4W0,
                        EncodingForm::C4W1IgnoredX,
                    ] {
                        let operands = if matches!(form, EncodingForm::C5) {
                            LOW_OPERANDS
                        } else {
                            FULL_OPERANDS
                        };
                        let (dst, src1, src2) = operands[ordinal % operands.len()];
                        cases.push(NativeCase {
                            level,
                            form,
                            opcode,
                            pp,
                            l,
                            dst,
                            src1,
                            src2,
                        });
                        ordinal += 1;
                    }
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP_LOGIC_CHILD_RANGE";

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
    for &case in &cases[range] {
        let bytes = encoding(
            case.form,
            case.opcode,
            case.pp,
            case.l,
            case.dst,
            case.src1,
            case.src2,
        );
        let initial = initial_state(case);
        assert_eq!(
            execute_native(&bytes, &initial, case.level),
            interpret(&bytes, &initial, case.level),
            "{case:?} {bytes:02X?}"
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
        .expect("run isolated native VEX floating logic differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 96);
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
    let bytes = encoding(
        case.form,
        case.opcode,
        case.pp,
        case.l,
        case.dst,
        case.src1,
        case.src2,
    );
    panic!(
        "isolated native VEX floating logic failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_c4_c5_widths_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX floating logic differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp_logic_replay::\
         replay_matches_o0_o2_interpretation_for_c4_c5_widths_aliases_and_full_state",
    );
}
