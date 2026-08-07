//! Native replay coverage for register-only legacy SSE4.1 blend instructions.

use super::*;
use crate::smir::ir::types::{FunctionId, SourceArch, VecElementType};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, is_native_clobber_safe, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB1E4;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlendKind {
    ImmediateF32,
    ImmediateF64,
    ImmediateI16,
    VariableI8,
    VariableF32,
    VariableF64,
}

impl BlendKind {
    const ALL: [Self; 6] = [
        Self::ImmediateF32,
        Self::ImmediateF64,
        Self::ImmediateI16,
        Self::VariableI8,
        Self::VariableF32,
        Self::VariableF64,
    ];

    const fn map(self) -> u8 {
        match self {
            Self::ImmediateF32 | Self::ImmediateF64 | Self::ImmediateI16 => 0x3A,
            Self::VariableI8 | Self::VariableF32 | Self::VariableF64 => 0x38,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::ImmediateF32 => 0x0C,
            Self::ImmediateF64 => 0x0D,
            Self::ImmediateI16 => 0x0E,
            Self::VariableI8 => 0x10,
            Self::VariableF32 => 0x14,
            Self::VariableF64 => 0x15,
        }
    }

    const fn element(self) -> VecElementType {
        match self {
            Self::VariableI8 => VecElementType::I8,
            Self::ImmediateI16 => VecElementType::I16,
            Self::ImmediateF32 | Self::VariableF32 => VecElementType::I32,
            Self::ImmediateF64 | Self::VariableF64 => VecElementType::I64,
        }
    }

    const fn is_immediate(self) -> bool {
        matches!(
            self,
            Self::ImmediateF32 | Self::ImmediateF64 | Self::ImmediateI16
        )
    }
}

fn encoding(kind: BlendKind, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, kind.map(), kind.opcode(), modrm]);
    if kind.is_immediate() {
        bytes.push(immediate);
    }
    bytes
}

fn canonical_encoding(
    kind: BlendKind,
    destination: u8,
    source: u8,
    immediate: u8,
    ignored_rex_bits: u8,
) -> Vec<u8> {
    assert!(destination < 16 && source < 16);
    let rex = 0x40
        | (ignored_rex_bits & 0x0A)
        | if destination >= 8 { 0x04 } else { 0 }
        | if source >= 8 { 0x01 } else { 0 };
    encoding(
        kind,
        Some(rex),
        0xC0 | ((destination & 7) << 3) | (source & 7),
        immediate,
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
        X86InstructionBytes::new(bytes).expect("legacy blend provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse41_avx_and_the_ymm16_bridge_only() {
    let bytes = canonical_encoding(BlendKind::VariableF32, 9, 11, 0, 0x0A);
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
    expected.needs_sse41 = true;
    expected.needs_avx = true;
    assert_eq!(requirements, expected);

    #[cfg(target_arch = "x86_64")]
    {
        let supported =
            std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("avx");
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
fn all_19_584_o0_o1_o2_rex_register_graphs_lower_to_the_exact_source_instruction() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for kind in BlendKind::ALL {
            for modrm in 0xC0..=0xFF {
                let bytes = encoding(kind, rex, modrm, modrm ^ rex.unwrap_or(0));
                for level in LEVELS {
                    let function = function(&bytes, level, false);
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
    assert_eq!(lowered, LEVELS.len() * 6 * 17 * 64);
}

#[test]
fn exact_graph_validation_rejects_every_operation_mutation_hint_and_virtual_escape() {
    use crate::smir::ir::ops::{OpKind, X86OpHint};

    for kind in BlendKind::ALL {
        let bytes = canonical_encoding(kind, 9, 11, 0xA5, 0x0A);
        for level in LEVELS {
            let function = function(&bytes, level, false);
            let block = &function.blocks[0];
            for spans in [
                crate::smir::ir::x86_legacy_blend_replay_spans(
                    block,
                    &function.x86_instruction_bytes,
                ),
                crate::smir::ir::x86_native_replay_spans(block, &function.x86_instruction_bytes),
            ] {
                let span = spans
                    .get(&0)
                    .unwrap_or_else(|| panic!("{level:?} {kind:?} {bytes:02X?}"));
                assert_eq!(span.end, block.ops.len());
                assert_eq!(span.instruction.as_slice(), bytes);
            }

            for index in 0..block.ops.len() {
                let mut mutated = function.clone();
                mutated.blocks[0].ops[index].kind = OpKind::Nop;
                assert!(
                    crate::smir::ir::x86_native_replay_spans(
                        &mutated.blocks[0],
                        &mutated.x86_instruction_bytes,
                    )
                    .is_empty(),
                    "{level:?} {kind:?} op {index}"
                );
            }

            let mut hinted = function.clone();
            hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
            assert!(
                crate::smir::ir::x86_native_replay_spans(
                    &hinted.blocks[0],
                    &hinted.x86_instruction_bytes,
                )
                .is_empty(),
                "{level:?} {kind:?} hinted"
            );

            let escaped = block.ops.iter().find_map(|op| {
                op.kind
                    .dests()
                    .into_iter()
                    .find(|register| matches!(register, crate::smir::ir::types::VReg::Virtual(_)))
            });
            let mut escaped_function = function.clone();
            escaped_function.blocks[0].set_terminator(Terminator::Return {
                values: vec![escaped.expect("legacy blend temporary")],
            });
            assert!(
                crate::smir::ir::x86_native_replay_spans(
                    &escaped_function.blocks[0],
                    &escaped_function.x86_instruction_bytes,
                )
                .is_empty(),
                "{level:?} {kind:?} escaped"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlendState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    kind: BlendKind,
    destination: u8,
    source: u8,
    immediate: u8,
    ignored_rex_bits: u8,
    data_case: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const OPERANDS: [(u8, u8); 7] = [(1, 3), (9, 11), (1, 1), (0, 3), (3, 0), (0, 0), (15, 8)];
    let mut cases = Vec::new();
    for (kind_index, kind) in BlendKind::ALL.into_iter().enumerate() {
        for (operand_index, (destination, source)) in OPERANDS.into_iter().enumerate() {
            for data_case in 0..4 {
                cases.push(NativeCase {
                    kind,
                    destination,
                    source,
                    immediate: if kind.is_immediate() {
                        [0x00, 0xFF, 0xA5, 0x5A][data_case]
                    } else {
                        0
                    },
                    ignored_rex_bits: ((kind_index + operand_index + data_case) as u8 & 3) << 1,
                    data_case,
                });
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: NativeCase, ordinal: usize) -> BlendState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (ordinal as u64).rotate_left((word * 7) as u32)
        })
    });
    if !case.kind.is_immediate() {
        let patterns = [
            [0x00; 16],
            [0x80; 16],
            std::array::from_fn(|lane| if lane & 1 == 0 { 0x80 } else { 0x00 }),
            std::array::from_fn(|lane| {
                if matches!(lane, 0 | 7 | 8 | 15) {
                    0x80
                } else {
                    0
                }
            }),
        ];
        let mut mask = vectors[0];
        let mut bytes = [0u8; 64];
        for (index, word) in mask.iter().enumerate() {
            bytes[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
        }
        bytes[..16].copy_from_slice(&patterns[case.data_case]);
        for (index, word) in mask.iter_mut().enumerate() {
            *word = u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap());
        }
        vectors[0] = mask;
    }
    BlendState {
        gprs: std::array::from_fn(|register| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 5) as u32)
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
        ac_flag: (ordinal & 1) as u64,
        mxcsr: 0x1F80 | ((ordinal as u32 & 3) << 13) | (ordinal as u32 & 0x3F),
    }
}

#[cfg(target_arch = "x86_64")]
fn vector_bytes(vector: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (index, word) in vector.into_iter().enumerate() {
        bytes[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn vector_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn architectural_expected(case: NativeCase, initial: &BlendState) -> BlendState {
    let mut expected = *initial;
    let mut destination = vector_bytes(initial.vectors[usize::from(case.destination)]);
    let source = vector_bytes(initial.vectors[usize::from(case.source)]);
    let mask = vector_bytes(initial.vectors[0]);
    let element_bytes = case.kind.element().bytes() as usize;
    let lanes = 16 / element_bytes;
    for lane in 0..lanes {
        let selected = if case.kind.is_immediate() {
            case.immediate >> lane & 1 != 0
        } else {
            mask[(lane + 1) * element_bytes - 1] & 0x80 != 0
        };
        if selected {
            let start = lane * element_bytes;
            destination[start..start + element_bytes]
                .copy_from_slice(&source[start..start + element_bytes]);
        }
    }
    expected.vectors[usize::from(case.destination)] = vector_words(destination);
    expected
}

#[cfg(target_arch = "x86_64")]
fn case_bytes(case: NativeCase) -> Vec<u8> {
    canonical_encoding(
        case.kind,
        case.destination,
        case.source,
        case.immediate,
        case.ignored_rex_bits,
    )
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: NativeCase, initial: &BlendState, level: OptLevel) -> BlendState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let bytes = case_bytes(case);
    let function = function(&bytes, level, true);
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

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    BlendState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &BlendState, level: OptLevel) -> BlendState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case_bytes(case);
    let function = function(&bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map legacy blend replay");
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
    BlendState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_BLEND_CHILD_RANGE";

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
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in LEVELS {
            assert_eq!(
                interpret(case, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {:02X?}",
                case_bytes(case)
            );
            assert_eq!(
                execute_native(case, &initial, level),
                expected,
                "native {level:?} {case:?} {:02X?}",
                case_bytes(case)
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
        .expect("run isolated native legacy blend differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 168);
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
    panic!(
        "isolated native legacy blend failure at case {start}/{}: {case:?} {:02X?}; \
         whole status {}; singleton status {}; singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case_bytes(case),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_replay_matches_sdm_o0_o1_o2_equations_aliases_masks_and_full_state() {
    if !std::is_x86_feature_detected!("sse4.1") || !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy blend differential: host lacks SSE4.1 or AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_blend_replay::\
         native_replay_matches_sdm_o0_o1_o2_equations_aliases_masks_and_full_state",
    );
}
