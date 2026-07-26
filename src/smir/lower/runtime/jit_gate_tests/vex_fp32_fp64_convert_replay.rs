//! Native replay coverage for register-only AVX VEX `VCVTPS2PD` and
//! `VCVTPD2PS`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConvertKind {
    Widen,
    Narrow,
}

impl ConvertKind {
    const ALL: [Self; 2] = [Self::Widen, Self::Narrow];

    fn pp(self) -> u8 {
        match self {
            Self::Widen => 0,
            Self::Narrow => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn l(self) -> u8 {
        u8::from(self == Self::V256)
    }

    fn result_qwords(self, kind: ConvertKind) -> usize {
        match (kind, self) {
            (ConvertKind::Widen, Self::V128) => 2,
            (ConvertKind::Widen, Self::V256) => 4,
            (ConvertKind::Narrow, Self::V128) => 1,
            (ConvertKind::Narrow, Self::V256) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertCase {
    kind: ConvertKind,
    width: Width,
    form: EncodingForm,
    destination: u8,
    source: u8,
}

fn encoding(case: ConvertCase) -> Vec<u8> {
    let ConvertCase {
        kind,
        width,
        form,
        destination,
        source,
    } = case;
    assert!(destination < 16 && source < 16);
    let p1_low = 0x78 | (width.l() << 2) | kind.pp();

    match form {
        EncodingForm::VexC5 => {
            assert!(source < 8);
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 }) | p1_low,
                0x5A,
                0xC0 | ((destination & 7) << 3) | source,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if destination >= 8 {
                p0 &= !0x80;
            }
            if form == EncodingForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if source >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if form == EncodingForm::VexC4W1IgnoredX {
                    0x80
                } else {
                    0
                }) | p1_low,
                0x5A,
                0xC0 | ((destination & 7) << 3) | (source & 7),
            ]
        }
    }
}

fn cases() -> Vec<ConvertCase> {
    let mut cases = Vec::new();
    for kind in ConvertKind::ALL {
        for width in [Width::V128, Width::V256] {
            for form in [
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ] {
                let operands: &[(u8, u8)] = if form == EncodingForm::VexC5 {
                    &[(1, 2), (9, 2), (1, 1), (7, 7)]
                } else {
                    &[(1, 2), (9, 10), (15, 15), (1, 1), (1, 9), (9, 1)]
                };
                for &(destination, source) in operands {
                    cases.push(ConvertCase {
                        kind,
                        width,
                        form,
                        destination,
                        source,
                    });
                }
            }
        }
    }
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

#[test]
fn replay_features_select_the_avx_ymm16_boundary_without_avx512() {
    for case in [
        ConvertCase {
            kind: ConvertKind::Widen,
            width: Width::V128,
            form: EncodingForm::VexC5,
            destination: 9,
            source: 2,
        },
        ConvertCase {
            kind: ConvertKind::Widen,
            width: Width::V256,
            form: EncodingForm::VexC4W1IgnoredX,
            destination: 9,
            source: 10,
        },
        ConvertCase {
            kind: ConvertKind::Narrow,
            width: Width::V128,
            form: EncodingForm::VexC4W0,
            destination: 1,
            source: 9,
        },
        ConvertCase {
            kind: ConvertKind::Narrow,
            width: Width::V256,
            form: EncodingForm::VexC4W1IgnoredX,
            destination: 15,
            source: 15,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
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
fn replay_feature_aggregation_is_monotonic_across_vex_and_evex_spans() {
    let vex = encoding(ConvertCase {
        kind: ConvertKind::Widen,
        width: Width::V256,
        form: EncodingForm::VexC4W1IgnoredX,
        destination: 9,
        source: 10,
    });
    // EVEX.512.0F.W0 5A /r VCVTPS2PD zmm1, ymm2.
    let evex = [0x62, 0xF1, 0x7C, 0x48, 0x5A, 0xCA];

    for (first, second) in [(&vex[..], &evex[..]), (&evex[..], &vex[..])] {
        let mut mixed = function_at(first, BlockId(0), PC);
        let mut trailing = function_at(second, BlockId(1), PC + 0x100);
        mixed.add_block(trailing.blocks.remove(0));
        mixed
            .x86_instruction_bytes
            .extend(trailing.x86_instruction_bytes);

        let requirements =
            x86_native_replay_feature_requirements(&mixed, &std::collections::HashMap::new());
        assert!(requirements.any);
        assert!(!requirements.all_spans_support_avx_ymm16);
        assert!(requirements.needs_avx);
        assert!(requirements.needs_avx512bw);
        assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
            &mixed,
            &std::collections::HashMap::new()
        ));
    }
}

#[test]
fn replay_admits_and_emits_all_4608_legal_register_encodings_at_o2() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut admitted = 0usize;
    for pp in [0u8, 1] {
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x78 != 0x78 || p1 & 0x03 != pp {
                continue;
            }
            for modrm in 0xC0..=0xFF {
                let bytes = [0xC5, p1, 0x5A, modrm];
                assert_admitted_and_emitted_at_o2(&bytes, &mut admitted);
            }
        }

        for p0 in u8::MIN..=u8::MAX {
            if p0 & 0x1F != 1 {
                continue;
            }
            for p1 in u8::MIN..=u8::MAX {
                if p1 & 0x78 != 0x78 || p1 & 0x03 != pp {
                    continue;
                }
                for modrm in 0xC0..=0xFF {
                    let bytes = [0xC4, p0, p1, 0x5A, modrm];
                    assert_admitted_and_emitted_at_o2(&bytes, &mut admitted);
                }
            }
        }
    }
    assert_eq!(admitted, 4_608);

    fn assert_admitted_and_emitted_at_o2(bytes: &[u8], admitted: &mut usize) {
        let mut function = function(bytes);
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert!(
            code.windows(bytes.len()).any(|window| window == bytes),
            "{bytes:02X?}"
        );
        *admitted += 1;
    }
}

#[test]
fn replay_survives_o0_o2_aliases_extensions_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 64);
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
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_avx_ymm16_vector_state(true);
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
    assert_eq!(lowered, 128);

    let bytes = encoding(ConvertCase {
        kind: ConvertKind::Widen,
        width: Width::V256,
        form: EncodingForm::VexC4W0,
        destination: 9,
        source: 10,
    });
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory = bytes.clone();
    *memory.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = base.clone();
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));

    let mut reserved_vvvv = bytes;
    reserved_vvvv[2] &= !0x08;
    let mut reserved_metadata = base;
    reserved_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&reserved_vvvv).unwrap(),
    );
    assert!(!is_native_clobber_safe(&reserved_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConvertState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const F32_PATTERNS: [u32; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x0000_0001,
    0x007F_FFFF,
    0x0080_0000,
    0x3F80_0000,
    0xBF80_0000,
    0x3F00_0000,
    0x7F7F_FFFF,
    0xFF7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0xFFC1_2345,
    0x7F81_2345,
    0xFF81_2345,
];

const F64_PATTERNS: [u64; 20] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x000F_FFFF_FFFF_FFFF,
    0x0010_0000_0000_0000,
    0x3690_0000_0000_0000,
    0x36A0_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0xBFF0_0000_0000_0000,
    0x3FE0_0000_0000_0000,
    0x47EF_FFFF_E000_0000,
    0x47EF_FFFF_F000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0xFFEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_1234_5678_9ABC,
    0xFFF8_1234_5678_9ABC,
    0x7FF0_1234_5678_9ABC,
    0xFFF0_1234_5678_9ABC,
];

fn patterned_vector(kind: ConvertKind, register: usize, profile: usize) -> [u64; 8] {
    match kind {
        ConvertKind::Widen => {
            let lanes: [u32; 16] = std::array::from_fn(|lane| {
                F32_PATTERNS[(lane + register * 7 + profile * 3) % F32_PATTERNS.len()]
            });
            std::array::from_fn(|lane| {
                u64::from(lanes[lane * 2]) | (u64::from(lanes[lane * 2 + 1]) << 32)
            })
        }
        ConvertKind::Narrow => std::array::from_fn(|lane| {
            F64_PATTERNS[(lane + register * 7 + profile * 3) % F64_PATTERNS.len()]
        }),
    }
}

fn initial_state(kind: ConvertKind, profile: usize) -> ConvertState {
    ConvertState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(kind, register, profile)),
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
        rflags: 0x2 | 0x8D5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][profile % 4],
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
    initial: &ConvertState,
    level: crate::smir::optimize::OptLevel,
) -> ConvertState {
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
    ConvertState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_preserves_non_destinations_and_applies_vex_upper_zeroing() {
    for (ordinal, case) in cases().into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(case.kind, ordinal);
        let o0 = interpret(&bytes, &initial, crate::smir::optimize::OptLevel::O0);
        let o2 = interpret(&bytes, &initial, crate::smir::optimize::OptLevel::O2);
        assert_eq!(o2, o0, "{case:?} {bytes:02X?}");
        assert_eq!(o0.gprs, initial.gprs, "{case:?}");
        assert_eq!(o0.masks, initial.masks, "{case:?}");
        assert_eq!(o0.rflags, initial.rflags, "{case:?}");
        for register in 0..32 {
            if register != usize::from(case.destination) {
                assert_eq!(
                    o0.vectors[register], initial.vectors[register],
                    "{case:?} register={register}"
                );
            }
        }
        assert!(
            o0.vectors[usize::from(case.destination)][case.width.result_qwords(case.kind)..]
                .iter()
                .all(|word| *word == 0),
            "{case:?} {bytes:02X?}"
        );
    }
}

#[test]
fn interpreter_matches_exact_finite_conversion_results_at_o0_and_o2() {
    const SOURCE_F32: [u32; 4] = [0x0000_0000, 0x8000_0000, 0x3F80_0000, 0xC020_0000];
    const EXPECTED_F64: [u64; 4] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xC004_0000_0000_0000,
    ];
    const SOURCE_F64: [u64; 4] = EXPECTED_F64;
    const EXPECTED_F32: [u64; 2] = [0x8000_0000_0000_0000, 0xC020_0000_3F80_0000];

    for kind in ConvertKind::ALL {
        for width in [Width::V128, Width::V256] {
            let case = ConvertCase {
                kind,
                width,
                form: EncodingForm::VexC4W1IgnoredX,
                destination: 9,
                source: 10,
            };
            let bytes = encoding(case);
            let mut initial = initial_state(kind, 0);
            initial.mxcsr = 0x1F80;
            initial.vectors[usize::from(case.source)] = match kind {
                ConvertKind::Widen => [
                    u64::from(SOURCE_F32[0]) | (u64::from(SOURCE_F32[1]) << 32),
                    u64::from(SOURCE_F32[2]) | (u64::from(SOURCE_F32[3]) << 32),
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
                ConvertKind::Narrow => [
                    SOURCE_F64[0],
                    SOURCE_F64[1],
                    SOURCE_F64[2],
                    SOURCE_F64[3],
                    0,
                    0,
                    0,
                    0,
                ],
            };

            let expected = match (kind, width) {
                (ConvertKind::Widen, Width::V128) => {
                    [EXPECTED_F64[0], EXPECTED_F64[1], 0, 0, 0, 0, 0, 0]
                }
                (ConvertKind::Widen, Width::V256) => [
                    EXPECTED_F64[0],
                    EXPECTED_F64[1],
                    EXPECTED_F64[2],
                    EXPECTED_F64[3],
                    0,
                    0,
                    0,
                    0,
                ],
                (ConvertKind::Narrow, Width::V128) => [EXPECTED_F32[0], 0, 0, 0, 0, 0, 0, 0],
                (ConvertKind::Narrow, Width::V256) => {
                    [EXPECTED_F32[0], EXPECTED_F32[1], 0, 0, 0, 0, 0, 0]
                }
            };

            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let actual = interpret(&bytes, &initial, level);
                assert_eq!(
                    actual.vectors[usize::from(case.destination)],
                    expected,
                    "{level:?} {case:?} {bytes:02X?}"
                );
                assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?} {case:?}");
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ConvertState,
    level: crate::smir::optimize::OptLevel,
) -> ConvertState {
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
    let exec = ExecMem::new(&code).expect("map VEX FP32/FP64 conversion replay");
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
    ConvertState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP32_FP64_CONVERT_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ConvertCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case.kind, ordinal);
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
        .expect("run isolated native VEX FP32/FP64 conversion differential")
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
    let case = cases[start];
    let bytes = encoding(case);
    panic!(
        "isolated native VEX FP32/FP64 conversion failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_widths_aliases_mxcsr_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX FP32/FP64 conversion differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp32_fp64_convert_replay::\
         replay_matches_o0_o2_interpretation_for_widths_aliases_mxcsr_and_full_state",
    );
}
