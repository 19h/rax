//! Native replay coverage for canonical register-only legacy scalar
//! floating-point flag comparisons.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, FunctionId, SourceArch, VReg, VecElementType, X86Reg};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, is_native_clobber_safe, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x2F2E;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    F32,
    F64,
}

impl Format {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    const fn bit_mask(self) -> u64 {
        match self {
            Self::F32 => u32::MAX as u64,
            Self::F64 => u64::MAX,
        }
    }

    const fn exponent_mask(self) -> u64 {
        match self {
            Self::F32 => 0x7F80_0000,
            Self::F64 => 0x7FF0_0000_0000_0000,
        }
    }

    const fn fraction_mask(self) -> u64 {
        match self {
            Self::F32 => 0x007F_FFFF,
            Self::F64 => 0x000F_FFFF_FFFF_FFFF,
        }
    }

    const fn quiet_bit(self) -> u64 {
        match self {
            Self::F32 => 0x0040_0000,
            Self::F64 => 0x0008_0000_0000_0000,
        }
    }

    const fn sign_mask(self) -> u64 {
        match self {
            Self::F32 => 0x8000_0000,
            Self::F64 => 0x8000_0000_0000_0000,
        }
    }
}

fn encoding(format: Format, opcode: u8, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(matches!(opcode, 0x2E | 0x2F));
    let mut bytes = Vec::new();
    if format == Format::F64 {
        bytes.push(0x66);
    }
    bytes.extend(rex);
    bytes.extend([0x0F, opcode, modrm]);
    bytes
}

fn canonical_encoding(
    format: Format,
    opcode: u8,
    first: u8,
    second: u8,
    rex_ignored_bits: Option<u8>,
) -> Vec<u8> {
    assert!(first < 16 && second < 16);
    let rex = rex_ignored_bits.map(|ignored| {
        0x40 | (ignored & 0x0A)
            | if first >= 8 { 0x04 } else { 0 }
            | if second >= 8 { 0x01 } else { 0 }
    });
    assert!(rex.is_some() || first < 8 && second < 8);
    encoding(
        format,
        opcode,
        rex,
        0xC0 | ((first & 7) << 3) | (second & 7),
    )
}

fn function(bytes: &[u8], level: OptLevel, halt: bool) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(if halt {
        Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        }
    } else {
        Terminator::Return { values: Vec::new() }
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy flag-compare provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_only_avx_and_the_ymm16_state_bridge() {
    let bytes = canonical_encoding(Format::F64, 0x2F, 9, 11, Some(0x0A));
    let function = function(&bytes, OptLevel::O2, false);
    let excluded = std::collections::HashMap::new();
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(&function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_avx = true;
    assert_eq!(requirements, expected);

    #[cfg(target_arch = "x86_64")]
    {
        let supported = std::is_x86_feature_detected!("avx");
        assert_eq!(requirements.x86_host_supported(), supported);
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            supported
        );
    }

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn all_13056_o0_o1_o2_canonical_register_graphs_lower_to_exact_source_bytes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for format in Format::ALL {
        for opcode in [0x2Eu8, 0x2F] {
            for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
                for modrm in 0xC0u8..=0xFF {
                    let bytes = encoding(format, opcode, rex, modrm);
                    for level in LEVELS {
                        let function = function(&bytes, level, false);
                        let block = &function.blocks[0];
                        for spans in [
                            crate::smir::ir::x86_legacy_fp_flag_compare_replay_spans(
                                block,
                                &function.x86_instruction_bytes,
                            ),
                            crate::smir::ir::x86_native_replay_spans(
                                block,
                                &function.x86_instruction_bytes,
                            ),
                        ] {
                            let span = spans
                                .get(&0)
                                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
                            assert_eq!(span.end, 1, "{level:?} {bytes:02X?}");
                            assert_eq!(span.instruction.as_slice(), bytes, "{level:?}");
                        }
                        assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");

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
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, LEVELS.len() * 2 * 2 * 17 * 64);
}

fn assert_replay_rejected(function: &SmirFunction, label: &str) {
    assert!(
        crate::smir::ir::x86_legacy_fp_flag_compare_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "family selector admitted {label}"
    );
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "aggregate selector admitted {label}"
    );
}

#[test]
fn exact_graph_validation_rejects_every_semantic_field_hint_provenance_and_group_mutation() {
    for format in Format::ALL {
        for opcode in [0x2Eu8, 0x2F] {
            let bytes = canonical_encoding(format, opcode, 9, 11, Some(0x0A));
            for level in LEVELS {
                let base = function(&bytes, level, false);
                assert_eq!(base.blocks[0].ops.len(), 1, "{level:?} {bytes:02X?}");

                let mut source1 = base.clone();
                if let OpKind::X86FpCompare { src1, .. } = &mut source1.blocks[0].ops[0].kind {
                    *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
                }
                assert_replay_rejected(&source1, "source 1");

                let mut source2 = base.clone();
                if let OpKind::X86FpCompare { src2, .. } = &mut source2.blocks[0].ops[0].kind {
                    *src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
                }
                assert_replay_rejected(&source2, "source 2");

                let mut element = base.clone();
                if let OpKind::X86FpCompare { elem, .. } = &mut element.blocks[0].ops[0].kind {
                    *elem = if format == Format::F32 {
                        VecElementType::F64
                    } else {
                        VecElementType::F32
                    };
                }
                assert_replay_rejected(&element, "element");

                let mut signaling = base.clone();
                if let OpKind::X86FpCompare { signaling, .. } = &mut signaling.blocks[0].ops[0].kind
                {
                    *signaling = !*signaling;
                }
                assert_replay_rejected(&signaling, "signaling policy");

                let mut suppression = base.clone();
                if let OpKind::X86FpCompare {
                    suppress_exceptions,
                    ..
                } = &mut suppression.blocks[0].ops[0].kind
                {
                    *suppress_exceptions = true;
                }
                assert_replay_rejected(&suppression, "exception suppression");

                let mut hint = base.clone();
                hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::Rep,
                    opcode: opcode ^ 1,
                });
                assert_replay_rejected(&hint, "encoding hint");

                let mut kind = base.clone();
                kind.blocks[0].ops[0].kind = OpKind::Nop;
                assert_replay_rejected(&kind, "operation kind");

                let mut extra = base.clone();
                extra.blocks[0].ops.push(SmirOp::new(
                    crate::smir::ir::types::OpId(1),
                    PC,
                    OpKind::Nop,
                ));
                assert_replay_rejected(&extra, "extra same-PC operation");

                let mut missing = base.clone();
                missing.x86_instruction_bytes.clear();
                assert_replay_rejected(&missing, "missing provenance");

                let mut memory = base.clone();
                let mut memory_bytes = bytes.clone();
                *memory_bytes.last_mut().unwrap() &= 0x3F;
                memory.x86_instruction_bytes.insert(
                    (BlockId(0), PC),
                    X86InstructionBytes::new(&memory_bytes).unwrap(),
                );
                assert_replay_rejected(&memory, "memory-form provenance");
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompareState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
}

fn initial_state(
    format: Format,
    first_register: usize,
    first: u64,
    second_register: usize,
    second: u64,
    mxcsr: u32,
    ordinal: usize,
) -> CompareState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (ordinal as u64).rotate_left((word * 7) as u32)
        })
    });
    let mask = format.bit_mask();
    vectors[first_register][0] = (vectors[first_register][0] & !mask) | (first & mask);
    vectors[second_register][0] = (vectors[second_register][0] & !mask) | (second & mask);
    CompareState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
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
        rflags: 0x2 | 0x0CD5,
        ac_flag: (ordinal & 1) as u64,
        mxcsr,
    }
}

fn is_nan(format: Format, value: u64) -> bool {
    let value = value & format.bit_mask();
    value & format.exponent_mask() == format.exponent_mask() && value & format.fraction_mask() != 0
}

fn is_signaling_nan(format: Format, value: u64) -> bool {
    is_nan(format, value) && value & format.quiet_bit() == 0
}

fn is_denormal(format: Format, value: u64) -> bool {
    let value = value & format.bit_mask();
    value & format.exponent_mask() == 0 && value & format.fraction_mask() != 0
}

fn apply_daz(format: Format, value: u64, mxcsr: u32) -> u64 {
    if mxcsr & (1 << 6) != 0 && is_denormal(format, value) {
        value & format.sign_mask()
    } else {
        value & format.bit_mask()
    }
}

fn architectural_expected(
    format: Format,
    opcode: u8,
    first_register: u8,
    second_register: u8,
    initial: &CompareState,
) -> CompareState {
    let mut expected = initial.clone();
    let raw_first = initial.vectors[usize::from(first_register)][0] & format.bit_mask();
    let raw_second = initial.vectors[usize::from(second_register)][0] & format.bit_mask();
    let first = apply_daz(format, raw_first, initial.mxcsr);
    let second = apply_daz(format, raw_second, initial.mxcsr);
    let first_nan = is_nan(format, first);
    let second_nan = is_nan(format, second);
    let invalid = is_signaling_nan(format, first)
        || is_signaling_nan(format, second)
        || (opcode == 0x2F && (first_nan || second_nan));
    let status = if first_nan || second_nan {
        u32::from(invalid)
    } else {
        u32::from(
            initial.mxcsr & (1 << 6) == 0
                && (is_denormal(format, raw_first) || is_denormal(format, raw_second)),
        ) << 1
    };
    expected.mxcsr |= status;

    let result_flags = if first_nan || second_nan {
        (1 << 6) | (1 << 2) | 1
    } else {
        let ordering = match format {
            Format::F32 => f32::from_bits(first as u32)
                .partial_cmp(&f32::from_bits(second as u32))
                .unwrap(),
            Format::F64 => f64::from_bits(first)
                .partial_cmp(&f64::from_bits(second))
                .unwrap(),
        };
        match ordering {
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Equal => 1 << 6,
            std::cmp::Ordering::Greater => 0,
        }
    };
    expected.rflags = (initial.rflags & !STATUS_FLAGS) | result_flags;
    expected
}

fn interpret(bytes: &[u8], initial: &CompareState, level: OptLevel) -> CompareState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = function(bytes, level, true);
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
    context.flags.materialized.ac = initial.ac_flag != 0;
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
    CompareState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
    }
}

fn value_pairs(format: Format) -> [(u64, u64); 14] {
    match format {
        Format::F32 => [
            (0x3F80_0000, 0x3F80_0000),
            (0x3F80_0000, 0x4000_0000),
            (0x4000_0000, 0x3F80_0000),
            (0x0000_0000, 0x8000_0000),
            (0x7FC0_0001, 0x3F80_0000),
            (0x3F80_0000, 0x7FC0_0001),
            (0x7F80_0001, 0x3F80_0000),
            (0x3F80_0000, 0x7F80_0001),
            (0x0000_0001, 0x0000_0000),
            (0x8000_0001, 0x8000_0000),
            (0x7F80_0000, 0x7F80_0000),
            (0xFF80_0000, 0x7F80_0000),
            (0xBF80_0000, 0xBF80_0000),
            (0x0080_0000, 0x007F_FFFF),
        ],
        Format::F64 => [
            (0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x3FF0_0000_0000_0000, 0x4000_0000_0000_0000),
            (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
            (0x7FF8_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x3FF0_0000_0000_0000, 0x7FF8_0000_0000_0001),
            (0x7FF0_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x3FF0_0000_0000_0000, 0x7FF0_0000_0000_0001),
            (0x0000_0000_0000_0001, 0x0000_0000_0000_0000),
            (0x8000_0000_0000_0001, 0x8000_0000_0000_0000),
            (0x7FF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (0xFFF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (0xBFF0_0000_0000_0000, 0xBFF0_0000_0000_0000),
            (0x0010_0000_0000_0000, 0x000F_FFFF_FFFF_FFFF),
        ],
    }
}

#[test]
fn interpreter_o0_o1_o2_matches_primary_truth_table_nan_and_denormal_policy() {
    for format in Format::ALL {
        for opcode in [0x2Eu8, 0x2F] {
            let bytes = canonical_encoding(format, opcode, 9, 11, Some(0x0A));
            for (ordinal, (first, second)) in value_pairs(format).into_iter().enumerate() {
                for daz in [false, true] {
                    let mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
                    let initial = initial_state(format, 9, first, 11, second, mxcsr, ordinal);
                    let expected = architectural_expected(format, opcode, 9, 11, &initial);
                    for level in LEVELS {
                        assert_eq!(
                            interpret(&bytes, &initial, level),
                            expected,
                            "{level:?} {bytes:02X?} {first:016X}/{second:016X} DAZ={daz}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    format: Format,
    opcode: u8,
    first_register: u8,
    second_register: u8,
    rex_ignored_bits: Option<u8>,
    first: u64,
    second: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const FORMS: [(u8, u8, Option<u8>); 8] = [
        (1, 3, None),
        (1, 3, Some(0)),
        (1, 3, Some(0x0A)),
        (9, 11, Some(0)),
        (15, 8, Some(0x0A)),
        (0, 0, Some(0x08)),
        (3, 3, Some(0x02)),
        (0, 3, Some(0)),
    ];
    let mut cases = Vec::new();
    for format in Format::ALL {
        for opcode in [0x2E, 0x2F] {
            for (first_register, second_register, rex_ignored_bits) in FORMS {
                for (ordinal, (first, mut second)) in value_pairs(format).into_iter().enumerate() {
                    if first_register == second_register {
                        second = first;
                    }
                    let prior_status = 1 << (ordinal % 6);
                    let rounding = ((ordinal as u32) & 3) << 13;
                    let daz_ftz = if ordinal & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    cases.push(NativeCase {
                        format,
                        opcode,
                        first_register,
                        second_register,
                        rex_ignored_bits,
                        first,
                        second,
                        mxcsr: 0x1F80 | prior_status | rounding | daz_ftz,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &CompareState, level: OptLevel) -> CompareState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map legacy scalar flag-compare replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
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
    CompareState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_FP_FLAG_COMPARE_CHILD_RANGE";

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
    for (ordinal, case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = canonical_encoding(
            case.format,
            case.opcode,
            case.first_register,
            case.second_register,
            case.rex_ignored_bits,
        );
        let initial = initial_state(
            case.format,
            usize::from(case.first_register),
            case.first,
            usize::from(case.second_register),
            case.second,
            case.mxcsr,
            ordinal,
        );
        let expected = architectural_expected(
            case.format,
            case.opcode,
            case.first_register,
            case.second_register,
            &initial,
        );
        for level in LEVELS {
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
        .expect("run isolated native legacy scalar flag-compare differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 2 * 2 * 8 * 14);
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
    let bytes = canonical_encoding(
        case.format,
        case.opcode,
        case.first_register,
        case.second_register,
        case.rex_ignored_bits,
    );
    panic!(
        "isolated native legacy scalar flag-compare failure at case {start}/{}: \
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
fn replay_matches_o0_o1_o2_primary_semantics_for_full_state_nan_daz_aliases_and_rex() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy scalar flag-compare differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_fp_flag_compare_replay::\
         replay_matches_o0_o1_o2_primary_semantics_for_full_state_nan_daz_aliases_and_rex",
    );
}
