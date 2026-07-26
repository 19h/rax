//! Native replay coverage for operandless AVX `VZEROUPPER` and `VZEROALL`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A77;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZeroKind {
    Upper,
    All,
}

impl ZeroKind {
    fn l(self) -> u8 {
        u8::from(self == Self::All)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZeroCase {
    kind: ZeroKind,
    bytes: Vec<u8>,
}

fn cases() -> Vec<ZeroCase> {
    let mut cases = Vec::new();
    for kind in [ZeroKind::Upper, ZeroKind::All] {
        for r in [0, 0x80] {
            cases.push(ZeroCase {
                kind,
                bytes: vec![0xC5, r | 0x78 | (kind.l() << 2), 0x77],
            });
        }
        for extensions in 0u8..8 {
            for w in [0, 0x80] {
                cases.push(ZeroCase {
                    kind,
                    bytes: vec![
                        0xC4,
                        (extensions << 5) | 1,
                        w | 0x78 | (kind.l() << 2),
                        0x77,
                    ],
                });
            }
        }
    }
    assert_eq!(cases.len(), 36);
    cases
}

fn function_at(bytes: &[u8], block_id: BlockId, pc: u64) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(pc, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(block_id, pc);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, pc);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((block_id, pc), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
    function_at(bytes, BlockId(0), PC)
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

#[test]
fn replay_features_require_only_avx_and_select_the_ymm16_bridge() {
    for case in cases() {
        let function = function(&case.bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
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

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx"),
            "{case:?}"
        );
    }
}

#[test]
fn replay_feature_aggregation_is_monotonic_with_evex_state() {
    let vex = [0xC4, 0x01, 0xFC, 0x77];
    // EVEX.512.66.0F38.W0 20 /r VPMOVSXBW zmm1, ymm2.
    let evex = [0x62, 0xF2, 0x7D, 0x48, 0x20, 0xCA];

    for (first, second) in [(&vex[..], &evex[..]), (&evex[..], &vex[..])] {
        let mut mixed = function_at(first, BlockId(0), PC);
        let mut trailing = function_at(second, BlockId(1), PC + 0x100);
        mixed.add_block(trailing.blocks.remove(0));
        mixed
            .x86_instruction_bytes
            .extend(trailing.x86_instruction_bytes);

        let requirements =
            x86_native_replay_feature_requirements(&mixed, &std::collections::HashMap::new());
        assert!(!requirements.all_spans_support_avx_ymm16);
        assert!(requirements.needs_avx);
        assert!(requirements.needs_avx512bw);
    }
}

fn lower_code(bytes: &[u8], level: crate::smir::optimize::OptLevel, avx_ymm16: bool) -> Vec<u8> {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(avx_ymm16);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"))
}

#[test]
fn replay_admits_and_emits_all_36_exact_encodings_at_o0_and_o2() {
    let mut lowered = 0usize;
    for case in cases() {
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&case.bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(is_native_clobber_safe(&function), "{level:?} {case:?}");
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?}"
            );
            assert!(
                x86_native_vector_uses_avx_ymm16_only_excluding(
                    &function,
                    &std::collections::HashMap::new()
                ),
                "{level:?} {case:?}"
            );
            let code = lower_code(&case.bytes, level, true);
            assert!(
                code.windows(case.bytes.len())
                    .any(|window| window == case.bytes),
                "{level:?} {case:?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 72);
}

fn bulk_upper_clear_postlude() -> Vec<u8> {
    let mut bytes = vec![
        0x9C,
        0x50,
        0x51,
        0x48,
        0x8B,
        0x45,
        X86_STATE_PTR_AT_RBP as u8,
        0x48,
        0x8D,
        0x80,
    ];
    bytes.extend_from_slice(&(X86_GUEST_ZMM_OFFSET + 32).to_le_bytes());
    bytes.extend_from_slice(&[0xB9, 16, 0, 0, 0]);
    for offset in [0u8, 8, 16, 24] {
        bytes.extend_from_slice(&[0x48, 0xC7, 0x40, offset, 0, 0, 0, 0]);
    }
    bytes.extend_from_slice(&[
        0x48, 0x83, 0xC0, 0x40, // add rax,64
        0xFF, 0xC9, // dec ecx
        0x75, 0xD8, // jnz -40 bytes
        0x59, 0x58, 0x9D, // restore rcx, rax, rflags
    ]);
    bytes
}

#[test]
fn ymm16_replay_emits_the_exact_bulk_state_backed_upper_clear() {
    let postlude = bulk_upper_clear_postlude();
    assert_eq!(postlude.len(), 62);

    for instruction in [&[0xC5, 0xF8, 0x77][..], &[0xC4, 0x01, 0xFC, 0x77]] {
        let code = lower_code(instruction, crate::smir::optimize::OptLevel::O2, true);
        let mut expected = instruction.to_vec();
        expected.extend_from_slice(&postlude);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{instruction:02X?}"
        );

        let full_vector_code = lower_code(instruction, crate::smir::optimize::OptLevel::O2, false);
        assert!(
            full_vector_code
                .windows(instruction.len())
                .any(|window| window == instruction),
            "{instruction:02X?}"
        );
        assert!(
            !full_vector_code
                .windows(postlude.len())
                .any(|window| window == postlude),
            "{instruction:02X?}"
        );
    }
}

#[test]
fn replay_fails_closed_without_exact_source_provenance() {
    let bytes = [0xC5, 0xF8, 0x77];
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut reserved_vvvv = base.clone();
    reserved_vvvv.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&[0xC5, 0xE8, 0x77]).unwrap(),
    );
    assert!(!is_native_clobber_safe(&reserved_vvvv));

    let mut trailing = base;
    trailing.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&[0xC5, 0xF8, 0x77, 0x90]).unwrap(),
    );
    assert!(!is_native_clobber_safe(&trailing));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZeroState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(profile: usize) -> ZeroState {
    ZeroState {
        gprs: std::array::from_fn(|register| {
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                    ^ (profile as u64).wrapping_mul(0x1020_4081_0204_0810)
                    ^ ((register * 8 + word) as u64).wrapping_mul(0x8040_2010_0804_0201)
            })
        }),
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
        // Includes DF so the clearing loop must preserve more than ALU flags.
        rflags: 0x2 | 0xCD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][profile % 4],
    }
}

fn architectural_expected(kind: ZeroKind, initial: &ZeroState) -> ZeroState {
    let mut expected = initial.clone();
    for vector in &mut expected.vectors[..16] {
        if kind == ZeroKind::All {
            vector.fill(0);
        } else {
            vector[2..].fill(0);
        }
    }
    expected
}

fn interpret(
    bytes: &[u8],
    initial: &ZeroState,
    level: crate::smir::optimize::OptLevel,
) -> ZeroState {
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
    ZeroState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_maxvl_equations_at_o0_o2_for_all_exact_encodings() {
    for (ordinal, case) in cases().into_iter().enumerate() {
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case.kind, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&case.bytes, &initial, level),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ZeroState,
    level: crate::smir::optimize::OptLevel,
) -> ZeroState {
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
    let exec = ExecMem::new(&code).expect("map VZERO replay");
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
    ZeroState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_ZERO_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ZeroCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case.kind, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&case.bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?}"
            );
            assert_eq!(
                execute_native(&case.bytes, &initial, level),
                expected,
                "native {level:?} {case:?}"
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
        .expect("run isolated native VZERO differential")
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
    let case = &cases[start];
    panic!(
        "isolated native VZERO failure at case {start}/{}: {case:?}; \
         whole status {}; singleton status {}; singleton stdout: {}; \
         singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_exact_o0_o2_maxvl_equations_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VZERO differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_zero_replay::\
         replay_matches_exact_o0_o2_maxvl_equations_and_full_state",
    );
}
