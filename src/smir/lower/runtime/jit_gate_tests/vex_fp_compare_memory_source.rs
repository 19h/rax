//! Exact helper-backed VEX VCMPPS/VCMPPD memory-source coverage.

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
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexFpCompareMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_fp_compare_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0xC2F0;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

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
struct CompareMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    predicate: u8,
    form: EncodingForm,
}

impl CompareMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Distinct low operands force helper scratch XMM/YMM2.
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

    const fn pp(self) -> u8 {
        if matches!(self.elem, VecElementType::F64) {
            1
        } else {
            0
        }
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
                    | self.pp(),
                0xC2,
                modrm,
                DISP as u8,
                self.predicate,
            ],
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | self.pp(),
                0xC2,
                modrm,
                DISP as u8,
                self.predicate,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
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
                    | self.pp(),
                0xC2,
                modrm,
                self.predicate,
            ]
        } else {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | (((!source1) & 0x0F) << 3) | (l << 2) | self.pp(),
                0xC2,
                modrm,
                self.predicate,
            ]
        }
    }
}

fn all_cases() -> Vec<CompareMemoryCase> {
    let mut cases = Vec::new();
    for elem in [VecElementType::F32, VecElementType::F64] {
        for width in [VecWidth::V128, VecWidth::V256] {
            for predicate in 0..32 {
                for form in EncodingForm::ALL {
                    cases.push(CompareMemoryCase {
                        elem,
                        width,
                        predicate,
                        form,
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
        _ => unreachable!("VEX packed compare has only 128-/256-bit forms"),
    })
}

fn expected_address(case: CompareMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexFpCompareMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_fp_compare_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_pair(function: &SmirFunction, case: CompareMemoryCase) {
    let [load, consumer] = function.blocks[0].ops.as_slice() else {
        panic!("{case:?}: expected exact VLoad + X86VectorFpCompare pair")
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
    assert_eq!(
        consumer.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: if case.elem == VecElementType::F32 {
                X86SsePrefix::None
            } else {
                X86SsePrefix::OpSize
            },
            opcode: 0xC2,
            width: case.width,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    let OpKind::X86VectorFpCompare {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        predicate,
        scalar,
        mask_destination,
        zero_upper,
        suppress_exceptions,
    } = consumer.kind
    else {
        panic!("{case:?}: expected X86VectorFpCompare consumer")
    };
    assert_eq!(dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(src1, vector(case.source1(), case.width), "{case:?}");
    assert_eq!(src2, temporary, "{case:?}");
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(elem, case.elem, "{case:?}");
    assert_eq!(width, case.width, "{case:?}");
    assert_eq!(lanes, case.width.lanes(case.elem) as u8, "{case:?}");
    assert_eq!(predicate, case.predicate, "{case:?}");
    assert!(!scalar, "{case:?}");
    assert!(!mask_destination, "{case:?}");
    assert!(zero_upper, "{case:?}");
    assert!(!suppress_exceptions, "{case:?}");
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexFpCompareMemorySequence {
            consumed: 2,
            memory_size: case.width.bytes(),
            destination: case.destination(),
            source1: case.source1(),
            elem: case.elem,
            width: case.width,
            predicate: case.predicate,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
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

fn lift_case(case: CompareMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_pair(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexFpCompareMemorySequence) {
    let sequence =
        classified_sequence(function, true).expect("classified VCMPPS/VCMPPD memory pair");
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
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("helper-backed VEX packed FP compare lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX packed FP compare"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_1_152_c4_c5_wig_format_width_predicate_and_optimization_cells_admit_and_lower() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 32 * 3);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function, case);
            let (code, _, _) = lower(&function);
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
    assert_eq!(lowered, 1_152);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    let c5 = CompareMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        predicate: 31,
        form: EncodingForm::C5,
    };
    assert_eq!(c5.bytes(), [0xC5, 0xF0, 0xC2, 0x43, 0x20, 0x1F]);
    assert_eq!(c5.emitted_bytes(), [0xC5, 0xF0, 0xC2, 0xC2, 0x1F]);

    let c4 = CompareMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V256,
        predicate: 31,
        form: EncodingForm::C4W0,
    };
    assert_eq!(c4.bytes(), [0xC4, 0x41, 0x7D, 0xC2, 0x7B, 0x20, 0x1F]);
    assert_eq!(c4.emitted_bytes(), [0xC5, 0x7D, 0xC2, 0xF9, 0x1F]);
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vcmpps xmm1,xmm2,[rip+0x44332211],7
        &[0xC5, 0xE8, 0xC2, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x07],
        // vcmppd ymm0,ymm1,fs:[rcx*4+0x44332211],15
        &[
            0x64, 0xC5, 0xF5, 0xC2, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x0F,
        ],
        // vcmpps ymm14,ymm10,fs:addr32 [esi*2+0x44332211],31
        &[
            0x64, 0x67, 0xC4, 0x61, 0x2C, 0xC2, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0x1F,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function);
            assert!(matches!(sequence.width, VecWidth::V128 | VecWidth::V256));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed packed FP compare pair"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed packed FP compare pair"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed packed FP compare pair"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_lowerer_fail_closed_for_every_graph_hint_and_provenance_invariant() {
    let case = CompareMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        predicate: 31,
        form: EncodingForm::C4W0,
    };
    let base = lift_case(case);
    let temporary = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: temporary,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("temporary used twice", extra_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VLoad {
            dst: temporary,
            addr: expected_address(case),
            width: VecWidth::V128,
        },
    ));
    malformed.push(("temporary defined twice", duplicate_definition));

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing unaligned load hint", missing_load_hint));

    let mut aligned_load_hint = base.clone();
    aligned_load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(("aligned load hint", aligned_load_hint));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }
    malformed.push(("load/consumer width mismatch", load_width));

    let mut invalid_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut invalid_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", invalid_address));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("different guest PCs", wrong_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mutate_compare = |function: &mut SmirFunction,
                          mutation: &mut dyn FnMut(
        &mut VReg,
        &mut VReg,
        &mut VReg,
        &mut Option<VReg>,
        &mut VecElementType,
        &mut VecWidth,
        &mut u8,
        &mut u8,
        &mut bool,
        &mut bool,
        &mut bool,
        &mut bool,
    )| {
        let OpKind::X86VectorFpCompare {
            dst,
            src1,
            src2,
            mask,
            elem,
            width,
            lanes,
            predicate,
            scalar,
            mask_destination,
            zero_upper,
            suppress_exceptions,
        } = &mut function.blocks[0].ops[1].kind
        else {
            unreachable!()
        };
        mutation(
            dst,
            src1,
            src2,
            mask,
            elem,
            width,
            lanes,
            predicate,
            scalar,
            mask_destination,
            zero_upper,
            suppress_exceptions,
        );
    };

    macro_rules! malformed_compare {
        ($name:literal, $body:expr) => {{
            let mut function = base.clone();
            mutate_compare(&mut function, &mut $body);
            malformed.push(($name, function));
        }};
    }
    malformed_compare!(
        "consumer bypasses temporary",
        |_, _, src2, _, _, _, _, _, _, _, _, _| {
            *src2 = vector(2, VecWidth::V128);
        }
    );
    malformed_compare!(
        "consumer width",
        |_, _, _, _, _, width, _, _, _, _, _, _| {
            *width = VecWidth::V256;
        }
    );
    malformed_compare!(
        "consumer lane count",
        |_, _, _, _, _, _, lanes, _, _, _, _, _| {
            *lanes = 3;
        }
    );
    malformed_compare!(
        "masked VEX compare",
        |_, _, _, mask, _, _, _, _, _, _, _, _| {
            *mask = Some(x86(X86Reg::K(1)));
        }
    );
    malformed_compare!(
        "consumer element",
        |_, _, _, _, elem, _, _, _, _, _, _, _| {
            *elem = VecElementType::F64;
        }
    );
    malformed_compare!(
        "consumer predicate",
        |_, _, _, _, _, _, _, predicate, _, _, _, _| {
            *predicate = 30;
        }
    );
    malformed_compare!(
        "scalar consumer",
        |_, _, _, _, _, _, _, _, scalar, _, _, _| {
            *scalar = true;
        }
    );
    malformed_compare!(
        "mask destination",
        |_, _, _, _, _, _, _, _, _, mask_destination, _, _| {
            *mask_destination = true;
        }
    );
    malformed_compare!(
        "missing upper clear",
        |_, _, _, _, _, _, _, _, _, _, zero_upper, _| {
            *zero_upper = false;
        }
    );
    malformed_compare!(
        "suppressed exceptions",
        |_, _, _, _, _, _, _, _, _, _, _, suppress| {
            *suppress = true;
        }
    );
    malformed_compare!(
        "high EVEX-only destination",
        |dst, _, _, _, _, _, _, _, _, _, _, _| {
            *dst = x86(X86Reg::Xmm(16));
        }
    );
    malformed_compare!(
        "high EVEX-only first source",
        |_, src1, _, _, _, _, _, _, _, _, _, _| {
            *src1 = x86(X86Reg::Xmm(16));
        }
    );
    malformed_compare!(
        "destination namespace",
        |dst, _, _, _, _, _, _, _, _, _, _, _| {
            *dst = x86(X86Reg::Ymm(case.destination()));
        }
    );

    for (name, hint) in [
        ("missing consumer hint", None),
        (
            "hint map",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint prefix",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint opcode",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0xC3,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint width",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0xC2,
                width: VecWidth::V256,
                w: false,
            }),
        ),
        (
            "hint W",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: true,
            }),
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[1].x86_hint = hint;
        malformed.push((name, function));
    }

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x02),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded width", 2, 0x04),
        ("encoded W", 2, 0x80),
        ("encoded predicate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }
    let mut reserved_predicate = base.clone();
    let mut bytes = case.bytes();
    bytes[6] = 0x20;
    replace_instruction_bytes(&mut reserved_predicate, &bytes);
    malformed.push(("reserved predicate bits", reserved_predicate));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

const F32_PATTERNS: [u64; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0xBF80_0000,
    0x4000_0000,
    0x3F00_0000,
    0x0000_0001,
    0x8000_0001,
    0x0080_0000,
    0x7F7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0x7F81_2345,
    0x3F80_0001,
    0x3EAA_AAAB,
];

const F64_PATTERNS: [u64; 16] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0xBFF0_0000_0000_0000,
    0x4000_0000_0000_0000,
    0x3FE0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x8000_0000_0000_0001,
    0x0010_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_2468_ACE0_1357,
    0x7FF0_2468_ACE0_1357,
    0x3FF0_0000_0000_0001,
    0x3FD5_5555_5555_5555,
];

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

fn patterned_vector(elem: VecElementType, shift: usize) -> [u64; 8] {
    let element_bytes = elem.bytes() as usize;
    let patterns: &[u64] = if elem == VecElementType::F32 {
        &F32_PATTERNS
    } else {
        &F64_PATTERNS
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element_bytes {
        let value = patterns[(lane + shift) % patterns.len()].to_le_bytes();
        let base = lane * element_bytes;
        bytes[base..base + element_bytes].copy_from_slice(&value[..element_bytes]);
    }
    bytes_to_words(bytes)
}

fn get_lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    let base = lane * elem.bytes() as usize;
    match elem {
        VecElementType::F32 => u64::from(u32::from_le_bytes(
            bytes[base..base + 4].try_into().unwrap(),
        )),
        VecElementType::F64 => u64::from_le_bytes(bytes[base..base + 8].try_into().unwrap()),
        _ => unreachable!("packed FP compare element"),
    }
}

fn set_lane(bytes: &mut [u8; 64], lane: usize, elem: VecElementType, value: u64) {
    let base = lane * elem.bytes() as usize;
    let encoded = value.to_le_bytes();
    bytes[base..base + elem.bytes() as usize].copy_from_slice(&encoded[..elem.bytes() as usize]);
}

fn is_denormal(bits: u64, elem: VecElementType) -> bool {
    match elem {
        VecElementType::F32 => bits & 0x7F80_0000 == 0 && bits & 0x007F_FFFF != 0,
        VecElementType::F64 => {
            bits & 0x7FF0_0000_0000_0000 == 0 && bits & 0x000F_FFFF_FFFF_FFFF != 0
        }
        _ => unreachable!("packed FP compare element"),
    }
}

fn is_nan(bits: u64, elem: VecElementType) -> bool {
    match elem {
        VecElementType::F32 => bits & 0x7F80_0000 == 0x7F80_0000 && bits & 0x007F_FFFF != 0,
        VecElementType::F64 => {
            bits & 0x7FF0_0000_0000_0000 == 0x7FF0_0000_0000_0000
                && bits & 0x000F_FFFF_FFFF_FFFF != 0
        }
        _ => unreachable!("packed FP compare element"),
    }
}

fn is_snan(bits: u64, elem: VecElementType) -> bool {
    is_nan(bits, elem)
        && match elem {
            VecElementType::F32 => bits & 0x0040_0000 == 0,
            VecElementType::F64 => bits & 0x0008_0000_0000_0000 == 0,
            _ => unreachable!("packed FP compare element"),
        }
}

fn apply_daz(bits: u64, elem: VecElementType, mxcsr: u32) -> (u64, u32) {
    if !is_denormal(bits, elem) {
        return (bits, 0);
    }
    if mxcsr & (1 << 6) == 0 {
        return (bits, 1 << 1);
    }
    let sign = match elem {
        VecElementType::F32 => bits & 0x8000_0000,
        VecElementType::F64 => bits & 0x8000_0000_0000_0000,
        _ => unreachable!("packed FP compare element"),
    };
    (sign, 0)
}

fn relation(first: u64, second: u64, elem: VecElementType) -> usize {
    let ordering = match elem {
        VecElementType::F32 => {
            f32::from_bits(first as u32).partial_cmp(&f32::from_bits(second as u32))
        }
        VecElementType::F64 => f64::from_bits(first).partial_cmp(&f64::from_bits(second)),
        _ => unreachable!("packed FP compare element"),
    };
    match ordering {
        Some(std::cmp::Ordering::Greater) => 0,
        Some(std::cmp::Ordering::Less) => 1,
        Some(std::cmp::Ordering::Equal) => 2,
        None => 3,
    }
}

fn independent_compare(
    case: CompareMemoryCase,
    source1: [u64; 8],
    source2: [u64; 8],
    old_destination: [u64; 8],
    mxcsr: u32,
) -> ([u64; 8], u32) {
    const TRUTH_TABLES: [u8; 16] = [
        0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100, 0b1010, 0b1110,
        0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
    ];
    let signaling = matches!(
        case.predicate,
        1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
    );
    let source1 = words_to_bytes(source1);
    let source2 = words_to_bytes(source2);
    let mut result = words_to_bytes(old_destination);
    result[case.width.bytes() as usize..].fill(0);
    let mut status = 0u32;
    for lane in 0..case.width.lanes(case.elem) as usize {
        let (first, first_status) =
            apply_daz(get_lane(&source1, lane, case.elem), case.elem, mxcsr);
        let (second, second_status) =
            apply_daz(get_lane(&source2, lane, case.elem), case.elem, mxcsr);
        let first_nan = is_nan(first, case.elem);
        let second_nan = is_nan(second, case.elem);
        let invalid = is_snan(first, case.elem)
            || is_snan(second, case.elem)
            || (signaling && (first_nan || second_nan));
        let lane_relation = if first_nan || second_nan {
            status |= u32::from(invalid);
            3
        } else {
            status |= first_status | second_status;
            relation(first, second, case.elem)
        };
        let true_lane =
            TRUTH_TABLES[usize::from(case.predicate & 0x0F)] & (1 << lane_relation) != 0;
        set_lane(
            &mut result,
            lane,
            case.elem,
            if true_lane {
                if case.elem == VecElementType::F32 {
                    u64::from(u32::MAX)
                } else {
                    u64::MAX
                }
            } else {
                0
            },
        );
    }
    (bytes_to_words(result), mxcsr | status)
}

#[test]
fn independent_oracle_covers_all_predicates_relations_nan_classes_daz_and_status() {
    let source1 = bytes_to_words({
        let mut bytes = [0u8; 64];
        for (lane, value) in [2.0f32, -1.0, 1.0, f32::from_bits(0x7FC1_2345)]
            .into_iter()
            .enumerate()
        {
            bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    });
    let source2 = bytes_to_words({
        let mut bytes = [0u8; 64];
        for (lane, value) in [1.0f32, 1.0, 1.0, 0.0].into_iter().enumerate() {
            bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    });
    let mut distinct_masks = std::collections::HashSet::new();
    for predicate in 0..32 {
        let case = CompareMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            predicate,
            form: EncodingForm::C5,
        };
        let (result, mxcsr) = independent_compare(case, source1, source2, [u64::MAX; 8], 0x1F80);
        distinct_masks.insert([result[0], result[1]]);
        let signaling = matches!(
            predicate,
            1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
        );
        assert_eq!(mxcsr & 1 != 0, signaling, "predicate {predicate}");
    }
    assert_eq!(distinct_masks.len(), 16);

    let denormal = patterned_vector(VecElementType::F64, 6);
    let normal = patterned_vector(VecElementType::F64, 2);
    let case = CompareMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V256,
        predicate: 0,
        form: EncodingForm::C4W0,
    };
    let (_, without_daz) = independent_compare(case, denormal, normal, [0; 8], 0x1F80);
    let (_, with_daz) = independent_compare(case, denormal, normal, [0; 8], 0x1F80 | (1 << 6));
    assert_ne!(without_daz & (1 << 1), 0);
    assert_eq!(with_daz & (1 << 1), 0);
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
fn full_guest_regs(case: CompareMemoryCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80
            | ((ordinal as u32).rotate_left(3) & 0x3F)
            | (((ordinal as u32 >> 2) & 3) << 13)
            | (u32::from(ordinal & 1 != 0) << 6)
            | (u32::from(ordinal & 2 != 0) << 15),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(case.elem, index * 5 + ordinal);
    }
    registers.zmm[usize::from(case.source1())] =
        patterned_vector(case.elem, ordinal.wrapping_mul(3));
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] =
            std::array::from_fn(|word| 0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7) as u32));
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: CompareMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1())];
    let old_destination = registers.zmm[usize::from(case.destination())];
    let (destination, mxcsr) =
        independent_compare(case, source1, source2, old_destination, registers.mxcsr);
    registers.zmm[usize::from(case.destination())] = destination;
    registers.mxcsr = mxcsr;
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
fn native_packed_fp_compare_matches_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed FP compare memory differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut newly_observed_status = 0u32;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source2 = patterned_vector(case.elem, ordinal.wrapping_mul(7).wrapping_add(3));

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
            newly_observed_status |= expected.mxcsr & !initial.mxcsr;

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

    assert_eq!(
        newly_observed_status & 0x03,
        0x03,
        "native differential did not newly exercise both MXCSR.IE and MXCSR.DE"
    );
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX packed FP compare memory cases"
    );
}
