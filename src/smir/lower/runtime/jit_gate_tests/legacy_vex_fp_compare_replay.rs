//! Native replay coverage for register-only legacy SSE and AVX VEX
//! floating-point comparisons.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xC242;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl CompareKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn pp(self) -> u8 {
        match self {
            Self::PackedF32 => 0,
            Self::PackedF64 => 1,
            Self::ScalarF32 => 2,
            Self::ScalarF64 => 3,
        }
    }

    fn legacy_prefix(self) -> Option<u8> {
        match self {
            Self::PackedF32 => None,
            Self::PackedF64 => Some(0x66),
            Self::ScalarF32 => Some(0xF3),
            Self::ScalarF64 => Some(0xF2),
        }
    }

    fn element_bytes(self) -> usize {
        if matches!(self, Self::PackedF32 | Self::ScalarF32) {
            4
        } else {
            8
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareForm {
    Legacy,
    LegacyRex,
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

impl CompareForm {
    fn is_vex(self) -> bool {
        matches!(self, Self::VexC5 | Self::VexC4W0 | Self::VexC4W1IgnoredX)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompareCase {
    form: CompareForm,
    kind: CompareKind,
    l: bool,
    predicate: u8,
    dst: u8,
    src1: u8,
    src2: u8,
}

fn encoding(case: CompareCase) -> Vec<u8> {
    let CompareCase {
        form,
        kind,
        l,
        predicate,
        dst,
        src1,
        src2,
    } = case;
    assert!(dst < 16 && src1 < 16 && src2 < 16);
    assert!(predicate < if form.is_vex() { 32 } else { 8 });
    let pp = kind.pp();
    match form {
        CompareForm::Legacy | CompareForm::LegacyRex => {
            assert!(!l && src1 == dst);
            if form == CompareForm::Legacy {
                assert!(dst < 8 && src2 < 8);
            }
            let mut bytes = Vec::new();
            if let Some(prefix) = kind.legacy_prefix() {
                bytes.push(prefix);
            }
            if form == CompareForm::LegacyRex {
                // W and X are ignored for register forms; R and B extend the
                // destructive destination and second source.
                bytes.push(
                    0x4A | (if dst >= 8 { 0x04 } else { 0 }) | (if src2 >= 8 { 1 } else { 0 }),
                );
            }
            bytes.extend([0x0F, 0xC2, 0xC0 | ((dst & 7) << 3) | (src2 & 7), predicate]);
            bytes
        }
        CompareForm::VexC5 => {
            assert!(src2 < 8);
            vec![
                0xC5,
                (if dst < 8 { 0x80 } else { 0 })
                    | ((!src1 & 0x0F) << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                0xC2,
                0xC0 | ((dst & 7) << 3) | src2,
                predicate,
            ]
        }
        CompareForm::VexC4W0 | CompareForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if dst >= 8 {
                p0 &= !0x80;
            }
            if form == CompareForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if src2 >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if form == CompareForm::VexC4W1IgnoredX {
                    0x80
                } else {
                    0
                }) | ((!src1 & 0x0F) << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                0xC2,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
                predicate,
            ]
        }
    }
}

fn cases() -> Vec<CompareCase> {
    let mut cases = Vec::new();
    for kind in CompareKind::ALL {
        for form in [
            CompareForm::Legacy,
            CompareForm::LegacyRex,
            CompareForm::VexC5,
            CompareForm::VexC4W0,
            CompareForm::VexC4W1IgnoredX,
        ] {
            let predicates = if form.is_vex() { 0..32 } else { 0..8 };
            let lengths: &[bool] = if form.is_vex()
                && matches!(kind, CompareKind::PackedF32 | CompareKind::PackedF64)
            {
                &[false, true]
            } else {
                // Intel documents scalar VEX.L=1 as generation-dependent
                // unpredictable even though the opcode table labels L as LIG.
                &[false]
            };
            let operands: &[(u8, u8, u8)] = match form {
                CompareForm::Legacy => &[(1, 1, 3), (1, 1, 1)],
                CompareForm::LegacyRex => &[(9, 9, 11), (9, 9, 9)],
                CompareForm::VexC5 => &[(1, 2, 3), (9, 10, 3), (1, 1, 2), (1, 2, 1), (1, 1, 1)],
                CompareForm::VexC4W0 | CompareForm::VexC4W1IgnoredX => {
                    &[(1, 2, 3), (9, 10, 11), (1, 1, 2), (1, 2, 1), (1, 1, 1)]
                }
            };
            for predicate in predicates {
                for &l in lengths {
                    for &(dst, src1, src2) in operands {
                        cases.push(CompareCase {
                            form,
                            kind,
                            l,
                            predicate,
                            dst,
                            src1,
                            src2,
                        });
                    }
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
fn replay_features_require_avx_only_for_vex_encodings() {
    for (form, expected_avx) in [
        (CompareForm::LegacyRex, false),
        (CompareForm::VexC5, true),
        (CompareForm::VexC4W1IgnoredX, true),
    ] {
        let case = CompareCase {
            form,
            kind: CompareKind::ScalarF64,
            l: false,
            predicate: if form.is_vex() { 31 } else { 7 },
            dst: 9,
            src1: 9,
            src2: if form == CompareForm::VexC5 { 3 } else { 11 },
        };
        let bytes = encoding(case);
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{case:?} {bytes:02X?}");
        assert_eq!(actual.needs_avx, expected_avx, "{case:?}");
        assert!(actual.needs_avx512bw, "{case:?}");
        assert!(!actual.needs_fma, "{case:?}");
        assert!(!actual.needs_avx512vl, "{case:?}");
        assert!(!actual.needs_avx512dq, "{case:?}");
        assert!(!actual.needs_avx512fp16, "{case:?}");
        assert!(!actual.needs_avx512cd, "{case:?}");
        assert!(!actual.needs_gfni, "{case:?}");
        assert!(!actual.needs_avx512vp2intersect, "{case:?}");
        assert!(!actual.needs_vpclmulqdq, "{case:?}");
    }
}

#[test]
fn replay_admits_and_emits_6_016_optimized_legal_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 3_008);
    let mut lowered = 0usize;
    let mut fail_closed_checked = false;
    for case in cases {
        let bytes = encoding(case);
        let base = function(&bytes);
        if !fail_closed_checked && case.form.is_vex() {
            let mut missing = base.clone();
            missing.x86_instruction_bytes.clear();
            assert!(!is_native_clobber_safe(&missing));

            let mut memory_bytes = bytes.clone();
            let modrm = memory_bytes.len() - 2;
            memory_bytes[modrm] &= 0x3F;
            let mut memory = base.clone();
            memory.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
            );
            assert!(!is_native_clobber_safe(&memory));

            let mut reserved_bytes = bytes.clone();
            *reserved_bytes.last_mut().unwrap() |= 0x20;
            let mut reserved = base.clone();
            reserved.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                crate::smir::ir::X86InstructionBytes::new(&reserved_bytes).unwrap(),
            );
            assert!(!is_native_clobber_safe(&reserved));
            fail_closed_checked = true;
        }

        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = base.clone();
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(is_native_clobber_safe(&function), "{level:?} {case:?}");
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{level:?} {case:?} {bytes:02X?}"
            );
            lowered += 1;
        }
    }
    assert!(fail_closed_checked);
    assert_eq!(lowered, 6_016);

    let scalar = CompareCase {
        form: CompareForm::VexC5,
        kind: CompareKind::ScalarF32,
        l: false,
        predicate: 31,
        dst: 1,
        src1: 2,
        src2: 3,
    };
    let mut scalar_l1 = encoding(scalar);
    scalar_l1[1] |= 0x04;
    assert!(!is_native_clobber_safe(&function(&scalar_l1)));
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CompareState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u64; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0xBF80_0000,
    0x4000_0000,
    0x3F00_0000,
    0x0000_0001,
    0x8000_0001,
    0x0080_0000,
    0x7F7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0x7F81_2345,
    0x3F80_0001,
    0x3EAA_AAAB,
];

#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 16] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0xBFF0_0000_0000_0000,
    0x4000_0000_0000_0000,
    0x3FE0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x8000_0000_0000_0001,
    0x0010_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_2468_ACE0_1357,
    0x7FF0_2468_ACE0_1357,
    0x3FF0_0000_0000_0001,
    0x3FD5_5555_5555_5555,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(kind: CompareKind, register: usize) -> [u64; 8] {
    let element_size = kind.element_bytes();
    let patterns: &[u64] = if element_size == 4 {
        &F32_PATTERNS
    } else {
        &F64_PATTERNS
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element_size {
        let value = patterns[(lane + register * 5) % patterns.len()].to_le_bytes();
        let base = lane * element_size;
        bytes[base..base + element_size].copy_from_slice(&value[..element_size]);
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: CompareCase, ordinal: usize) -> CompareState {
    let prior_status = (ordinal as u32).rotate_left(3) & 0x3F;
    let rc = ((ordinal as u32 >> 2) & 3) << 13;
    let denormal_controls = if ordinal & 1 == 0 {
        0
    } else {
        (1 << 6) | (1 << 15)
    };
    CompareState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        // Rotate the low-lane operands across zeroes, infinities, denormals,
        // QNaNs, SNaNs, and finite values as predicates/forms advance.
        vectors: std::array::from_fn(|register| patterned_vector(case.kind, register + ordinal)),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_9696,
            0,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            1,
        ],
        rflags: 0x2 | 0x0CD5,
        // Keep all six SIMD exceptions masked. The CPU-level JIT boundary
        // rejects native vector execution when any mask is clear.
        mxcsr: 0x1F80 | prior_status | rc | denormal_controls,
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
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
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
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map legacy/VEX FP compare replay");
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
    CompareState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_VEX_FP_COMPARE_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[CompareCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let level = if ordinal & 1 == 0 {
            crate::smir::optimize::OptLevel::O0
        } else {
            crate::smir::optimize::OptLevel::O2
        };
        assert_eq!(
            execute_native(&bytes, &initial, level),
            interpret(&bytes, &initial, level),
            "{level:?} {case:?} {bytes:02X?}"
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
        .expect("run isolated native legacy/VEX FP compare differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 3_008);
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
        "isolated native legacy/VEX FP compare failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_all_encodings_predicates_aliases_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx")
        || !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
    {
        eprintln!("skipping native legacy/VEX FP compare differential: host lacks AVX/AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_vex_fp_compare_replay::\
         replay_matches_o0_o2_interpretation_for_all_encodings_predicates_aliases_and_mxcsr",
    );
}
