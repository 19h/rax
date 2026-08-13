//! Native replay coverage for register-only AVX VEX `VPMULUDQ` and
//! `VPMULDQ`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1018;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Unsigned,
    Signed,
}

impl Operation {
    fn map_opcode(self) -> (u8, u8) {
        match self {
            Self::Unsigned => (1, 0xF4),
            Self::Signed => (2, 0x28),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn is_256(self) -> bool {
        self == Self::V256
    }

    fn qwords(self) -> usize {
        if self.is_256() { 4 } else { 2 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MultiplyCase {
    operation: Operation,
    width: Width,
    form: EncodingForm,
    dst: u8,
    src1: u8,
    src2: u8,
}

fn encoding(case: MultiplyCase) -> Vec<u8> {
    let MultiplyCase {
        operation,
        width,
        form,
        dst,
        src1,
        src2,
    } = case;
    let (map, opcode) = operation.map_opcode();
    assert!(dst < 16 && src1 < 16 && src2 < 16);

    match form {
        EncodingForm::VexC5 => {
            assert_eq!(operation, Operation::Unsigned);
            assert_eq!(map, 1);
            assert!(src2 < 8);
            vec![
                0xC5,
                (if dst < 8 { 0x80 } else { 0 })
                    | ((!src1 & 0x0F) << 3)
                    | (u8::from(width.is_256()) << 2)
                    | 1,
                opcode,
                0xC0 | ((dst & 7) << 3) | src2,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE0 | map;
            if dst >= 8 {
                p0 &= !0x80;
            }
            if form == EncodingForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if src2 >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if form == EncodingForm::VexC4W1IgnoredX {
                    0x80
                } else {
                    0
                }) | ((!src1 & 0x0F) << 3)
                    | (u8::from(width.is_256()) << 2)
                    | 1,
                opcode,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
            ]
        }
    }
}

fn cases() -> Vec<MultiplyCase> {
    let mut cases = Vec::new();
    for operation in [Operation::Unsigned, Operation::Signed] {
        let forms: &[EncodingForm] = if operation == Operation::Unsigned {
            &[
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ]
        } else {
            &[EncodingForm::VexC4W0, EncodingForm::VexC4W1IgnoredX]
        };
        for &form in forms {
            let operands: &[(u8, u8, u8)] = if form == EncodingForm::VexC5 {
                &[(1, 2, 3), (9, 10, 3), (1, 1, 2), (1, 2, 1), (1, 1, 1)]
            } else {
                &[(1, 2, 3), (9, 10, 11), (1, 1, 2), (1, 2, 1), (1, 1, 1)]
            };
            for width in [Width::V128, Width::V256] {
                for &(dst, src1, src2) in operands {
                    cases.push(MultiplyCase {
                        operation,
                        width,
                        form,
                        dst,
                        src1,
                        src2,
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
fn replay_features_distinguish_avx_128_from_avx2_256() {
    for (case, expected_avx2) in [
        (
            MultiplyCase {
                operation: Operation::Unsigned,
                width: Width::V128,
                form: EncodingForm::VexC5,
                dst: 9,
                src1: 10,
                src2: 3,
            },
            false,
        ),
        (
            MultiplyCase {
                operation: Operation::Signed,
                width: Width::V128,
                form: EncodingForm::VexC4W1IgnoredX,
                dst: 9,
                src1: 10,
                src2: 11,
            },
            false,
        ),
        (
            MultiplyCase {
                operation: Operation::Signed,
                width: Width::V256,
                form: EncodingForm::VexC4W1IgnoredX,
                dst: 9,
                src1: 10,
                src2: 11,
            },
            true,
        ),
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert_eq!(requirements.needs_avx2, expected_avx2, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(!requirements.needs_avx512cd, "{case:?}");
        assert!(!requirements.needs_gfni, "{case:?}");
        assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
        assert!(!requirements.needs_vpclmulqdq, "{case:?}");

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx")
                && (!expected_avx2 || std::is_x86_feature_detected!("avx2"))
        );
    }
}

#[test]
fn replay_admits_and_emits_100_o0_o2_safe_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 50);
    let mut lowered = 0usize;
    for case in cases {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?} {bytes:02X?}"
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
    assert_eq!(lowered, 100);

    let case = MultiplyCase {
        operation: Operation::Signed,
        width: Width::V256,
        form: EncodingForm::VexC4W0,
        dst: 1,
        src1: 2,
        src2: 3,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes.clone();
    *memory_bytes.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MultiplyState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const DWORD_BOUNDARIES: [u32; 12] = [
    0,
    1,
    2,
    0x3FFF_FFFF,
    0x4000_0000,
    0x7FFF_FFFF,
    0x8000_0000,
    0x8000_0001,
    0xBFFF_FFFF,
    0xFFFF_FFFD,
    0xFFFF_FFFE,
    0xFFFF_FFFF,
];

fn initial_state(ordinal: usize) -> MultiplyState {
    MultiplyState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                let boundary =
                    DWORD_BOUNDARIES[(ordinal + register * 3 + word * 5) % DWORD_BOUNDARIES.len()];
                let odd_dword = 0xA5A5_5A5Au32.rotate_left((register * 11 + word * 7) as u32)
                    ^ (ordinal as u32).wrapping_mul(0x0102_0408);
                (u64::from(odd_dword) << 32) | u64::from(boundary)
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn architectural_expected(case: MultiplyCase, initial: &MultiplyState) -> MultiplyState {
    let mut expected = initial.clone();
    let source1 = initial.vectors[usize::from(case.src1)];
    let source2 = initial.vectors[usize::from(case.src2)];
    let destination = &mut expected.vectors[usize::from(case.dst)];
    destination.fill(0);
    for lane in 0..case.width.qwords() {
        let lhs = source1[lane] as u32;
        let rhs = source2[lane] as u32;
        destination[lane] = match case.operation {
            Operation::Unsigned => u64::from(lhs) * u64::from(rhs),
            Operation::Signed => {
                let product = i64::from(lhs as i32) * i64::from(rhs as i32);
                product as u64
            }
        };
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
    initial: &MultiplyState,
    level: crate::smir::optimize::OptLevel,
) -> MultiplyState {
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
    MultiplyState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_equations_boundaries_aliases_and_upper_lanes() {
    let cases = cases();
    assert_eq!(cases.len(), 50);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
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
    initial: &MultiplyState,
    level: crate::smir::optimize::OptLevel,
) -> MultiplyState {
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
    let exec = ExecMem::new(&code).expect("map VEX widening-dword-multiply replay");
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
    MultiplyState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_WIDENING_DWORD_MULTIPLY_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[MultiplyCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, &initial, level),
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
        .expect("run isolated native VEX widening-dword-multiply differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 50);
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
        "isolated native VEX widening-dword-multiply failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_equations_boundaries_aliases_and_upper_lanes() {
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping native VEX widening-dword-multiply differential: host lacks AVX2");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_widening_dword_multiply_replay::\
         replay_matches_intel_o0_o2_equations_boundaries_aliases_and_upper_lanes",
    );
}
