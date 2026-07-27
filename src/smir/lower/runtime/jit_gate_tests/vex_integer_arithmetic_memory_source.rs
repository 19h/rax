//! Exact helper-backed VEX packed-integer add/subtract memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecElementType, VecWidth,
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

const PC: u64 = 0xB940;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArithmeticKind {
    AddWrapI8,
    AddWrapI16,
    AddWrapI32,
    AddWrapI64,
    AddSignedSatI8,
    AddSignedSatI16,
    AddUnsignedSatI8,
    AddUnsignedSatI16,
    SubWrapI8,
    SubWrapI16,
    SubWrapI32,
    SubWrapI64,
    SubSignedSatI8,
    SubSignedSatI16,
    SubUnsignedSatI8,
    SubUnsignedSatI16,
}

impl ArithmeticKind {
    const ALL: [Self; 16] = [
        Self::AddWrapI8,
        Self::AddWrapI16,
        Self::AddWrapI32,
        Self::AddWrapI64,
        Self::AddSignedSatI8,
        Self::AddSignedSatI16,
        Self::AddUnsignedSatI8,
        Self::AddUnsignedSatI16,
        Self::SubWrapI8,
        Self::SubWrapI16,
        Self::SubWrapI32,
        Self::SubWrapI64,
        Self::SubSignedSatI8,
        Self::SubSignedSatI16,
        Self::SubUnsignedSatI8,
        Self::SubUnsignedSatI16,
    ];

    const fn elem(self) -> VecElementType {
        match self {
            Self::AddWrapI8
            | Self::AddSignedSatI8
            | Self::AddUnsignedSatI8
            | Self::SubWrapI8
            | Self::SubSignedSatI8
            | Self::SubUnsignedSatI8 => VecElementType::I8,
            Self::AddWrapI16
            | Self::AddSignedSatI16
            | Self::AddUnsignedSatI16
            | Self::SubWrapI16
            | Self::SubSignedSatI16
            | Self::SubUnsignedSatI16 => VecElementType::I16,
            Self::AddWrapI32 | Self::SubWrapI32 => VecElementType::I32,
            Self::AddWrapI64 | Self::SubWrapI64 => VecElementType::I64,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::AddWrapI8 => 0xFC,
            Self::AddWrapI16 => 0xFD,
            Self::AddWrapI32 => 0xFE,
            Self::AddWrapI64 => 0xD4,
            Self::AddSignedSatI8 => 0xEC,
            Self::AddSignedSatI16 => 0xED,
            Self::AddUnsignedSatI8 => 0xDC,
            Self::AddUnsignedSatI16 => 0xDD,
            Self::SubWrapI8 => 0xF8,
            Self::SubWrapI16 => 0xF9,
            Self::SubWrapI32 => 0xFA,
            Self::SubWrapI64 => 0xFB,
            Self::SubSignedSatI8 => 0xE8,
            Self::SubSignedSatI16 => 0xE9,
            Self::SubUnsignedSatI8 => 0xD8,
            Self::SubUnsignedSatI16 => 0xD9,
        }
    }

    const fn subtract(self) -> bool {
        matches!(
            self,
            Self::SubWrapI8
                | Self::SubWrapI16
                | Self::SubWrapI32
                | Self::SubWrapI64
                | Self::SubSignedSatI8
                | Self::SubSignedSatI16
                | Self::SubUnsignedSatI8
                | Self::SubUnsignedSatI16
        )
    }

    const fn saturation(self) -> Option<bool> {
        match self {
            Self::AddSignedSatI8
            | Self::AddSignedSatI16
            | Self::SubSignedSatI8
            | Self::SubSignedSatI16 => Some(true),
            Self::AddUnsignedSatI8
            | Self::AddUnsignedSatI16
            | Self::SubUnsignedSatI8
            | Self::SubUnsignedSatI16 => Some(false),
            _ => None,
        }
    }

    fn apply(self, source1: u64, source2: u64) -> u64 {
        let bits = self.elem().bytes() * 8;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        match self.saturation() {
            None if self.subtract() => source1.wrapping_sub(source2) & mask,
            None => source1.wrapping_add(source2) & mask,
            Some(false) => {
                let value = if self.subtract() {
                    i128::from(source1) - i128::from(source2)
                } else {
                    i128::from(source1) + i128::from(source2)
                };
                value.clamp(0, i128::from(mask)) as u64
            }
            Some(true) => {
                let shift = 64 - bits;
                let source1 = ((source1 << shift) as i64 >> shift) as i128;
                let source2 = ((source2 << shift) as i64 >> shift) as i128;
                let value = if self.subtract() {
                    source1 - source2
                } else {
                    source1 + source2
                };
                let minimum = -(1i128 << (bits - 1));
                let maximum = (1i128 << (bits - 1)) - 1;
                (value.clamp(minimum, maximum) as u64) & mask
            }
        }
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
struct ArithmeticMemoryCase {
    kind: ArithmeticKind,
    width: VecWidth,
    form: EncodingForm,
}

impl ArithmeticMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Destination/source1 occupy XMM/YMM0/1, forcing scratch register 2.
            EncodingForm::C5 => (0, 1, 3),
            // A high destination plus source1 XMM/YMM0 forces scratch register 1.
            EncodingForm::C4W0 => (15, 0, 11),
            // Aliased high destination/source1 force scratch register 0.
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
                    | 0x01,
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode(),
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_arithmetic_bytes(self) -> Vec<u8> {
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
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 0x01,
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
        _ => unreachable!("VEX packed arithmetic has only 128-/256-bit forms"),
    })
}

fn expected_address(case: ArithmeticMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn consumer_matches(kind: &OpKind, case: ArithmeticMemoryCase, temporary: VReg) -> bool {
    let destination = vector(case.destination(), case.width);
    let source1 = vector(case.source1(), case.width);
    let lanes = case.width.lanes(case.kind.elem()) as u8;
    match (kind, case.kind.saturation()) {
        (
            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes: actual_lanes,
            },
            None,
        ) if !case.kind.subtract() => {
            (*dst, *src1, *src2, *elem, *actual_lanes)
                == (destination, source1, temporary, case.kind.elem(), lanes)
        }
        (
            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes: actual_lanes,
            },
            None,
        ) if case.kind.subtract() => {
            (*dst, *src1, *src2, *elem, *actual_lanes)
                == (destination, source1, temporary, case.kind.elem(), lanes)
        }
        (
            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes: actual_lanes,
                subtract,
                signed,
            },
            Some(expected_signed),
        ) => {
            (*dst, *src1, *src2, *elem, *actual_lanes, *subtract, *signed)
                == (
                    destination,
                    source1,
                    temporary,
                    case.kind.elem(),
                    lanes,
                    case.kind.subtract(),
                    expected_signed,
                )
        }
        _ => false,
    }
}

fn assert_exact_pair(ops: &[SmirOp], case: ArithmeticMemoryCase) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected exact VLoad + arithmetic pair, got {ops:?}")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
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
    assert_eq!(
        consumer.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.kind.opcode(),
            width: case.width,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert!(
        consumer_matches(&consumer.kind, case, temporary),
        "{case:?}: unexpected arithmetic consumer {consumer:?}"
    );
}

fn lift_case(case: ArithmeticMemoryCase) -> SmirFunction {
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

fn lower(function: &SmirFunction, case: ArithmeticMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("helper-backed VEX arithmetic lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX arithmetic"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ArithmeticMemoryCase> {
    let mut cases = Vec::new();
    for kind in ArithmeticKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in EncodingForm::ALL {
                cases.push(ArithmeticMemoryCase { kind, width, form });
            }
        }
    }
    cases
}

#[test]
fn all_96_c4_c5_wig_width_and_arithmetic_shapes_are_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 16 * 2 * 3);
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
            let expected = case.emitted_arithmetic_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 96 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), true,),
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

#[test]
fn arithmetic_classifier_and_lowerer_fail_closed_for_every_pair_invariant() {
    let case = ArithmeticMemoryCase {
        kind: ArithmeticKind::AddWrapI8,
        width: VecWidth::V128,
        form: EncodingForm::C5,
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

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x6F,
        width: VecWidth::V128,
        w: false,
    });

    let mut load_width_mismatch = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width_mismatch.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }

    let mut invalid_lanes = base.clone();
    if let OpKind::VAdd { lanes, .. } = &mut invalid_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }

    let mut wrong_map = base.clone();
    wrong_map.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_prefix = base.clone();
    wrong_prefix.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0xFC,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_opcode = base.clone();
    wrong_opcode.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFD,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_hint_width = base.clone();
    wrong_hint_width.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width: VecWidth::V256,
        w: false,
    });

    let mut evex_hint = base.clone();
    evex_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut wrong_source = base.clone();
    if let OpKind::VAdd { src2, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src2 = vector(2, VecWidth::V128);
    }

    let mut high_destination = base.clone();
    if let OpKind::VAdd { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V128);
    }

    let mut semantic_mismatch = lift_case(ArithmeticMemoryCase {
        kind: ArithmeticKind::AddSignedSatI8,
        width: VecWidth::V128,
        form: EncodingForm::C5,
    });
    if let OpKind::VAddSubSat { signed, .. } = &mut semantic_mismatch.blocks[0].ops[1].kind {
        *signed = false;
    }

    let malformed = [
        ("temporary used twice", extra_use),
        ("load carries an encoding hint", load_hint),
        ("load/consumer width mismatch", load_width_mismatch),
        ("nonintegral lane geometry", invalid_lanes),
        ("wrong VEX map", wrong_map),
        ("missing mandatory 66", wrong_prefix),
        ("opcode/element mismatch", wrong_opcode),
        ("hint/operation width mismatch", wrong_hint_width),
        ("EVEX consumer", evex_hint),
        ("different guest PCs", wrong_pc),
        ("virtual address component", virtual_address),
        ("consumer bypasses temporary", wrong_source),
        ("high EVEX-only destination", high_destination),
        ("saturation signedness mismatch", semantic_mismatch),
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

fn signed_lane(value: i64, bits: u32) -> u64 {
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    (value as u64) & mask
}

fn boundary_operands(kind: ArithmeticKind, lane: usize) -> (u64, u64) {
    let bits = kind.elem().bytes() * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let signed_min = if bits == 64 {
        i64::MIN
    } else {
        -(1i64 << (bits - 1))
    };
    let signed_max = if bits == 64 {
        i64::MAX
    } else {
        (1i64 << (bits - 1)) - 1
    };
    match (kind.saturation(), kind.subtract(), lane % 4) {
        (None, false, 0) => (mask, 1),
        (None, false, 1) => (mask - 1, 3),
        (None, false, 2) => (0, mask),
        (None, false, _) => (0x55 & mask, 0xAA & mask),
        (None, true, 0) => (0, 1),
        (None, true, 1) => (1, mask),
        (None, true, 2) => (mask, mask),
        (None, true, _) => (0x55 & mask, 0xAA & mask),
        (Some(false), false, 0) => (mask, 1),
        (Some(false), false, 1) => (mask - 1, 1),
        (Some(false), false, 2) => (0, 0),
        (Some(false), false, _) => (0x55 & mask, 0x22 & mask),
        (Some(false), true, 0) => (0, 1),
        (Some(false), true, 1) => (1, 1),
        (Some(false), true, 2) => (mask, 1),
        (Some(false), true, _) => (0x55 & mask, 0x22 & mask),
        (Some(true), false, 0) => (signed_lane(signed_max, bits), 1),
        (Some(true), false, 1) => (signed_lane(signed_min, bits), signed_lane(-1, bits)),
        (Some(true), false, 2) => (signed_lane(signed_max - 1, bits), 1),
        (Some(true), false, _) => (signed_lane(-5, bits), signed_lane(3, bits)),
        (Some(true), true, 0) => (signed_lane(signed_max, bits), signed_lane(-1, bits)),
        (Some(true), true, 1) => (signed_lane(signed_min, bits), 1),
        (Some(true), true, 2) => (signed_lane(signed_min + 1, bits), 1),
        (Some(true), true, _) => (signed_lane(-5, bits), signed_lane(-3, bits)),
        _ => unreachable!(),
    }
}

fn operand_vectors(case: ArithmeticMemoryCase) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    for lane in 0..lanes {
        let (left, right) = boundary_operands(case.kind, lane);
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
fn full_guest_regs(case: ArithmeticMemoryCase, ordinal: usize) -> GuestRegs {
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
    case: ArithmeticMemoryCase,
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
    case: ArithmeticMemoryCase,
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
fn native_memory_arithmetic_matches_independent_model_and_interpreter_and_faults_precisely() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory-arithmetic differential: host lacks AVX");
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

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
