//! Native replay coverage for binary32/binary64 ADD/MUL/SUB/MIN/DIV/MAX.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5858;
const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FpKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl FpKind {
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

    fn packed(self) -> bool {
        matches!(self, Self::PackedF32 | Self::PackedF64)
    }

    fn elem_bytes(self) -> usize {
        if matches!(self, Self::PackedF32 | Self::ScalarF32) {
            4
        } else {
            8
        }
    }

    fn evex_controls(self) -> Vec<(u8, bool)> {
        (0..=2)
            .map(|ll| (ll, false))
            .chain((0..=3).map(|ll| (ll, true)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NonEvexForm {
    Legacy,
    LegacyRex,
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

impl NonEvexForm {
    fn is_vex(self) -> bool {
        matches!(self, Self::VexC5 | Self::VexC4W0 | Self::VexC4W1IgnoredX)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NonEvexCase {
    form: NonEvexForm,
    kind: FpKind,
    opcode: u8,
    l: bool,
    dst: u8,
    src1: u8,
    src2: u8,
}

fn non_evex_encoding(case: NonEvexCase) -> Vec<u8> {
    let NonEvexCase {
        form,
        kind,
        opcode,
        l,
        dst,
        src1,
        src2,
    } = case;
    assert!(OPCODES.contains(&opcode));
    assert!(dst < 16 && src1 < 16 && src2 < 16);
    assert!(kind.packed() || !l);
    let pp = kind.pp();
    match form {
        NonEvexForm::Legacy | NonEvexForm::LegacyRex => {
            assert!(!l);
            if form == NonEvexForm::Legacy {
                assert!(dst < 8 && src2 < 8);
            }
            let mut bytes = Vec::new();
            match pp {
                0 => {}
                1 => bytes.push(0x66),
                2 => bytes.push(0xF3),
                3 => bytes.push(0xF2),
                _ => unreachable!(),
            }
            if form == NonEvexForm::LegacyRex {
                // REX.W/X are ignored for these register forms. REX.R/B
                // select the architectural destination and second source.
                bytes.push(
                    0x4A | (if dst >= 8 { 0x04 } else { 0 }) | (if src2 >= 8 { 1 } else { 0 }),
                );
            }
            bytes.extend([0x0F, opcode, 0xC0 | ((dst & 7) << 3) | (src2 & 7)]);
            bytes
        }
        NonEvexForm::VexC5 => {
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
        NonEvexForm::VexC4W0 | NonEvexForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if dst >= 8 {
                p0 &= !0x80;
            }
            if form == NonEvexForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if src2 >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if form == NonEvexForm::VexC4W1IgnoredX {
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

fn non_evex_cases() -> Vec<NonEvexCase> {
    let mut cases = Vec::new();
    for opcode in OPCODES {
        for kind in FpKind::ALL {
            let lengths: &[bool] = if kind.packed() {
                &[false, true]
            } else {
                &[false]
            };
            for &l in lengths {
                for form in [
                    NonEvexForm::Legacy,
                    NonEvexForm::LegacyRex,
                    NonEvexForm::VexC5,
                    NonEvexForm::VexC4W0,
                    NonEvexForm::VexC4W1IgnoredX,
                ] {
                    if !form.is_vex() && l {
                        continue;
                    }
                    let operands: &[(u8, u8, u8)] = match form {
                        NonEvexForm::Legacy => &[(1, 1, 3), (1, 1, 1)],
                        NonEvexForm::LegacyRex => &[(9, 9, 11), (9, 9, 9)],
                        NonEvexForm::VexC5 => &[(1, 2, 3), (9, 10, 3), (1, 1, 2), (1, 2, 1)],
                        NonEvexForm::VexC4W0 | NonEvexForm::VexC4W1IgnoredX => {
                            &[(1, 2, 3), (9, 10, 11), (1, 1, 2), (1, 2, 1)]
                        }
                    };
                    for &(dst, src1, src2) in operands {
                        cases.push(NonEvexCase {
                            form,
                            kind,
                            opcode,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexCase {
    kind: FpKind,
    opcode: u8,
    ll: u8,
    embedded_control: bool,
    dst: u8,
    src1: u8,
    src2: u8,
    mask: u8,
    zeroing: bool,
}

fn evex_encoding(case: EvexCase) -> [u8; 6] {
    let EvexCase {
        kind,
        opcode,
        ll,
        embedded_control,
        dst,
        src1,
        src2,
        mask,
        zeroing,
    } = case;
    assert!(OPCODES.contains(&opcode));
    assert!(ll < 4 && dst < 32 && src1 < 32 && src2 < 32 && mask < 8);
    assert!(embedded_control || ll < 3);
    assert!(!zeroing || mask != 0);

    let mut p0 = 0xF1;
    if dst & 0x08 != 0 {
        p0 &= !0x80;
    }
    if dst & 0x10 != 0 {
        p0 &= !0x10;
    }
    if src2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if src2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (if kind.elem_bytes() == 8 { 0x80 } else { 0 })
            | (((!src1) & 0x0F) << 3)
            | 0x04
            | kind.pp(),
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | (if embedded_control { 0x10 } else { 0 })
            | (if src1 < 16 { 0x08 } else { 0 })
            | mask,
        opcode,
        0xC0 | ((dst & 7) << 3) | (src2 & 7),
    ]
}

fn evex_requirements(case: EvexCase) -> bool {
    case.kind.packed() && !case.embedded_control && case.ll != 2
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
fn replay_features_distinguish_legacy_vex_and_evex_vector_lengths() {
    let representatives = [
        (
            non_evex_encoding(NonEvexCase {
                form: NonEvexForm::LegacyRex,
                kind: FpKind::ScalarF64,
                opcode: 0x5E,
                l: false,
                dst: 9,
                src1: 9,
                src2: 11,
            }),
            true,
            true,
            false,
        ),
        (
            non_evex_encoding(NonEvexCase {
                form: NonEvexForm::VexC4W1IgnoredX,
                kind: FpKind::PackedF64,
                opcode: 0x59,
                l: true,
                dst: 9,
                src1: 10,
                src2: 11,
            }),
            true,
            true,
            false,
        ),
        (
            evex_encoding(EvexCase {
                kind: FpKind::PackedF32,
                opcode: 0x58,
                ll: 0,
                embedded_control: false,
                dst: 17,
                src1: 18,
                src2: 19,
                mask: 1,
                zeroing: true,
            })
            .to_vec(),
            false,
            false,
            true,
        ),
        (
            evex_encoding(EvexCase {
                kind: FpKind::PackedF32,
                opcode: 0x5D,
                ll: 3,
                embedded_control: true,
                dst: 17,
                src1: 18,
                src2: 19,
                mask: 1,
                zeroing: false,
            })
            .to_vec(),
            false,
            false,
            false,
        ),
    ];

    for (bytes, needs_avx, avx_ymm16, needs_vl) in representatives {
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{bytes:02X?}");
        assert_eq!(
            actual.all_spans_support_avx_ymm16, avx_ymm16,
            "{bytes:02X?}"
        );
        assert_eq!(actual.needs_avx, needs_avx, "{bytes:02X?}");
        assert_eq!(actual.needs_avx512bw, !avx_ymm16, "{bytes:02X?}");
        assert_eq!(actual.needs_avx512vl, needs_vl, "{bytes:02X?}");
        assert!(!actual.needs_fma);
        assert!(!actual.needs_avx512dq);
        assert!(!actual.needs_avx512fp16);
    }
}

#[test]
fn replay_admits_and_emits_1_056_non_evex_shapes_at_o0_o2_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = non_evex_cases();
    assert_eq!(cases.len(), 528);
    let mut lowered = 0usize;
    for case in cases {
        let bytes = non_evex_encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
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
                "{level:?} {case:?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 1_056);

    let case = NonEvexCase {
        form: NonEvexForm::VexC5,
        kind: FpKind::ScalarF32,
        opcode: 0x58,
        l: false,
        dst: 1,
        src1: 2,
        src2: 3,
    };
    let bytes = non_evex_encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory = bytes.clone();
    *memory.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));

    let mut scalar_l1 = bytes.clone();
    scalar_l1[1] |= 0x04;
    let scalar_l1_function = function(&scalar_l1);
    assert!(is_native_clobber_safe(&scalar_l1_function));
    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&scalar_l1_function)
        .expect("lower canonical scalar VEX.L=1 arithmetic replay");
    let code = lowerer
        .finalize()
        .expect("finalize canonical scalar VEX.L=1 arithmetic replay");
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    assert!(
        !code
            .windows(scalar_l1.len())
            .any(|window| window == scalar_l1)
    );
}

#[test]
fn replay_admits_and_emits_5_040_evex_shapes_at_o0_o2_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [(1u8, 2u8, 3u8), (17, 18, 19), (31, 31, 31)];
    let masks = [(0u8, false), (1, false), (1, true), (2, false), (3, true)];
    let mut lowered = 0usize;
    let mut fail_closed_checked = false;
    for opcode in OPCODES {
        for kind in FpKind::ALL {
            for (ll, embedded_control) in kind.evex_controls() {
                for (dst, src1, src2) in operands {
                    for (mask, zeroing) in masks {
                        let case = EvexCase {
                            kind,
                            opcode,
                            ll,
                            embedded_control,
                            dst,
                            src1,
                            src2,
                            mask,
                            zeroing,
                        };
                        let bytes = evex_encoding(case);
                        let base = function(&bytes);
                        if !fail_closed_checked {
                            let mut missing = base.clone();
                            missing.x86_instruction_bytes.clear();
                            assert!(!is_native_clobber_safe(&missing));

                            let mut memory_bytes = bytes;
                            memory_bytes[5] &= 0x3F;
                            let mut memory_metadata = base.clone();
                            memory_metadata.x86_instruction_bytes.insert(
                                (BlockId(0), PC),
                                crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
                            );
                            assert!(!is_native_clobber_safe(&memory_metadata));
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
                                uses_x86_native_vectors_excluding(
                                    &function,
                                    &std::collections::HashMap::new()
                                ),
                                "{level:?} {case:?}"
                            );
                            let requirements = x86_native_replay_feature_requirements(
                                &function,
                                &std::collections::HashMap::new(),
                            );
                            assert_eq!(
                                requirements.needs_avx512vl,
                                evex_requirements(case),
                                "{case:?}"
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
                                "{level:?} {case:?}"
                            );
                            lowered += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(fail_closed_checked);
    assert_eq!(lowered, 5_040);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeCase {
    NonEvex(NonEvexCase),
    Evex(EvexCase),
}

impl NativeCase {
    fn bytes(self) -> Vec<u8> {
        match self {
            Self::NonEvex(case) => non_evex_encoding(case),
            Self::Evex(case) => evex_encoding(case).to_vec(),
        }
    }

    fn kind(self) -> FpKind {
        match self {
            Self::NonEvex(case) => case.kind,
            Self::Evex(case) => case.kind,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn host_supported(self) -> bool {
        match self {
            Self::NonEvex(_) => true,
            Self::Evex(case) => {
                !evex_requirements(case) || std::is_x86_feature_detected!("avx512vl")
            }
        }
    }
}

fn native_cases() -> Vec<NativeCase> {
    let mut cases: Vec<_> = non_evex_cases()
        .into_iter()
        .map(NativeCase::NonEvex)
        .collect();
    let operands = [
        (1u8, 2u8, 3u8),
        (17, 18, 19),
        (1, 1, 3),
        (2, 3, 2),
        (4, 4, 4),
    ];
    let masks = [(0u8, false), (1, false), (1, true), (2, false)];
    for opcode in OPCODES {
        for kind in FpKind::ALL {
            for (ll, embedded_control) in kind.evex_controls() {
                for (dst, src1, src2) in operands {
                    for (mask, zeroing) in masks {
                        cases.push(NativeCase::Evex(EvexCase {
                            kind,
                            opcode,
                            ll,
                            embedded_control,
                            dst,
                            src1,
                            src2,
                            mask,
                            zeroing,
                        }));
                    }
                }
            }
        }
    }
    cases
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArithmeticState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
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

fn patterned_vector(kind: FpKind, register: usize) -> [u64; 8] {
    let element_size = kind.elem_bytes();
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

fn initial_state(case: NativeCase, ordinal: usize) -> ArithmeticState {
    let prior_status = (ordinal as u32).rotate_left(3) & 0x3F;
    let rc = ((ordinal as u32 >> 2) & 3) << 13;
    let denormal_controls = if ordinal & 1 == 0 {
        0
    } else {
        (1 << 6) | (1 << 15)
    };
    ArithmeticState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(case.kind(), register)),
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
        // All six exception masks remain set. The CPU-level JIT boundary
        // rejects source replay when any mask is clear, preventing host
        // SIGFPE while preserving MXCSR status, RC, DAZ, and FTZ coverage.
        mxcsr: 0x1F80 | prior_status | rc | denormal_controls,
    }
}

#[cfg(target_arch = "x86_64")]
fn finite_normal_vector(kind: FpKind, register: usize, ordinal: usize) -> [u64; 8] {
    let seed = |lane: usize| register * 17 + ordinal * 11 + lane * 5;
    match kind.elem_bytes() {
        4 => std::array::from_fn(|word| {
            let bits = |lane| {
                let value = seed(lane);
                let sign = u32::from(value & 1 != 0) << 31;
                let exponent = (124 + ((value >> 1) % 7)) as u32;
                sign | (exponent << 23)
            };
            u64::from(bits(word * 2)) | (u64::from(bits(word * 2 + 1)) << 32)
        }),
        8 => std::array::from_fn(|word| {
            let value = seed(word);
            let sign = u64::from(value & 1 != 0) << 63;
            let exponent = (1020 + ((value >> 1) % 7)) as u64;
            sign | (exponent << 52)
        }),
        _ => unreachable!("validated binary floating-point element width"),
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
    initial: &ArithmeticState,
    level: crate::smir::optimize::OptLevel,
) -> ArithmeticState {
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
    ArithmeticState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_gives_nan_precedence_over_same_lane_denormal_status() {
    let cases = native_cases();
    let case = cases[2];
    assert_eq!(
        case,
        NativeCase::NonEvex(NonEvexCase {
            form: NonEvexForm::LegacyRex,
            kind: FpKind::PackedF32,
            opcode: 0x58,
            l: false,
            dst: 9,
            src1: 9,
            src2: 11,
        })
    );
    let bytes = case.bytes();
    assert_eq!(bytes, [0x4F, 0x0F, 0x58, 0xCB]);
    let initial = initial_state(case, 2);
    assert_eq!(initial.mxcsr, 0x1F90);
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let result = interpret(&bytes, &initial, level);
        assert_eq!(
            result.mxcsr, 0x1FB1,
            "{level:?}: IE|UE|PE prior/final status, without DE"
        );
    }
}

fn divps_denormal_over_zero_case_and_state() -> (NativeCase, ArithmeticState) {
    let case = NativeCase::NonEvex(NonEvexCase {
        form: NonEvexForm::Legacy,
        kind: FpKind::PackedF32,
        opcode: 0x5E,
        l: false,
        dst: 1,
        src1: 1,
        src2: 3,
    });
    assert_eq!(native_cases()[352], case);
    assert_eq!(case.bytes(), [0x0F, 0x5E, 0xCB]);

    let initial = initial_state(case, 352);
    assert_eq!(initial.mxcsr, 0x1F80);
    assert_eq!(initial.vectors[1][0], 0x0000_0001_3F00_0000);
    assert_eq!(initial.vectors[1][1], 0x0080_0000_8000_0001);
    assert_eq!(initial.vectors[3][0], 0x0000_0000_3EAA_AAAB);
    assert_eq!(initial.vectors[3][1], 0x3F80_0000_8000_0000);
    (case, initial)
}

#[test]
fn lifted_divps_zero_divide_suppresses_same_lane_denormal_status() {
    let (case, initial) = divps_denormal_over_zero_case_and_state();
    let bytes = case.bytes();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let result = interpret(&bytes, &initial, level);
        assert_eq!(result.vectors[1][0], 0x7F80_0000_3FC0_0000);
        assert_eq!(result.vectors[1][1], 0x0080_0000_7F80_0000);
        assert_eq!(
            result.mxcsr & 0x3F,
            (1 << 2) | (1 << 5),
            "{level:?}: ZE|PE without lower-priority same-lane DE"
        );
    }
}

fn daz_vminps_case_and_state() -> (NativeCase, ArithmeticState) {
    let case = NativeCase::NonEvex(NonEvexCase {
        form: NonEvexForm::VexC5,
        kind: FpKind::PackedF32,
        opcode: 0x5D,
        l: false,
        dst: 1,
        src1: 2,
        src2: 1,
    });
    let bytes = case.bytes();
    assert_eq!(bytes, [0xC5, 0xE8, 0x5D, 0xC9]);

    let mut initial = initial_state(case, 0);
    initial.vectors[1] = [0; 8];
    initial.vectors[2] = [0; 8];
    // Lane 0 selects src2 because src1 is a QNaN. MXCSR.DAZ must first
    // transform the selected negative minimum subnormal into -0.0.
    initial.vectors[1][0] = 0x3F80_0000_8000_0001;
    initial.vectors[1][1] = 0x3F80_0000_3F80_0000;
    initial.vectors[2][0] = 0x3F80_0000_7FC1_2345;
    initial.vectors[2][1] = 0x3F80_0000_3F80_0000;
    initial.mxcsr = 0x1F80 | (1 << 6) | (1 << 15);
    (case, initial)
}

#[test]
fn lifted_vminps_daz_transforms_nan_selected_negative_denormal_src2() {
    let (case, initial) = daz_vminps_case_and_state();
    let bytes = case.bytes();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let result = interpret(&bytes, &initial, level);
        assert_eq!(
            result.vectors[1][0], 0x3F80_0000_8000_0000,
            "{level:?}: DAZ-selected src2"
        );
        assert_eq!(result.vectors[1][1], 0x3F80_0000_3F80_0000);
        assert_eq!(
            result.mxcsr & 0x3F,
            1,
            "{level:?}: invalid without denormal status"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ArithmeticState,
    level: crate::smir::optimize::OptLevel,
    avx_ymm16_vector_state: bool,
) -> ArithmeticState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(avx_ymm16_vector_state);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map binary FP arithmetic replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: if avx_ymm16_vector_state {
            X86_VECTOR_STATE_YMM16
        } else {
            X86_VECTOR_STATE_K64
        },
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
    ArithmeticState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_vminps_daz_matches_interpreter_without_requiring_avx512() {
    #[cfg(target_os = "macos")]
    if running_under_rosetta() {
        eprintln!(
            "skipping native VMINPS DAZ regression: Rosetta does not apply MXCSR.DAZ to \
             MIN/MAX src2 selected by a NaN source"
        );
        return;
    }
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VMINPS DAZ regression: host lacks AVX");
        return;
    }

    let (case, initial) = daz_vminps_case_and_state();
    let bytes = case.bytes();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let native = execute_native(&bytes, &initial, level, true);
        let interpreted = interpret(&bytes, &initial, level);
        assert_eq!(native, interpreted, "{level:?} {case:?} {bytes:02X?}");
        assert_eq!(native.vectors[1][0], 0x3F80_0000_8000_0000);
        assert_eq!(native.mxcsr & 0x3F, 1);
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_divps_zero_divide_precedence_matches_interpreter_without_requiring_avx512() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native DIVPS exception-priority regression: host lacks AVX");
        return;
    }

    let (case, initial) = divps_denormal_over_zero_case_and_state();
    let bytes = case.bytes();
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        let native = execute_native(&bytes, &initial, level, true);
        let interpreted = interpret(&bytes, &initial, level);
        assert_eq!(native, interpreted, "{level:?} {case:?} {bytes:02X?}");
        assert_eq!(native.mxcsr & 0x3F, (1 << 2) | (1 << 5));
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_all_1_056_non_evex_exact_normal_cases_use_avx_ymm16_and_match_interpreter() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native non-EVEX FP arithmetic differential: host lacks AVX");
        return;
    }

    let cases = non_evex_cases();
    assert_eq!(cases.len(), 528);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let native_case = NativeCase::NonEvex(case);
        let bytes = native_case.bytes();
        let requirements = x86_native_replay_feature_requirements(
            &function(&bytes),
            &std::collections::HashMap::new(),
        );
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(
            requirements.all_spans_support_avx_ymm16,
            "{case:?} {bytes:02X?}"
        );
        assert!(requirements.needs_avx, "{case:?} {bytes:02X?}");
        assert!(!requirements.needs_avx512bw, "{case:?} {bytes:02X?}");

        let mut initial = initial_state(native_case, ordinal);
        initial.vectors =
            std::array::from_fn(|register| finite_normal_vector(case.kind, register, ordinal));
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                execute_native(&bytes, &initial, level, true),
                interpret(&bytes, &initial, level),
                "{level:?} {case:?} {bytes:02X?}"
            );
            executions += 1;
        }
    }
    assert_eq!(executions, 1_056);
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_FP_ARITHMETIC_CHILD_RANGE";

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
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        if !case.host_supported() {
            continue;
        }
        let bytes = case.bytes();
        let initial = initial_state(case, ordinal);
        let level = if ordinal & 1 == 0 {
            crate::smir::optimize::OptLevel::O0
        } else {
            crate::smir::optimize::OptLevel::O2
        };
        assert_eq!(
            execute_native(&bytes, &initial, level, false),
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
        .expect("run isolated native binary FP arithmetic differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 3_888);
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
    let bytes = case.bytes();
    panic!(
        "isolated native binary FP arithmetic failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_all_ops_formats_controls_masks_aliases_and_mxcsr() {
    #[cfg(target_os = "macos")]
    if running_under_rosetta() {
        eprintln!(
            "skipping native binary FP arithmetic differential: Rosetta does not apply \
             MXCSR.DAZ to MIN/MAX src2 selected by a NaN source"
        );
        return;
    }
    if !std::is_x86_feature_detected!("avx")
        || !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
    {
        eprintln!("skipping native binary FP arithmetic differential: host lacks AVX/AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::fp_arithmetic_replay::\
         replay_matches_o0_o2_interpretation_for_all_ops_formats_controls_masks_aliases_and_mxcsr",
    );
}
