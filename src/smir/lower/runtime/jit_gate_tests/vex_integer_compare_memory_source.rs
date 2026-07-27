//! Exact helper-backed VEX fixed-predicate integer-compare memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecCmpCond, VecElementType,
    VecWidth, VirtualId, X86Reg,
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

const PC: u64 = 0xBA40;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const SCENARIOS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareKind {
    EqI8,
    EqI16,
    EqI32,
    EqI64,
    GtI8,
    GtI16,
    GtI32,
    GtI64,
}

impl CompareKind {
    const ALL: [Self; 8] = [
        Self::EqI8,
        Self::EqI16,
        Self::EqI32,
        Self::EqI64,
        Self::GtI8,
        Self::GtI16,
        Self::GtI32,
        Self::GtI64,
    ];

    const fn elem(self) -> VecElementType {
        match self {
            Self::EqI8 | Self::GtI8 => VecElementType::I8,
            Self::EqI16 | Self::GtI16 => VecElementType::I16,
            Self::EqI32 | Self::GtI32 => VecElementType::I32,
            Self::EqI64 | Self::GtI64 => VecElementType::I64,
        }
    }

    const fn cond(self) -> VecCmpCond {
        match self {
            Self::EqI8 | Self::EqI16 | Self::EqI32 | Self::EqI64 => VecCmpCond::Eq,
            Self::GtI8 | Self::GtI16 | Self::GtI32 | Self::GtI64 => VecCmpCond::Gt,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::GtI8 => 0x64,
            Self::GtI16 => 0x65,
            Self::GtI32 => 0x66,
            Self::EqI8 => 0x74,
            Self::EqI16 => 0x75,
            Self::EqI32 => 0x76,
            Self::EqI64 => 0x29,
            Self::GtI64 => 0x37,
        }
    }

    const fn map(self) -> X86VecMap {
        match self.elem() {
            VecElementType::I64 => X86VecMap::Map0F38,
            _ => X86VecMap::Map0F,
        }
    }

    const fn map_bits(self) -> u8 {
        match self.map() {
            X86VecMap::Map0F => 1,
            X86VecMap::Map0F38 => 2,
            _ => unreachable!(),
        }
    }

    fn apply(self, left: u64, right: u64) -> u64 {
        let bits = self.elem().bytes() * 8;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let predicate = match self.cond() {
            VecCmpCond::Eq => (left & mask) == (right & mask),
            VecCmpCond::Gt => {
                let shift = 64 - bits;
                ((left & mask) << shift) as i64 > ((right & mask) << shift) as i64
            }
            _ => unreachable!(),
        };
        if predicate { mask } else { 0 }
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
    const C4_ONLY: [Self; 2] = [Self::C4W0, Self::C4W1];

    const fn w(self) -> bool {
        matches!(self, Self::C4W1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompareMemoryCase {
    kind: CompareKind,
    width: VecWidth,
    form: EncodingForm,
}

impl CompareMemoryCase {
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
            EncodingForm::C5 => {
                assert_eq!(self.kind.map(), X86VecMap::Map0F);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 })
                        | (((!source1) & 0x0F) << 3)
                        | (l << 2)
                        | 1,
                    self.kind.opcode(),
                    modrm,
                    DISP as u8,
                ]
            }
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | self.kind.map_bits(),
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode(),
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_compare_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0xC0 | ((destination & 7) << 3) | scratch;
        if self.kind.map() == X86VecMap::Map0F && !self.form.w() {
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
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | self.kind.map_bits(),
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
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
        _ => unreachable!("VEX fixed compares have only 128-/256-bit forms"),
    })
}

fn expected_address(case: CompareMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_pair(ops: &[SmirOp], case: CompareMemoryCase) {
    assert_pair_with_address(ops, case, &expected_address(case));
}

fn assert_pair_with_address(ops: &[SmirOp], case: CompareMemoryCase, expected: &Address) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected exact VLoad + VCmp pair, got {ops:?}")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let temporary = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, expected, "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *temporary
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };
    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(
        consumer.x86_hint,
        Some(X86OpHint::VexOp {
            map: case.kind.map(),
            pp: X86SsePrefix::OpSize,
            opcode: case.kind.opcode(),
            width: case.width,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert!(
        matches!(
            &consumer.kind,
            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } if (*dst, *src1, *src2, *cond, *elem, *lanes)
                == (
                    vector(case.destination(), case.width),
                    vector(case.source1(), case.width),
                    temporary,
                    case.kind.cond(),
                    case.kind.elem(),
                    case.width.lanes(case.kind.elem()) as u8,
                )
        ),
        "{case:?}: unexpected compare consumer {consumer:?}"
    );
}

fn lift_case(case: CompareMemoryCase) -> SmirFunction {
    lift_bytes(case, &case.bytes())
}

fn lift_bytes(case: CompareMemoryCase, bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction, case: CompareMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("helper-backed VEX compare lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX compare"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<CompareMemoryCase> {
    let mut cases = Vec::new();
    for kind in CompareKind::ALL {
        let forms = if kind.elem() == VecElementType::I64 {
            &EncodingForm::C4_ONLY[..]
        } else {
            &EncodingForm::ALL[..]
        };
        for width in [VecWidth::V128, VecWidth::V256] {
            for &form in forms {
                cases.push(CompareMemoryCase { kind, width, form });
            }
        }
    }
    cases
}

#[test]
fn all_44_c4_c5_wig_width_and_fixed_compare_shapes_are_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 6 * 2 * 3 + 2 * 2 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function.blocks[0].ops, case);
            let (code, _) = lower(&function, case);
            let expected = case.emitted_compare_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 44 * LEVELS.len());
}

#[test]
fn complete_modrm_sib_displacement_addr32_and_segment_shapes_are_admitted_and_lowered() {
    let case = CompareMemoryCase {
        kind: CompareKind::EqI8,
        width: VecWidth::V128,
        form: EncodingForm::C4W0,
    };
    let address_cases = [
        (
            "base",
            vec![0xC4, 0x61, 0x79, 0x74, 0x3B],
            Address::Direct(x86(X86Reg::Rbx)),
        ),
        (
            "base+disp8",
            case.bytes(),
            Address::BaseOffset {
                base: x86(X86Reg::R11),
                offset: DISP,
                disp_size: DispSize::Disp8,
            },
        ),
        (
            "base+disp32",
            vec![0xC4, 0x41, 0x79, 0x74, 0xBB, 0x78, 0x56, 0x34, 0x12],
            Address::BaseOffset {
                base: x86(X86Reg::R11),
                offset: 0x1234_5678,
                disp_size: DispSize::Disp32,
            },
        ),
        (
            "extended SIB base+index*4+disp8",
            vec![0xC4, 0x01, 0x79, 0x74, 0x7C, 0x93, 0x20],
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R11)),
                index: x86(X86Reg::R10),
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
        ),
        (
            "extended SIB index*8+disp32",
            vec![0xC4, 0x21, 0x79, 0x74, 0x3C, 0xD5, 0x78, 0x56, 0x34, 0x12],
            Address::BaseIndexScale {
                base: None,
                index: x86(X86Reg::R10),
                scale: 8,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            },
        ),
        (
            "RIP+disp32",
            vec![0xC4, 0x61, 0x79, 0x74, 0x3D, 0x20, 0, 0, 0],
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 9),
            },
        ),
        (
            "addr32 base",
            vec![0x67, 0xC4, 0x61, 0x79, 0x74, 0x3B],
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS base",
            vec![0x64, 0xC4, 0x61, 0x79, 0x74, 0x3B],
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 extended SIB",
            vec![0x65, 0x67, 0xC4, 0x01, 0x79, 0x74, 0x7C, 0x93, 0x20],
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::R11)),
                index: Some(x86(X86Reg::R10)),
                scale: 4,
                disp: 0x20,
            })),
        ),
    ];

    let mut lowered = 0usize;
    for (name, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(case, &bytes), level);
            assert_pair_with_address(&function.blocks[0].ops, case, &expected_address);
            let (code, _) = lower(&function, case);
            let expected = case.emitted_compare_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name} {level:?}: missing {expected:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 9 * LEVELS.len());
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

fn replace_metadata(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("malformed test instruction fits metadata"),
    );
}

#[test]
fn compare_classifier_and_lowerer_fail_closed_for_ir_and_byte_provenance_invariants() {
    let case = CompareMemoryCase {
        kind: CompareKind::EqI8,
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
    let mut wrong_source = base.clone();
    if let OpKind::VCmp { src2, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src2 = vector(2, VecWidth::V128);
    }
    let mut wrong_condition = base.clone();
    if let OpKind::VCmp { cond, .. } = &mut wrong_condition.blocks[0].ops[1].kind {
        *cond = VecCmpCond::Gt;
    }
    let mut wrong_element = base.clone();
    if let OpKind::VCmp { elem, lanes, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I16;
        *lanes = 8;
    }
    let mut wrong_lanes = base.clone();
    if let OpKind::VCmp { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }
    let mut wrong_map = base.clone();
    if let Some(X86OpHint::VexOp { map, .. }) = &mut wrong_map.blocks[0].ops[1].x86_hint {
        *map = X86VecMap::Map0F38;
    }
    let mut wrong_prefix = base.clone();
    if let Some(X86OpHint::VexOp { pp, .. }) = &mut wrong_prefix.blocks[0].ops[1].x86_hint {
        *pp = X86SsePrefix::None;
    }
    let mut wrong_opcode = base.clone();
    if let Some(X86OpHint::VexOp { opcode, .. }) = &mut wrong_opcode.blocks[0].ops[1].x86_hint {
        *opcode = 0x75;
    }
    let mut wrong_hint_width = base.clone();
    if let Some(X86OpHint::VexOp { width, .. }) = &mut wrong_hint_width.blocks[0].ops[1].x86_hint {
        *width = VecWidth::V256;
    }
    let mut wrong_hint_w = base.clone();
    if let Some(X86OpHint::VexOp { w, .. }) = &mut wrong_hint_w.blocks[0].ops[1].x86_hint {
        *w = true;
    }
    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();

    let original = case.bytes();
    let mut byte_destination = base.clone();
    let mut bytes = original.clone();
    bytes[4] ^= 0x08;
    replace_metadata(&mut byte_destination, &bytes);
    let mut byte_source1 = base.clone();
    let mut bytes = original.clone();
    bytes[2] ^= 0x08;
    replace_metadata(&mut byte_source1, &bytes);
    let mut byte_map = base.clone();
    let mut bytes = original.clone();
    bytes[1] = (bytes[1] & !0x1F) | 2;
    replace_metadata(&mut byte_map, &bytes);
    let mut byte_prefix = base.clone();
    let mut bytes = original.clone();
    bytes[2] &= !0x03;
    replace_metadata(&mut byte_prefix, &bytes);
    let mut byte_opcode = base.clone();
    let mut bytes = original.clone();
    bytes[3] = 0x75;
    replace_metadata(&mut byte_opcode, &bytes);
    let mut byte_width = base.clone();
    let mut bytes = original.clone();
    bytes[2] ^= 0x04;
    replace_metadata(&mut byte_width, &bytes);
    let mut byte_w = base.clone();
    let mut bytes = original.clone();
    bytes[2] ^= 0x80;
    replace_metadata(&mut byte_w, &bytes);
    let mut byte_register = base.clone();
    let mut bytes = original.clone();
    bytes[4] |= 0xC0;
    bytes.pop();
    replace_metadata(&mut byte_register, &bytes);
    let mut byte_trailing = base.clone();
    let mut bytes = original.clone();
    bytes.push(0);
    replace_metadata(&mut byte_trailing, &bytes);
    let mut byte_truncated = base.clone();
    let mut bytes = original.clone();
    bytes.pop();
    replace_metadata(&mut byte_truncated, &bytes);
    let mut forbidden_prefix = base.clone();
    let mut bytes = vec![0x66];
    bytes.extend(original);
    replace_metadata(&mut forbidden_prefix, &bytes);

    let malformed = [
        ("temporary used twice", extra_use),
        ("consumer bypasses temporary", wrong_source),
        ("condition/opcode mismatch", wrong_condition),
        ("element/opcode mismatch", wrong_element),
        ("nonintegral lane geometry", wrong_lanes),
        ("wrong VEX map hint", wrong_map),
        ("missing mandatory 66 hint", wrong_prefix),
        ("wrong opcode hint", wrong_opcode),
        ("hint/operation width mismatch", wrong_hint_width),
        ("hint/encoded W mismatch", wrong_hint_w),
        ("load carries an encoding hint", load_hint),
        ("different guest PCs", wrong_pc),
        ("virtual address component", virtual_address),
        ("missing source-byte provenance", missing_metadata),
        ("encoded destination mismatch", byte_destination),
        ("encoded source1 mismatch", byte_source1),
        ("encoded map mismatch", byte_map),
        ("encoded mandatory prefix mismatch", byte_prefix),
        ("encoded opcode mismatch", byte_opcode),
        ("encoded vector length mismatch", byte_width),
        ("encoded W mismatch", byte_w),
        ("encoded register source", byte_register),
        ("trailing source byte", byte_trailing),
        ("truncated displacement", byte_truncated),
        ("forbidden legacy prefix", forbidden_prefix),
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

fn lane_operands(case: CompareMemoryCase, lane: usize, scenario: usize) -> (u64, u64) {
    let bits = case.kind.elem().bytes() * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let sign = 1u64 << (bits - 1);
    match (scenario, lane % 4) {
        (0, 0) => (0, 0),
        (0, 1) => (mask, mask),
        (0, 2) => (1, 2),
        (0, _) => (2, 1),
        (1, 0) => (sign, 0),
        (1, 1) => (0, sign),
        (1, 2) => (sign, sign),
        (1, _) => (mask, 0),
        (2, 0) => ((0x55AA_55AA_55AA_55AA & mask) | sign, 0x33 & mask),
        (2, 1) => (0x7F7F_7F7F_7F7F_7F7F & mask, sign | 1),
        (2, 2) => (0x0123_4567_89AB_CDEF & mask, 0x0123_4567_89AB_CDEF & mask),
        (2, _) => (0xAAAA_AAAA_AAAA_AAAA & mask, 0x5555_5555_5555_5555 & mask),
        _ => unreachable!(),
    }
}

fn operand_vectors(case: CompareMemoryCase, scenario: usize) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    let lane_size = case.kind.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / lane_size;
    for lane in 0..lanes {
        let (left, right) = lane_operands(case, lane, scenario);
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
fn full_guest_regs(case: CompareMemoryCase, ordinal: usize, scenario: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
                .wrapping_add((scenario as u64) * 0x100)
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
    let (source1, _) = operand_vectors(case, scenario);
    registers.zmm[usize::from(case.source1())] = source1;
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] = std::array::from_fn(|word| {
            0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7 + ordinal + scenario) as u32)
        });
    }
    registers.gpr[usize::from(case.base())] =
        0x2000 + ((ordinal & 0x0F) as u64) * 0x40 + (scenario as u64) * 0x400;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: CompareMemoryCase,
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
    case: CompareMemoryCase,
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
fn native_memory_compares_match_independent_model_and_interpreter_and_fault_precisely() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory-compare differential: host lacks AVX");
        return;
    }

    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases = all_cases()
        .into_iter()
        .filter(|case| avx2 || case.width == VecWidth::V128)
        .collect::<Vec<_>>();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len() * SCENARIOS;
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            for scenario in 0..SCENARIOS {
                let (_, source2) = operand_vectors(case, scenario);
                let mut context = VectorMemoryContext {
                    value: source2,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = full_guest_regs(case, ordinal, scenario);
                let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as usize as u64;
                let initial = registers;
                let mut expected = expected_success(registers, case, source2);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected,
                    "{level:?} {case:?} scenario {scenario}: success"
                );
                assert_eq!(context.calls, 1, "{level:?} {case:?} scenario {scenario}");
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
                let mut registers = full_guest_regs(case, ordinal ^ 0x55, scenario);
                let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected,
                    "{level:?} {case:?} scenario {scenario}: fault"
                );
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
    }

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
