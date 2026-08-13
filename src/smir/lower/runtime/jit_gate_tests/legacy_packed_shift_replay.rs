//! Native replay coverage for register-only legacy SSE2 packed shifts.
//!
//! Encoding, count saturation, low-64-bit shared-count selection, destructive
//! XMM destinations, unaffected flags, and preservation above bit 127 follow
//! Intel SDM Order No. 325383-092US (June 2026), Vol. 2B, pp. 4-441--4-478,
//! and AMD APM 26568 Rev. 3.26 (January 2026), pp. 643--670.

use super::*;
use crate::smir::ir::types::{FunctionId, ShiftOp, SourceArch, VecElementType, VecWidth};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x5A1F_7002;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const MATERIALIZED_FLAG_MASK: u64 = 0xCD5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    opcode: u8,
    group: Option<u8>,
    elem: VecElementType,
    shift: ShiftOp,
    byte_lane: bool,
}

impl Shape {
    const fn immediate(
        opcode: u8,
        group: u8,
        elem: VecElementType,
        shift: ShiftOp,
        byte_lane: bool,
    ) -> Self {
        Self {
            opcode,
            group: Some(group),
            elem,
            shift,
            byte_lane,
        }
    }

    const fn register(opcode: u8, elem: VecElementType, shift: ShiftOp) -> Self {
        Self {
            opcode,
            group: None,
            elem,
            shift,
            byte_lane: false,
        }
    }

    const fn immediate_count(self) -> bool {
        self.group.is_some()
    }

    const fn element_bits(self) -> u64 {
        self.elem.bytes() as u64 * 8
    }
}

const SHAPES: [Shape; 18] = [
    Shape::immediate(0x71, 2, VecElementType::I16, ShiftOp::Lsr, false),
    Shape::immediate(0x71, 4, VecElementType::I16, ShiftOp::Asr, false),
    Shape::immediate(0x71, 6, VecElementType::I16, ShiftOp::Lsl, false),
    Shape::immediate(0x72, 2, VecElementType::I32, ShiftOp::Lsr, false),
    Shape::immediate(0x72, 4, VecElementType::I32, ShiftOp::Asr, false),
    Shape::immediate(0x72, 6, VecElementType::I32, ShiftOp::Lsl, false),
    Shape::immediate(0x73, 2, VecElementType::I64, ShiftOp::Lsr, false),
    Shape::immediate(0x73, 3, VecElementType::I8, ShiftOp::Lsr, true),
    Shape::immediate(0x73, 6, VecElementType::I64, ShiftOp::Lsl, false),
    Shape::immediate(0x73, 7, VecElementType::I8, ShiftOp::Lsl, true),
    Shape::register(0xD1, VecElementType::I16, ShiftOp::Lsr),
    Shape::register(0xD2, VecElementType::I32, ShiftOp::Lsr),
    Shape::register(0xD3, VecElementType::I64, ShiftOp::Lsr),
    Shape::register(0xE1, VecElementType::I16, ShiftOp::Asr),
    Shape::register(0xE2, VecElementType::I32, ShiftOp::Asr),
    Shape::register(0xF1, VecElementType::I16, ShiftOp::Lsl),
    Shape::register(0xF2, VecElementType::I32, ShiftOp::Lsl),
    Shape::register(0xF3, VecElementType::I64, ShiftOp::Lsl),
];

fn encoding(shape: Shape, rex: Option<u8>, operand: u8, amount: u8) -> Vec<u8> {
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, shape.opcode]);
    if let Some(group) = shape.group {
        bytes.extend([0xC0 | (group << 3) | operand, amount]);
    } else {
        bytes.push(operand);
    }
    bytes
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
        X86InstructionBytes::new(bytes).expect("legacy packed-shift provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[test]
fn feature_requirements_select_sse2_baseline_and_avx_ymm16_state() {
    for bytes in [
        encoding(SHAPES[0], Some(0x49), 2, 15),
        encoding(SHAPES[17], Some(0x45), 0xCA, 0),
    ] {
        let function = function(&bytes, OptLevel::O2, false);
        let excluded = std::collections::HashMap::new();
        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(uses_x86_native_vectors_excluding(&function, &excluded));
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements {
                any: true,
                all_spans_support_avx_ymm16: true,
                needs_avx: true,
                ..X86NativeReplayFeatureRequirements::default()
            },
            "{bytes:02X?}"
        );
        assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
            &function, &excluded
        ));

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            std::is_x86_feature_detected!("avx"),
            "{bytes:02X?}"
        );

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{bytes:02X?}"
        );
    }
}

fn assert_exact_replay(code: &[u8], bytes: &[u8]) {
    let positions: Vec<_> = code
        .windows(bytes.len())
        .enumerate()
        .filter_map(|(position, window)| (window == bytes).then_some(position))
        .collect();
    assert_eq!(positions.len(), 1, "source={bytes:02X?}");
}

fn assert_admitted_and_emitted(bytes: &[u8], level: OptLevel) {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = function(bytes, level, false);
    assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_exact_replay(&code, bytes);
}

#[test]
fn all_30192_shape_rex_register_and_o0_o1_o2_cases_admit_and_emit_exactly() {
    let mut lowered = 0usize;
    for shape in SHAPES {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            let operands: Box<dyn Iterator<Item = u8>> = if shape.immediate_count() {
                Box::new(0..8)
            } else {
                Box::new(0xC0..=0xFF)
            };
            for operand in operands {
                let bytes = encoding(shape, rex, operand, 0xA5);
                for level in LEVELS {
                    assert_admitted_and_emitted(&bytes, level);
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, (10 * 17 * 8 + 8 * 17 * 64) * LEVELS.len());
}

#[test]
fn all_7680_immediate_shape_value_and_level_cases_admit_and_emit_exactly() {
    let mut lowered = 0usize;
    for shape in SHAPES.into_iter().filter(|shape| shape.immediate_count()) {
        for amount in u8::MIN..=u8::MAX {
            let bytes = encoding(shape, Some(0x45), 2, amount);
            for level in LEVELS {
                assert_admitted_and_emitted(&bytes, level);
                lowered += 1;
            }
        }
    }
    assert_eq!(lowered, 10 * 256 * LEVELS.len());
}

#[test]
fn admission_fails_closed_for_missing_mismatched_memory_and_noncanonical_provenance() {
    let bytes = encoding(SHAPES[16], Some(0x45), 0xCA, 0);
    let baseline = function(&bytes, OptLevel::O0, false);

    let mut missing = baseline.clone();
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing), "missing provenance");

    for metadata in [
        encoding(SHAPES[16], Some(0x45), 0xD3, 0),
        encoding(SHAPES[15], Some(0x45), 0xCA, 0),
        {
            let mut prefixed = vec![0x67];
            prefixed.extend(encoding(SHAPES[16], None, 0xCA, 0));
            prefixed
        },
    ] {
        let mut malformed = baseline.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&metadata).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{metadata:02X?}");
        assert!(
            !x86_native_replay_feature_requirements(&malformed, &Default::default()).any,
            "{metadata:02X?}"
        );
    }

    let memory = function(&[0x66, 0x0F, 0xF2, 0x0A], OptLevel::O2, false);
    assert!(!is_native_clobber_safe(&memory), "memory source");
    assert!(
        !is_native_clobber_safe_excluding(&memory, &Default::default(), true),
        "memory source with helper admission"
    );
    assert!(
        !x86_native_replay_feature_requirements(&memory, &Default::default()).any,
        "memory-source replay requirements"
    );

    // Prefix-free MMX forms already use their independent direct lowerer and
    // x87-tag bridge; this XMM replay classifier must not claim them.
    let mmx = function(&[0x0F, 0xF2, 0xCA], OptLevel::O2, false);
    assert!(is_native_clobber_safe(&mmx));
    assert!(
        crate::smir::ir::x86_native_replay_spans(&mmx.blocks[0], &mmx.x86_instruction_bytes,)
            .is_empty()
    );
}

#[derive(Clone, Copy, Debug)]
struct NativeCase {
    shape: Shape,
    level: OptLevel,
    rex: Option<u8>,
    operand: u8,
    amount: u8,
    shared_count: u64,
    seed: usize,
}

impl NativeCase {
    fn bytes(self) -> Vec<u8> {
        encoding(self.shape, self.rex, self.operand, self.amount)
    }

    fn destination(self) -> usize {
        let rex = self.rex.unwrap_or(0);
        if self.shape.immediate_count() {
            usize::from((self.operand & 7) | ((rex & 1) << 3))
        } else {
            usize::from(((self.operand >> 3) & 7) | ((rex & 4) << 1))
        }
    }

    fn count_source(self) -> Option<usize> {
        (!self.shape.immediate_count()).then(|| {
            let rex = self.rex.unwrap_or(0);
            usize::from((self.operand & 7) | ((rex & 1) << 3))
        })
    }

    fn count(self) -> u64 {
        if self.shape.immediate_count() {
            u64::from(self.amount)
        } else {
            self.shared_count
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackedShiftState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    mm: [u64; 8],
    masks: [u64; 8],
    rflags: u64,
    ac_flag: u64,
    mxcsr: u32,
    x87_tag_word: u64,
}

fn initial_state(case: NativeCase) -> PackedShiftState {
    let mut state = PackedShiftState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
                ^ (case.seed as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x8123_C567_09AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                    ^ (case.seed as u64).wrapping_mul(0x8040_2010_0804_0201)
            })
        }),
        mm: std::array::from_fn(|index| {
            0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + case.seed) as u32)
        }),
        masks: std::array::from_fn(|index| {
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + case.seed) as u32)
        }),
        rflags: 0x2 | 0x8D5 | (1 << 10) | (3 << 12),
        ac_flag: (case.seed & 1) as u64,
        mxcsr: 0x1F80 | (1 << (case.seed % 6)) | (((case.seed / 3) as u32 & 3) << 13),
        x87_tag_word: [0xFFFF, 0xA5A5, 0x0000, 0x6996][case.seed & 3],
    };
    if let Some(source) = case.count_source() {
        // Only bits 63:0 select the count; bits 127:64 remain adversarial.
        state.vectors[source][0] = case.shared_count;
        state.vectors[source][1] = !case.shared_count;
    }
    state
}

fn shifted_lane(value: u64, bits: u64, count: u64, shift: ShiftOp) -> u64 {
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let value = value & mask;
    match shift {
        ShiftOp::Lsl => (count < bits).then(|| (value << count) & mask).unwrap_or(0),
        ShiftOp::Lsr => (count < bits).then(|| value >> count).unwrap_or(0),
        ShiftOp::Asr => {
            let sign = value & (1u64 << (bits - 1)) != 0;
            if count >= bits {
                if sign { mask } else { 0 }
            } else {
                let signed = if bits == 64 {
                    value as i64
                } else {
                    ((value << (64 - bits)) as i64) >> (64 - bits)
                };
                (signed >> count) as u64 & mask
            }
        }
        ShiftOp::Ror | ShiftOp::Rrx => unreachable!(),
    }
}

fn architectural_expected(case: NativeCase, initial: &PackedShiftState) -> PackedShiftState {
    let mut expected = initial.clone();
    let destination = case.destination();
    let input = u128::from(initial.vectors[destination][0])
        | (u128::from(initial.vectors[destination][1]) << 64);
    let output = if case.shape.byte_lane {
        let bits = case.count().saturating_mul(8);
        if bits >= 128 {
            0
        } else if case.shape.shift == ShiftOp::Lsl {
            input << bits
        } else {
            input >> bits
        }
    } else {
        let bits = case.shape.element_bits();
        let lanes = 128 / bits;
        let lane_mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let mut result = 0u128;
        for lane in 0..lanes {
            let value = ((input >> (lane * bits)) as u64) & lane_mask;
            result |= u128::from(shifted_lane(value, bits, case.count(), case.shape.shift))
                << (lane * bits);
        }
        result
    };
    expected.vectors[destination][0] = output as u64;
    expected.vectors[destination][1] = (output >> 64) as u64;
    expected
}

fn interpret(case: NativeCase, initial: &PackedShiftState) -> PackedShiftState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let bytes = case.bytes();
    let function = function(&bytes, case.level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        x86.mm = initial.mm;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags | (initial.ac_flag << 18);
        x86.mxcsr = initial.mxcsr;
        x86.x87.tag_word = initial.x87_tag_word as u16;
    }
    context.flags.materialized =
        MaterializedFlags::from_rflags(initial.rflags | (initial.ac_flag << 18));
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
    PackedShiftState {
        gprs: x86.gpr,
        vectors,
        mm: x86.mm,
        masks: x86.k,
        rflags: (initial.rflags & !MATERIALIZED_FLAG_MASK)
            | (context.flags.materialized.to_rflags() & MATERIALIZED_FLAG_MASK),
        ac_flag: u64::from(context.flags.materialized.ac),
        mxcsr: x86.mxcsr,
        x87_tag_word: u64::from(x86.x87.tag_word),
    }
}

fn native_cases() -> Vec<NativeCase> {
    const REX: [Option<u8>; 4] = [None, Some(0x40), Some(0x45), Some(0x4F)];
    const SHARED_OPERANDS: [u8; 4] = [0xC0, 0xCA, 0xC9, 0xFF];
    let mut cases = Vec::new();
    let mut seed = 0usize;
    for level in LEVELS {
        for shape in SHAPES {
            if shape.immediate_count() {
                let limit = if shape.byte_lane {
                    16
                } else {
                    shape.element_bits()
                };
                for (index, amount) in [
                    0,
                    1,
                    limit.saturating_sub(1) as u8,
                    limit as u8,
                    limit.saturating_add(1) as u8,
                    127,
                    128,
                    255,
                ]
                .into_iter()
                .enumerate()
                {
                    cases.push(NativeCase {
                        shape,
                        level,
                        rex: REX[index % REX.len()],
                        operand: (index % 8) as u8,
                        amount,
                        shared_count: 0,
                        seed,
                    });
                    seed += 1;
                }
            } else {
                let bits = shape.element_bits();
                for (encoding_index, operand) in SHARED_OPERANDS.into_iter().enumerate() {
                    for count in [
                        0,
                        1,
                        bits - 1,
                        bits,
                        bits + 1,
                        0x0000_0001_0000_0000,
                        u64::MAX,
                    ] {
                        cases.push(NativeCase {
                            shape,
                            level,
                            rex: REX[encoding_index],
                            operand,
                            amount: 0,
                            shared_count: count,
                            seed,
                        });
                        seed += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 3 * (10 * 8 + 8 * 4 * 7));
    cases
}

#[test]
fn interpreter_matches_manual_all_912_boundary_alias_level_and_full_state_cases() {
    for case in native_cases() {
        let initial = initial_state(case);
        let expected = architectural_expected(case, &initial);
        assert_eq!(interpret(case, &initial), expected, "{case:?}");
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: NativeCase, initial: &PackedShiftState) -> PackedShiftState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let bytes = case.bytes();
    let function = function(&bytes, case.level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_exact_replay(&code, &bytes);
    let exec = ExecMem::new(&code).expect("map legacy packed-shift replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        ac_flag: initial.ac_flag,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        mm: initial.mm,
        x87_tag_word: initial.x87_tag_word,
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
    PackedShiftState {
        gprs: registers.gpr,
        vectors,
        mm: registers.mm,
        masks: registers.k,
        rflags: registers.rflags,
        ac_flag: registers.ac_flag,
        mxcsr: registers.mxcsr,
        x87_tag_word: registers.x87_tag_word,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_PACKED_SHIFT_CHILD_RANGE";

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
        let initial = initial_state(*case);
        let expected = architectural_expected(*case, &initial);
        assert_eq!(
            interpret(*case, &initial),
            expected,
            "{case:?}: interpreter"
        );
        assert_eq!(
            execute_native(*case, &initial),
            expected,
            "{case:?}: native"
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
        .expect("run isolated native legacy packed-shift differential")
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
    panic!(
        "isolated native legacy packed-shift failure at case {start}/{}: \
         {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case.bytes(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn all_912_native_cases_match_interpretation_equation_and_full_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy packed-shift differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_packed_shift_replay::\
         all_912_native_cases_match_interpretation_equation_and_full_state",
    );
}
