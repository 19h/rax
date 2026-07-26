//! Native replay coverage for register-only AVX/AVX2 VEX packed
//! sign/zero-extension moves.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2035;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operation {
    opcode: u8,
    source_bits: u8,
    destination_bits: u8,
    signed: bool,
}

const OPERATIONS: [Operation; 12] = [
    Operation {
        opcode: 0x20,
        source_bits: 8,
        destination_bits: 16,
        signed: true,
    },
    Operation {
        opcode: 0x21,
        source_bits: 8,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        opcode: 0x22,
        source_bits: 8,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x23,
        source_bits: 16,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        opcode: 0x24,
        source_bits: 16,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x25,
        source_bits: 32,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        opcode: 0x30,
        source_bits: 8,
        destination_bits: 16,
        signed: false,
    },
    Operation {
        opcode: 0x31,
        source_bits: 8,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        opcode: 0x32,
        source_bits: 8,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        opcode: 0x33,
        source_bits: 16,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        opcode: 0x34,
        source_bits: 16,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        opcode: 0x35,
        source_bits: 32,
        destination_bits: 64,
        signed: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn l(self) -> u8 {
        u8::from(self == Self::V256)
    }

    fn bits(self) -> usize {
        if self == Self::V256 { 256 } else { 128 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    W0,
    W1IgnoredX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtendCase {
    operation: Operation,
    width: Width,
    form: EncodingForm,
    destination: u8,
    source: u8,
}

fn encoding(case: ExtendCase) -> [u8; 5] {
    assert!(case.destination < 16 && case.source < 16);
    let mut p0 = 0xE2;
    if case.destination >= 8 {
        p0 &= !0x80;
    }
    if case.form == EncodingForm::W1IgnoredX {
        p0 &= !0x40;
    }
    if case.source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (if case.form == EncodingForm::W1IgnoredX {
            0x80
        } else {
            0
        }) | 0x79
            | (case.width.l() << 2),
        case.operation.opcode,
        0xC0 | ((case.destination & 7) << 3) | (case.source & 7),
    ]
}

fn cases() -> Vec<ExtendCase> {
    const OPERANDS: [(u8, u8); 6] = [(1, 2), (9, 10), (1, 1), (9, 9), (1, 9), (9, 1)];
    let mut cases = Vec::new();
    for operation in OPERATIONS {
        for width in [Width::V128, Width::V256] {
            for form in [EncodingForm::W0, EncodingForm::W1IgnoredX] {
                for (destination, source) in OPERANDS {
                    cases.push(ExtendCase {
                        operation,
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
fn replay_features_select_avx_for_128_and_avx2_for_256_without_avx512() {
    for (width, expected_avx2) in [(Width::V128, false), (Width::V256, true)] {
        let case = ExtendCase {
            operation: OPERATIONS[5],
            width,
            form: EncodingForm::W1IgnoredX,
            destination: 9,
            source: 10,
        };
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert_eq!(requirements.needs_avx2, expected_avx2, "{case:?}");
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
            std::is_x86_feature_detected!("avx")
                && (!expected_avx2 || std::is_x86_feature_detected!("avx2")),
            "{case:?}"
        );
    }
}

#[test]
fn replay_feature_aggregation_is_monotonic_across_widths_and_evex_spans() {
    let narrow = encoding(ExtendCase {
        operation: OPERATIONS[0],
        width: Width::V128,
        form: EncodingForm::W0,
        destination: 1,
        source: 2,
    });
    let wide = encoding(ExtendCase {
        operation: OPERATIONS[11],
        width: Width::V256,
        form: EncodingForm::W1IgnoredX,
        destination: 9,
        source: 10,
    });
    // EVEX.512.66.0F38.W0 20 /r VPMOVSXBW zmm1, ymm2.
    let evex = [0x62, 0xF2, 0x7D, 0x48, 0x20, 0xCA];

    for (first, second) in [(&narrow[..], &wide[..]), (&wide[..], &narrow[..])] {
        let mut mixed = function_at(first, BlockId(0), PC);
        let mut trailing = function_at(second, BlockId(1), PC + 0x100);
        mixed.add_block(trailing.blocks.remove(0));
        mixed
            .x86_instruction_bytes
            .extend(trailing.x86_instruction_bytes);

        let requirements =
            x86_native_replay_feature_requirements(&mixed, &std::collections::HashMap::new());
        assert!(requirements.all_spans_support_avx_ymm16);
        assert!(requirements.needs_avx);
        assert!(requirements.needs_avx2);
        assert!(!requirements.needs_avx512bw);
    }

    for (first, second) in [(&wide[..], &evex[..]), (&evex[..], &wide[..])] {
        let mut mixed = function_at(first, BlockId(0), PC);
        let mut trailing = function_at(second, BlockId(1), PC + 0x100);
        mixed.add_block(trailing.blocks.remove(0));
        mixed
            .x86_instruction_bytes
            .extend(trailing.x86_instruction_bytes);

        let requirements =
            x86_native_replay_feature_requirements(&mixed, &std::collections::HashMap::new());
        assert!(!requirements.all_spans_support_avx_ymm16);
        assert!(requirements.needs_avx);
        assert!(requirements.needs_avx2);
        assert!(requirements.needs_avx512bw);
    }
}

#[test]
fn replay_admits_lifts_and_emits_all_24576_legal_register_encodings_at_o2() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut admitted = 0usize;
    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 2 {
            continue;
        }
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x78 != 0x78 || p1 & 0x03 != 1 {
                continue;
            }
            for opcode in (0x20..=0x25).chain(0x30..=0x35) {
                for modrm in 0xC0..=0xFF {
                    let bytes = [0xC4, p0, p1, opcode, modrm];
                    let mut function = function(&bytes);
                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
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
                    admitted += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 24_576);
}

#[test]
fn replay_survives_o0_o2_aliases_extensions_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 288);
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
    assert_eq!(lowered, 576);

    let bytes = encoding(ExtendCase {
        operation: OPERATIONS[0],
        width: Width::V256,
        form: EncodingForm::W0,
        destination: 9,
        source: 10,
    });
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory = bytes;
    memory[4] &= 0x3F;
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
struct ExtendState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const BYTE_PATTERNS: [u64; 10] = [0, 1, 0x7E, 0x7F, 0x80, 0x81, 0xFE, 0xFF, 0x55, 0xAA];
const WORD_PATTERNS: [u64; 10] = [
    0, 1, 0x7FFE, 0x7FFF, 0x8000, 0x8001, 0xFFFE, 0xFFFF, 0x5555, 0xAAAA,
];
const DWORD_PATTERNS: [u64; 10] = [
    0,
    1,
    0x7FFF_FFFE,
    0x7FFF_FFFF,
    0x8000_0000,
    0x8000_0001,
    0xFFFF_FFFE,
    0xFFFF_FFFF,
    0x5555_5555,
    0xAAAA_AAAA,
];

fn lane_pattern(source_bits: u8, lane: usize, profile: usize) -> u64 {
    let index = (lane * 3 + profile * 7) % BYTE_PATTERNS.len();
    match source_bits {
        8 => BYTE_PATTERNS[index],
        16 => WORD_PATTERNS[index],
        32 => DWORD_PATTERNS[index],
        _ => unreachable!(),
    }
}

fn insert_lane(vector: &mut [u64; 8], lane: usize, bits: u8, value: u64) {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    debug_assert!(shift + usize::from(bits) <= 64);
    vector[word] = (vector[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn extract_lane(vector: &[u64; 8], lane: usize, bits: u8) -> u64 {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    debug_assert!(shift + usize::from(bits) <= 64);
    (vector[word] >> shift) & mask
}

fn source_vector(operation: Operation, profile: usize) -> [u64; 8] {
    let mut vector = [0u64; 8];
    for lane in 0..(128 / usize::from(operation.source_bits)) {
        insert_lane(
            &mut vector,
            lane,
            operation.source_bits,
            lane_pattern(operation.source_bits, lane, profile),
        );
    }
    vector
}

fn initial_state(case: ExtendCase, profile: usize) -> ExtendState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 13) as u32)
                ^ (profile as u64).wrapping_mul(0x1020_4081_0204_0810)
        })
    });
    vectors[usize::from(case.source)] = source_vector(case.operation, profile);
    ExtendState {
        gprs: std::array::from_fn(|register| {
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors,
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

fn architectural_expected(case: ExtendCase, initial: &ExtendState) -> ExtendState {
    let mut expected = initial.clone();
    let source = initial.vectors[usize::from(case.source)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    destination.fill(0);
    let lanes = case.width.bits() / usize::from(case.operation.destination_bits);
    for lane in 0..lanes {
        let raw = extract_lane(&source, lane, case.operation.source_bits);
        let extended = if case.operation.signed {
            let shift = 64 - u32::from(case.operation.source_bits);
            (((raw << shift) as i64) >> shift) as u64
        } else {
            raw
        };
        insert_lane(destination, lane, case.operation.destination_bits, extended);
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

fn interpret(
    bytes: &[u8],
    initial: &ExtendState,
    level: crate::smir::optimize::OptLevel,
) -> ExtendState {
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
    ExtendState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_exact_equations_at_o0_o2_for_all_shapes_boundaries_and_aliases() {
    let cases = cases();
    assert_eq!(cases.len(), 288);
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

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &ExtendState,
    level: crate::smir::optimize::OptLevel,
) -> ExtendState {
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
    let exec = ExecMem::new(&code).expect("map VEX packed-extension replay");
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
    ExtendState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PACKED_EXTEND_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[ExtendCase], range: std::ops::Range<usize>) {
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
        .expect("run isolated native VEX packed-extension differential")
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
        "isolated native VEX packed-extension failure at case {start}/{}: \
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
fn replay_matches_exact_o0_o2_equations_for_all_shapes_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping native VEX packed-extension differential: host lacks AVX2");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_packed_extend_replay::\
         replay_matches_exact_o0_o2_equations_for_all_shapes_aliases_and_full_state",
    );
}
