//! Native replay coverage for register-only AVX VEX immediate blends.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xB1E0;
const OPERANDS: [(u8, u8, u8); 9] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 2),
    (1, 2, 1),
    (1, 2, 2),
    (1, 3, 1),
    (1, 1, 3),
    (1, 3, 3),
    (1, 1, 1),
];
const IMMEDIATES: [u8; 5] = [0x00, 0xFF, 0xA5, 0x5A, 0x81];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    Dword,
    PackedSingle,
    PackedDouble,
    Word,
}

impl Blend {
    const ALL: [Self; 4] = [
        Self::Dword,
        Self::PackedSingle,
        Self::PackedDouble,
        Self::Word,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::Dword => 0x02,
            Self::PackedSingle => 0x0C,
            Self::PackedDouble => 0x0D,
            Self::Word => 0x0E,
        }
    }

    fn element_bits(self) -> usize {
        match self {
            Self::Dword | Self::PackedSingle => 32,
            Self::PackedDouble => 64,
            Self::Word => 16,
        }
    }

    fn legal_w(self) -> &'static [bool] {
        match self {
            Self::Dword => &[false],
            Self::PackedSingle | Self::PackedDouble | Self::Word => &[false, true],
        }
    }

    fn needs_avx2(self, width: Width) -> bool {
        self == Self::Dword || self == Self::Word && width == Width::V256
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    V128,
    V256,
}

impl Width {
    fn is_256(self) -> bool {
        self == Self::V256
    }

    fn bytes(self) -> usize {
        if self.is_256() { 32 } else { 16 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlendCase {
    blend: Blend,
    width: Width,
    w: bool,
    dst: u8,
    src1: u8,
    src2: u8,
    imm: u8,
    clear_ignored_x: bool,
}

impl BlendCase {
    fn needs_avx2(self) -> bool {
        self.blend.needs_avx2(self.width)
    }
}

fn encoding(case: BlendCase) -> [u8; 6] {
    assert!(case.blend.legal_w().contains(&case.w));
    assert!(case.dst < 16 && case.src1 < 16 && case.src2 < 16);
    let mut p0 = 0xE3;
    if case.dst >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.src2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(case.w) << 7)
            | ((!case.src1 & 0x0F) << 3)
            | (u8::from(case.width.is_256()) << 2)
            | 1,
        case.blend.opcode(),
        0xC0 | ((case.dst & 7) << 3) | (case.src2 & 7),
        case.imm,
    ]
}

fn cases() -> Vec<BlendCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for blend in Blend::ALL {
        for &w in blend.legal_w() {
            for width in [Width::V128, Width::V256] {
                for (dst, src1, src2) in OPERANDS {
                    for imm in IMMEDIATES {
                        cases.push(BlendCase {
                            blend,
                            width,
                            w,
                            dst,
                            src1,
                            src2,
                            imm,
                            clear_ignored_x: ordinal & 1 != 0,
                        });
                        ordinal += 1;
                    }
                }
            }
        }
    }
    cases
}

fn function_at(bytes: &[u8; 6], block_id: BlockId, pc: u64) -> crate::smir::ir::SmirFunction {
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

fn function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
    function_at(bytes, BlockId(0), PC)
}

#[test]
fn replay_features_select_avx_ymm16_and_distinguish_avx_from_avx2_forms() {
    for (blend, width, expected_avx2) in [
        (Blend::Dword, Width::V128, true),
        (Blend::Dword, Width::V256, true),
        (Blend::PackedSingle, Width::V128, false),
        (Blend::PackedSingle, Width::V256, false),
        (Blend::PackedDouble, Width::V128, false),
        (Blend::PackedDouble, Width::V256, false),
        (Blend::Word, Width::V128, false),
        (Blend::Word, Width::V256, true),
    ] {
        let case = BlendCase {
            blend,
            width,
            w: false,
            dst: 9,
            src1: 10,
            src2: 11,
            imm: 0xA5,
            clear_ignored_x: true,
        };
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert_eq!(requirements.needs_avx2, expected_avx2, "{case:?}");
        assert!(!requirements.needs_fma, "{case:?}");
        assert!(!requirements.needs_fma4, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(!requirements.needs_avx512cd, "{case:?}");
        assert!(!requirements.needs_gfni, "{case:?}");
        assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
        assert!(!requirements.needs_vpclmulqdq, "{case:?}");
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        {
            let supported = std::is_x86_feature_detected!("avx")
                && (!expected_avx2 || std::is_x86_feature_detected!("avx2"));
            assert_eq!(requirements.x86_host_supported(), supported, "{case:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                supported,
                "{case:?}"
            );
        }
    }

    let blend = encoding(BlendCase {
        blend: Blend::Word,
        width: Width::V256,
        w: true,
        dst: 9,
        src1: 10,
        src2: 11,
        imm: 0xA5,
        clear_ignored_x: true,
    });
    let mut mixed = function(&blend);
    let fma4 = [0xC4, 0xE3, 0xE9, 0x68, 0xCC, 0x30];
    let mut fma4_function = function_at(&fma4, BlockId(1), PC + 0x10);
    mixed.add_block(fma4_function.blocks.remove(0));
    mixed
        .x86_instruction_bytes
        .extend(fma4_function.x86_instruction_bytes);
    let requirements =
        x86_native_replay_feature_requirements(&mixed, &std::collections::HashMap::new());
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(requirements.needs_avx2);
    assert!(requirements.needs_fma4);
    assert!(!requirements.needs_avx512bw);
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &mixed,
        &std::collections::HashMap::new()
    ));
}

#[test]
fn replay_admits_and_emits_1_260_o0_o2_family_width_w_alias_and_immediate_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 630);
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
    assert_eq!(lowered, 1_260);

    let case = BlendCase {
        blend: Blend::Word,
        width: Width::V256,
        w: true,
        dst: 1,
        src1: 2,
        src2: 3,
        imm: 0xA5,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlendState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> BlendState {
    BlendState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
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

fn element_bits(vector: &[u64; 8], width: usize, lane: usize) -> u64 {
    let bit = lane * width;
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    (vector[bit / 64] >> (bit % 64)) & mask
}

fn set_element_bits(vector: &mut [u64; 8], width: usize, lane: usize, value: u64) {
    let bit = lane * width;
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let shift = bit % 64;
    vector[bit / 64] = (vector[bit / 64] & !(mask << shift)) | ((value & mask) << shift);
}

fn architectural_expected(case: BlendCase, initial: &BlendState) -> BlendState {
    let source1 = initial.vectors[usize::from(case.src1)];
    let source2 = initial.vectors[usize::from(case.src2)];
    let element_width = case.blend.element_bits();
    let lanes = case.width.bytes() * 8 / element_width;
    let mut expected = initial.clone();
    let destination = &mut expected.vectors[usize::from(case.dst)];
    destination.fill(0);
    for lane in 0..lanes {
        let immediate_bit = if case.blend == Blend::Word {
            lane & 7
        } else {
            lane
        };
        let source = if case.imm & (1 << immediate_bit) == 0 {
            &source1
        } else {
            &source2
        };
        set_element_bits(
            destination,
            element_width,
            lane,
            element_bits(source, element_width, lane),
        );
    }
    expected
}

fn optimized_function(
    bytes: &[u8; 6],
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
    bytes: &[u8; 6],
    initial: &BlendState,
    level: crate::smir::optimize::OptLevel,
) -> BlendState {
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
    BlendState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_lane_equations_aliases_repeated_masks_and_upper_zeroing() {
    let cases = cases();
    assert_eq!(cases.len(), 630);
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
    bytes: &[u8; 6],
    initial: &BlendState,
    level: crate::smir::optimize::OptLevel,
) -> BlendState {
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
    let exec = ExecMem::new(&code).expect("map VEX immediate-blend replay");
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
    BlendState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<BlendCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for blend in Blend::ALL {
        for &w in blend.legal_w() {
            for width in [Width::V128, Width::V256] {
                for operand_index in 0..OPERANDS.len() {
                    for imm in [0x00, 0xA5] {
                        let (dst, src1, src2) = OPERANDS[operand_index];
                        cases.push(BlendCase {
                            blend,
                            width,
                            w,
                            dst,
                            src1,
                            src2,
                            imm,
                            clear_ignored_x: ordinal & 1 != 0,
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
const CHILD_RANGE_ENV: &str = "RAX_VEX_IMMEDIATE_BLEND_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[BlendCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        if case.needs_avx2() && !std::is_x86_feature_detected!("avx2") {
            continue;
        }
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
        .expect("run isolated native VEX immediate-blend differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 252);
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
        "isolated native VEX immediate-blend failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_equations_aliases_immediates_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX immediate-blend differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_immediate_blend_replay::\
         replay_matches_intel_o0_o2_equations_aliases_immediates_and_full_state",
    );
}
