//! Native replay coverage for defined register-only VEX ROUND instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x0B0A_0908;
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoundKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl RoundKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::PackedF32 => 0x08,
            Self::PackedF64 => 0x09,
            Self::ScalarF32 => 0x0A,
            Self::ScalarF64 => 0x0B,
        }
    }

    fn scalar(self) -> bool {
        matches!(self, Self::ScalarF32 | Self::ScalarF64)
    }

    fn element_bits(self) -> u32 {
        match self {
            Self::PackedF32 | Self::ScalarF32 => 32,
            Self::PackedF64 | Self::ScalarF64 => 64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoundInstruction {
    kind: RoundKind,
    w: bool,
    l: bool,
    ignored_x_clear: bool,
    destination: u8,
    merge: u8,
    source: u8,
    immediate: u8,
}

fn encoding(instruction: RoundInstruction) -> [u8; 6] {
    assert!(instruction.destination < 16 && instruction.merge < 16 && instruction.source < 16);
    let mut p0 = 0xE3;
    if instruction.destination >= 8 {
        p0 &= !0x80;
    }
    if instruction.ignored_x_clear {
        p0 &= !0x40;
    }
    if instruction.source >= 8 {
        p0 &= !0x20;
    }
    let encoded_vvvv = if instruction.kind.scalar() {
        ((!instruction.merge) & 15) << 3
    } else {
        0x78
    };
    [
        0xC4,
        p0,
        (u8::from(instruction.w) << 7) | encoded_vvvv | (u8::from(instruction.l) << 2) | 1,
        instruction.kind.opcode(),
        0xC0 | ((instruction.destination & 7) << 3) | (instruction.source & 7),
        instruction.immediate,
    ]
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

fn representative(kind: RoundKind) -> RoundInstruction {
    RoundInstruction {
        kind,
        w: true,
        l: matches!(kind, RoundKind::PackedF32 | RoundKind::ScalarF32),
        ignored_x_clear: true,
        destination: 9,
        merge: 10,
        source: 11,
        immediate: 0xFD,
    }
}

#[test]
fn replay_features_require_exactly_avx_and_the_ymm16_state_boundary() {
    for kind in RoundKind::ALL {
        let bytes = encoding(representative(kind));
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{kind:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{kind:?}");
        assert!(requirements.needs_avx, "{kind:?}");
        assert!(!requirements.needs_avx2, "{kind:?}");
        assert!(!requirements.needs_f16c, "{kind:?}");
        assert!(!requirements.needs_sse3, "{kind:?}");
        assert!(!requirements.needs_avx512bw, "{kind:?}");
        assert!(!requirements.needs_avx512vl, "{kind:?}");
        assert!(!requirements.needs_avx512dq, "{kind:?}");
        assert!(!requirements.needs_avx512fp16, "{kind:?}");
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{kind:?}"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx"),
            "{kind:?}"
        );
    }
}

fn assert_admitted_and_emitted(
    instruction: RoundInstruction,
    level: crate::smir::optimize::OptLevel,
) {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = encoding(instruction);
    let mut function = function(&bytes);
    crate::smir::optimize::optimize_function(&mut function, level);
    assert!(
        is_native_clobber_safe(&function),
        "{level:?} {instruction:?} {bytes:02X?}"
    );
    assert!(
        x86_native_vector_uses_avx_ymm16_only_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        "{level:?} {instruction:?} {bytes:02X?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {instruction:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {instruction:?} {bytes:02X?}: {error:?}"));
    assert!(
        code.windows(bytes.len()).any(|window| window == bytes),
        "{level:?} {instruction:?} {bytes:02X?}"
    );
}

#[test]
fn replay_admits_and_emits_2560_immediate_prefix_register_and_alias_shapes() {
    let levels = [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ];
    let mut emitted = 0usize;

    for kind in RoundKind::ALL {
        for immediate in u8::MIN..=u8::MAX {
            let mut instruction = representative(kind);
            instruction.immediate = immediate;
            for level in levels {
                assert_admitted_and_emitted(instruction, level);
                emitted += 1;
            }
        }
    }

    let register_shapes = [
        (0, 1, 2),
        (1, 1, 2),
        (2, 1, 2),
        (9, 10, 11),
        (9, 9, 10),
        (9, 10, 9),
        (15, 15, 15),
        (8, 7, 15),
    ];
    for kind in RoundKind::ALL {
        for w in [false, true] {
            for l in [false, true] {
                for ignored_x_clear in [false, true] {
                    for (destination, merge, source) in register_shapes {
                        let instruction = RoundInstruction {
                            kind,
                            w,
                            l,
                            ignored_x_clear,
                            destination,
                            merge,
                            source,
                            immediate: 0xA5,
                        };
                        for level in levels {
                            assert_admitted_and_emitted(instruction, level);
                            emitted += 1;
                        }
                    }
                }
            }
        }
    }

    assert_eq!(emitted, 2_560);
}

#[test]
fn replay_fails_closed_without_exact_defined_source_provenance() {
    let bytes = encoding(representative(RoundKind::PackedF64));
    let base = function(&bytes);

    let mut candidates = Vec::new();
    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    candidates.push(missing);

    for invalid in [
        {
            let mut value = bytes;
            value[1] = (value[1] & 0xE0) | 2; // map 0F38
            value
        },
        {
            let mut value = bytes;
            value[2] &= !0x08; // packed VEX.vvvv != 1111b
            value
        },
        {
            let mut value = bytes;
            value[2] = (value[2] & !3) | 2; // wrong mandatory prefix
            value
        },
        {
            let mut value = bytes;
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
        representative(RoundKind::PackedF32),
        RoundInstruction {
            kind: RoundKind::ScalarF64,
            w: false,
            l: true,
            ignored_x_clear: false,
            destination: 12,
            merge: 13,
            source: 14,
            immediate: 4,
        },
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
        let mut expected = bytes.to_vec();
        expected.extend_from_slice(&upper_clear_postlude(instruction.destination));
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{instruction:?} {bytes:02X?}"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RoundState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn f32_source_words(seed: usize) -> [u64; 8] {
    let values = [
        0x3FC0_0000u32, // +1.5
        0x4020_0000,    // +2.5
        0xBFC0_0000,    // -1.5
        0xC020_0000,    // -2.5
        0x7FC0_0001,    // QNaN
        0x7F80_0001,    // SNaN
        0x0000_0001,    // minimum positive subnormal
        0x8000_0000,    // -0
        0x7F80_0000,    // +infinity
        0xFF80_0000,    // -infinity
        0x3F00_0000,    // +0.5
        0xBF00_0000,    // -0.5
        0x4B00_0001,    // 2^23 + 1, already integral
        0xCB00_0001,    // -(2^23 + 1), already integral
        0x0000_0000,    // +0
        0x3F80_0000,    // +1
    ];
    let mut words = [0u64; 8];
    for lane in 0..16 {
        let value = values[(lane + seed) % values.len()];
        words[lane / 2] |= u64::from(value) << ((lane % 2) * 32);
    }
    words
}

fn f64_source_words(seed: usize) -> [u64; 8] {
    let values = [
        0x3FF8_0000_0000_0000u64, // +1.5
        0x4004_0000_0000_0000,    // +2.5
        0xBFF8_0000_0000_0000,    // -1.5
        0xC004_0000_0000_0000,    // -2.5
        0x7FF8_0000_0000_0001,    // QNaN
        0x7FF0_0000_0000_0001,    // SNaN
        0x0000_0000_0000_0001,    // minimum positive subnormal
        0x8000_0000_0000_0000,    // -0
        0x7FF0_0000_0000_0000,    // +infinity
        0xFFF0_0000_0000_0000,    // -infinity
        0x3FE0_0000_0000_0000,    // +0.5
        0xBFE0_0000_0000_0000,    // -0.5
        0x4340_0000_0000_0001,    // 2^53 + 2, already integral
        0xC340_0000_0000_0001,    // -(2^53 + 2), already integral
        0x0000_0000_0000_0000,    // +0
        0x3FF0_0000_0000_0000,    // +1
    ];
    std::array::from_fn(|lane| values[(lane + seed) % values.len()])
}

fn initial_state(instruction: RoundInstruction, seed: usize, mxcsr: u32) -> RoundState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (seed as u64).wrapping_mul(0x8040_2010_0804_0201)
        })
    });
    if instruction.kind.scalar() {
        let source = usize::from(instruction.source);
        if instruction.kind.element_bits() == 32 {
            let value = f32_source_words(seed)[0] & u64::from(u32::MAX);
            vectors[source][0] = (vectors[source][0] & !u64::from(u32::MAX)) | value;
        } else {
            vectors[source][0] = f64_source_words(seed)[0];
        }
    } else {
        let words = if instruction.l { 4 } else { 2 };
        let source = usize::from(instruction.source);
        let values = if instruction.kind.element_bits() == 32 {
            f32_source_words(seed)
        } else {
            f64_source_words(seed)
        };
        vectors[source][..words].copy_from_slice(&values[..words]);
    }

    RoundState {
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
    initial: &RoundState,
    level: crate::smir::optimize::OptLevel,
) -> RoundState {
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
    RoundState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_o0_o2_obeys_round_control_precision_suppression_daz_and_merge_zeroing() {
    let expectations = [
        (0u8, 0x4000_0000u64, 0x4000_0000_0000_0000u64),
        (1, 0x3F80_0000, 0x3FF0_0000_0000_0000),
        (2, 0x4000_0000, 0x4000_0000_0000_0000),
        (3, 0x3F80_0000, 0x3FF0_0000_0000_0000),
    ];
    for kind in [RoundKind::ScalarF32, RoundKind::ScalarF64] {
        for (mode, expected_f32, expected_f64) in expectations {
            for dynamic in [false, true] {
                for suppress_precision in [false, true] {
                    let mut instruction = representative(kind);
                    instruction.immediate =
                        if dynamic { 4 | 3 } else { mode } | (u8::from(suppress_precision) << 3);
                    let bytes = encoding(instruction);
                    let mut initial = initial_state(
                        instruction,
                        usize::from(mode),
                        0x1F80 | (u32::from(mode) << 13),
                    );
                    if kind == RoundKind::ScalarF32 {
                        let source = usize::from(instruction.source);
                        initial.vectors[source][0] =
                            (initial.vectors[source][0] & !u64::from(u32::MAX)) | 0x3FC0_0000;
                    } else {
                        initial.vectors[usize::from(instruction.source)][0] = 0x3FF8_0000_0000_0000;
                    }
                    for level in [
                        crate::smir::optimize::OptLevel::O0,
                        crate::smir::optimize::OptLevel::O2,
                    ] {
                        let actual = interpret(&bytes, &initial, level);
                        let expected = if kind == RoundKind::ScalarF32 {
                            expected_f32
                        } else {
                            expected_f64
                        };
                        let mask = if kind == RoundKind::ScalarF32 {
                            u64::from(u32::MAX)
                        } else {
                            u64::MAX
                        };
                        assert_eq!(
                            actual.vectors[usize::from(instruction.destination)][0] & mask,
                            expected,
                            "{level:?} {instruction:?}"
                        );
                        assert_eq!(
                            actual.mxcsr & (1 << 5),
                            if suppress_precision { 0 } else { 1 << 5 },
                            "{level:?} {instruction:?}"
                        );
                        assert_eq!(actual.gprs, initial.gprs);
                        assert_eq!(actual.masks, initial.masks);
                        assert_eq!(actual.rflags, initial.rflags);
                        let merge = initial.vectors[usize::from(instruction.merge)];
                        if kind == RoundKind::ScalarF32 {
                            assert_eq!(
                                actual.vectors[usize::from(instruction.destination)][0]
                                    & !u64::from(u32::MAX),
                                merge[0] & !u64::from(u32::MAX),
                                "{level:?} {instruction:?}"
                            );
                        }
                        assert_eq!(
                            actual.vectors[usize::from(instruction.destination)][1],
                            merge[1],
                            "{level:?} {instruction:?}"
                        );
                        assert_eq!(
                            actual.vectors[usize::from(instruction.destination)][2..],
                            [0; 6],
                            "{level:?} {instruction:?}"
                        );
                    }
                }
            }
        }
    }

    for kind in [RoundKind::ScalarF32, RoundKind::ScalarF64] {
        let mut instruction = representative(kind);
        instruction.immediate = 2;
        let bytes = encoding(instruction);
        for daz in [false, true] {
            let mut initial =
                initial_state(instruction, 0, 0x1F80 | (u32::from(daz) << 6) | (1 << 15));
            initial.vectors[usize::from(instruction.source)][0] = 1;
            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let actual = interpret(&bytes, &initial, level);
                let expected = if daz {
                    0
                } else if kind == RoundKind::ScalarF32 {
                    u64::from(1.0f32.to_bits())
                } else {
                    1.0f64.to_bits()
                };
                let mask = if kind == RoundKind::ScalarF32 {
                    u64::from(u32::MAX)
                } else {
                    u64::MAX
                };
                assert_eq!(
                    actual.vectors[usize::from(instruction.destination)][0] & mask,
                    expected,
                    "{level:?} {kind:?} daz={daz}"
                );
                assert_eq!(actual.mxcsr & (1 << 1), 0, "{level:?} {kind:?}");
                assert_eq!(
                    actual.mxcsr & (1 << 5),
                    if daz { 0 } else { 1 << 5 },
                    "{level:?} {kind:?}"
                );
            }
        }
    }

    for kind in [RoundKind::ScalarF32, RoundKind::ScalarF64] {
        let mut instruction = representative(kind);
        instruction.immediate = 8;
        let bytes = encoding(instruction);
        let (quiet, signaling, quieted) = if kind == RoundKind::ScalarF32 {
            (0x7FC0_0001, 0x7F80_0001, 0x7FC0_0001)
        } else {
            (
                0x7FF8_0000_0000_0001,
                0x7FF0_0000_0000_0001,
                0x7FF8_0000_0000_0001,
            )
        };
        for (source, expected, invalid) in [(quiet, quiet, 0), (signaling, quieted, 1)] {
            let mut initial = initial_state(instruction, 0, 0x1F80);
            initial.vectors[usize::from(instruction.source)][0] = source;
            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let actual = interpret(&bytes, &initial, level);
                let mask = if kind == RoundKind::ScalarF32 {
                    u64::from(u32::MAX)
                } else {
                    u64::MAX
                };
                assert_eq!(
                    actual.vectors[usize::from(instruction.destination)][0] & mask,
                    expected,
                    "{level:?} {kind:?} source={source:#018X}"
                );
                assert_eq!(
                    actual.mxcsr & 1,
                    invalid,
                    "{level:?} {kind:?} source={source:#018X}"
                );
                assert_eq!(actual.mxcsr & (1 << 5), 0, "{level:?} {kind:?}");
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &RoundState,
    level: crate::smir::optimize::OptLevel,
) -> RoundState {
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
    let exec = ExecMem::new(&code).expect("map VEX round replay");
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
    RoundState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FP_ROUND_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    instruction: RoundInstruction,
    seed: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let forms = [
        (RoundKind::PackedF32, false, false, false, 1, 0, 2),
        (RoundKind::PackedF32, true, true, true, 9, 0, 10),
        (RoundKind::PackedF64, true, false, true, 9, 0, 9),
        (RoundKind::PackedF64, false, true, false, 15, 0, 15),
        (RoundKind::ScalarF32, false, false, false, 1, 2, 3),
        (RoundKind::ScalarF32, true, true, true, 9, 10, 11),
        (RoundKind::ScalarF32, true, false, false, 9, 9, 10),
        (RoundKind::ScalarF32, false, true, true, 9, 10, 9),
        (RoundKind::ScalarF64, false, false, true, 12, 13, 14),
        (RoundKind::ScalarF64, true, true, false, 15, 15, 15),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for &(kind, w, l, ignored_x_clear, destination, merge, source) in &forms {
            for control in 0u8..16 {
                let rc_values = if control & 4 != 0 {
                    &[0u32, 1, 2, 3][..]
                } else {
                    &[0u32][..]
                };
                for &rc in rc_values {
                    let high = [0x00, 0xA0, 0xF0][ordinal % 3];
                    let prior_status = 1 << (ordinal % 6);
                    let daz_ftz = if ordinal & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    cases.push(NativeCase {
                        level,
                        instruction: RoundInstruction {
                            kind,
                            w,
                            l,
                            ignored_x_clear,
                            destination,
                            merge,
                            source,
                            immediate: high | control,
                        },
                        seed: ordinal,
                        mxcsr: 0x1F80 | prior_status | (rc << 13) | daz_ftz,
                    });
                    ordinal += 1;
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
        .expect("run isolated native VEX round differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 800);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    // Raw source replay can terminate the child with SIGILL before Rust can
    // report assertion context. Bisect child ranges in O(log N) launches and
    // report the exact guest encoding without killing the parent test binary.
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
        "isolated native VEX round failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_rounding_exceptions_aliases_lig_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX round differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fp_round_replay::\
         replay_matches_o0_o2_interpretation_for_rounding_exceptions_aliases_lig_and_full_state",
    );
}
