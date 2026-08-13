//! Native replay coverage for register-only AVX VEX FMA3 instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xF3A0;

fn fma_opcodes() -> impl Iterator<Item = u8> {
    (0x96..=0x9F).chain(0xA6..=0xAF).chain(0xB6..=0xBF)
}

fn encoding(
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src2: u8,
    src3: u8,
    clear_ignored_x: bool,
) -> [u8; 5] {
    assert!(fma_opcodes().any(|candidate| candidate == opcode));
    assert!(dst < 16 && src2 < 16 && src3 < 16);
    let mut p0 = 0xE2;
    if dst >= 8 {
        p0 &= !0x80;
    }
    if clear_ignored_x {
        p0 &= !0x40;
    }
    if src3 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (if w { 0x80 } else { 0 }) | ((!src2 & 0x0F) << 3) | (if l { 0x04 } else { 0 }) | 1,
        opcode,
        0xC0 | ((dst & 7) << 3) | (src3 & 7),
    ]
}

fn function(bytes: &[u8; 5]) -> crate::smir::ir::SmirFunction {
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
    let bytes = encoding(0xBF, true, true, 15, 14, 13, true);
    let function = function(&bytes);
    let requirements =
        x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(requirements.needs_fma);
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
            std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma")
        );
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma")
        );
    }

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn replay_admits_emits_o0_o2_all_families_widths_lengths_aliases_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    const OPERANDS: [(u8, u8, u8); 6] = [
        (1, 2, 3),
        (9, 10, 11),
        (1, 1, 2),
        (1, 2, 1),
        (1, 2, 2),
        (1, 1, 1),
    ];
    let mut lowered = 0usize;
    for opcode in fma_opcodes() {
        for w in [false, true] {
            for l in [false, true] {
                for (operand_index, (dst, src2, src3)) in OPERANDS.into_iter().enumerate() {
                    let bytes = encoding(opcode, w, l, dst, src2, src3, operand_index & 1 != 0);
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
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 1_440);

    let bytes = encoding(0x98, false, false, 1, 2, 3, false);
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

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct FmaState {
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
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src2: u8,
    src3: u8,
    clear_ignored_x: bool,
    data_case: usize,
}

#[cfg(target_arch = "x86_64")]
fn f32_lane(data_case: usize, role: usize, lane: usize) -> u32 {
    const CASES: [[[u32; 4]; 3]; 6] = [
        [
            [0x3F80_0000, 0xC000_0000, 0x4040_0000, 0xC080_0000],
            [0x4000_0000, 0x4040_0000, 0xC080_0000, 0xC0A0_0000],
            [0x4120_0000, 0x4130_0000, 0xC140_0000, 0xC150_0000],
        ],
        [
            [0x3F80_0001, 0x3F7F_FFFF, 0x3F80_0000, 0xBF80_0000],
            [0xBF80_0000, 0x3F80_0000, 0x3380_0000, 0xB380_0000],
            [0x3F7F_FFFF, 0x3F80_0001, 0x3F80_0000, 0x3F80_0000],
        ],
        [
            [0x7FC0_0011, 0x7F80_0001, 0x0000_0000, 0x7F80_0000],
            [0x7FC0_0022, 0x3F80_0000, 0xFF80_0000, 0x0000_0000],
            [0x7FC0_0033, 0x4000_0000, 0x7F80_0000, 0x7F80_0000],
        ],
        [
            [0x0080_0000, 0x7F7F_FFFF, 0x0000_0001, 0x8000_0000],
            [0x0000_0000, 0x0000_0000, 0x0000_0001, 0x0000_0000],
            [0x3F00_0000, 0x4000_0000, 0x3F80_0000, 0x8000_0000],
        ],
        [
            [0x3F80_0000, 0xBF80_0000, 0x3F80_0000, 0xBF80_0000],
            [0x3380_0000, 0xB380_0000, 0x3380_0000, 0xB380_0000],
            [0x3F80_0000, 0x3F80_0000, 0x3F80_0000, 0x3F80_0000],
        ],
        [
            [0x7FC0_0011, 0x0000_0001, 0x7F80_0011, 0x8000_0001],
            [0x0000_0001, 0x7FC0_0022, 0x8000_0001, 0x7F80_0022],
            [0x3F80_0000; 4],
        ],
    ];
    CASES[data_case % CASES.len()][role][lane & 3]
}

#[cfg(target_arch = "x86_64")]
fn f64_lane(data_case: usize, role: usize, lane: usize) -> u64 {
    const CASES: [[[u64; 4]; 3]; 6] = [
        [
            [
                0x3FF0_0000_0000_0000,
                0xC000_0000_0000_0000,
                0x4008_0000_0000_0000,
                0xC010_0000_0000_0000,
            ],
            [
                0x4000_0000_0000_0000,
                0x4008_0000_0000_0000,
                0xC010_0000_0000_0000,
                0xC014_0000_0000_0000,
            ],
            [
                0x4024_0000_0000_0000,
                0x4026_0000_0000_0000,
                0xC028_0000_0000_0000,
                0xC02A_0000_0000_0000,
            ],
        ],
        [
            [
                0x3FF0_0000_0000_0001,
                0x3FEF_FFFF_FFFF_FFFF,
                0x3FF0_0000_0000_0000,
                0xBFF0_0000_0000_0000,
            ],
            [
                0xBFF0_0000_0000_0000,
                0x3FF0_0000_0000_0000,
                0x3CA0_0000_0000_0000,
                0xBCA0_0000_0000_0000,
            ],
            [
                0x3FEF_FFFF_FFFF_FFFF,
                0x3FF0_0000_0000_0001,
                0x3FF0_0000_0000_0000,
                0x3FF0_0000_0000_0000,
            ],
        ],
        [
            [
                0x7FF8_0000_0000_0011,
                0x7FF0_0000_0000_0001,
                0,
                0x7FF0_0000_0000_0000,
            ],
            [
                0x7FF8_0000_0000_0022,
                0x3FF0_0000_0000_0000,
                0xFFF0_0000_0000_0000,
                0,
            ],
            [
                0x7FF8_0000_0000_0033,
                0x4000_0000_0000_0000,
                0x7FF0_0000_0000_0000,
                0x7FF0_0000_0000_0000,
            ],
        ],
        [
            [
                0x0010_0000_0000_0000,
                0x7FEF_FFFF_FFFF_FFFF,
                1,
                0x8000_0000_0000_0000,
            ],
            [0, 0, 1, 0],
            [
                0x3FE0_0000_0000_0000,
                0x4000_0000_0000_0000,
                0x3FF0_0000_0000_0000,
                0x8000_0000_0000_0000,
            ],
        ],
        [
            [
                0x3FF0_0000_0000_0000,
                0xBFF0_0000_0000_0000,
                0x3FF0_0000_0000_0000,
                0xBFF0_0000_0000_0000,
            ],
            [
                0x3CA0_0000_0000_0000,
                0xBCA0_0000_0000_0000,
                0x3CA0_0000_0000_0000,
                0xBCA0_0000_0000_0000,
            ],
            [0x3FF0_0000_0000_0000; 4],
        ],
        [
            [
                0x7FF8_0000_0000_0011,
                0x0000_0000_0000_0001,
                0x7FF0_0000_0000_0011,
                0x8000_0000_0000_0001,
            ],
            [
                0x0000_0000_0000_0001,
                0x7FF8_0000_0000_0022,
                0x8000_0000_0000_0001,
                0x7FF0_0000_0000_0022,
            ],
            [0x3FF0_0000_0000_0000; 4],
        ],
    ];
    CASES[data_case % CASES.len()][role][lane & 3]
}

#[cfg(target_arch = "x86_64")]
fn role_vector(w: bool, data_case: usize, role: usize) -> [u64; 8] {
    if w {
        std::array::from_fn(|lane| f64_lane(data_case, role, lane))
    } else {
        std::array::from_fn(|word| {
            u64::from(f32_lane(data_case, role, word * 2))
                | (u64::from(f32_lane(data_case, role, word * 2 + 1)) << 32)
        })
    }
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> FmaState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    // Deliberate assignment order makes aliases consume one shared initial
    // register value exactly as the architectural instruction does.
    vectors[usize::from(case.dst)] = role_vector(case.w, case.data_case, 0);
    vectors[usize::from(case.src2)] = role_vector(case.w, case.data_case, 1);
    vectors[usize::from(case.src3)] = role_vector(case.w, case.data_case, 2);

    let rc = ((usize::from(case.opcode) + case.data_case + usize::from(case.l)) & 3) as u32;
    let mut mxcsr = 0x1F80 | (rc << 13);
    if case.data_case == 3 {
        mxcsr |= (1 << 6) | (1 << 15); // DAZ and FTZ.
    }
    if case.data_case == 1 {
        mxcsr |= 1 << 5; // Preserve an existing sticky precision flag.
    }

    FmaState {
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
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn optimized_function(
    bytes: &[u8; 5],
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
    bytes: &[u8; 5],
    initial: &FmaState,
    level: crate::smir::optimize::OptLevel,
) -> FmaState {
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
    FmaState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 5],
    initial: &FmaState,
    level: crate::smir::optimize::OptLevel,
) -> FmaState {
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
    let exec = ExecMem::new(&code).expect("map VEX FMA3 replay");
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
    FmaState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const OPERANDS: [(u8, u8, u8); 6] = [
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
        for opcode in fma_opcodes() {
            for w in [false, true] {
                for l in [false, true] {
                    let (dst, src2, src3) = OPERANDS[ordinal % OPERANDS.len()];
                    cases.push(NativeCase {
                        level,
                        opcode,
                        w,
                        l,
                        dst,
                        src2,
                        src3,
                        clear_ignored_x: ordinal & 1 != 0,
                        data_case: ordinal % 6,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FMA3_CHILD_RANGE";

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
            case.opcode,
            case.w,
            case.l,
            case.dst,
            case.src2,
            case.src3,
            case.clear_ignored_x,
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
        .expect("run isolated native VEX FMA3 differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 240);
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
        case.opcode,
        case.w,
        case.l,
        case.dst,
        case.src2,
        case.src3,
        case.clear_ignored_x,
    );
    panic!(
        "isolated native VEX FMA3 failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_all_families_widths_aliases_mxcsr_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("fma") {
        eprintln!("skipping native VEX FMA3 differential: host lacks AVX/FMA");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fma3_replay::\
         replay_matches_o0_o2_interpretation_for_all_families_widths_aliases_mxcsr_and_full_state",
    );
}
