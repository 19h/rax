//! Native replay coverage for defined register-only AVX VEX signed integer-to-
//! scalar-floating-point conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2A64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestinationFormat {
    F32,
    F64,
}

impl DestinationFormat {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 2,
            Self::F64 => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4 { w: bool, ignored_x_clear: bool },
}

impl VexForm {
    #[cfg(target_arch = "x86_64")]
    fn w(self) -> bool {
        match self {
            Self::C5 => false,
            Self::C4 { w, .. } => w,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertInstruction {
    format: DestinationFormat,
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
                    | instruction.format.pp(),
                0x2A,
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
                (u8::from(w) << 7) | encoded_vvvv | instruction.format.pp(),
                0x2A,
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

#[cfg(target_arch = "x86_64")]
fn sequence_function(
    instructions: &[&[u8]],
    level: crate::smir::optimize::OptLevel,
    halt: bool,
) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let mut block = SmirBlock::new(BlockId(0), PC);
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    let mut pc = PC;
    for bytes in instructions {
        let result = lifter
            .lift_insn(pc, bytes, &mut context)
            .unwrap_or_else(|error| panic!("{pc:#X} {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{pc:#X} {bytes:02X?}");
        block.ops.extend(result.ops);
        function
            .x86_instruction_bytes
            .insert((block.id, pc), X86InstructionBytes::new(bytes).unwrap());
        pc += bytes.len() as u64;
    }
    block.set_terminator(if halt {
        Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        }
    } else {
        Terminator::Return { values: Vec::new() }
    });
    function.add_block(block);
    crate::smir::optimize::optimize_function(&mut function, level);
    function
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

fn expected_replay_bytes(bytes: &[u8]) -> Vec<u8> {
    let instruction = crate::smir::ir::X86InstructionBytes::new(bytes).unwrap();
    let destination = instruction
        .vex_scalar_int_to_fp_destination_index()
        .expect("defined VEX scalar integer-to-FP encoding");
    let source = instruction
        .vex_scalar_int_to_fp_source_index()
        .expect("defined VEX scalar integer-to-FP source");
    let mut expected = if matches!(source, 4 | 5) {
        let rewritten = instruction
            .vex_scalar_int_to_fp_with_source(0)
            .expect("stack source must rewrite to RAX");
        let mut bridge = vec![
            0x50,
            0x48,
            0x8B,
            0x45,
            X86_STATE_PTR_AT_RBP as u8,
            0x48,
            0x8B,
            0x40,
            source * 8,
        ];
        bridge.extend_from_slice(rewritten.as_slice());
        bridge.push(0x58);
        bridge
    } else {
        bytes.to_vec()
    };
    expected.extend_from_slice(&upper_clear_postlude(destination));
    expected
}

fn assert_replay_emitted(code: &[u8], bytes: &[u8]) {
    let expected = expected_replay_bytes(bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "source={bytes:02X?} expected={expected:02X?}"
    );
}

#[test]
fn replay_features_require_exactly_avx_and_the_ymm16_state_boundary() {
    for instruction in [
        ConvertInstruction {
            format: DestinationFormat::F32,
            form: VexForm::C5,
            destination: 9,
            merge: 10,
            source: 4,
        },
        ConvertInstruction {
            format: DestinationFormat::F64,
            form: VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            destination: 12,
            merge: 13,
            source: 5,
        },
        ConvertInstruction {
            format: DestinationFormat::F32,
            form: VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            destination: 15,
            merge: 15,
            source: 14,
        },
    ] {
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
    assert_replay_emitted(&code, bytes);
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
                        &[0xC5, p1, 0x2A, modrm],
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
                            &[0xC4, p0, p1, 0x2A, modrm],
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
fn rsp_rbp_source_bridges_and_destination_upper_clears_are_byte_exact() {
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for instruction in [
            ConvertInstruction {
                format: DestinationFormat::F32,
                form: VexForm::C5,
                destination: 9,
                merge: 10,
                source: 4,
            },
            ConvertInstruction {
                format: DestinationFormat::F64,
                form: VexForm::C5,
                destination: 15,
                merge: 15,
                source: 5,
            },
            ConvertInstruction {
                format: DestinationFormat::F32,
                form: VexForm::C4 {
                    w: false,
                    ignored_x_clear: true,
                },
                destination: 12,
                merge: 13,
                source: 4,
            },
            ConvertInstruction {
                format: DestinationFormat::F64,
                form: VexForm::C4 {
                    w: true,
                    ignored_x_clear: false,
                },
                destination: 3,
                merge: 4,
                source: 5,
            },
        ] {
            assert_admitted_and_emitted(&encoding(instruction), level);
        }
    }
}

#[test]
fn replay_fails_closed_without_exact_defined_source_provenance() {
    let instruction = ConvertInstruction {
        format: DestinationFormat::F64,
        form: VexForm::C4 {
            w: true,
            ignored_x_clear: true,
        },
        destination: 12,
        merge: 13,
        source: 5,
    };
    let bytes = encoding(instruction);
    let base = function(&bytes);

    let mut candidates = Vec::new();
    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    candidates.push(missing);

    for invalid in [
        {
            let mut value = bytes.clone();
            value[2] |= 0x04; // VEX.L=1 is generation-dependent unpredictable.
            value
        },
        {
            let mut value = bytes.clone();
            value[1] = (value[1] & 0xE0) | 2; // Map 0F38.
            value
        },
        {
            let mut value = bytes.clone();
            value[2] = (value[2] & !3) | 1; // Wrong mandatory prefix.
            value
        },
        {
            let mut value = bytes.clone();
            value[3] = 0x2B; // Non-family opcode.
            value
        },
        {
            let mut value = bytes.clone();
            value[4] &= 0x3F; // Memory source.
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

    for mut candidate in candidates {
        crate::smir::optimize::optimize_function(
            &mut candidate,
            crate::smir::optimize::OptLevel::O2,
        );
        assert!(!is_native_clobber_safe(&candidate));
        assert!(
            !x86_native_replay_feature_requirements(&candidate, &std::collections::HashMap::new())
                .any
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversionState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn integer_patterns(w: bool) -> &'static [u64] {
    if w {
        &[
            0,
            1,
            2,
            u64::MAX,
            i64::MAX as u64,
            i64::MIN as u64,
            (1 << 24) - 1,
            1 << 24,
            (1 << 24) + 1,
            (1 << 24) + 3,
            (1u64 << 53) - 1,
            1u64 << 53,
            (1u64 << 53) + 1,
            (1u64 << 53) + 3,
            (-(1i64 << 24) - 1) as u64,
            (-(1i64 << 53) - 1) as u64,
        ]
    } else {
        &[
            0x0000_0000,
            0x0000_0001,
            0x0000_0002,
            0xFFFF_FFFF,
            0x7FFF_FFFF,
            0x8000_0000,
            0x00FF_FFFF,
            0x0100_0000,
            0x0100_0001,
            0x0100_0003,
            0xFEFF_FFFF,
            0xFEFF_FFFD,
        ]
    }
}

#[cfg(target_arch = "x86_64")]
fn initial_state(
    instruction: ConvertInstruction,
    source_bits: u64,
    seed: usize,
    mxcsr: u32,
) -> ConversionState {
    let mut gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
            ^ (seed as u64).wrapping_mul(0x8040_2010_0804_0201)
    });
    let source = usize::from(instruction.source);
    gprs[source] = if instruction.form.w() {
        source_bits
    } else {
        (gprs[source] & !u64::from(u32::MAX)) | u64::from(source_bits as u32)
    };
    ConversionState {
        gprs,
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                    ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
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
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
fn interpret(
    bytes: &[u8],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    let function = optimized_function(bytes, level, true);
    interpret_function(&function, initial)
}

#[cfg(target_arch = "x86_64")]
fn interpret_function(
    function: &crate::smir::ir::SmirFunction,
    initial: &ConversionState,
) -> ConversionState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

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
    ConversionState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn interpreter_o0_o2_obeys_rc_precision_merge_and_vex_upper_zeroing() {
    let instruction = ConvertInstruction {
        format: DestinationFormat::F32,
        form: VexForm::C4 {
            w: false,
            ignored_x_clear: true,
        },
        destination: 9,
        merge: 10,
        source: 5,
    };
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for (source_bits, expected_by_rc) in [
            (
                0x0100_0001,
                [0x4B80_0000u64, 0x4B80_0000, 0x4B80_0001, 0x4B80_0000],
            ),
            (
                0xFEFF_FFFF,
                [0xCB80_0000u64, 0xCB80_0001, 0xCB80_0000, 0xCB80_0000],
            ),
        ] {
            for (rc, expected) in expected_by_rc.into_iter().enumerate() {
                let initial =
                    initial_state(instruction, source_bits, rc, 0x1F80 | ((rc as u32) << 13));
                let actual = interpret(&encoding(instruction), &initial, level);
                let destination = usize::from(instruction.destination);
                let merge = initial.vectors[usize::from(instruction.merge)];
                assert_eq!(
                    actual.vectors[destination][0] & u64::from(u32::MAX),
                    expected,
                    "{level:?} rc={rc}"
                );
                assert_eq!(
                    actual.vectors[destination][0] & !u64::from(u32::MAX),
                    merge[0] & !u64::from(u32::MAX)
                );
                assert_eq!(actual.vectors[destination][1], merge[1]);
                assert_eq!(actual.vectors[destination][2..], [0; 6]);
                assert_ne!(actual.mxcsr & (1 << 5), 0, "{level:?} rc={rc}");
                assert_eq!(actual.gprs, initial.gprs);
                assert_eq!(actual.masks, initial.masks);
                assert_eq!(actual.rflags, initial.rflags);
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    let function = optimized_function(bytes, level, false);
    execute_native_function(&function, initial, &[bytes])
}

#[cfg(target_arch = "x86_64")]
fn execute_native_function(
    function: &crate::smir::ir::SmirFunction,
    initial: &ConversionState,
    expected_replays: &[&[u8]],
) -> ConversionState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{expected_replays:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{expected_replays:02X?}: {error:?}"));
    for bytes in expected_replays {
        assert_replay_emitted(&code, bytes);
    }
    let exec = ExecMem::new(&code).expect("map VEX scalar integer-to-FP replay");
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
    ConversionState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn chained_rsp_rbp_destination_then_source_replay_observes_the_committed_value() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping chained VEX scalar conversion differential: host lacks AVX");
        return;
    }

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for stack_gpr in [4u8, 5] {
            let fp_to_int = [0xC5, 0xFA, 0x2D, 0xC0 | (stack_gpr << 3) | 1];
            let int_to_fp_instruction = ConvertInstruction {
                format: DestinationFormat::F32,
                form: VexForm::C4 {
                    w: false,
                    ignored_x_clear: true,
                },
                destination: 9,
                merge: 10,
                source: stack_gpr,
            };
            let int_to_fp = encoding(int_to_fp_instruction);
            let sequence = [&fp_to_int[..], int_to_fp.as_slice()];
            let interpreted_function = sequence_function(&sequence, level, true);
            let native_function = sequence_function(&sequence, level, false);
            let mut initial = initial_state(int_to_fp_instruction, 0xDEAD_BEEF, 0, 0x1F80);
            initial.vectors[1][0] =
                (initial.vectors[1][0] & !u64::from(u32::MAX)) | u64::from(42.0f32.to_bits());

            let interpreted = interpret_function(&interpreted_function, &initial);
            let native =
                execute_native_function(&native_function, &initial, &[int_to_fp.as_slice()]);
            assert_eq!(native, interpreted, "{level:?} stack_gpr={stack_gpr}");
            assert_eq!(native.gprs[usize::from(stack_gpr)], 42);
            assert_eq!(
                native.vectors[usize::from(int_to_fp_instruction.destination)][0]
                    & u64::from(u32::MAX),
                u64::from(42.0f32.to_bits())
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_INT_TO_FP_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    instruction: ConvertInstruction,
    source_bits: u64,
    seed: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let w0_forms = [
        (VexForm::C5, 1, 2, 0),
        (VexForm::C5, 9, 10, 4),
        (VexForm::C5, 15, 15, 5),
        (VexForm::C5, 0, 0, 7),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            9,
            10,
            4,
        ),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: true,
            },
            15,
            15,
            14,
        ),
    ];
    let w1_forms = [
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: false,
            },
            3,
            4,
            5,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            12,
            13,
            14,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: false,
            },
            10,
            10,
            4,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            15,
            0,
            9,
        ),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in DestinationFormat::ALL {
            for (w, forms) in [(false, &w0_forms[..]), (true, &w1_forms[..])] {
                for (seed, &source_bits) in integer_patterns(w).iter().enumerate() {
                    let (form, destination, merge, source) = forms[ordinal % forms.len()];
                    let prior_status = 1 << (ordinal % 6);
                    let rc = ((ordinal & 3) as u32) << 13;
                    let daz_ftz = if ordinal & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    cases.push(NativeCase {
                        level,
                        instruction: ConvertInstruction {
                            format,
                            form,
                            destination,
                            merge,
                            source,
                        },
                        source_bits,
                        seed,
                        // Production admission requires all six exception masks.
                        mxcsr: 0x1F80 | prior_status | rc | daz_ftz,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    assert_eq!(cases.len(), 112);
    assert!(cases.iter().any(|case| case.instruction.source == 4));
    assert!(cases.iter().any(|case| case.instruction.source == 5));
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.destination == case.instruction.merge)
    );
    assert!(cases.iter().any(|case| case.instruction.source >= 8));
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
        let initial = initial_state(case.instruction, case.source_bits, case.seed, case.mxcsr);
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
        .expect("run isolated native VEX scalar integer-to-FP differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
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
        "isolated native VEX scalar integer-to-FP failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_rounding_aliases_stack_gprs_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar integer-to-FP differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_int_to_fp_replay::\
         replay_matches_o0_o2_interpretation_for_rounding_aliases_stack_gprs_and_full_state",
    );
}
