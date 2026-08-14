//! Native replay coverage for legacy MMX/XMM MOVD/MOVQ transfers whose GPR
//! operand is guest RSP or RBP. Semantics follow Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2B, `MOVD/MOVQ`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5060;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferKind {
    GprToMmx,
    MmxToGpr,
    GprToXmm,
    XmmToGpr,
}

impl TransferKind {
    const ALL: [Self; 4] = [
        Self::GprToMmx,
        Self::MmxToGpr,
        Self::GprToXmm,
        Self::XmmToGpr,
    ];

    fn mmx(self) -> bool {
        matches!(self, Self::GprToMmx | Self::MmxToGpr)
    }

    fn vector_destination(self) -> bool {
        matches!(self, Self::GprToMmx | Self::GprToXmm)
    }

    fn opcode(self) -> u8 {
        if self.vector_destination() {
            0x6E
        } else {
            0x7E
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransferCase {
    kind: TransferKind,
    rex: Option<u8>,
    gpr: u8,
    encoded_vector: u8,
}

impl TransferCase {
    fn vector(self) -> u8 {
        if self.kind.mmx() {
            self.encoded_vector
        } else {
            ((self.rex.unwrap_or(0) >> 2) & 1) << 3 | self.encoded_vector
        }
    }

    fn width(self) -> crate::smir::ir::types::OpWidth {
        if self.rex.unwrap_or(0) & 8 == 0 {
            crate::smir::ir::types::OpWidth::W32
        } else {
            crate::smir::ir::types::OpWidth::W64
        }
    }
}

fn encoding(case: TransferCase) -> Vec<u8> {
    assert!(matches!(case.gpr, 4 | 5));
    assert!(case.encoded_vector < 8);
    let mut bytes = Vec::with_capacity(5);
    if !case.kind.mmx() {
        bytes.push(0x66);
    }
    if let Some(rex) = case.rex {
        bytes.push(rex);
    }
    bytes.extend_from_slice(&[
        0x0F,
        case.kind.opcode(),
        0xC0 | (case.encoded_vector << 3) | case.gpr,
    ]);
    bytes
}

fn exhaustive_cases() -> Vec<TransferCase> {
    let mut cases = Vec::with_capacity(576);
    for kind in TransferKind::ALL {
        for gpr in [4, 5] {
            for encoded_vector in 0..8 {
                cases.push(TransferCase {
                    kind,
                    rex: None,
                    gpr,
                    encoded_vector,
                });
            }
        }
        for rex in [0x40, 0x42, 0x44, 0x46, 0x48, 0x4A, 0x4C, 0x4E] {
            for gpr in [4, 5] {
                for encoded_vector in 0..8 {
                    cases.push(TransferCase {
                        kind,
                        rex: Some(rex),
                        gpr,
                        encoded_vector,
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), 576);
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

fn expected_replay_bytes(case: TransferCase, bytes: &[u8]) -> Vec<u8> {
    let mut rewritten = bytes.to_vec();
    *rewritten.last_mut().unwrap() &= !0x07;

    if case.kind.vector_destination() {
        let mut expected = vec![
            0x50,
            0x48,
            0x8B,
            0x45,
            X86_STATE_PTR_AT_RBP as u8,
            0x48,
            0x8B,
            0x40,
            case.gpr * 8,
        ];
        expected.extend_from_slice(&rewritten);
        expected.push(0x58);
        expected
    } else {
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
            case.gpr * 8,
        ]);
        if case.gpr == 5 {
            expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
        }
        expected.extend_from_slice(&[0x59, 0x58]);
        expected
    }
}

fn assert_replay_emitted(code: &[u8], case: TransferCase, bytes: &[u8]) {
    let expected = expected_replay_bytes(case, bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?} source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn replay_features_keep_mmx_independent_and_select_avx_ymm16_for_xmm() {
    for case in exhaustive_cases() {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(is_native_clobber_safe(&function), "{case:?}");
        assert!(
            x86_native_mmx_pairs_valid_excluding(&function, &excluded),
            "{case:?}"
        );
        assert_eq!(
            uses_x86_native_mmx_excluding(&function, &excluded),
            case.kind.mmx(),
            "{case:?}"
        );
        assert_eq!(
            uses_x86_x87_tag_state_excluding(&function, &excluded),
            case.kind.mmx(),
            "{case:?}"
        );
        assert_eq!(
            uses_x86_native_vectors_excluding(&function, &excluded),
            !case.kind.mmx(),
            "{case:?}"
        );
        assert!(
            x86_native_mmx_features_supported_excluding(&function, &excluded),
            "{case:?}"
        );
        assert_eq!(requirements.any, !case.kind.mmx(), "{case:?}");
        assert_eq!(
            requirements.all_spans_support_avx_ymm16,
            !case.kind.mmx(),
            "{case:?}"
        );
        assert_eq!(requirements.needs_avx, !case.kind.mmx(), "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_ssse3, "{case:?}");
        assert!(!requirements.needs_sse41, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert_eq!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            !case.kind.mmx(),
            "{case:?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            case.kind.mmx() || std::is_x86_feature_detected!("avx"),
            "{case:?}"
        );
    }
}

#[test]
fn replay_emits_exact_state_backed_transfers_for_all_1728_optimized_cases() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for case in exhaustive_cases() {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            if !case.kind.mmx() {
                lowerer.set_avx_ymm16_vector_state(true);
            }
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
    assert_eq!(lowered, 1728);
}

#[test]
fn replay_fails_closed_without_matching_bytes_marker_and_semantic_graph() {
    for kind in TransferKind::ALL {
        let case = TransferCase {
            kind,
            rex: Some(0x4E),
            gpr: 5,
            encoded_vector: 7,
        };
        let bytes = encoding(case);
        let base = function(&bytes);

        let mut missing = base.clone();
        missing.x86_instruction_bytes.clear();

        let mut ordinary_bytes = bytes.clone();
        *ordinary_bytes.last_mut().unwrap() &= !0x07;
        let mut ordinary_metadata = base.clone();
        ordinary_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&ordinary_bytes).unwrap(),
        );

        let mut memory_bytes = bytes;
        *memory_bytes.last_mut().unwrap() &= 0x3F;
        let mut memory_metadata = base.clone();
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
        );

        let mut wrong_width = base.clone();
        let OpKind::X86MovdQ { width, .. } =
            &mut wrong_width.blocks[0].ops.last_mut().unwrap().kind
        else {
            unreachable!()
        };
        *width = if *width == crate::smir::ir::types::OpWidth::W32 {
            crate::smir::ir::types::OpWidth::W64
        } else {
            crate::smir::ir::types::OpWidth::W32
        };

        let mut wrong_zero_upper = base.clone();
        let OpKind::X86MovdQ { zero_upper, .. } =
            &mut wrong_zero_upper.blocks[0].ops.last_mut().unwrap().kind
        else {
            unreachable!()
        };
        *zero_upper = true;

        let mut missing_hint = base.clone();
        missing_hint.blocks[0].ops.last_mut().unwrap().x86_hint = None;

        let mut nonmatching = vec![
            missing,
            ordinary_metadata,
            memory_metadata,
            wrong_width,
            wrong_zero_upper,
            missing_hint,
        ];
        if kind.mmx() {
            let mut wrong_marker = base;
            wrong_marker.blocks[0].ops[0].kind = OpKind::Nop;
            nonmatching.push(wrong_marker);
        }
        for nonmatching in nonmatching {
            assert!(!is_native_clobber_safe(&nonmatching), "{kind:?}");
            assert!(
                !x86_native_replay_feature_requirements(
                    &nonmatching,
                    &std::collections::HashMap::new()
                )
                .any,
                "{kind:?}"
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransferState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(ordinal: usize) -> TransferState {
    TransferState {
        gprs: std::array::from_fn(|register| {
            0x89AB_CDEF_0123_4567u64.rotate_left((register * 9) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xC33C_F00F_6996_A55Au64
                    .rotate_left(((ordinal * 5 + register * 13 + word * 19) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        mm: std::array::from_fn(|register| {
            0xA5A5_5A5A_6996_9669u64.rotate_left(((register * 11 + ordinal * 7) & 63) as u32)
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
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][ordinal % 4],
    }
}

fn architectural_expected(case: TransferCase, initial: &TransferState) -> TransferState {
    let mut expected = initial.clone();
    let mask = if case.width() == crate::smir::ir::types::OpWidth::W32 {
        u64::from(u32::MAX)
    } else {
        u64::MAX
    };
    let vector = usize::from(case.vector());
    let gpr = usize::from(case.gpr);
    if case.kind.vector_destination() {
        let value = initial.gprs[gpr] & mask;
        if case.kind.mmx() {
            expected.mm[vector] = value;
        } else {
            expected.vectors[vector][0] = value;
            expected.vectors[vector][1] = 0;
        }
    } else {
        let value = if case.kind.mmx() {
            initial.mm[vector]
        } else {
            initial.vectors[vector][0]
        };
        expected.gprs[gpr] = value & mask;
    }
    if case.kind.mmx() {
        expected.x87_tag_word = 0;
    }
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
    initial: &TransferState,
    level: crate::smir::optimize::OptLevel,
) -> TransferState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        x86.mm = initial.mm;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.x87.tag_word = initial.x87_tag_word as u16;
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
    TransferState {
        gprs: x86.gpr,
        vectors,
        mm: x86.mm,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
        x87_tag_word: u64::from(x86.x87.tag_word),
    }
}

#[test]
fn interpreter_matches_intel_o0_o1_o2_all_576_encodings_and_full_state() {
    for (ordinal, case) in exhaustive_cases().into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
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
    case: TransferCase,
    initial: &TransferState,
    level: crate::smir::optimize::OptLevel,
) -> TransferState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    if !case.kind.mmx() {
        lowerer.set_avx_ymm16_vector_state(true);
    }
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_replay_emitted(&code, case, bytes);
    let exec = ExecMem::new(&code).expect("map legacy MOVD/MOVQ stack-GPR replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: if case.kind.mmx() {
            0
        } else {
            X86_VECTOR_STATE_YMM16
        },
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        mmx_active: u64::from(case.kind.mmx()),
        x87_tag_word: initial.x87_tag_word,
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
    TransferState {
        gprs: registers.gpr,
        vectors,
        mm: registers.mm,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
        x87_tag_word: registers.x87_tag_word,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_MOVD_Q_STACK_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[TransferCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O1,
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
        .expect("run isolated native legacy MOVD/MOVQ stack-GPR differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let mut cases = exhaustive_cases();
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("host lacks AVX; exercising MMX MOVD/MOVQ cases only");
        cases.retain(|case| case.kind.mmx());
    }
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
        "isolated native legacy MOVD/MOVQ stack-GPR failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o1_o2_all_576_encodings_and_full_state() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_movd_q_stack_replay::\
         replay_matches_intel_o0_o1_o2_all_576_encodings_and_full_state",
    );
}
