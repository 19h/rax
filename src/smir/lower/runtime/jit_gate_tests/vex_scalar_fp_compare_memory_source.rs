//! Exact helper-backed VEX VCMPSS/VCMPSD memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
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

const PC: u64 = 0xC2_50;
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

    #[cfg(target_arch = "x86_64")]
    const fn ordinal(self) -> usize {
        match self {
            Self::C5 => 0,
            Self::C4W0 => 1,
            Self::C4W1 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarCompareMemoryCase {
    elem: VecElementType,
    predicate: u8,
    form: EncodingForm,
}

impl ScalarCompareMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Distinct low operands force helper scratch XMM2.
            EncodingForm::C5 => (0, 1, 3),
            // High destination plus source1 0 forces helper scratch XMM1.
            EncodingForm::C4W0 => (15, 0, 11),
            // Aliased high destination/source1 force helper scratch XMM0.
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
        if matches!(self.elem, VecElementType::F32) {
            2
        } else {
            3
        }
    }

    const fn memory_size(self) -> u32 {
        if matches!(self.elem, VecElementType::F32) {
            4
        } else {
            8
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source1())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match self.form {
            EncodingForm::C5 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | self.pp(),
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
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | self.pp(),
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
        let modrm = 0xC0 | ((destination & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | self.pp(),
                0xC2,
                modrm,
                self.predicate,
            ]
        } else {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | (((!source1) & 0x0F) << 3) | self.pp(),
                0xC2,
                modrm,
                self.predicate,
            ]
        }
    }
}

fn all_cases() -> Vec<ScalarCompareMemoryCase> {
    let mut cases = Vec::new();
    for elem in [VecElementType::F32, VecElementType::F64] {
        for predicate in 0..32 {
            for form in EncodingForm::ALL {
                cases.push(ScalarCompareMemoryCase {
                    elem,
                    predicate,
                    form,
                });
            }
        }
    }
    cases
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn expected_address(case: ScalarCompareMemoryCase) -> Address {
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

fn assert_raw_quad(function: &SmirFunction, case: ScalarCompareMemoryCase) {
    assert_eq!(function.blocks[0].ops.len(), 4, "{case:?}");
    assert_exact_sequence(function, case);
}

fn assert_compare_consumer(consumer: &SmirOp, source_vector: VReg, case: ScalarCompareMemoryCase) {
    assert_eq!(consumer.guest_pc, PC, "{case:?}");
    assert_eq!(
        consumer.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: if case.elem == VecElementType::F32 {
                X86SsePrefix::Rep
            } else {
                X86SsePrefix::Repne
            },
            opcode: 0xC2,
            width: VecWidth::V128,
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
    assert_eq!(dst, xmm(case.destination()), "{case:?}");
    assert_eq!(src1, xmm(case.source1()), "{case:?}");
    assert_eq!(src2, source_vector, "{case:?}");
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(elem, case.elem, "{case:?}");
    assert_eq!(width, VecWidth::V128, "{case:?}");
    assert_eq!(lanes, 1, "{case:?}");
    assert_eq!(predicate, case.predicate, "{case:?}");
    assert!(scalar, "{case:?}");
    assert!(!mask_destination, "{case:?}");
    assert!(zero_upper, "{case:?}");
    assert!(!suppress_exceptions, "{case:?}");
}

fn assert_exact_sequence(function: &SmirFunction, case: ScalarCompareMemoryCase) {
    let ops = function.blocks[0].ops.as_slice();
    let (initialization, load, broadcast, consumer) = match ops {
        [load, broadcast, consumer] => (None, load, broadcast, consumer),
        [initialization, load, broadcast, consumer] => {
            (Some(initialization), load, broadcast, consumer)
        }
        _ => {
            panic!(
                "{case:?}: expected optional Mov + Load + VBroadcast + \
                 X86VectorFpCompare, got {:?}",
                function.blocks[0].ops
            )
        }
    };
    let loaded_scalar = match &load.kind {
        OpKind::Load {
            dst: loaded_scalar @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(
                *width,
                if case.elem == VecElementType::F32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                "{case:?}"
            );
            *loaded_scalar
        }
        other => panic!("{case:?}: expected scalar Load, got {other:?}"),
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    if let Some(initialization) = initialization {
        assert_eq!(initialization.guest_pc, load.guest_pc, "{case:?}");
        assert_eq!(initialization.x86_hint, None, "{case:?}");
        assert!(
            matches!(
                &initialization.kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                } if *dst == loaded_scalar
            ),
            "{case:?}: unexpected scalar initialization {:?}",
            initialization.kind
        );
    }
    let source_vector = match broadcast.kind {
        OpKind::VBroadcast {
            dst: source_vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: 1,
        } => {
            assert_eq!(scalar, loaded_scalar, "{case:?}");
            assert_eq!(elem, case.elem, "{case:?}");
            source_vector
        }
        ref other => panic!("{case:?}: expected one-lane broadcast, got {other:?}"),
    };
    assert_eq!(broadcast.x86_hint, None, "{case:?}");
    assert_compare_consumer(consumer, source_vector, case);
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexFpCompareMemorySequence {
            consumed: ops.len(),
            memory_size: case.memory_size(),
            destination: case.destination(),
            source1: case.source1(),
            elem: case.elem,
            width: VecWidth::V128,
            predicate: case.predicate,
            scalar: true,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_case(case: ScalarCompareMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_raw_quad(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexFpCompareMemorySequence) {
    let sequence =
        classified_sequence(function, true).expect("classified VCMPSS/VCMPSD memory triplet");
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
        panic!("helper-backed VEX scalar FP compare lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX scalar FP compare"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_576_c4_c5_wig_format_predicate_and_optimization_cells_admit_and_lower() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 32 * 3);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let (code, _, _) = lower(&function);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xB9, case.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing scalar byte size"
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
    assert_eq!(lowered, 576);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    let c5 = ScalarCompareMemoryCase {
        elem: VecElementType::F32,
        predicate: 31,
        form: EncodingForm::C5,
    };
    assert_eq!(c5.bytes(), [0xC5, 0xF2, 0xC2, 0x43, 0x20, 0x1F]);
    assert_eq!(c5.emitted_bytes(), [0xC5, 0xF2, 0xC2, 0xC2, 0x1F]);

    let c4 = ScalarCompareMemoryCase {
        elem: VecElementType::F64,
        predicate: 31,
        form: EncodingForm::C4W0,
    };
    assert_eq!(c4.bytes(), [0xC4, 0x41, 0x7B, 0xC2, 0x7B, 0x20, 0x1F]);
    assert_eq!(c4.emitted_bytes(), [0xC5, 0x7B, 0xC2, 0xF9, 0x1F]);
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vcmpss xmm1,xmm2,[rip+0x44332211],7
        &[0xC5, 0xEA, 0xC2, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x07],
        // vcmpsd xmm0,xmm1,fs:[rcx*4+0x44332211],15
        &[
            0x64, 0xC5, 0xF3, 0xC2, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x0F,
        ],
        // vcmpss xmm14,xmm10,fs:addr32 [esi*2+0x44332211],31
        &[
            0x64, 0x67, 0xC5, 0x2A, 0xC2, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0x1F,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function);
            assert!(sequence.scalar);
            assert!(matches!(sequence.memory_size, 4 | 8));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed scalar FP compare triplet"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed scalar FP compare triplet"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed scalar FP compare triplet"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_lowerer_fail_closed_for_graph_hint_and_provenance_invariants() {
    let case = ScalarCompareMemoryCase {
        elem: VecElementType::F32,
        predicate: 31,
        form: EncodingForm::C4W0,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    assert_exact_sequence(&base, case);
    let load_index = base.blocks[0].ops.len() - 3;
    let broadcast_index = load_index + 1;
    let consumer_index = load_index + 2;
    let loaded_scalar = match base.blocks[0].ops[load_index].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let source_vector = match base.blocks[0].ops[broadcast_index].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    assert_eq!(load_index, 1, "O0 must preserve the lifter initialization");
    let mut initialization_value = base.clone();
    if let OpKind::Mov { src, .. } = &mut initialization_value.blocks[0].ops[0].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("initialization value", initialization_value));

    let mut initialization_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut initialization_width.blocks[0].ops[0].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("initialization width", initialization_width));

    let mut initialization_pc = base.clone();
    initialization_pc.blocks[0].ops[0].guest_pc += 1;
    malformed.push(("initialization guest PC", initialization_pc));

    let mut initialization_hint = base.clone();
    initialization_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x10,
        width: VecWidth::V128,
        w: false,
    });
    malformed.push(("initialization hint", initialization_hint));

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF0)),
            src: SrcOperand::Reg(loaded_scalar),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded scalar used twice", extra_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::Mov {
            dst: loaded_scalar,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded scalar defined twice", duplicate_definition));

    let mut source_use = base.clone();
    source_use.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VExtractLane {
            dst: VReg::Virtual(VirtualId(0xFFF1)),
            vec: source_vector,
            lane: 0,
            elem: VecElementType::F32,
            sign: SignExtend::Zero,
        },
    ));
    malformed.push(("broadcast vector used twice", source_use));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[load_index].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x10,
        width: VecWidth::V128,
        w: false,
    });
    malformed.push(("unexpected load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[load_index].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("load width", load_width));

    let mut load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[load_index].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("load sign extension", load_sign));

    let mut invalid_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut invalid_address.blocks[0].ops[load_index].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", invalid_address));

    let mut broadcast_pc = base.clone();
    broadcast_pc.blocks[0].ops[broadcast_index].guest_pc += 1;
    malformed.push(("broadcast guest PC", broadcast_pc));

    let mut broadcast_hint = base.clone();
    broadcast_hint.blocks[0].ops[broadcast_index].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x18,
        width: VecWidth::V128,
        w: false,
    });
    malformed.push(("broadcast hint", broadcast_hint));

    for (name, mutation) in [
        ("broadcast scalar", 0u8),
        ("broadcast element", 1),
        ("broadcast lanes", 2),
    ] {
        let mut function = base.clone();
        let OpKind::VBroadcast {
            scalar,
            elem,
            lanes,
            ..
        } = &mut function.blocks[0].ops[broadcast_index].kind
        else {
            unreachable!()
        };
        match mutation {
            0 => *scalar = VReg::Virtual(VirtualId(0xFF00)),
            1 => *elem = VecElementType::F64,
            2 => *lanes = 2,
            _ => unreachable!(),
        }
        malformed.push((name, function));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[consumer_index].guest_pc += 1;
    malformed.push(("consumer guest PC", wrong_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(3), PC, OpKind::Nop));
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
        } = &mut function.blocks[0].ops[consumer_index].kind
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
        "consumer bypasses broadcast",
        |_, _, src2, _, _, _, _, _, _, _, _, _| {
            *src2 = xmm(2);
        }
    );
    malformed_compare!("destination", |dst, _, _, _, _, _, _, _, _, _, _, _| {
        *dst = xmm(14);
    });
    malformed_compare!("first source", |_, src1, _, _, _, _, _, _, _, _, _, _| {
        *src1 = xmm(1);
    });
    malformed_compare!("mask", |_, _, _, mask, _, _, _, _, _, _, _, _| {
        *mask = Some(x86(X86Reg::K(1)));
    });
    malformed_compare!("element", |_, _, _, _, elem, _, _, _, _, _, _, _| {
        *elem = VecElementType::F64;
    });
    malformed_compare!("width", |_, _, _, _, _, width, _, _, _, _, _, _| {
        *width = VecWidth::V256;
    });
    malformed_compare!("lanes", |_, _, _, _, _, _, lanes, _, _, _, _, _| {
        *lanes = 2;
    });
    malformed_compare!("predicate", |_, _, _, _, _, _, _, predicate, _, _, _, _| {
        *predicate = 30;
    });
    malformed_compare!(
        "packed semantic",
        |_, _, _, _, _, _, _, _, scalar, _, _, _| {
            *scalar = false;
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
        "exception suppression",
        |_, _, _, _, _, _, _, _, _, _, _, suppress| {
            *suppress = true;
        }
    );

    for (name, hint) in [
        ("missing compare hint", None),
        (
            "hint map",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::Rep,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint prefix",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint opcode",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0xC3,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "hint width",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0xC2,
                width: VecWidth::V256,
                w: false,
            }),
        ),
        (
            "hint W",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0xC2,
                width: VecWidth::V128,
                w: true,
            }),
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[consumer_index].x86_hint = hint;
        malformed.push((name, function));
    }

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x01),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
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

fn get_scalar(words: [u64; 8], elem: VecElementType) -> u64 {
    if elem == VecElementType::F32 {
        words[0] & u64::from(u32::MAX)
    } else {
        words[0]
    }
}

fn set_scalar(words: &mut [u64; 8], elem: VecElementType, value: u64) {
    if elem == VecElementType::F32 {
        words[0] = (words[0] & !u64::from(u32::MAX)) | (value & u64::from(u32::MAX));
    } else {
        words[0] = value;
    }
}

fn is_denormal(bits: u64, elem: VecElementType) -> bool {
    match elem {
        VecElementType::F32 => bits & 0x7F80_0000 == 0 && bits & 0x007F_FFFF != 0,
        VecElementType::F64 => {
            bits & 0x7FF0_0000_0000_0000 == 0 && bits & 0x000F_FFFF_FFFF_FFFF != 0
        }
        _ => unreachable!("scalar FP compare element"),
    }
}

fn is_nan(bits: u64, elem: VecElementType) -> bool {
    match elem {
        VecElementType::F32 => bits & 0x7F80_0000 == 0x7F80_0000 && bits & 0x007F_FFFF != 0,
        VecElementType::F64 => {
            bits & 0x7FF0_0000_0000_0000 == 0x7FF0_0000_0000_0000
                && bits & 0x000F_FFFF_FFFF_FFFF != 0
        }
        _ => unreachable!("scalar FP compare element"),
    }
}

fn is_snan(bits: u64, elem: VecElementType) -> bool {
    is_nan(bits, elem)
        && match elem {
            VecElementType::F32 => bits & 0x0040_0000 == 0,
            VecElementType::F64 => bits & 0x0008_0000_0000_0000 == 0,
            _ => unreachable!("scalar FP compare element"),
        }
}

fn apply_daz(bits: u64, elem: VecElementType, mxcsr: u32) -> (u64, u32) {
    if !is_denormal(bits, elem) {
        return (bits, 0);
    }
    if mxcsr & (1 << 6) == 0 {
        return (bits, 1 << 1);
    }
    let sign = if elem == VecElementType::F32 {
        bits & 0x8000_0000
    } else {
        bits & 0x8000_0000_0000_0000
    };
    (sign, 0)
}

fn relation(first: u64, second: u64, elem: VecElementType) -> usize {
    let ordering = match elem {
        VecElementType::F32 => {
            f32::from_bits(first as u32).partial_cmp(&f32::from_bits(second as u32))
        }
        VecElementType::F64 => f64::from_bits(first).partial_cmp(&f64::from_bits(second)),
        _ => unreachable!("scalar FP compare element"),
    };
    match ordering {
        Some(std::cmp::Ordering::Greater) => 0,
        Some(std::cmp::Ordering::Less) => 1,
        Some(std::cmp::Ordering::Equal) => 2,
        None => 3,
    }
}

fn independent_compare(
    case: ScalarCompareMemoryCase,
    source1: [u64; 8],
    source2: [u64; 8],
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
    let (first, first_status) = apply_daz(get_scalar(source1, case.elem), case.elem, mxcsr);
    let (second, second_status) = apply_daz(get_scalar(source2, case.elem), case.elem, mxcsr);
    let first_nan = is_nan(first, case.elem);
    let second_nan = is_nan(second, case.elem);
    let mut status = 0u32;
    let lane_relation = if first_nan || second_nan {
        status |= u32::from(
            is_snan(first, case.elem)
                || is_snan(second, case.elem)
                || (signaling && (first_nan || second_nan)),
        );
        3
    } else {
        status |= first_status | second_status;
        relation(first, second, case.elem)
    };
    let true_lane = TRUTH_TABLES[usize::from(case.predicate & 0x0F)] & (1 << lane_relation) != 0;

    let mut result = source1;
    let mut result_bytes = words_to_bytes(result);
    result_bytes[16..].fill(0);
    result = bytes_to_words(result_bytes);
    set_scalar(
        &mut result,
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
    (result, mxcsr | status)
}

#[test]
fn independent_oracle_covers_all_predicates_relations_nan_classes_daz_and_status() {
    const TRUTH_TABLES: [u8; 16] = [
        0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100, 0b1010, 0b1110,
        0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
    ];
    let pairs = [
        (0x4000_0000, 0x3F80_0000),
        (0xBF80_0000, 0x3F80_0000),
        (0x3F80_0000, 0x3F80_0000),
        (0x7FC1_2345, 0),
    ];
    let mut distinct_predicates = std::collections::HashSet::new();
    for predicate in 0..32 {
        let case = ScalarCompareMemoryCase {
            elem: VecElementType::F32,
            predicate,
            form: EncodingForm::C5,
        };
        let mut relation_mask = 0u8;
        for (relation, (first, second)) in pairs.into_iter().enumerate() {
            let mut source1 = [0; 8];
            let mut source2 = [0; 8];
            set_scalar(&mut source1, case.elem, first);
            set_scalar(&mut source2, case.elem, second);
            let (result, _) = independent_compare(case, source1, source2, 0x1F80);
            if get_scalar(result, case.elem) != 0 {
                relation_mask |= 1 << relation;
            }
        }
        assert_eq!(
            relation_mask,
            TRUTH_TABLES[usize::from(predicate & 0x0F)],
            "predicate {predicate}"
        );
        distinct_predicates.insert(relation_mask);
    }
    assert_eq!(distinct_predicates.len(), 16);

    let case = ScalarCompareMemoryCase {
        elem: VecElementType::F64,
        predicate: 0,
        form: EncodingForm::C4W0,
    };
    let mut denormal = [0; 8];
    set_scalar(&mut denormal, case.elem, 1);
    let (_, without_daz) = independent_compare(case, denormal, [0; 8], 0x1F80);
    let (_, with_daz) = independent_compare(case, denormal, [0; 8], 0x1F80 | (1 << 6));
    assert_ne!(without_daz & (1 << 1), 0);
    assert_eq!(with_daz & (1 << 1), 0);

    let mut quiet_nan = [0; 8];
    set_scalar(&mut quiet_nan, case.elem, 0x7FF8_2468_ACE0_1357);
    let quiet_case = ScalarCompareMemoryCase {
        predicate: 0,
        ..case
    };
    let signaling_case = ScalarCompareMemoryCase {
        predicate: 1,
        ..case
    };
    assert_eq!(
        independent_compare(quiet_case, quiet_nan, [0; 8], 0x1F80).1 & 1,
        0
    );
    assert_eq!(
        independent_compare(signaling_case, quiet_nan, [0; 8], 0x1F80).1 & 1,
        1
    );
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct ScalarMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut ScalarMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8)
    {
        return 0;
    }
    let source = words_to_bytes(context.value);
    let mut scratch = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    scratch[..size as usize].copy_from_slice(&source[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

#[cfg(target_arch = "x86_64")]
fn patterned_vector(shift: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x0123_4567_89AB_CDEFu64.rotate_left(((word * 9 + shift) % 64) as u32)
            ^ (shift as u64).wrapping_mul(0x0101_0101_0101_0101)
    })
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ScalarCompareMemoryCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80 | (((ordinal as u32 >> 2) & 3) << 13) | (u32::from(ordinal & 1 != 0) << 15),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(index * 5 + ordinal);
    }
    registers.zmm[usize::from(case.source1())] = patterned_vector(ordinal.wrapping_mul(3));
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] =
            std::array::from_fn(|word| 0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7) as u32));
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn operand_pair(elem: VecElementType, variant: usize) -> (u64, u64) {
    match (elem, variant) {
        (VecElementType::F32, 0) => (0x4000_0000, 0x3F80_0000),
        (VecElementType::F32, 1) => (0xBF80_0000, 0x3F80_0000),
        (VecElementType::F32, 2) => (0x3F80_0000, 0x3F80_0000),
        (VecElementType::F32, 3) => (0x7FC1_2345, 0),
        (VecElementType::F32, 4) => (0x7F81_2345, 0),
        (VecElementType::F32, 5) => (1, 0),
        (VecElementType::F64, 0) => (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (VecElementType::F64, 1) => (0xBFF0_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (VecElementType::F64, 2) => (0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (VecElementType::F64, 3) => (0x7FF8_2468_ACE0_1357, 0),
        (VecElementType::F64, 4) => (0x7FF0_2468_ACE0_1357, 0),
        (VecElementType::F64, 5) => (1, 0),
        _ => unreachable!("six scalar comparison variants"),
    }
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: ScalarCompareMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1())];
    let (destination, mxcsr) = independent_compare(case, source1, source2, registers.mxcsr);
    registers.zmm[usize::from(case.destination())] = destination;
    registers.mxcsr = mxcsr;
    let source = words_to_bytes(source2);
    let mut scratch = [0; 64];
    scratch[..case.memory_size() as usize].copy_from_slice(&source[..case.memory_size() as usize]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: ScalarCompareMemoryCase,
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
    memory.load(address as usize, &bytes[..case.memory_size() as usize]);
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
fn native_scalar_fp_compare_matches_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar FP compare memory differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut newly_observed_status = 0u32;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for (level_index, level) in DIFFERENTIAL_LEVELS.into_iter().enumerate() {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let variant = case.form.ordinal() * DIFFERENTIAL_LEVELS.len() + level_index;
            let (first, second) = operand_pair(case.elem, variant);
            let mut source2 = patterned_vector(ordinal.wrapping_mul(7).wrapping_add(3));
            set_scalar(&mut source2, case.elem, second);

            let mut context = ScalarMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            set_scalar(
                &mut registers.zmm[usize::from(case.source1())],
                case.elem,
                first,
            );
            registers.mxcsr &= !0x03;
            if variant == 5 && case.predicate & 1 != 0 {
                registers.mxcsr |= 1 << 6;
            } else {
                registers.mxcsr &= !(1 << 6);
            }
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = scalar_load_helper as usize as u64;
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
            assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function, &initial, &expected, source2, address, case, level,
            );
            successes += 1;

            let mut context = ScalarMemoryContext {
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
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = scalar_load_helper as usize as u64;
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
                case.memory_size(),
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
        "executed {successes} successful and {faults} faulting native VEX scalar FP compare memory cases"
    );
}
