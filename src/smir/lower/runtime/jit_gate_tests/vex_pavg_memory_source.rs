//! Exact helper-backed VEX packed unsigned rounded-average memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VLaneOp, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xBA60;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AverageKind {
    Byte,
    Word,
}

impl AverageKind {
    const ALL: [Self; 2] = [Self::Byte, Self::Word];

    const fn elem(self) -> VecElementType {
        match self {
            Self::Byte => VecElementType::I8,
            Self::Word => VecElementType::I16,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::Byte => 0xE0,
            Self::Word => 0xE3,
        }
    }

    fn apply(self, source1: u64, source2: u64) -> u64 {
        let bits = self.elem().bytes() * 8;
        let mask = (1u64 << bits) - 1;
        ((source1 & mask) + (source2 & mask) + 1) >> 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    C5,
    C4W0,
    C4W1,
}

impl EncodingForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];

    const fn w(self) -> bool {
        matches!(self, Self::C4W1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AverageMemoryCase {
    kind: AverageKind,
    width: VecWidth,
    form: EncodingForm,
}

impl AverageMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Low distinct operands force helper scratch XMM/YMM2.
            EncodingForm::C5 => (0, 1, 3),
            // High destination plus source1 0 forces helper scratch 1.
            EncodingForm::C4W0 => (15, 0, 11),
            // Aliased high destination/source1 force helper scratch 0.
            EncodingForm::C4W1 => (9, 9, 11),
        }
    }

    const fn destination(self) -> u8 {
        self.operands().0
    }

    const fn source1(self) -> u8 {
        self.operands().1
    }

    const fn base(self) -> u8 {
        self.operands().2
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source1())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match self.form {
            EncodingForm::C5 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | 1,
                self.kind.opcode(),
                modrm,
                DISP as u8,
            ],
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode(),
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_average_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0xC0 | ((destination & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | 1,
                self.kind.opcode(),
                modrm,
            ]
        } else {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode(),
                modrm,
            ]
        }
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX packed average has only 128-/256-bit forms"),
    })
}

fn expected_address(case: AverageMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_pair(ops: &[SmirOp], case: AverageMemoryCase) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected exact VLoad + VLane pair, got {ops:?}")
    };
    assert_eq!(
        load.x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}"
    );
    let temporary = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *temporary
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };
    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(consumer.x86_hint, None, "{case:?}");
    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op,
        signed,
        set_ovf,
    } = &consumer.kind
    else {
        panic!("{case:?}: expected VLane consumer, got {consumer:?}")
    };
    assert_eq!(*dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(*src1, vector(case.source1(), case.width), "{case:?}");
    assert_eq!(*src2, temporary, "{case:?}");
    assert_eq!(*elem, case.kind.elem(), "{case:?}");
    assert_eq!(*lanes, case.width.lanes(case.kind.elem()) as u8, "{case:?}");
    assert_eq!(*op, VLaneOp::AvgRnd, "{case:?}");
    assert!(!signed, "{case:?}");
    assert!(!set_ovf, "{case:?}");
}

fn lift_case(case: AverageMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_pair(&result.ops, case);

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction, case: AverageMemoryCase) -> (Vec<u8>, usize) {
    let excluded = std::collections::HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert_eq!(
        requirements.needs_avx2,
        case.width == VecWidth::V256,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX average lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX average"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<AverageMemoryCase> {
    let mut cases = Vec::new();
    for kind in AverageKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in EncodingForm::ALL {
                cases.push(AverageMemoryCase { kind, width, form });
            }
        }
    }
    cases
}

#[test]
fn all_12_c4_c5_wig_width_and_average_shapes_are_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 3);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function.blocks[0].ops, case);
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            let expected = case.emitted_average_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 12 * LEVELS.len());
}

#[test]
fn register_rewrite_matches_independent_llvm_23_encodings() {
    for (case, expected) in [
        (
            AverageMemoryCase {
                kind: AverageKind::Byte,
                width: VecWidth::V128,
                form: EncodingForm::C5,
            },
            &[0xC5, 0xF1, 0xE0, 0xC2][..],
        ),
        (
            AverageMemoryCase {
                kind: AverageKind::Word,
                width: VecWidth::V128,
                form: EncodingForm::C4W0,
            },
            &[0xC5, 0x79, 0xE3, 0xF9][..],
        ),
        (
            AverageMemoryCase {
                kind: AverageKind::Word,
                width: VecWidth::V256,
                form: EncodingForm::C5,
            },
            &[0xC5, 0xF5, 0xE3, 0xC2][..],
        ),
    ] {
        assert_eq!(case.emitted_average_bytes(), expected, "{case:?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), true),
        "{name}: clobber gate admitted malformed pair"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed pair"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn packed_average_classifier_and_lowerer_fail_closed_for_every_pair_invariant() {
    let case = AverageMemoryCase {
        kind: AverageKind::Byte,
        width: VecWidth::V128,
        form: EncodingForm::C4W0,
    };
    let base = lift_case(case);
    let temporary = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: temporary,
            width: VecWidth::V128,
        },
    ));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::VLoad {
            dst: temporary,
            addr: expected_address(case),
            width: VecWidth::V128,
        },
    ));

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;

    let mut aligned_load_hint = base.clone();
    aligned_load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }

    let mut invalid_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut invalid_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;

    let mut wrong_source2 = base.clone();
    if let OpKind::VLane { src2, .. } = &mut wrong_source2.blocks[0].ops[1].kind {
        *src2 = vector(2, VecWidth::V128);
    }

    let mut wrong_operation = base.clone();
    if let OpKind::VLane { op, .. } = &mut wrong_operation.blocks[0].ops[1].kind {
        *op = VLaneOp::Avg;
    }

    let mut signed = base.clone();
    if let OpKind::VLane {
        signed: is_signed, ..
    } = &mut signed.blocks[0].ops[1].kind
    {
        *is_signed = true;
    }

    let mut sets_overflow = base.clone();
    if let OpKind::VLane { set_ovf, .. } = &mut sets_overflow.blocks[0].ops[1].kind {
        *set_ovf = true;
    }

    let mut wrong_element = base.clone();
    if let OpKind::VLane { elem, lanes, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I16;
        *lanes = 8;
    }

    let mut wrong_lanes = base.clone();
    if let OpKind::VLane { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }

    let mut high_destination = base.clone();
    if let OpKind::VLane { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V128);
    }

    let mut high_source1 = base.clone();
    if let OpKind::VLane { src1, .. } = &mut high_source1.blocks[0].ops[1].kind {
        *src1 = vector(16, VecWidth::V128);
    }

    let mut wrong_namespace = base.clone();
    if let OpKind::VLane { dst, .. } = &mut wrong_namespace.blocks[0].ops[1].kind {
        *dst = x86(X86Reg::Ymm(case.destination()));
    }

    let mut consumer_hint = base.clone();
    consumer_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xE0,
        width: VecWidth::V128,
        w: false,
    });

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();

    let mut encoded_destination = case.bytes();
    encoded_destination[4] = (encoded_destination[4] & !0x38) | 0x30;
    let mut byte_destination_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_destination_mismatch, &encoded_destination);

    let mut encoded_source1 = case.bytes();
    encoded_source1[2] = (encoded_source1[2] & !0x78) | (((!1u8) & 0x0F) << 3);
    let mut byte_source1_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_source1_mismatch, &encoded_source1);

    let mut encoded_element = case.bytes();
    encoded_element[3] = 0xE3;
    let mut byte_element_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_element_mismatch, &encoded_element);

    let mut encoded_width = case.bytes();
    encoded_width[2] |= 0x04;
    let mut byte_width_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_width_mismatch, &encoded_width);

    let malformed = [
        ("temporary used twice", extra_use),
        ("temporary defined twice", duplicate_definition),
        ("missing unaligned load hint", missing_load_hint),
        ("aligned load hint", aligned_load_hint),
        ("load/consumer width mismatch", load_width),
        ("virtual address component", invalid_address),
        ("different guest PCs", wrong_pc),
        ("consumer bypasses temporary", wrong_source2),
        ("truncating average", wrong_operation),
        ("signed average", signed),
        ("overflow-setting average", sets_overflow),
        ("element/opcode mismatch", wrong_element),
        ("nonintegral lane geometry", wrong_lanes),
        ("high EVEX-only destination", high_destination),
        ("high EVEX-only first source", high_source1),
        ("destination register namespace mismatch", wrong_namespace),
        ("consumer carries an encoding hint", consumer_hint),
        ("missing instruction-byte provenance", missing_bytes),
        ("encoded destination mismatch", byte_destination_mismatch),
        ("encoded first-source mismatch", byte_source1_mismatch),
        ("encoded element mismatch", byte_element_mismatch),
        ("encoded width mismatch", byte_width_mismatch),
    ];
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn read_lane(bytes: &[u8], offset: usize, size: usize) -> u64 {
    bytes[offset..offset + size]
        .iter()
        .enumerate()
        .fold(0u64, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        })
}

fn write_lane(bytes: &mut [u8], offset: usize, size: usize, value: u64) {
    let encoded = value.to_le_bytes();
    bytes[offset..offset + size].copy_from_slice(&encoded[..size]);
}

fn operand_vectors(case: AverageMemoryCase) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    let bits = case.kind.elem().bytes() * 8;
    let mask = (1u64 << bits) - 1;
    let values = [
        (0, 0),
        (0, mask),
        (mask, 0),
        (mask, mask),
        (mask - 1, mask),
        (1, 2),
        (2, 3),
        (0x55 & mask, 0xAA & mask),
    ];
    for lane in 0..lanes {
        let (left, right) = values[lane % values.len()];
        write_lane(&mut source1, lane * lane_size, lane_size, left);
        write_lane(&mut source2, lane * lane_size, lane_size, right);
    }
    (bytes_to_words(source1), bytes_to_words(source2))
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32)
    {
        return 0;
    }

    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: AverageMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    let (source1, _) = operand_vectors(case);
    registers.zmm[usize::from(case.source1())] = source1;
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] = std::array::from_fn(|word| {
            0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7 + ordinal) as u32)
        });
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: AverageMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = words_to_bytes(registers.zmm[usize::from(case.source1())]);
    let source2_bytes = words_to_bytes(source2);
    let mut result = [0; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    for lane in 0..lanes {
        let offset = lane * lane_size;
        let left = read_lane(&source1, offset, lane_size);
        let right = read_lane(&source2_bytes, offset, lane_size);
        write_lane(&mut result, offset, lane_size, case.kind.apply(left, right));
    }
    registers.zmm[usize::from(case.destination())] = bytes_to_words(result);
    let words = (case.width.bytes() / 8) as usize;
    registers.vector_scratch =
        std::array::from_fn(|word| if word < words { source2[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: AverageMemoryCase,
    level: OptLevel,
) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let bytes = words_to_bytes(source2);
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, value) in expected.zmm.iter().enumerate() {
        assert_eq!(
            &x86.xmm[index][..8],
            value,
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(x86.k, expected.k, "{level:?} {case:?}: masks");
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_packed_average_matches_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX PAVG memory differential: host lacks AVX");
        return;
    }

    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases = all_cases()
        .into_iter()
        .filter(|case| avx2 || case.width == VecWidth::V128)
        .collect::<Vec<_>>();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let (_, source2) = operand_vectors(case);

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, source2);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function, &initial, &expected, source2, address, case, level,
            );
            successes += 1;

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!("executed {successes} successful and {faults} faulting native VEX PAVG memory cases");
}
