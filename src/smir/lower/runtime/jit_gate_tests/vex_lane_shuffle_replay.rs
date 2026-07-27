//! Native replay coverage for register-only VEX one-source lane shuffles.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1041;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShuffleKind {
    MoveSlDup,
    MoveShDup,
    MoveDDup,
    ShuffleDword,
    ShuffleHighWord,
    ShuffleLowWord,
}

impl ShuffleKind {
    const ALL: [Self; 6] = [
        Self::MoveSlDup,
        Self::MoveShDup,
        Self::MoveDDup,
        Self::ShuffleDword,
        Self::ShuffleHighWord,
        Self::ShuffleLowWord,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::MoveSlDup | Self::MoveDDup => 0x12,
            Self::MoveShDup => 0x16,
            Self::ShuffleDword | Self::ShuffleHighWord | Self::ShuffleLowWord => 0x70,
        }
    }

    fn pp(self) -> u8 {
        match self {
            Self::MoveSlDup | Self::MoveShDup | Self::ShuffleHighWord => 2,
            Self::MoveDDup | Self::ShuffleLowWord => 3,
            Self::ShuffleDword => 1,
        }
    }

    fn immediate(self) -> bool {
        matches!(
            self,
            Self::ShuffleDword | Self::ShuffleHighWord | Self::ShuffleLowWord
        )
    }

    fn element_bits(self) -> u8 {
        match self {
            Self::MoveDDup => 64,
            Self::MoveSlDup | Self::MoveShDup | Self::ShuffleDword => 32,
            Self::ShuffleHighWord | Self::ShuffleLowWord => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn l(self) -> bool {
        self == Self::V256
    }

    fn bits(self) -> usize {
        if self == Self::V256 { 256 } else { 128 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShuffleCase {
    kind: ShuffleKind,
    width: Width,
    form: EncodingForm,
    destination: u8,
    source: u8,
    immediate: u8,
}

impl ShuffleCase {
    fn needs_avx2(self) -> bool {
        self.kind.immediate() && self.width == Width::V256
    }
}

fn encoding(case: ShuffleCase) -> Vec<u8> {
    assert!(case.destination < 16 && case.source < 16);
    let mut bytes = match case.form {
        EncodingForm::VexC5 => {
            assert!(case.source < 8);
            vec![
                0xC5,
                (if case.destination < 8 { 0x80 } else { 0 })
                    | 0x78
                    | (u8::from(case.width.l()) << 2)
                    | case.kind.pp(),
                case.kind.opcode(),
                0xC0 | ((case.destination & 7) << 3) | case.source,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if case.destination >= 8 {
                p0 &= !0x80;
            }
            if case.form == EncodingForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if case.source >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if case.form == EncodingForm::VexC4W1IgnoredX {
                    0x80
                } else {
                    0
                }) | 0x78
                    | (u8::from(case.width.l()) << 2)
                    | case.kind.pp(),
                case.kind.opcode(),
                0xC0 | ((case.destination & 7) << 3) | (case.source & 7),
            ]
        }
    };
    if case.kind.immediate() {
        bytes.push(case.immediate);
    }
    bytes
}

fn cases() -> Vec<ShuffleCase> {
    const IMMEDIATES: [u8; 6] = [0x00, 0x1B, 0x4E, 0xA5, 0xE4, 0xFF];
    let mut cases = Vec::new();
    for kind in ShuffleKind::ALL {
        for width in [Width::V128, Width::V256] {
            for form in [
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ] {
                let operands: &[(u8, u8)] = match form {
                    EncodingForm::VexC5 => &[(1, 3), (9, 3), (1, 1)],
                    EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
                        &[(1, 3), (9, 11), (1, 1), (1, 9), (9, 1)]
                    }
                };
                let immediates: &[u8] = if kind.immediate() { &IMMEDIATES } else { &[0] };
                for &(destination, source) in operands {
                    for &immediate in immediates {
                        cases.push(ShuffleCase {
                            kind,
                            width,
                            form,
                            destination,
                            source,
                            immediate,
                        });
                    }
                }
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
fn replay_features_select_avx_for_duplicates_and_xmm_shuffles_and_avx2_for_ymm_shuffles() {
    for case in [
        ShuffleCase {
            kind: ShuffleKind::MoveShDup,
            width: Width::V256,
            form: EncodingForm::VexC4W1IgnoredX,
            destination: 9,
            source: 11,
            immediate: 0,
        },
        ShuffleCase {
            kind: ShuffleKind::ShuffleDword,
            width: Width::V128,
            form: EncodingForm::VexC5,
            destination: 9,
            source: 3,
            immediate: 0x1B,
        },
        ShuffleCase {
            kind: ShuffleKind::ShuffleHighWord,
            width: Width::V256,
            form: EncodingForm::VexC4W1IgnoredX,
            destination: 9,
            source: 11,
            immediate: 0x4E,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert_eq!(requirements.needs_avx2, case.needs_avx2(), "{case:?}");
        assert!(!requirements.needs_sse3, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            "{case:?}"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx")
                && (!case.needs_avx2() || std::is_x86_feature_detected!("avx2")),
            "{case:?}"
        );
    }
}

#[test]
fn replay_admits_and_emits_1092_optimized_representatives_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 546);
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
    assert_eq!(lowered, 1_092);

    let case = ShuffleCase {
        kind: ShuffleKind::ShuffleDword,
        width: Width::V256,
        form: EncodingForm::VexC4W0,
        destination: 9,
        source: 11,
        immediate: 0x1B,
    };
    let bytes = encoding(case);
    let base = function(&bytes);

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();

    let mut memory = bytes.clone();
    memory[4] &= 0x3F;
    let mut memory_metadata = base.clone();
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
    );

    let mut nonreserved_vvvv = bytes.clone();
    nonreserved_vvvv[2] &= !0x08;
    let mut invalid_metadata = base;
    invalid_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&nonreserved_vvvv).unwrap(),
    );

    for nonmatching in [missing, memory_metadata, invalid_metadata] {
        assert!(!is_native_clobber_safe(&nonmatching));
        assert!(
            !x86_native_replay_feature_requirements(
                &nonmatching,
                &std::collections::HashMap::new()
            )
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

    for case in [
        ShuffleCase {
            kind: ShuffleKind::MoveDDup,
            width: Width::V128,
            form: EncodingForm::VexC5,
            destination: 9,
            source: 3,
            immediate: 0,
        },
        ShuffleCase {
            kind: ShuffleKind::ShuffleLowWord,
            width: Width::V256,
            form: EncodingForm::VexC4W1IgnoredX,
            destination: 11,
            source: 9,
            immediate: 0xE4,
        },
    ] {
        let bytes = encoding(case);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer
            .lower_function(&function(&bytes))
            .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
        let mut expected = bytes.clone();
        expected.extend_from_slice(&upper_clear_postlude(case.destination));
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{case:?} {bytes:02X?}"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShuffleState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> ShuffleState {
    ShuffleState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                    ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (ordinal as u64).wrapping_mul(0x8040_2010_0804_0201)
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
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

fn source_lane(case: ShuffleCase, destination_lane: usize) -> usize {
    match case.kind {
        ShuffleKind::MoveSlDup | ShuffleKind::MoveDDup => destination_lane / 2 * 2,
        ShuffleKind::MoveShDup => destination_lane / 2 * 2 + 1,
        ShuffleKind::ShuffleDword => {
            let block = destination_lane / 4 * 4;
            let within = destination_lane % 4;
            block + usize::from((case.immediate >> (within * 2)) & 3)
        }
        ShuffleKind::ShuffleHighWord => {
            let block = destination_lane / 8 * 8;
            let within = destination_lane % 8;
            if within < 4 {
                destination_lane
            } else {
                block + 4 + usize::from((case.immediate >> ((within - 4) * 2)) & 3)
            }
        }
        ShuffleKind::ShuffleLowWord => {
            let block = destination_lane / 8 * 8;
            let within = destination_lane % 8;
            if within < 4 {
                block + usize::from((case.immediate >> (within * 2)) & 3)
            } else {
                destination_lane
            }
        }
    }
}

fn architectural_expected(case: ShuffleCase, initial: &ShuffleState) -> ShuffleState {
    let mut expected = initial.clone();
    let source = initial.vectors[usize::from(case.source)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    destination.fill(0);
    let bits = case.kind.element_bits();
    let lanes = case.width.bits() / usize::from(bits);
    for lane in 0..lanes {
        let value = extract_lane(&source, source_lane(case, lane), bits);
        insert_lane(destination, lane, bits, value);
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
    initial: &ShuffleState,
    level: crate::smir::optimize::OptLevel,
) -> ShuffleState {
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
    ShuffleState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_equations_for_all_shapes_aliases_and_immediates() {
    let cases = cases();
    assert_eq!(cases.len(), 546);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
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
    initial: &ShuffleState,
    level: crate::smir::optimize::OptLevel,
) -> ShuffleState {
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
    let exec = ExecMem::new(&code).expect("map VEX lane-shuffle replay");
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
    ShuffleState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_LANE_SHUFFLE_CHILD_RANGE";

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
fn native_cases() -> Vec<ShuffleCase> {
    let avx2 = std::is_x86_feature_detected!("avx2");
    cases()
        .into_iter()
        .filter(|case| !case.needs_avx2() || avx2)
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[ShuffleCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
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
        .expect("run isolated native VEX lane-shuffle differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert!(!cases.is_empty());
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
        "isolated native VEX lane-shuffle failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_state_for_all_supported_shapes_aliases_and_immediates() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX lane-shuffle differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_lane_shuffle_replay::\
         replay_matches_intel_o0_o2_state_for_all_supported_shapes_aliases_and_immediates",
    );
}
