//! Native replay coverage for defined register-only VEX scalar FP conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A32_64;
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conversion {
    F64ToF32,
    F32ToF64,
}

impl Conversion {
    const ALL: [Self; 2] = [Self::F64ToF32, Self::F32ToF64];

    fn pp(self) -> u8 {
        match self {
            Self::F64ToF32 => 3,
            Self::F32ToF64 => 2,
        }
    }

    fn source_mask(self) -> u64 {
        match self {
            Self::F64ToF32 => u64::MAX,
            Self::F32ToF64 => u64::from(u32::MAX),
        }
    }

    fn result_mask(self) -> u64 {
        match self {
            Self::F64ToF32 => u64::from(u32::MAX),
            Self::F32ToF64 => u64::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4 { w: bool, ignored_x_clear: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertInstruction {
    conversion: Conversion,
    form: VexForm,
    destination: u8,
    merge: u8,
    source: u8,
}

fn encoding(instruction: ConvertInstruction) -> Vec<u8> {
    assert!(instruction.destination < 16 && instruction.merge < 16 && instruction.source < 16);
    let modrm = 0xC0 | ((instruction.destination & 7) << 3) | (instruction.source & 7);
    let encoded_vvvv = ((!instruction.merge) & 15) << 3;
    match instruction.form {
        VexForm::C5 => {
            assert!(instruction.source < 8, "C5 has no VEX.B extension");
            vec![
                0xC5,
                (if instruction.destination < 8 { 0x80 } else { 0 })
                    | encoded_vvvv
                    | instruction.conversion.pp(),
                0x5A,
                modrm,
            ]
        }
        VexForm::C4 { w, ignored_x_clear } => {
            let mut p0 = 0xE1;
            if instruction.destination >= 8 {
                p0 &= !0x80;
            }
            if ignored_x_clear {
                p0 &= !0x40;
            }
            if instruction.source >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (u8::from(w) << 7) | encoded_vvvv | instruction.conversion.pp(),
                0x5A,
                modrm,
            ]
        }
    }
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

fn representative(conversion: Conversion) -> ConvertInstruction {
    ConvertInstruction {
        conversion,
        form: VexForm::C4 {
            w: true,
            ignored_x_clear: true,
        },
        destination: 9,
        merge: 10,
        source: 11,
    }
}

#[test]
fn replay_features_require_exactly_avx_and_the_ymm16_state_boundary() {
    for conversion in Conversion::ALL {
        for form in [
            VexForm::C5,
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
        ] {
            let mut instruction = representative(conversion);
            instruction.form = form;
            if form == VexForm::C5 {
                instruction.source = 3;
            }
            let bytes = encoding(instruction);
            let function = function(&bytes);
            let excluded = std::collections::HashMap::new();
            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any, "{instruction:?}");
            assert!(requirements.all_spans_support_avx_ymm16, "{instruction:?}");
            assert!(requirements.needs_avx, "{instruction:?}");
            assert!(!requirements.needs_avx2, "{instruction:?}");
            assert!(!requirements.needs_f16c, "{instruction:?}");
            assert!(!requirements.needs_sse3, "{instruction:?}");
            assert!(!requirements.needs_avx512bw, "{instruction:?}");
            assert!(!requirements.needs_avx512vl, "{instruction:?}");
            assert!(!requirements.needs_avx512dq, "{instruction:?}");
            assert!(!requirements.needs_avx512fp16, "{instruction:?}");
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));

            let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
            assert_eq!(
                x86_native_replay_feature_requirements(&function, &excluded),
                X86NativeReplayFeatureRequirements::default(),
                "{instruction:?}"
            );

            #[cfg(target_arch = "x86_64")]
            assert_eq!(
                x86_native_vector_features_supported_excluding(
                    &function,
                    &std::collections::HashMap::new()
                ),
                std::is_x86_feature_detected!("avx"),
                "{instruction:?}"
            );
        }
    }
}

fn assert_admitted_and_emitted(bytes: &[u8], level: crate::smir::optimize::OptLevel) {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut function = function(bytes);
    crate::smir::optimize::optimize_function(&mut function, level);
    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
    assert!(
        x86_native_vector_uses_avx_ymm16_only_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        "{level:?} {bytes:02X?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
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
}

#[test]
fn replay_lifts_admits_and_emits_all_36864_defined_register_images_at_o2() {
    let mut emitted = 0usize;
    for encoded_r in [false, true] {
        for encoded_vvvv in 0u8..16 {
            for pp in [2u8, 3] {
                let p1 = (u8::from(encoded_r) << 7) | (encoded_vvvv << 3) | pp;
                for modrm in 0xC0u8..=0xFF {
                    assert_admitted_and_emitted(
                        &[0xC5, p1, 0x5A, modrm],
                        crate::smir::optimize::OptLevel::O2,
                    );
                    emitted += 1;
                }
            }
        }
    }
    for extension_bits in 0u8..8 {
        let p0 = (extension_bits << 5) | 1;
        for w in [false, true] {
            for encoded_vvvv in 0u8..16 {
                for pp in [2u8, 3] {
                    let p1 = (u8::from(w) << 7) | (encoded_vvvv << 3) | pp;
                    for modrm in 0xC0u8..=0xFF {
                        assert_admitted_and_emitted(
                            &[0xC4, p0, p1, 0x5A, modrm],
                            crate::smir::optimize::OptLevel::O2,
                        );
                        emitted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(emitted, 36_864);
}

#[test]
fn replay_fails_closed_without_exact_defined_source_provenance() {
    let bytes = encoding(representative(Conversion::F64ToF32));
    let base = function(&bytes);

    let mut candidates = Vec::new();
    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    candidates.push(missing);

    for invalid in [
        {
            let mut value = bytes.clone();
            value[2] |= 0x04; // VEX.L=1 is generation-dependent unpredictable
            value
        },
        {
            let mut value = bytes.clone();
            value[1] = (value[1] & 0xE0) | 2; // map 0F38
            value
        },
        {
            let mut value = bytes.clone();
            value[2] = (value[2] & !3) | 1; // wrong mandatory prefix
            value
        },
        {
            let mut value = bytes.clone();
            value[4] &= 0x3F; // memory source
            value
        },
    ] {
        let mut malformed = base.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&invalid).unwrap(),
        );
        candidates.push(malformed);
    }

    for candidate in candidates {
        assert!(!is_native_clobber_safe(&candidate));
        assert!(
            !x86_native_replay_feature_requirements(&candidate, &std::collections::HashMap::new())
                .any
        );
    }
}

fn upper_clear_postlude(destination: u8) -> Vec<u8> {
    assert!(destination < 16);
    let mut bytes = vec![0x9C, 0x50, 0x48, 0x8B, 0x45, X86_STATE_PTR_AT_RBP as u8];
    let upper = X86_GUEST_ZMM_OFFSET + i32::from(destination) * 64 + 32;
    for offset in (upper..upper + 32).step_by(8) {
        bytes.extend_from_slice(&[0x48, 0xC7, 0x80]);
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
    }
    bytes.extend_from_slice(&[0x58, 0x9D]);
    bytes
}

#[test]
fn ymm16_replay_clears_the_exact_state_backed_destination_upper_half() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    for instruction in [
        ConvertInstruction {
            conversion: Conversion::F64ToF32,
            form: VexForm::C5,
            destination: 9,
            merge: 10,
            source: 3,
        },
        representative(Conversion::F32ToF64),
    ] {
        let bytes = encoding(instruction);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer
            .lower_function(&function(&bytes))
            .unwrap_or_else(|error| panic!("{instruction:?} {bytes:02X?}: {error:?}"));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{instruction:?} {bytes:02X?}: {error:?}"));
        let mut expected = bytes.clone();
        expected.extend_from_slice(&upper_clear_postlude(instruction.destination));
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{instruction:?} {bytes:02X?}"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConvertState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn f64_inputs() -> [u64; 16] {
    [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
        0x3FF0_0000_1000_0000,
        0xBFF0_0000_1000_0000,
        0x47F0_0000_0000_0000,
        0x0010_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0x7FF0_0000_0000_0001,
        0x36A0_0000_0000_0000,
        0x380F_FFFF_E000_0000,
        0x3810_0000_0000_0000,
    ]
}

fn f32_inputs() -> [u64; 16] {
    [
        0x0000_0000,
        0x8000_0000,
        0x3F80_0000,
        0xBF80_0000,
        0x3FC0_0000,
        0xBFC0_0000,
        0x7F7F_FFFF,
        0x0080_0000,
        0x0000_0001,
        0x8000_0001,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC0_0001,
        0x7F80_0001,
        0x3F00_0000,
        0xBF00_0000,
    ]
}

fn input_bits(conversion: Conversion, seed: usize) -> u64 {
    match conversion {
        Conversion::F64ToF32 => f64_inputs()[seed % 16],
        Conversion::F32ToF64 => f32_inputs()[seed % 16],
    }
}

fn initial_state(instruction: ConvertInstruction, seed: usize, mxcsr: u32) -> ConvertState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (seed as u64).wrapping_mul(0x8040_2010_0804_0201)
        })
    });
    let source = usize::from(instruction.source);
    let mask = instruction.conversion.source_mask();
    vectors[source][0] =
        (vectors[source][0] & !mask) | (input_bits(instruction.conversion, seed) & mask);
    ConvertState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_9696,
            0,
            u64::MAX,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr,
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
    context.flags.materialize_all();

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
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_obeys_rounding_daz_status_merge_and_upper_zeroing() {
    for (rc, expected_positive, expected_negative) in [
        (0u32, 0x3F80_0000u64, 0xBF80_0000u64),
        (1, 0x3F80_0000, 0xBF80_0001),
        (2, 0x3F80_0001, 0xBF80_0000),
        (3, 0x3F80_0000, 0xBF80_0000),
    ] {
        let instruction = representative(Conversion::F64ToF32);
        let bytes = encoding(instruction);
        for (source, expected) in [
            (0x3FF0_0000_1000_0000, expected_positive),
            (0xBFF0_0000_1000_0000, expected_negative),
        ] {
            let mut initial = initial_state(instruction, 0, 0x1F80 | (rc << 13));
            initial.vectors[usize::from(instruction.source)][0] = source;
            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let actual = interpret(&bytes, &initial, level);
                let destination = usize::from(instruction.destination);
                let merge = initial.vectors[usize::from(instruction.merge)];
                assert_eq!(
                    actual.vectors[destination][0] & u64::from(u32::MAX),
                    expected,
                    "{level:?} rc={rc} source={source:#018X}"
                );
                assert_eq!(
                    actual.vectors[destination][0] & !u64::from(u32::MAX),
                    merge[0] & !u64::from(u32::MAX)
                );
                assert_eq!(actual.vectors[destination][1], merge[1]);
                assert_eq!(actual.vectors[destination][2..], [0; 6]);
                assert_eq!(actual.mxcsr & (1 << 5), 1 << 5);
                assert_eq!(actual.gprs, initial.gprs);
                assert_eq!(actual.masks, initial.masks);
                assert_eq!(actual.rflags, initial.rflags);
            }
        }
    }

    let widening = representative(Conversion::F32ToF64);
    let bytes = encoding(widening);
    for daz in [false, true] {
        let mut initial = initial_state(
            widening,
            0,
            0x1F80 | (u32::from(daz) << 6) | (3 << 13) | (1 << 15),
        );
        initial.vectors[usize::from(widening.source)][0] = 1;
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let actual = interpret(&bytes, &initial, level);
            let expected = if daz {
                0
            } else {
                f64::from(f32::from_bits(1)).to_bits()
            };
            assert_eq!(
                actual.vectors[usize::from(widening.destination)][0],
                expected,
                "{level:?} daz={daz}"
            );
            assert_eq!(
                actual.mxcsr & (1 << 1),
                if daz { 0 } else { 1 << 1 },
                "{level:?} daz={daz}"
            );
            assert_eq!(actual.mxcsr & !0x3F, initial.mxcsr & !0x3F);
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
    let exec = ExecMem::new(&code).expect("map VEX scalar FP-conversion replay");
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
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_FP_CONVERT_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    instruction: ConvertInstruction,
    seed: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let forms = [
        (VexForm::C5, 1, 2, 3),
        (VexForm::C5, 9, 10, 3),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            1,
            2,
            3,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            9,
            10,
            11,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: false,
            },
            9,
            9,
            10,
        ),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: true,
            },
            9,
            10,
            9,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            15,
            15,
            15,
        ),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for conversion in Conversion::ALL {
            for &(form, destination, merge, source) in &forms {
                for seed in 0usize..16 {
                    for rc in 0u32..4 {
                        let prior_status = 1 << (ordinal % 6);
                        let daz_ftz = if ordinal & 1 == 0 {
                            0
                        } else {
                            (1 << 6) | (1 << 15)
                        };
                        cases.push(NativeCase {
                            level,
                            instruction: ConvertInstruction {
                                conversion,
                                form,
                                destination,
                                merge,
                                source,
                            },
                            seed,
                            // All six exception masks remain set at the native
                            // boundary; RC, DAZ, FTZ, and sticky status vary.
                            mxcsr: 0x1F80 | prior_status | (rc << 13) | daz_ftz,
                        });
                        ordinal += 1;
                    }
                }
            }
        }
    }
    cases
}

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
    for case in &cases[range] {
        let bytes = encoding(case.instruction);
        let initial = initial_state(case.instruction, case.seed, case.mxcsr);
        assert_eq!(
            execute_native(&bytes, &initial, case.level),
            interpret(&bytes, &initial, case.level),
            "{case:?} bytes={bytes:02X?}"
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
        .expect("run isolated native VEX scalar FP-conversion differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 1_792);
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
    let bytes = encoding(case.instruction);
    panic!(
        "isolated native VEX scalar FP-conversion failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_rounding_exceptions_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar FP-conversion differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_fp_convert_replay::\
         replay_matches_o0_o2_interpretation_for_rounding_exceptions_aliases_and_full_state",
    );
}
