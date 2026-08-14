//! Native replay coverage for register-only AVX VEX packed-string compares.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x6063;
const STATUS_FLAGS: u64 = 0x08D5;

fn encoding(opcode: u8, w: bool, src1: u8, src2: u8, imm: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x60..=0x63));
    assert!(src1 < 16 && src2 < 16);
    let mut p0 = 0xE3;
    if src1 >= 8 {
        p0 &= !0x80;
    }
    if src2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (if w { 0x80 } else { 0 }) | 0x79,
        opcode,
        0xC0 | ((src1 & 7) << 3) | (src2 & 7),
        imm,
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
fn replay_feature_aggregation_requires_only_the_avx_ymm16_state_boundary() {
    let bytes = encoding(0x60, true, 15, 8, 0xFF);
    let function = function(&bytes);
    let requirements =
        x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
    assert!(requirements.any);
    assert!(requirements.needs_avx);
    assert!(!requirements.needs_sse42);
    assert!(!requirements.needs_fma);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_avx512cd);
    assert!(!requirements.needs_gfni);
    assert!(!requirements.needs_avx512vp2intersect);
    assert!(!requirements.needs_vpclmulqdq);

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
    );

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn replay_admits_emits_o0_o2_register_forms_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [(1, 2), (9, 10), (15, 15), (0, 1), (1, 0)];
    let mut lowered = 0usize;
    for opcode in 0x60..=0x63 {
        for w in [false, true] {
            for (src1, src2) in operands {
                for imm in [0x00, 0x40, 0x7F, 0xFF] {
                    let bytes = encoding(opcode, w, src1, src2, imm);
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
    assert_eq!(lowered, 320);

    let bytes = encoding(0x60, false, 2, 1, 0);
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
struct PackedStringState {
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
    src1: u8,
    src2: u8,
    imm: u8,
    data_case: usize,
    length_case: usize,
}

#[cfg(target_arch = "x86_64")]
fn input_pair(index: usize) -> ([u8; 16], [u8; 16]) {
    const INPUTS: [([u8; 16], [u8; 16]); 6] = [
        (*b"abc\0ABCDEFGHIJKL", *b"xbycz\0ABCDEFGHIJ"),
        (
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (
            [
                0x80, 0xFF, 0x7F, 0, 0x81, 1, 0xFE, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            ],
            [
                0x80, 0x7F, 0xFF, 1, 0x82, 2, 0xFD, 3, 4, 5, 6, 7, 8, 9, 10, 0,
            ],
        ),
        (
            [1, 0, 2, 0, 0, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0],
            [2, 0, 1, 0, 3, 0, 0, 0, 5, 0, 4, 0, 7, 0, 6, 0],
        ),
        (
            [0xFE, 0xFF, 2, 0, 0, 0, 4, 0, 6, 0, 8, 0, 10, 0, 12, 0],
            [0xFD, 0xFF, 0xFE, 0xFF, 2, 0, 0, 0, 3, 0, 5, 0, 7, 0, 9, 0],
        ),
        ([0xFF; 16], [0; 16]),
    ];
    INPUTS[index % INPUTS.len()]
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase) -> PackedStringState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    let (first, second) = input_pair(case.data_case);
    vectors[usize::from(case.src1)][0] = u64::from_le_bytes(first[..8].try_into().unwrap());
    vectors[usize::from(case.src1)][1] = u64::from_le_bytes(first[8..].try_into().unwrap());
    vectors[usize::from(case.src2)][0] = u64::from_le_bytes(second[..8].try_into().unwrap());
    vectors[usize::from(case.src2)][1] = u64::from_le_bytes(second[8..].try_into().unwrap());

    let mut gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
    });
    const LENGTHS: [(u64, u64); 6] = [
        (3, 5),
        ((-3i64) as u64, 5),
        (0x0000_0001_0000_0003, 0x0000_0001_0000_0005),
        (i64::MIN as u64, i64::MAX as u64),
        (u64::MAX, 0),
        (16, 8),
    ];
    let (rax, rdx) = LENGTHS[case.length_case % LENGTHS.len()];
    gprs[0] = rax;
    gprs[2] = rdx;

    PackedStringState {
        gprs,
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
        mxcsr: 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15),
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
    initial: &PackedStringState,
    level: crate::smir::optimize::OptLevel,
) -> PackedStringState {
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
    PackedStringState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &PackedStringState,
    level: crate::smir::optimize::OptLevel,
) -> PackedStringState {
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
    let exec = ExecMem::new(&code).expect("map VEX packed-string replay");
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
    PackedStringState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const IMMEDIATES: [u8; 13] = [
        0x00, 0x01, 0x02, 0x03, 0x0C, 0x18, 0x24, 0x30, 0x40, 0x47, 0x7F, 0x80, 0xFF,
    ];
    const OPERANDS: [(u8, u8); 6] = [(1, 2), (9, 10), (15, 15), (0, 1), (1, 0), (0, 0)];
    let mut cases = Vec::new();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for opcode in 0x60..=0x63 {
            for w in [false, true] {
                for (index, imm) in IMMEDIATES.into_iter().enumerate() {
                    let (src1, src2) = OPERANDS
                        [(index + usize::from(opcode - 0x60) + usize::from(w)) % OPERANDS.len()];
                    cases.push(NativeCase {
                        level,
                        opcode,
                        w,
                        src1,
                        src2,
                        imm,
                        data_case: index + usize::from(opcode - 0x60),
                        length_case: index + usize::from(w),
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PACKED_STRING_CHILD_RANGE";

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
        let bytes = encoding(case.opcode, case.w, case.src1, case.src2, case.imm);
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
        .expect("run isolated native VEX packed-string differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 208);
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
    let bytes = encoding(case.opcode, case.w, case.src1, case.src2, case.imm);
    panic!(
        "isolated native VEX packed-string failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_modes_lengths_aliases_flags_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed-string differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_packed_string_replay::\
         replay_matches_o0_o2_interpretation_for_modes_lengths_aliases_flags_and_full_state",
    );
}
