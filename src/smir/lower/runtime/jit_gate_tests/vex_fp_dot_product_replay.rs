//! Native replay coverage for register-only AVX VEX floating-point dot products.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xD070;
const OPERANDS: [(u8, u8, u8); 10] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (15, 15, 15),
    (15, 8, 13),
    (13, 14, 15),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotKind {
    Ps128,
    Ps256,
    Pd128,
}

impl DotKind {
    const ALL: [Self; 3] = [Self::Ps128, Self::Ps256, Self::Pd128];

    fn opcode(self) -> u8 {
        match self {
            Self::Ps128 | Self::Ps256 => 0x40,
            Self::Pd128 => 0x41,
        }
    }

    fn ymm(self) -> bool {
        matches!(self, Self::Ps256)
    }

    fn lanes_per_group(self) -> usize {
        match self {
            Self::Ps128 | Self::Ps256 => 4,
            Self::Pd128 => 2,
        }
    }

    fn groups(self) -> usize {
        usize::from(self.ymm()) + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DotCase {
    kind: DotKind,
    w: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    immediate: u8,
    clear_ignored_x: bool,
}

fn encoding(case: DotCase) -> [u8; 6] {
    assert!(case.destination < 16 && case.source1 < 16 && case.source2 < 16);
    let mut p0 = 0xE3;
    if case.destination >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.source2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(case.w) << 7)
            | (((!case.source1) & 0x0F) << 3)
            | (u8::from(case.kind.ymm()) << 2)
            | 1,
        case.kind.opcode(),
        0xC0 | ((case.destination & 7) << 3) | (case.source2 & 7),
        case.immediate,
    ]
}

fn exhaustive_cases() -> Vec<DotCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in DotKind::ALL {
        for w in [false, true] {
            for immediate in u8::MIN..=u8::MAX {
                let (destination, source1, source2) = OPERANDS[ordinal % OPERANDS.len()];
                cases.push(DotCase {
                    kind,
                    w,
                    destination,
                    source1,
                    source2,
                    immediate,
                    clear_ignored_x: ordinal & 1 != 0,
                });
                ordinal += 1;
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
fn replay_features_require_avx_and_select_the_ymm16_state_bridge() {
    for kind in DotKind::ALL {
        for w in [false, true] {
            let case = DotCase {
                kind,
                w,
                destination: 13,
                source1: 14,
                source2: 15,
                immediate: 0xA5,
                clear_ignored_x: true,
            };
            let function = function(&encoding(case));
            let excluded = std::collections::HashMap::new();
            let spans = crate::smir::ir::x86_vex_fp_dot_product_replay_spans(
                &function.blocks[0],
                &function.x86_instruction_bytes,
            );
            assert_eq!(spans.len(), 1, "{case:?}");
            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any, "{case:?}");
            assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
            assert!(requirements.needs_avx, "{case:?}");
            assert!(!requirements.needs_avx2, "{case:?}");
            assert!(!requirements.needs_sse3, "{case:?}");
            assert!(!requirements.needs_fma, "{case:?}");
            assert!(!requirements.needs_fma4, "{case:?}");
            assert!(!requirements.needs_avx512bw, "{case:?}");
            assert!(!requirements.needs_avx512vl, "{case:?}");
            assert!(!requirements.needs_avx512dq, "{case:?}");
            assert!(!requirements.needs_avx512fp16, "{case:?}");
            assert!(!requirements.needs_avx512cd, "{case:?}");
            assert!(!requirements.needs_gfni, "{case:?}");
            assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
            assert!(!requirements.needs_pclmulqdq, "{case:?}");
            assert!(!requirements.needs_vpclmulqdq, "{case:?}");
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));

            #[cfg(target_arch = "x86_64")]
            let expected = std::is_x86_feature_detected!("avx");
            #[cfg(not(target_arch = "x86_64"))]
            let expected = false;
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                expected,
                "{case:?}"
            );
        }
    }
}

#[test]
fn replay_admits_and_emits_238_o0_o2_width_wig_alias_extension_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        encoding(DotCase {
            kind: DotKind::Ps256,
            w: true,
            destination: 15,
            source1: 14,
            source2: 13,
            immediate: 0x5A,
            clear_ignored_x: true,
        }),
        [0xC4, 0x03, 0x8D, 0x40, 0xFD, 0x5A]
    );

    let cases: Vec<_> = exhaustive_cases().into_iter().step_by(13).collect();
    assert_eq!(cases.len(), 119);
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
    assert_eq!(lowered, 238);

    let case = DotCase {
        kind: DotKind::Ps256,
        w: true,
        destination: 1,
        source1: 2,
        source2: 3,
        immediate: 0xA5,
        clear_ignored_x: true,
    };
    let bytes = encoding(case);
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
    assert!(!is_native_clobber_safe(&function(&memory_bytes)));

    let invalid_l1 = encoding(DotCase {
        kind: DotKind::Ps256,
        ..case
    });
    let mut invalid_l1 = invalid_l1;
    invalid_l1[3] = 0x41;
    let mut invalid_metadata = function(&bytes);
    invalid_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&invalid_l1).unwrap(),
    );
    assert!(!is_native_clobber_safe(&invalid_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DotState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn set_f32_lane(vector: &mut [u64; 8], lane: usize, bits: u32) {
    let word = lane / 2;
    let shift = (lane % 2) * 32;
    vector[word] = (vector[word] & !(u64::from(u32::MAX) << shift)) | (u64::from(bits) << shift);
}

fn f32_lane(vector: &[u64; 8], lane: usize) -> u32 {
    (vector[lane / 2] >> ((lane % 2) * 32)) as u32
}

fn initial_state(case: DotCase, ordinal: usize) -> DotState {
    let mut vectors = [[0u64; 8]; 32];
    for (register, vector) in vectors.iter_mut().enumerate() {
        match case.kind {
            DotKind::Ps128 | DotKind::Ps256 => {
                for lane in 0..16 {
                    let value = 1 + ((register * 5 + lane * 3 + ordinal) % 7);
                    set_f32_lane(vector, lane, (value as f32).to_bits());
                }
            }
            DotKind::Pd128 => {
                for (lane, word) in vector.iter_mut().enumerate() {
                    let value = 1 + ((register * 5 + lane * 3 + ordinal) % 7);
                    *word = (value as f64).to_bits();
                }
            }
        }
    }

    DotState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
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
        // Every exception remains masked. Exact positive-integer products and
        // sums preserve all pre-existing status, RC, DAZ, and FTZ bits.
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn architectural_expected(case: DotCase, initial: &DotState) -> DotState {
    let mut destination = [0u64; 8];
    let lanes = case.kind.lanes_per_group();
    for group in 0..case.kind.groups() {
        let mut total = 0u32;
        for lane in 0..lanes {
            if case.immediate & (1 << (lane + 4)) == 0 {
                continue;
            }
            let vector_lane = group * lanes + lane;
            let product = match case.kind {
                DotKind::Ps128 | DotKind::Ps256 => {
                    let first = f32::from_bits(f32_lane(
                        &initial.vectors[usize::from(case.source1)],
                        vector_lane,
                    ));
                    let second = f32::from_bits(f32_lane(
                        &initial.vectors[usize::from(case.source2)],
                        vector_lane,
                    ));
                    debug_assert_eq!(first.fract(), 0.0);
                    debug_assert_eq!(second.fract(), 0.0);
                    (first as u32) * (second as u32)
                }
                DotKind::Pd128 => {
                    let first =
                        f64::from_bits(initial.vectors[usize::from(case.source1)][vector_lane]);
                    let second =
                        f64::from_bits(initial.vectors[usize::from(case.source2)][vector_lane]);
                    debug_assert_eq!(first.fract(), 0.0);
                    debug_assert_eq!(second.fract(), 0.0);
                    (first as u32) * (second as u32)
                }
            };
            total += product;
        }
        for lane in 0..lanes {
            if case.immediate & (1 << lane) == 0 {
                continue;
            }
            let vector_lane = group * lanes + lane;
            match case.kind {
                DotKind::Ps128 | DotKind::Ps256 => {
                    set_f32_lane(&mut destination, vector_lane, (total as f32).to_bits());
                }
                DotKind::Pd128 => {
                    destination[vector_lane] = (total as f64).to_bits();
                }
            }
        }
    }

    let mut expected = initial.clone();
    expected.vectors[usize::from(case.destination)] = destination;
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

fn interpret(bytes: &[u8], initial: &DotState, level: crate::smir::optimize::OptLevel) -> DotState {
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
    DotState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_all_1536_immediates_widths_wig_aliases_and_upper_zeroing() {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
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

fn exception_state(case: DotCase, first: [u32; 4], second: [u32; 4], mxcsr: u32) -> DotState {
    let mut state = initial_state(case, 0);
    state.mxcsr = mxcsr;
    state.vectors[usize::from(case.source1)] = [0; 8];
    state.vectors[usize::from(case.source2)] = [0; 8];
    for lane in 0..4 {
        set_f32_lane(
            &mut state.vectors[usize::from(case.source1)],
            lane,
            first[lane],
        );
        set_f32_lane(
            &mut state.vectors[usize::from(case.source2)],
            lane,
            second[lane],
        );
    }
    state
}

fn exception_cases() -> Vec<(DotCase, DotState, u32, Option<u32>)> {
    let base = DotCase {
        kind: DotKind::Ps128,
        w: false,
        destination: 1,
        source1: 2,
        source2: 3,
        immediate: 0x10,
        clear_ignored_x: false,
    };
    let snan = 0x7F80_0001;
    let one = 1.0f32.to_bits();
    let zero = 0.0f32.to_bits();
    let infinity = f32::INFINITY.to_bits();

    let mut cases = vec![
        // Input masks suppress all computation and therefore every exception.
        (
            DotCase {
                immediate: 0,
                ..base
            },
            exception_state(base, [snan; 4], [snan; 4], 0x1F80),
            0,
            None,
        ),
        // 0 * infinity is invalid. A zero output mask hides the
        // implementation-dependent NaN payload while retaining MXCSR.IE.
        (
            base,
            exception_state(
                base,
                [zero, snan, snan, snan],
                [infinity, snan, snan, snan],
                0x1F80,
            ),
            1,
            None,
        ),
        // An exact subnormal input is a denormal operand but neither
        // underflows nor loses precision.
        (
            base,
            exception_state(
                base,
                [f32::from_bits(1).to_bits(), snan, snan, snan],
                [one, snan, snan, snan],
                0x1F80,
            ),
            1 << 1,
            None,
        ),
        // MXCSR.DAZ converts a denormal input to +0.0 before arithmetic and
        // suppresses the denormal-operand exception.
        (
            DotCase {
                immediate: 0x11,
                ..base
            },
            exception_state(
                base,
                [f32::from_bits(1).to_bits(), snan, snan, snan],
                [one, snan, snan, snan],
                0x1F80 | (1 << 6),
            ),
            0,
            Some(zero),
        ),
        // MAX * 2 overflows and is inexact.
        (
            base,
            exception_state(
                base,
                [f32::MAX.to_bits(), snan, snan, snan],
                [2.0f32.to_bits(), snan, snan, snan],
                0x1F80,
            ),
            (1 << 3) | (1 << 5),
            None,
        ),
        // MIN_NORMAL * 0.1 is tiny and inexact. The resulting subnormal then
        // becomes an input to the architecturally subsequent addition stage,
        // which also accrues the denormal-operand flag.
        (
            base,
            exception_state(
                base,
                [f32::MIN_POSITIVE.to_bits(), snan, snan, snan],
                [0.1f32.to_bits(), snan, snan, snan],
                0x1F80,
            ),
            (1 << 1) | (1 << 4) | (1 << 5),
            None,
        ),
        // MXCSR.FTZ flushes the tiny inexact product to +0.0 after setting
        // underflow and precision, so the subsequent addition does not see a
        // denormal operand.
        (
            DotCase {
                immediate: 0x11,
                ..base
            },
            exception_state(
                base,
                [f32::MIN_POSITIVE.to_bits(), snan, snan, snan],
                [0.1f32.to_bits(), snan, snan, snan],
                0x1F80 | (1 << 15),
            ),
            (1 << 4) | (1 << 5),
            Some(zero),
        ),
        // Intel specifies (p0 + p1) + (p2 + p3), not a sequential reduction.
        // With L = 2^100, (L + 1) + (-L + 1) rounds to L + -L = 0,
        // whereas ((L + 1) + -L) + 1 would produce 1. Both pair sums are
        // inexact and therefore accrue MXCSR.PE.
        (
            DotCase {
                immediate: 0xF1,
                ..base
            },
            exception_state(
                base,
                [
                    2.0f32.powi(100).to_bits(),
                    one,
                    (-2.0f32.powi(100)).to_bits(),
                    one,
                ],
                [one; 4],
                0x1F80,
            ),
            1 << 5,
            Some(zero),
        ),
    ];

    // 1 + 2^-24 is exactly halfway between adjacent binary32 values. MXCSR.RC
    // selects the result while every mode accrues the precision flag.
    for rc in 0u32..4 {
        let case = DotCase {
            immediate: 0x31,
            ..base
        };
        let expected = if rc == 2 { 0x3F80_0001 } else { 0x3F80_0000 };
        cases.push((
            case,
            exception_state(
                case,
                [one, f32::from_bits(0x3380_0000).to_bits(), snan, snan],
                [one, one, snan, snan],
                0x1F80 | (rc << 13),
            ),
            1 << 5,
            Some(expected),
        ));
    }
    cases
}

#[test]
fn interpreter_preserves_dot_product_stage_order_masks_rounding_and_mxcsr_status() {
    for (case, initial, expected_status, expected_lane0) in exception_cases() {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let result = interpret(&bytes, &initial, level);
            assert_eq!(
                result.mxcsr & 0x3F,
                expected_status,
                "{level:?} {case:?} {bytes:02X?}"
            );
            if let Some(expected_lane0) = expected_lane0 {
                assert_eq!(
                    f32_lane(&result.vectors[usize::from(case.destination)], 0),
                    expected_lane0,
                    "{level:?} {case:?} {bytes:02X?}"
                );
            } else {
                assert_eq!(
                    result.vectors[usize::from(case.destination)],
                    [0; 8],
                    "{level:?} {case:?} {bytes:02X?}"
                );
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &DotState,
    level: crate::smir::optimize::OptLevel,
) -> DotState {
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
    let exec = ExecMem::new(&code).expect("map VEX floating-point dot-product replay");
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
    DotState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP_DOT_PRODUCT_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[DotCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
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
        .expect("run isolated native VEX floating-point dot-product differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX floating-point dot-product differential: host lacks AVX");
        return;
    }
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
    eprintln!(
        "executing {} native VEX floating-point dot-product cases",
        cases.len()
    );
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
        "isolated native VEX floating-point dot-product failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_all_1536_immediates_widths_wig_aliases_and_full_state() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp_dot_product_replay::\
         replay_matches_intel_o0_o2_all_1536_immediates_widths_wig_aliases_and_full_state",
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_mxcsr_exception_status_and_rounding_edges() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX floating-point dot-product MXCSR edges: host lacks AVX");
        return;
    }
    for (case, initial, expected_status, expected_lane0) in exception_cases() {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let interpreted = interpret(&bytes, &initial, level);
            let native = execute_native(&bytes, &initial, level);
            assert_eq!(native, interpreted, "{level:?} {case:?} {bytes:02X?}");
            assert_eq!(
                native.mxcsr & 0x3F,
                expected_status,
                "{level:?} {case:?} {bytes:02X?}"
            );
            if let Some(expected_lane0) = expected_lane0 {
                assert_eq!(
                    f32_lane(&native.vectors[usize::from(case.destination)], 0),
                    expected_lane0,
                    "{level:?} {case:?} {bytes:02X?}"
                );
            }
        }
    }
}
