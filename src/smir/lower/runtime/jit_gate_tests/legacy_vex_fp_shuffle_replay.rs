//! Native replay coverage for register-only legacy SSE and AVX VEX
//! floating-point shuffle/interleave instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1013;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    F32,
    F64,
}

impl Element {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 0,
            Self::F64 => 1,
        }
    }

    fn lanes_per_128(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F64 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    UnpackLow,
    UnpackHigh,
    Shuffle(u8),
}

impl Operation {
    fn fields(self) -> (u8, Option<u8>) {
        match self {
            Self::UnpackLow => (0x14, None),
            Self::UnpackHigh => (0x15, None),
            Self::Shuffle(imm) => (0xC6, Some(imm)),
        }
    }
}

fn operations(element: Element) -> [Operation; 6] {
    match element {
        Element::F32 => [
            Operation::UnpackLow,
            Operation::UnpackHigh,
            Operation::Shuffle(0x00),
            Operation::Shuffle(0x1B),
            Operation::Shuffle(0xE4),
            Operation::Shuffle(0xFF),
        ],
        Element::F64 => [
            Operation::UnpackLow,
            Operation::UnpackHigh,
            Operation::Shuffle(0x00),
            Operation::Shuffle(0x03),
            Operation::Shuffle(0x0A),
            Operation::Shuffle(0xFF),
        ],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    Legacy,
    LegacyRex,
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

impl EncodingForm {
    fn is_vex(self) -> bool {
        matches!(self, Self::VexC5 | Self::VexC4W0 | Self::VexC4W1IgnoredX)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShuffleCase {
    form: EncodingForm,
    element: Element,
    operation: Operation,
    l: bool,
    dst: u8,
    src1: u8,
    src2: u8,
}

fn encoding(case: ShuffleCase) -> Vec<u8> {
    let ShuffleCase {
        form,
        element,
        operation,
        l,
        dst,
        src1,
        src2,
    } = case;
    assert!(dst < 16 && src1 < 16 && src2 < 16);
    let (opcode, imm) = operation.fields();

    let mut bytes = match form {
        EncodingForm::Legacy | EncodingForm::LegacyRex => {
            assert!(!l && src1 == dst);
            if form == EncodingForm::Legacy {
                assert!(dst < 8 && src2 < 8);
            }
            let mut bytes = Vec::new();
            if element == Element::F64 {
                bytes.push(0x66);
            }
            if form == EncodingForm::LegacyRex {
                // W and X are ignored for register forms; R and B extend the
                // ModR/M register and r/m fields.
                bytes.push(
                    0x4A | (if dst >= 8 { 0x04 } else { 0 }) | (if src2 >= 8 { 1 } else { 0 }),
                );
            }
            bytes.extend([0x0F, opcode, 0xC0 | ((dst & 7) << 3) | (src2 & 7)]);
            bytes
        }
        EncodingForm::VexC5 => {
            assert!(src2 < 8);
            vec![
                0xC5,
                (if dst < 8 { 0x80 } else { 0 })
                    | ((!src1 & 0x0F) << 3)
                    | (if l { 0x04 } else { 0 })
                    | element.pp(),
                opcode,
                0xC0 | ((dst & 7) << 3) | src2,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
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
                    | (if l { 0x04 } else { 0 })
                    | element.pp(),
                opcode,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
            ]
        }
    };
    bytes.extend(imm);
    bytes
}

fn cases() -> Vec<ShuffleCase> {
    let mut cases = Vec::new();
    for element in Element::ALL {
        for operation in operations(element) {
            for form in [
                EncodingForm::Legacy,
                EncodingForm::LegacyRex,
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ] {
                let lengths: &[bool] = if form.is_vex() {
                    &[false, true]
                } else {
                    &[false]
                };
                let operands: &[(u8, u8, u8)] = match form {
                    EncodingForm::Legacy => &[(1, 1, 3), (1, 1, 1)],
                    EncodingForm::LegacyRex => &[(9, 9, 11), (9, 9, 9)],
                    EncodingForm::VexC5 => {
                        &[(1, 2, 3), (9, 10, 3), (1, 1, 2), (1, 2, 1), (1, 1, 1)]
                    }
                    EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
                        &[(1, 2, 3), (9, 10, 11), (1, 1, 2), (1, 2, 1), (1, 1, 1)]
                    }
                };
                for &l in lengths {
                    for &(dst, src1, src2) in operands {
                        cases.push(ShuffleCase {
                            form,
                            element,
                            operation,
                            l,
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
fn replay_features_use_avx_ymm16_boundary_for_legacy_and_vex() {
    for case in [
        ShuffleCase {
            form: EncodingForm::LegacyRex,
            element: Element::F64,
            operation: Operation::Shuffle(0xA5),
            l: false,
            dst: 9,
            src1: 9,
            src2: 11,
        },
        ShuffleCase {
            form: EncodingForm::VexC4W1IgnoredX,
            element: Element::F64,
            operation: Operation::UnpackHigh,
            l: true,
            dst: 9,
            src1: 10,
            src2: 11,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
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
        );
    }
}

#[test]
fn replay_admits_and_emits_816_optimized_legal_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 408);
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
    assert_eq!(lowered, 816);

    let unpack = ShuffleCase {
        form: EncodingForm::VexC5,
        element: Element::F32,
        operation: Operation::UnpackLow,
        l: false,
        dst: 1,
        src1: 2,
        src2: 3,
    };
    let bytes = encoding(unpack);
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
struct ShuffleState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> ShuffleState {
    ShuffleState {
        gprs: std::array::from_fn(|register| {
            0x89AB_CDEF_0123_4567u64.rotate_left((register * 9) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xE1D2_C3B4_A596_8778u64.rotate_left((register * 13 + word * 7) as u32)
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn lane(vector: &[u64; 8], element: Element, index: usize) -> u64 {
    match element {
        Element::F32 => (vector[index / 2] >> ((index % 2) * 32)) & 0xFFFF_FFFF,
        Element::F64 => vector[index],
    }
}

fn set_lane(vector: &mut [u64; 8], element: Element, index: usize, value: u64) {
    match element {
        Element::F32 => {
            let shift = (index % 2) * 32;
            let mask = 0xFFFF_FFFFu64 << shift;
            vector[index / 2] = (vector[index / 2] & !mask) | ((value << shift) & mask);
        }
        Element::F64 => vector[index] = value,
    }
}

fn architectural_expected(case: ShuffleCase, initial: &ShuffleState) -> ShuffleState {
    let mut expected = initial.clone();
    let source1 = initial.vectors[usize::from(case.src1)];
    let source2 = initial.vectors[usize::from(case.src2)];
    let destination = &mut expected.vectors[usize::from(case.dst)];
    if case.form.is_vex() {
        destination.fill(0);
    }

    let lanes_per_128 = case.element.lanes_per_128();
    let chunks = if case.form.is_vex() && case.l { 2 } else { 1 };
    for chunk in 0..chunks {
        let base = chunk * lanes_per_128;
        match case.operation {
            Operation::UnpackLow | Operation::UnpackHigh => {
                let half = lanes_per_128 / 2;
                let source_offset = if case.operation == Operation::UnpackHigh {
                    half
                } else {
                    0
                };
                for index in 0..half {
                    set_lane(
                        destination,
                        case.element,
                        base + index * 2,
                        lane(&source1, case.element, base + source_offset + index),
                    );
                    set_lane(
                        destination,
                        case.element,
                        base + index * 2 + 1,
                        lane(&source2, case.element, base + source_offset + index),
                    );
                }
            }
            Operation::Shuffle(imm) => match case.element {
                Element::F32 => {
                    for output in 0..4 {
                        let selector = usize::from((imm >> (output * 2)) & 3);
                        let source = if output < 2 { &source1 } else { &source2 };
                        set_lane(
                            destination,
                            case.element,
                            base + output,
                            lane(source, case.element, base + selector),
                        );
                    }
                }
                Element::F64 => {
                    let source1_selector = usize::from((imm >> (chunk * 2)) & 1);
                    let source2_selector = usize::from((imm >> (chunk * 2 + 1)) & 1);
                    set_lane(
                        destination,
                        case.element,
                        base,
                        lane(&source1, case.element, base + source1_selector),
                    );
                    set_lane(
                        destination,
                        case.element,
                        base + 1,
                        lane(&source2, case.element, base + source2_selector),
                    );
                }
            },
        }
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
    initial: &ShuffleState,
    level: crate::smir::optimize::OptLevel,
) -> ShuffleState {
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
    ShuffleState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_state_for_widths_controls_aliases_and_upper_lanes() {
    let cases = cases();
    assert_eq!(cases.len(), 408);
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
    initial: &ShuffleState,
    level: crate::smir::optimize::OptLevel,
) -> ShuffleState {
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
    let exec = ExecMem::new(&code).expect("map legacy/VEX floating-shuffle replay");
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
    ShuffleState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_VEX_FP_SHUFFLE_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ShuffleCase], range: std::ops::Range<usize>) {
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
        .expect("run isolated native legacy/VEX floating-shuffle differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 408);
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
        "isolated native legacy/VEX floating-shuffle failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_state_for_widths_controls_aliases_and_upper_lanes() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy/VEX floating-shuffle differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_vex_fp_shuffle_replay::\
         replay_matches_intel_o0_o2_state_for_widths_controls_aliases_and_upper_lanes",
    );
}
