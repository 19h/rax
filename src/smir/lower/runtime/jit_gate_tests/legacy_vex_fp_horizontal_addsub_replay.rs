//! Native replay coverage for register-only legacy SSE3 and AVX VEX packed
//! floating-point `HADD`/`HSUB`/`ADDSUB`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x7C7D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    F32,
    F64,
}

impl Element {
    fn pp(self) -> u8 {
        match self {
            Self::F32 => 3,
            Self::F64 => 1,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    HorizontalAdd,
    HorizontalSub,
    AddSub,
}

impl Operation {
    fn opcode(self) -> u8 {
        match self {
            Self::HorizontalAdd => 0x7C,
            Self::HorizontalSub => 0x7D,
            Self::AddSub => 0xD0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn bytes(self) -> usize {
        match self {
            Self::V128 => 16,
            Self::V256 => 32,
        }
    }

    fn is_256(self) -> bool {
        self == Self::V256
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
struct FpCase {
    element: Element,
    operation: Operation,
    width: Width,
    form: EncodingForm,
    dst: u8,
    src1: u8,
    src2: u8,
}

fn encoding(case: FpCase) -> Vec<u8> {
    let FpCase {
        element,
        operation,
        width,
        form,
        dst,
        src1,
        src2,
    } = case;
    let opcode = operation.opcode();
    let pp = element.pp();
    assert!(dst < 16 && src1 < 16 && src2 < 16);

    match form {
        EncodingForm::Legacy | EncodingForm::LegacyRex => {
            assert_eq!(width, Width::V128);
            assert_eq!(src1, dst);
            if form == EncodingForm::Legacy {
                assert!(dst < 8 && src2 < 8);
            }
            let mut bytes = vec![if element == Element::F32 { 0xF2 } else { 0x66 }];
            if form == EncodingForm::LegacyRex {
                // REX.W/X are ignored; REX.R/B select XMM8-XMM15.
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
                    | (u8::from(width.is_256()) << 2)
                    | pp,
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
                    | (u8::from(width.is_256()) << 2)
                    | pp,
                opcode,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
            ]
        }
    }
}

fn cases() -> Vec<FpCase> {
    let mut cases = Vec::new();
    for element in [Element::F32, Element::F64] {
        for operation in [
            Operation::HorizontalAdd,
            Operation::HorizontalSub,
            Operation::AddSub,
        ] {
            for form in [
                EncodingForm::Legacy,
                EncodingForm::LegacyRex,
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ] {
                let widths: &[Width] = if form.is_vex() {
                    &[Width::V128, Width::V256]
                } else {
                    &[Width::V128]
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
                for &width in widths {
                    for &(dst, src1, src2) in operands {
                        cases.push(FpCase {
                            element,
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
fn replay_features_track_sse3_and_use_avx_ymm16_boundary() {
    for (case, expected_sse3) in [
        (
            FpCase {
                element: Element::F32,
                operation: Operation::HorizontalAdd,
                width: Width::V128,
                form: EncodingForm::LegacyRex,
                dst: 9,
                src1: 9,
                src2: 11,
            },
            true,
        ),
        (
            FpCase {
                element: Element::F64,
                operation: Operation::AddSub,
                width: Width::V256,
                form: EncodingForm::VexC4W1IgnoredX,
                dst: 9,
                src1: 10,
                src2: 11,
            },
            false,
        ),
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert_eq!(requirements.needs_sse3, expected_sse3, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
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
            (!expected_sse3 || std::is_x86_feature_detected!("sse3"))
                && std::is_x86_feature_detected!("avx")
        );
    }
}

#[test]
fn replay_admits_and_emits_408_o0_o2_safe_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 204);
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
    assert_eq!(lowered, 408);

    let case = FpCase {
        element: Element::F64,
        operation: Operation::HorizontalSub,
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
struct FpState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const EXACT_VALUES: [f64; 16] = [
    0.0, -0.0, 0.25, -0.5, 1.0, -2.0, 4.0, -8.0, 16.0, -32.0, 64.0, -128.0, 256.0, -512.0, 1024.0,
    -2048.0,
];

fn element_bits(vector: &[u64; 8], element: Element, lane: usize) -> u64 {
    match element {
        Element::F32 => (vector[lane / 2] >> ((lane % 2) * 32)) & u64::from(u32::MAX),
        Element::F64 => vector[lane],
    }
}

fn set_element_bits(vector: &mut [u64; 8], element: Element, lane: usize, bits: u64) {
    match element {
        Element::F32 => {
            let shift = (lane % 2) * 32;
            let mask = u64::from(u32::MAX) << shift;
            vector[lane / 2] = (vector[lane / 2] & !mask) | ((bits & u64::from(u32::MAX)) << shift);
        }
        Element::F64 => vector[lane] = bits,
    }
}

fn exact_vector(element: Element, register: usize, ordinal: usize) -> [u64; 8] {
    let mut vector = [0u64; 8];
    for lane in 0..64 / element.bytes() {
        let value = EXACT_VALUES[(ordinal + register * 3 + lane * 5) % EXACT_VALUES.len()];
        let bits = match element {
            Element::F32 => u64::from((value as f32).to_bits()),
            Element::F64 => value.to_bits(),
        };
        set_element_bits(&mut vector, element, lane, bits);
    }
    vector
}

fn exact_initial_state(case: FpCase, ordinal: usize) -> FpState {
    FpState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| exact_vector(case.element, register, ordinal)),
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
        mxcsr: 0x1F80,
    }
}

fn exact_binary(element: Element, lhs: u64, rhs: u64, subtract: bool) -> u64 {
    match element {
        Element::F32 => {
            let lhs = f32::from_bits(lhs as u32);
            let rhs = f32::from_bits(rhs as u32);
            u64::from((if subtract { lhs - rhs } else { lhs + rhs }).to_bits())
        }
        Element::F64 => {
            let lhs = f64::from_bits(lhs);
            let rhs = f64::from_bits(rhs);
            (if subtract { lhs - rhs } else { lhs + rhs }).to_bits()
        }
    }
}

fn architectural_expected(case: FpCase, initial: &FpState) -> FpState {
    let mut expected = initial.clone();
    let source1 = initial.vectors[usize::from(case.src1)];
    let source2 = initial.vectors[usize::from(case.src2)];
    let destination = &mut expected.vectors[usize::from(case.dst)];
    if case.form.is_vex() {
        destination.fill(0);
    }

    let lanes = case.width.bytes() / case.element.bytes();
    match case.operation {
        Operation::AddSub => {
            for lane in 0..lanes {
                let result = exact_binary(
                    case.element,
                    element_bits(&source1, case.element, lane),
                    element_bits(&source2, case.element, lane),
                    lane & 1 == 0,
                );
                set_element_bits(destination, case.element, lane, result);
            }
        }
        Operation::HorizontalAdd | Operation::HorizontalSub => {
            let per_128 = 16 / case.element.bytes();
            let pairs = per_128 / 2;
            for block in 0..case.width.bytes() / 16 {
                let base = block * per_128;
                for (destination_half, source) in [source1, source2].into_iter().enumerate() {
                    for pair in 0..pairs {
                        let left_lane = base + pair * 2;
                        let result = exact_binary(
                            case.element,
                            element_bits(&source, case.element, left_lane),
                            element_bits(&source, case.element, left_lane + 1),
                            case.operation == Operation::HorizontalSub,
                        );
                        set_element_bits(
                            destination,
                            case.element,
                            base + destination_half * pairs + pair,
                            result,
                        );
                    }
                }
            }
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

fn interpret(bytes: &[u8], initial: &FpState, level: crate::smir::optimize::OptLevel) -> FpState {
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
    FpState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_exact_equations_lane_order_aliases_and_upper_lanes() {
    let cases = cases();
    assert_eq!(cases.len(), 204);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = exact_initial_state(case, ordinal);
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

fn patterned_vector(element: Element, register: usize) -> [u64; 8] {
    let patterns: &[u64] = if element == Element::F32 {
        &F32_PATTERNS
    } else {
        &F64_PATTERNS
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element.bytes() {
        let value = patterns[(lane + register * 5) % patterns.len()].to_le_bytes();
        let base = lane * element.bytes();
        bytes[base..base + element.bytes()].copy_from_slice(&value[..element.bytes()]);
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn adversarial_initial_state(case: FpCase, ordinal: usize) -> FpState {
    let prior_status = (ordinal as u32).rotate_left(3) & 0x3F;
    let rc = ((ordinal as u32 >> 2) & 3) << 13;
    let denormal_controls = if ordinal & 1 == 0 {
        0
    } else {
        (1 << 6) | (1 << 15)
    };
    FpState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(case.element, register)),
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
        // All exception masks remain set. The CPU-level JIT boundary rejects
        // replay when a mask is clear, preventing host SIGFPE while preserving
        // status, rounding-control, DAZ, and FTZ coverage.
        mxcsr: 0x1F80 | prior_status | rc | denormal_controls,
    }
}

#[test]
fn interpreter_o0_o2_accrues_ci_haddps_denormal_and_precision_status() {
    let case = cases()[0];
    assert_eq!(
        case,
        FpCase {
            element: Element::F32,
            operation: Operation::HorizontalAdd,
            width: Width::V128,
            form: EncodingForm::Legacy,
            dst: 1,
            src1: 1,
            src2: 3,
        }
    );
    let bytes = encoding(case);
    assert_eq!(bytes, [0xF2, 0x0F, 0x7C, 0xCB]);
    let initial = adversarial_initial_state(case, 0);
    assert_eq!(initial.mxcsr, 0x1F80);
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let result = interpret(&bytes, &initial, level);
        assert_eq!(result.mxcsr, 0x1FA2, "{level:?}: HADDPS must accrue DE|PE");
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &FpState,
    level: crate::smir::optimize::OptLevel,
) -> FpState {
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
    let exec = ExecMem::new(&code).expect("map legacy/VEX FP horizontal/add-sub replay");
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
    FpState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_VEX_FP_HORIZONTAL_ADDSUB_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[FpCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = adversarial_initial_state(case, ordinal);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                execute_native(&bytes, &initial, level),
                interpret(&bytes, &initial, level),
                "{level:?} {case:?} {bytes:02X?}"
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
        .expect("run isolated native legacy/VEX FP horizontal/add-sub differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 204);
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
        "isolated native legacy/VEX FP horizontal/add-sub failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_formats_mxcsr_aliases_and_upper_lanes() {
    if !std::is_x86_feature_detected!("sse3") || !std::is_x86_feature_detected!("avx") {
        eprintln!(
            "skipping native legacy/VEX FP horizontal/add-sub differential: \
             host lacks SSE3/AVX"
        );
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_vex_fp_horizontal_addsub_replay::\
         replay_matches_o0_o2_interpretation_for_formats_mxcsr_aliases_and_upper_lanes",
    );
}
