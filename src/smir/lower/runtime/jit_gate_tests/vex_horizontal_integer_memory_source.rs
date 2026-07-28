//! Exact helper-backed VEX packed-horizontal integer memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
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
    x86_jit_vex_binary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xBBC0;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HorizontalKind {
    AddWord,
    AddDoubleword,
    AddSaturatingWord,
    SubtractWord,
    SubtractDoubleword,
    SubtractSaturatingWord,
}

impl HorizontalKind {
    const ALL: [Self; 6] = [
        Self::AddWord,
        Self::AddDoubleword,
        Self::AddSaturatingWord,
        Self::SubtractWord,
        Self::SubtractDoubleword,
        Self::SubtractSaturatingWord,
    ];

    const fn elem(self) -> VecElementType {
        match self {
            Self::AddDoubleword | Self::SubtractDoubleword => VecElementType::I32,
            _ => VecElementType::I16,
        }
    }

    const fn subtract(self) -> bool {
        matches!(
            self,
            Self::SubtractWord | Self::SubtractDoubleword | Self::SubtractSaturatingWord
        )
    }

    const fn saturating(self) -> bool {
        matches!(self, Self::AddSaturatingWord | Self::SubtractSaturatingWord)
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::AddWord => 0x01,
            Self::AddDoubleword => 0x02,
            Self::AddSaturatingWord => 0x03,
            Self::SubtractWord => 0x05,
            Self::SubtractDoubleword => 0x06,
            Self::SubtractSaturatingWord => 0x07,
        }
    }

    fn apply(self, first: u64, second: u64) -> u64 {
        let bits = self.elem().bytes() * 8;
        let mask = (1u64 << bits) - 1;
        if self.saturating() {
            let sign_extend = |value: u64| ((value << (64 - bits)) as i64) >> (64 - bits);
            let first = sign_extend(first & mask);
            let second = sign_extend(second & mask);
            let value = if self.subtract() {
                first - second
            } else {
                first + second
            };
            let minimum = -(1i64 << (bits - 1));
            let maximum = (1i64 << (bits - 1)) - 1;
            value.clamp(minimum, maximum) as u64 & mask
        } else if self.subtract() {
            first.wrapping_sub(second) & mask
        } else {
            first.wrapping_add(second) & mask
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HorizontalMemoryCase {
    kind: HorizontalKind,
    width: VecWidth,
    w: bool,
    alias: bool,
}

impl HorizontalMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match (self.w, self.alias) {
            (false, false) => (0, 1, 3),
            (false, true) => (0, 0, 3),
            (true, false) => (14, 9, 11),
            (true, true) => (15, 15, 11),
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
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 2,
            (u8::from(self.w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.kind.opcode(),
            0x40 | ((destination & 7) << 3) | (base & 7),
            DISP as u8,
        ]
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 2,
            // VPHADD*/VPHSUB* are WIG; the lowerer emits canonical W=0.
            (((!source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.kind.opcode(),
            0xC0 | ((destination & 7) << 3) | scratch,
        ]
    }
}

fn all_cases() -> Vec<HorizontalMemoryCase> {
    let mut cases = Vec::new();
    for kind in HorizontalKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for w in [false, true] {
                for alias in [false, true] {
                    cases.push(HorizontalMemoryCase {
                        kind,
                        width,
                        w,
                        alias,
                    });
                }
            }
        }
    }
    cases
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX horizontal integer operations have only 128-/256-bit forms"),
    })
}

fn expected_address(case: HorizontalMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_pair(ops: &[SmirOp], case: HorizontalMemoryCase) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected exact VLoad + VHorizontalBin pair, got {ops:?}")
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
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: case.kind.opcode(),
            width: case.width,
            w: case.w,
        }),
        "{case:?}"
    );
    let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        subtract,
        saturating,
    } = &consumer.kind
    else {
        panic!("{case:?}: expected VHorizontalBin consumer, got {consumer:?}")
    };
    assert_eq!(*dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(*src1, vector(case.source1(), case.width), "{case:?}");
    assert_eq!(*src2, temporary, "{case:?}");
    assert_eq!(*elem, case.kind.elem(), "{case:?}");
    assert_eq!(*lanes, case.width.lanes(case.kind.elem()) as u8, "{case:?}");
    assert_eq!(
        *block_lanes,
        (16 / case.kind.elem().bytes()) as u8,
        "{case:?}"
    );
    assert_eq!(*subtract, case.kind.subtract(), "{case:?}");
    assert_eq!(*saturating, case.kind.saturating(), "{case:?}");
}

fn lift_case(case: HorizontalMemoryCase) -> SmirFunction {
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

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<crate::smir::lower::runtime::X86JitVexBinaryMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_binary_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: HorizontalMemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
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
    assert!(!requirements.needs_fma);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX horizontal lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX horizontal integer operation"),
        result.entry_offset,
    )
}

#[test]
fn all_kind_width_wig_and_alias_shapes_are_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 6 * 2 * 2 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function.blocks[0].ops, case);

            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(actual.consumed, 2, "{level:?} {case:?}");
            assert_eq!(actual.memory_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(actual.destination, case.destination(), "{level:?} {case:?}");
            assert_eq!(actual.source1, case.source1(), "{level:?} {case:?}");
            assert_eq!(actual.width, case.width, "{level:?} {case:?}");
            assert_eq!(actual.map, X86VecMap::Map0F38, "{level:?} {case:?}");
            assert_eq!(actual.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(actual.opcode, case.kind.opcode(), "{level:?} {case:?}");
            assert!(!actual.w, "{level:?} {case:?}: WIG replay must use W=0");
            assert_eq!(
                actual.needs_avx2,
                case.width == VecWidth::V256,
                "{level:?} {case:?}"
            );
            assert!(!actual.needs_fma, "{level:?} {case:?}");
            assert!(
                sequence(&function, false).is_none(),
                "{level:?} {case:?}: memory-disabled gate admitted sequence"
            );

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
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 48 * LEVELS.len());
}

#[test]
fn register_rewrites_match_independent_llvm_23_encodings() {
    for (case, expected) in [
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::AddWord,
                width: VecWidth::V128,
                w: true,
                alias: false,
            },
            &[0xC4, 0x62, 0x31, 0x01, 0xF0][..],
        ),
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::AddDoubleword,
                width: VecWidth::V256,
                w: true,
                alias: false,
            },
            &[0xC4, 0x62, 0x35, 0x02, 0xF0][..],
        ),
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::AddSaturatingWord,
                width: VecWidth::V256,
                w: true,
                alias: true,
            },
            &[0xC4, 0x62, 0x05, 0x03, 0xF8][..],
        ),
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::SubtractWord,
                width: VecWidth::V128,
                w: false,
                alias: false,
            },
            &[0xC4, 0xE2, 0x71, 0x05, 0xC2][..],
        ),
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::SubtractDoubleword,
                width: VecWidth::V256,
                w: true,
                alias: false,
            },
            &[0xC4, 0x62, 0x35, 0x06, 0xF0][..],
        ),
        (
            HorizontalMemoryCase {
                kind: HorizontalKind::SubtractSaturatingWord,
                width: VecWidth::V256,
                w: true,
                alias: true,
            },
            &[0xC4, 0x62, 0x05, 0x07, 0xF8][..],
        ),
    ] {
        assert_eq!(case.emitted_bytes(), expected, "{case:?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed pair"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
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
fn classifier_and_lowerer_fail_closed_for_every_pair_invariant() {
    let case = HorizontalMemoryCase {
        kind: HorizontalKind::AddSaturatingWord,
        width: VecWidth::V128,
        w: false,
        alias: false,
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

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

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
    if let OpKind::VHorizontalBin { src2, .. } = &mut wrong_source2.blocks[0].ops[1].kind {
        *src2 = vector(2, VecWidth::V128);
    }

    let mut wrong_element = base.clone();
    if let OpKind::VHorizontalBin { elem, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I32;
    }

    let mut wrong_lanes = base.clone();
    if let OpKind::VHorizontalBin { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }

    let mut wrong_block_lanes = base.clone();
    if let OpKind::VHorizontalBin { block_lanes, .. } = &mut wrong_block_lanes.blocks[0].ops[1].kind
    {
        *block_lanes /= 2;
    }

    let mut wrong_direction = base.clone();
    if let OpKind::VHorizontalBin { subtract, .. } = &mut wrong_direction.blocks[0].ops[1].kind {
        *subtract = true;
    }

    let mut wrong_saturation = base.clone();
    if let OpKind::VHorizontalBin { saturating, .. } = &mut wrong_saturation.blocks[0].ops[1].kind {
        *saturating = false;
    }

    let mut high_destination = base.clone();
    if let OpKind::VHorizontalBin { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V128);
    }

    let mut high_source1 = base.clone();
    if let OpKind::VHorizontalBin { src1, .. } = &mut high_source1.blocks[0].ops[1].kind {
        *src1 = vector(16, VecWidth::V128);
    }

    let mut wrong_namespace = base.clone();
    if let OpKind::VHorizontalBin { dst, .. } = &mut wrong_namespace.blocks[0].ops[1].kind {
        *dst = x86(X86Reg::Ymm(case.destination()));
    }

    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[1].x86_hint = None;

    let mut wrong_hint_map = base.clone();
    if let Some(X86OpHint::VexOp { map, .. }) = &mut wrong_hint_map.blocks[0].ops[1].x86_hint {
        *map = X86VecMap::Map0F;
    }

    let mut wrong_hint_prefix = base.clone();
    if let Some(X86OpHint::VexOp { pp, .. }) = &mut wrong_hint_prefix.blocks[0].ops[1].x86_hint {
        *pp = X86SsePrefix::Rep;
    }

    let mut wrong_hint_opcode = base.clone();
    if let Some(X86OpHint::VexOp { opcode, .. }) = &mut wrong_hint_opcode.blocks[0].ops[1].x86_hint
    {
        *opcode = 0x07;
    }

    let mut wrong_hint_width = base.clone();
    if let Some(X86OpHint::VexOp { width, .. }) = &mut wrong_hint_width.blocks[0].ops[1].x86_hint {
        *width = VecWidth::V256;
    }

    let mut wrong_hint_w = base.clone();
    if let Some(X86OpHint::VexOp { w, .. }) = &mut wrong_hint_w.blocks[0].ops[1].x86_hint {
        *w = true;
    }

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();

    let mut encoded_destination = case.bytes();
    encoded_destination[4] = (encoded_destination[4] & !0x38) | 0x30;
    let mut byte_destination_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_destination_mismatch, &encoded_destination);

    let mut encoded_source1 = case.bytes();
    encoded_source1[2] = (encoded_source1[2] & !0x78) | (((!2u8) & 0x0F) << 3);
    let mut byte_source1_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_source1_mismatch, &encoded_source1);

    let mut encoded_kind = case.bytes();
    encoded_kind[3] = 0x07;
    let mut byte_kind_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_kind_mismatch, &encoded_kind);

    let mut encoded_width = case.bytes();
    encoded_width[2] |= 0x04;
    let mut byte_width_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_width_mismatch, &encoded_width);

    let mut encoded_w = case.bytes();
    encoded_w[2] |= 0x80;
    let mut byte_w_mismatch = base.clone();
    replace_instruction_bytes(&mut byte_w_mismatch, &encoded_w);

    let malformed = [
        ("temporary used twice", extra_use),
        ("temporary defined twice", duplicate_definition),
        ("load carries an encoding hint", load_hint),
        ("load/consumer width mismatch", load_width),
        ("virtual address component", invalid_address),
        ("different guest PCs", wrong_pc),
        ("consumer bypasses temporary", wrong_source2),
        ("wrong element type", wrong_element),
        ("nonintegral lane geometry", wrong_lanes),
        ("wrong 128-bit block geometry", wrong_block_lanes),
        ("wrong add/subtract direction", wrong_direction),
        ("wrong saturation mode", wrong_saturation),
        ("high EVEX-only destination", high_destination),
        ("high EVEX-only first source", high_source1),
        ("destination register namespace mismatch", wrong_namespace),
        ("missing consumer hint", missing_hint),
        ("wrong hint map", wrong_hint_map),
        ("wrong hint prefix", wrong_hint_prefix),
        ("wrong hint opcode", wrong_hint_opcode),
        ("wrong hint width", wrong_hint_width),
        ("hint/source-byte W mismatch", wrong_hint_w),
        ("missing instruction-byte provenance", missing_bytes),
        ("encoded destination mismatch", byte_destination_mismatch),
        ("encoded first-source mismatch", byte_source1_mismatch),
        ("encoded kind mismatch", byte_kind_mismatch),
        ("encoded width mismatch", byte_width_mismatch),
        ("encoded W mismatch", byte_w_mismatch),
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

fn read_lane(bytes: &[u8], lane: usize, size: usize) -> u64 {
    bytes[lane * size..lane * size + size]
        .iter()
        .enumerate()
        .fold(0u64, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        })
}

fn write_lane(bytes: &mut [u8], lane: usize, size: usize, value: u64) {
    let encoded = value.to_le_bytes();
    bytes[lane * size..lane * size + size].copy_from_slice(&encoded[..size]);
}

fn operand_vectors(case: HorizontalMemoryCase) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    let word_first = [
        0x7FFF, 0x0001, 0x8000, 0x0001, 0x4000, 0x4000, 0xC000, 0xC001, 0x7FFE, 0x0002, 0x8001,
        0xFFFF, 0x1234, 0x4321, 0xEDCC, 0xBCDF,
    ];
    let word_second = [
        0x7FFF, 0x7FFF, 0x8000, 0x0001, 0x1111, 0xEEEE, 0x5555, 0xAAAA, 0x0000, 0xFFFF, 0x4000,
        0xC000, 0x1357, 0x2468, 0xFEDC, 0x0123,
    ];
    let dword_first = [
        0x7FFF_FFFF,
        0x0000_0001,
        0x8000_0000,
        0x0000_0001,
        0x4000_0000,
        0x4000_0000,
        0xC000_0000,
        0xC000_0001,
    ];
    let dword_second = [
        0x7FFF_FFFF,
        0x7FFF_FFFF,
        0x8000_0000,
        0x0000_0001,
        0x1234_5678,
        0xEDCB_A988,
        0x5555_AAAA,
        0xAAAA_5555,
    ];
    for lane in 0..lanes {
        let (first, second) = if case.kind.elem() == VecElementType::I16 {
            (word_first[lane], word_second[lane])
        } else {
            (dword_first[lane], dword_second[lane])
        };
        write_lane(&mut source1, lane, lane_size, first);
        write_lane(&mut source2, lane, lane_size, second);
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
fn full_guest_regs(case: HorizontalMemoryCase, ordinal: usize) -> GuestRegs {
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
    case: HorizontalMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1_bytes = words_to_bytes(registers.zmm[usize::from(case.source1())]);
    let source2_bytes = words_to_bytes(source2);
    let mut result = [0; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    let block_lanes = 16 / lane_size;
    let half = block_lanes / 2;
    for block_base in (0..lanes).step_by(block_lanes) {
        for pair in 0..half {
            let first_lane = block_base + pair * 2;
            let second_lane = first_lane + 1;
            write_lane(
                &mut result,
                block_base + pair,
                lane_size,
                case.kind.apply(
                    read_lane(&source1_bytes, first_lane, lane_size),
                    read_lane(&source1_bytes, second_lane, lane_size),
                ),
            );
            write_lane(
                &mut result,
                block_base + half + pair,
                lane_size,
                case.kind.apply(
                    read_lane(&source2_bytes, first_lane, lane_size),
                    read_lane(&source2_bytes, second_lane, lane_size),
                ),
            );
        }
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
    case: HorizontalMemoryCase,
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
fn native_horizontal_family_matches_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX horizontal memory differential: host lacks AVX");
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
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX horizontal memory cases"
    );
}
