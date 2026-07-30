//! Native replay coverage for defined register-only AVX VEX signed scalar
//! floating-point-to-integer conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2C2D_64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SourceFormat {
    F32,
    F64,
}

impl SourceFormat {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 2,
            Self::F64 => 3,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn patterns(self) -> &'static [u64] {
        match self {
            Self::F32 => &[
                0x0000_0000,
                0x8000_0000,
                0x0000_0001,
                0x007F_FFFF,
                0x3F00_0000,
                0xBF00_0000,
                0x3FC0_0000,
                0xBFC0_0000,
                0x4020_0000,
                0xC020_0000,
                0x4EFF_FFFF,
                0x4F00_0000,
                0xCF00_0000,
                0x5EFF_FFFF,
                0x5F00_0000,
                0xDF00_0000,
                0x5F80_0000,
                0x7F7F_FFFF,
                0x7F80_0000,
                0xFF80_0000,
                0x7FC1_2345,
                0xFFC1_2345,
                0x7F81_2345,
                0xFF81_2345,
            ],
            Self::F64 => &[
                0x0000_0000_0000_0000,
                0x8000_0000_0000_0000,
                0x0000_0000_0000_0001,
                0x000F_FFFF_FFFF_FFFF,
                0x3FE0_0000_0000_0000,
                0xBFE0_0000_0000_0000,
                0x3FF8_0000_0000_0000,
                0xBFF8_0000_0000_0000,
                0x4004_0000_0000_0000,
                0xC004_0000_0000_0000,
                0x41DF_FFFF_FFC0_0000,
                0x41E0_0000_0000_0000,
                0xC1E0_0000_0000_0000,
                0x43DF_FFFF_FFFF_FFFF,
                0x43E0_0000_0000_0000,
                0xC3E0_0000_0000_0000,
                0x43EF_FFFF_FFFF_FFFF,
                0x43F0_0000_0000_0000,
                0x7FEF_FFFF_FFFF_FFFF,
                0x7FF0_0000_0000_0000,
                0xFFF0_0000_0000_0000,
                0x7FF8_1234_5678_9ABC,
                0xFFF8_1234_5678_9ABC,
                0x7FF0_1234_5678_9ABC,
                0xFFF0_1234_5678_9ABC,
            ],
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
    format: SourceFormat,
    truncate: bool,
    form: VexForm,
    destination: u8,
    source: u8,
}

fn encoding(instruction: ConvertInstruction) -> Vec<u8> {
    assert!(instruction.destination < 16 && instruction.source < 16);
    let opcode = if instruction.truncate { 0x2C } else { 0x2D };
    let modrm = 0xC0 | ((instruction.destination & 7) << 3) | (instruction.source & 7);
    match instruction.form {
        VexForm::C5 => {
            assert!(instruction.source < 8, "C5 has no VEX.B extension");
            vec![
                0xC5,
                (if instruction.destination < 8 { 0x80 } else { 0 })
                    | 0x78
                    | instruction.format.pp(),
                opcode,
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
                (u8::from(w) << 7) | 0x78 | instruction.format.pp(),
                opcode,
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

fn expected_replay_bytes(bytes: &[u8]) -> Vec<u8> {
    let instruction = crate::smir::ir::X86InstructionBytes::new(bytes).unwrap();
    let destination = instruction
        .vex_scalar_fp_to_int_destination_index()
        .expect("defined VEX scalar FP-to-int encoding");
    if !matches!(destination, 4 | 5) {
        return bytes.to_vec();
    }

    let rewritten = instruction
        .vex_scalar_fp_to_int_with_destination(0)
        .expect("stack destination must rewrite to RAX");
    let mut expected = vec![0x50, 0x51];
    expected.extend_from_slice(rewritten.as_slice());
    expected.extend_from_slice(&[
        0x48,
        0x8B,
        0x4D,
        X86_STATE_PTR_AT_RBP as u8,
        0x48,
        0x89,
        0x41,
        destination * 8,
    ]);
    if destination == 5 {
        expected.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
    }
    expected.extend_from_slice(&[0x59, 0x58]);
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
            format: SourceFormat::F32,
            truncate: false,
            form: VexForm::C5,
            destination: 4,
            source: 7,
        },
        ConvertInstruction {
            format: SourceFormat::F64,
            truncate: true,
            form: VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            destination: 5,
            source: 15,
        },
        ConvertInstruction {
            format: SourceFormat::F64,
            truncate: false,
            form: VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            destination: 14,
            source: 8,
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
fn replay_lifts_admits_and_emits_all_4608_defined_register_images_at_o2() {
    let mut emitted = 0usize;
    for encoded_r in [false, true] {
        for pp in [2u8, 3] {
            let p1 = (u8::from(encoded_r) << 7) | 0x78 | pp;
            for opcode in [0x2Cu8, 0x2D] {
                for modrm in 0xC0u8..=0xFF {
                    assert_admitted_and_emitted(
                        &[0xC5, p1, opcode, modrm],
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
            for pp in [2u8, 3] {
                let p1 = (u8::from(w) << 7) | 0x78 | pp;
                for opcode in [0x2Cu8, 0x2D] {
                    for modrm in 0xC0u8..=0xFF {
                        assert_admitted_and_emitted(
                            &[0xC4, p0, p1, opcode, modrm],
                            crate::smir::optimize::OptLevel::O2,
                        );
                        emitted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(emitted, 4_608);
}

#[test]
fn rsp_rbp_destination_bridges_are_byte_exact_at_o0_and_o2() {
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for instruction in [
            ConvertInstruction {
                format: SourceFormat::F32,
                truncate: false,
                form: VexForm::C5,
                destination: 4,
                source: 7,
            },
            ConvertInstruction {
                format: SourceFormat::F64,
                truncate: true,
                form: VexForm::C5,
                destination: 5,
                source: 3,
            },
            ConvertInstruction {
                format: SourceFormat::F32,
                truncate: true,
                form: VexForm::C4 {
                    w: false,
                    ignored_x_clear: true,
                },
                destination: 4,
                source: 15,
            },
            ConvertInstruction {
                format: SourceFormat::F64,
                truncate: false,
                form: VexForm::C4 {
                    w: true,
                    ignored_x_clear: false,
                },
                destination: 5,
                source: 9,
            },
        ] {
            assert_admitted_and_emitted(&encoding(instruction), level);
        }
    }
}

#[test]
fn replay_fails_closed_without_exact_defined_source_provenance() {
    let instruction = ConvertInstruction {
        format: SourceFormat::F64,
        truncate: false,
        form: VexForm::C4 {
            w: true,
            ignored_x_clear: true,
        },
        destination: 5,
        source: 15,
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
            value[1] = (value[1] & 0xE0) | 2; // Map 0F38.
            value
        },
        {
            let mut value = bytes.clone();
            value[2] &= !0x08; // Reserved VEX.vvvv.
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

    let mut l1 = bytes.clone();
    l1[2] |= 0x04;
    let l1_function = function(&l1);
    assert!(is_native_clobber_safe(&l1_function));
    let spans = crate::smir::ir::x86_native_replay_spans(
        &l1_function.blocks[0],
        &l1_function.x86_instruction_bytes,
    );
    assert_eq!(
        spans
            .get(&0)
            .expect("canonical scalar FP-to-int span")
            .instruction,
        crate::smir::ir::X86InstructionBytes::new(&bytes).unwrap()
    );
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
fn initial_state(
    instruction: ConvertInstruction,
    source_bits: u64,
    seed: usize,
    mxcsr: u32,
) -> ConversionState {
    let mut vectors: [[u64; 8]; 32] = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    let source = usize::from(instruction.source);
    match instruction.format {
        SourceFormat::F32 => {
            vectors[source][0] =
                (vectors[source][0] & !u64::from(u32::MAX)) | u64::from(source_bits as u32);
        }
        SourceFormat::F64 => vectors[source][0] = source_bits,
    }
    ConversionState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (seed as u64).wrapping_mul(0x8040_2010_0804_0201)
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
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
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
fn interpreter_o0_o2_obeys_rc_truncation_precision_and_integer_indefinite() {
    let mut instruction = ConvertInstruction {
        format: SourceFormat::F32,
        truncate: false,
        form: VexForm::C4 {
            w: false,
            ignored_x_clear: true,
        },
        destination: 0,
        source: 15,
    };
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for (source_bits, expected_by_rc) in [
            (0x3FC0_0000, [2u64, 1, 2, 1]),
            (
                0xBFC0_0000,
                [0xFFFF_FFFEu64, 0xFFFF_FFFE, 0xFFFF_FFFF, 0xFFFF_FFFF],
            ),
        ] {
            for (rc, expected) in expected_by_rc.into_iter().enumerate() {
                let initial =
                    initial_state(instruction, source_bits, rc, 0x1F80 | ((rc as u32) << 13));
                let actual = interpret(&encoding(instruction), &initial, level);
                assert_eq!(actual.gprs[0], expected, "{level:?} rc={rc}");
                assert_ne!(actual.mxcsr & (1 << 5), 0, "{level:?} rc={rc}");
                assert_eq!(actual.vectors, initial.vectors);
                assert_eq!(actual.masks, initial.masks);
                assert_eq!(actual.rflags, initial.rflags);
            }
        }

        instruction.truncate = true;
        for rc in 0u32..4 {
            let initial = initial_state(instruction, 0xBFC0_0000, rc as usize, 0x1F80 | (rc << 13));
            let actual = interpret(&encoding(instruction), &initial, level);
            assert_eq!(actual.gprs[0], 0xFFFF_FFFF, "{level:?} rc={rc}");
        }

        instruction.truncate = false;
        for source_bits in [0x7F80_0000, 0x7FC1_2345, 0x7F81_2345] {
            let initial = initial_state(instruction, source_bits, source_bits as usize, 0x1F80);
            let actual = interpret(&encoding(instruction), &initial, level);
            assert_eq!(actual.gprs[0], 0x0000_0000_8000_0000, "{level:?}");
            assert_ne!(actual.mxcsr & 1, 0, "{level:?}");
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
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
    assert_replay_emitted(&code, bytes);
    let exec = ExecMem::new(&code).expect("map VEX scalar FP-to-int replay");
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
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_FP_TO_INT_CHILD_RANGE";

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
    let forms = [
        (VexForm::C5, 0, 1),
        (VexForm::C5, 4, 7),
        (VexForm::C5, 5, 3),
        (VexForm::C5, 9, 2),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: false,
            },
            15,
            8,
        ),
        (
            VexForm::C4 {
                w: false,
                ignored_x_clear: true,
            },
            10,
            14,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: false,
            },
            4,
            15,
        ),
        (
            VexForm::C4 {
                w: true,
                ignored_x_clear: true,
            },
            5,
            9,
        ),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for format in SourceFormat::ALL {
            for truncate in [false, true] {
                for (seed, &source_bits) in format.patterns().iter().enumerate() {
                    let (form, destination, source) = forms[ordinal % forms.len()];
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
                            truncate,
                            form,
                            destination,
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
    assert_eq!(cases.len(), 196);
    assert!(cases.iter().any(|case| case.instruction.destination == 4));
    assert!(cases.iter().any(|case| case.instruction.destination == 5));
    assert!(cases.iter().any(|case| case.instruction.source >= 8));
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.form.w() && case.instruction.format == SourceFormat::F64)
    );
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
        .expect("run isolated native VEX scalar FP-to-int differential")
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
        "isolated native VEX scalar FP-to-int failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_rounding_boundaries_stack_gprs_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar FP-to-int differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_fp_to_int_replay::\
         replay_matches_o0_o2_interpretation_for_rounding_boundaries_stack_gprs_and_full_state",
    );
}
