//! Exact helper-backed VEX scalar-FP arithmetic memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpId, OpWidth,
    SignExtend, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86FpBinaryOp, X86Reg,
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

const PC: u64 = 0xB958;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const FP_SCENARIOS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FpOperation {
    Add,
    Mul,
    Sub,
    Min,
    Div,
    Max,
}

impl FpOperation {
    const ALL: [Self; 6] = [
        Self::Add,
        Self::Mul,
        Self::Sub,
        Self::Min,
        Self::Div,
        Self::Max,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::Add => 0x58,
            Self::Mul => 0x59,
            Self::Sub => 0x5C,
            Self::Min => 0x5D,
            Self::Div => 0x5E,
            Self::Max => 0x5F,
        }
    }

    const fn op(self) -> X86FpBinaryOp {
        match self {
            Self::Add => X86FpBinaryOp::Add,
            Self::Mul => X86FpBinaryOp::Mul,
            Self::Sub => X86FpBinaryOp::Sub,
            Self::Min => X86FpBinaryOp::Min,
            Self::Div => X86FpBinaryOp::Div,
            Self::Max => X86FpBinaryOp::Max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FpFormat {
    F32,
    F64,
}

impl FpFormat {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn prefix(self) -> X86SsePrefix {
        match self {
            Self::F32 => X86SsePrefix::Rep,
            Self::F64 => X86SsePrefix::Repne,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::F32 => 2,
            Self::F64 => 3,
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn memory_size(self) -> u32 {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
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
struct ScalarMemoryCase {
    operation: FpOperation,
    format: FpFormat,
    form: EncodingForm,
}

impl ScalarMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Destination/source1 occupy XMM0/1, forcing scratch register 2.
            EncodingForm::C5 => (0, 1, 3),
            // A high destination plus source1 XMM0 forces scratch register 1.
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
            .expect("two scalar VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match self.form {
            EncodingForm::C5 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | self.format.pp(),
                self.operation.opcode(),
                modrm,
                DISP as u8,
            ],
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 0x01,
                (u8::from(self.form.w()) << 7) | (((!source1) & 0x0F) << 3) | self.format.pp(),
                self.operation.opcode(),
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_arithmetic_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        let modrm = 0xC0 | ((destination & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | self.format.pp(),
                self.operation.opcode(),
                modrm,
            ]
        } else {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 0x01,
                0x80 | (((!source1) & 0x0F) << 3) | self.format.pp(),
                self.operation.opcode(),
                modrm,
            ]
        }
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn expected_address(case: ScalarMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: ScalarMemoryCase) {
    let elem = case.format.elem();
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let expected_len = 2 * xmm_lanes + 5;
    assert_eq!(ops.len(), expected_len, "{case:?}: {ops:#?}");

    let loaded_scalar = match &ops[0].kind {
        OpKind::Load {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.format.memory_width(), "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected scalar load, got {other:?}"),
    };
    assert_eq!(ops[0].x86_hint, None, "{case:?}");

    let source_vector = match &ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } => {
            assert_eq!(*scalar, loaded_scalar, "{case:?}");
            assert_eq!(*broadcast_elem, elem, "{case:?}");
            *vector
        }
        other => panic!("{case:?}: expected source broadcast, got {other:?}"),
    };

    let binary_result = match &ops[2].kind {
        OpKind::X86FpBinary {
            dst: result @ VReg::Virtual(_),
            src1,
            src2,
            mask,
            elem: binary_elem,
            lanes,
            op,
            round,
            suppress_exceptions,
        } => {
            assert_eq!(*src1, xmm(case.source1()), "{case:?}");
            assert_eq!(*src2, source_vector, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*binary_elem, elem, "{case:?}");
            assert_eq!(*lanes, 1, "{case:?}");
            assert_eq!(*op, case.operation.op(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert!(!*suppress_exceptions, "{case:?}");
            *result
        }
        other => panic!("{case:?}: expected scalar FP binary op, got {other:?}"),
    };
    assert_eq!(
        ops[2].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: case.format.prefix(),
            opcode: case.operation.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert!(ops[2].kind.has_side_effects(), "{case:?}: MXCSR");

    let scalar_result = match &ops[3].kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, binary_result, "{case:?}");
            assert_eq!(*extract_elem, elem, "{case:?}");
            *scalar
        }
        other => panic!("{case:?}: expected result extraction, got {other:?}"),
    };

    let mut upper_scalars = Vec::new();
    for lane in 1..xmm_lanes {
        let scalar = match &ops[3 + lane].kind {
            OpKind::VExtractLane {
                dst: scalar @ VReg::Virtual(_),
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*vec, xmm(case.source1()), "{case:?}");
                assert_eq!(usize::from(*extract_lane), lane, "{case:?}");
                assert_eq!(*extract_elem, elem, "{case:?}");
                *scalar
            }
            other => panic!("{case:?}: expected upper extraction {lane}, got {other:?}"),
        };
        upper_scalars.push(scalar);
    }

    let zero_offset = 3 + xmm_lanes;
    let zero = match &ops[zero_offset].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *zero,
        other => panic!("{case:?}: expected vector-clear zero, got {other:?}"),
    };
    assert!(matches!(
        &ops[zero_offset + 1].kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: clear_elem,
            lanes: 1,
        } if *dst == xmm(case.destination()) && *scalar == zero && *clear_elem == elem
    ));
    assert!(matches!(
        &ops[zero_offset + 2].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: 0,
            elem: insert_elem,
        } if *dst == xmm(case.destination())
            && *vec == xmm(case.destination())
            && *scalar == scalar_result
            && *insert_elem == elem
    ));
    for (lane, scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        assert!(matches!(
            &ops[zero_offset + 2 + lane].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: insert_lane,
                elem: insert_elem,
            } if *dst == xmm(case.destination())
                && *vec == xmm(case.destination())
                && *inserted == scalar
                && usize::from(*insert_lane) == lane
                && *insert_elem == elem
        ));
    }
    assert!(
        ops.iter().all(|op| op.guest_pc == PC),
        "{case:?}: split guest provenance"
    );
}

fn function_from_lift(bytes: &[u8], result: crate::smir::lift::LiftResult) -> SmirFunction {
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

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    function_from_lift(bytes, result)
}

fn lift_case(case: ScalarMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let function = lift_bytes(&bytes);
    assert_exact_chain(&function.blocks[0].ops, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed scalar VEX FP lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar VEX FP arithmetic"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ScalarMemoryCase> {
    let mut cases = Vec::new();
    for operation in FpOperation::ALL {
        for format in FpFormat::ALL {
            for form in EncodingForm::ALL {
                cases.push(ScalarMemoryCase {
                    operation,
                    format,
                    form,
                });
            }
        }
    }
    cases
}

#[test]
fn all_36_defined_c4_c5_wig_format_and_operation_shapes_are_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 6 * 2 * 3);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);
            let (code, _) = lower(&function);
            assert!(
                code.windows(5)
                    .any(|window| { window == [0xB9, case.format.memory_size() as u8, 0, 0, 0,] }),
                "{level:?} {case:?}: missing scalar helper size"
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
    assert_eq!(lowered, 36 * LEVELS.len());
}

#[test]
fn complete_prefixed_sib_and_displacement_source_shapes_are_admitted() {
    let cases: &[&[u8]] = &[
        // FS + address-size override; VADDSS xmm0,xmm1,fs:[ebx+0x20].
        &[0x64, 0x67, 0xC5, 0xF2, 0x58, 0x43, 0x20],
        // VMULSS xmm15,xmm0,[r11+r10*4+0x20].
        &[0xC4, 0x01, 0x7A, 0x59, 0x7C, 0x93, 0x20],
        // VDIVSD xmm0,xmm1,[rip+0x44332211].
        &[0xC5, 0xF3, 0x5E, 0x05, 0x11, 0x22, 0x33, 0x44],
        // VMAXSD xmm9,xmm9,[r11+0x44332211] with WIG=1.
        &[0xC4, 0x41, 0xB3, 0x5F, 0x8B, 0x11, 0x22, 0x33, 0x44],
    ];
    for bytes in cases {
        let function = lift_bytes(bytes);
        let (code, _) = lower(&function);
        assert!(!code.is_empty(), "{bytes:02X?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), true,),
        "{name}: clobber gate admitted malformed scalar chain"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed scalar chain"
    );
}

#[test]
fn scalar_classifier_and_lowerer_fail_closed_for_semantic_and_provenance_invariants() {
    let case = ScalarMemoryCase {
        operation: FpOperation::Add,
        format: FpFormat::F32,
        form: EncodingForm::C5,
    };
    let base = lift_case(case);
    let loaded_scalar = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();

    let mut l1_metadata = base.clone();
    let mut l1 = case.bytes();
    l1[1] |= 0x04;
    l1_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&l1).unwrap());

    let mut wrong_destination_metadata = base.clone();
    let mut wrong_destination = case.bytes();
    wrong_destination[3] ^= 0x08;
    wrong_destination_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_destination).unwrap(),
    );

    let mut wrong_source1_metadata = base.clone();
    let mut wrong_source1 = case.bytes();
    wrong_source1[1] ^= 0x08;
    wrong_source1_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_source1).unwrap(),
    );

    let mut trailing_metadata = base.clone();
    let mut trailing = case.bytes();
    trailing.push(0);
    trailing_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&trailing).unwrap(),
    );

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(13),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: SrcOperand::Reg(loaded_scalar),
            width: OpWidth::W32,
        },
    ));

    let mut wrong_load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }

    let mut signed_load = base.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut wrong_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }

    let mut masked = base.clone();
    if let OpKind::X86FpBinary { mask, .. } = &mut masked.blocks[0].ops[2].kind {
        *mask = Some(x86(X86Reg::K(1)));
    }

    let mut embedded_round = base.clone();
    if let OpKind::X86FpBinary {
        round,
        suppress_exceptions,
        ..
    } = &mut embedded_round.blocks[0].ops[2].kind
    {
        *round = FpRoundMode::RoundUp;
        *suppress_exceptions = true;
    }

    let mut wrong_operation = base.clone();
    if let OpKind::X86FpBinary { op, .. } = &mut wrong_operation.blocks[0].ops[2].kind {
        *op = X86FpBinaryOp::Sub;
    }

    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::Rep,
        opcode: 0x58,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[8].guest_pc += 1;

    let mut wrong_result_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_result_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }

    let mut wrong_upper_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut wrong_upper_source.blocks[0].ops[4].kind {
        *vec = xmm(2);
    }

    let mut nonzero_clear = base.clone();
    if let OpKind::Mov { src, .. } = &mut nonzero_clear.blocks[0].ops[7].kind {
        *src = SrcOperand::Imm(1);
    }

    let mut wrong_destination = base.clone();
    if let OpKind::VBroadcast { dst, .. } = &mut wrong_destination.blocks[0].ops[8].kind {
        *dst = xmm(2);
    }

    let mut wrong_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut wrong_insert_scalar.blocks[0].ops[9].kind {
        *scalar = loaded_scalar;
    }

    let malformed = [
        ("missing source metadata", missing_metadata),
        ("VEX.L=1 source metadata", l1_metadata),
        ("metadata destination mismatch", wrong_destination_metadata),
        ("metadata source1 mismatch", wrong_source1_metadata),
        ("trailing source byte", trailing_metadata),
        ("load temporary used twice", extra_use),
        ("scalar load width mismatch", wrong_load_width),
        ("signed scalar load", signed_load),
        ("virtual address component", virtual_address),
        ("source broadcast lane mismatch", wrong_broadcast_lanes),
        ("masked scalar operation", masked),
        ("embedded round/SAE", embedded_round),
        ("hint/operation mismatch", wrong_operation),
        ("wrong VEX hint", wrong_hint),
        ("split guest PC", wrong_pc),
        ("result lane mismatch", wrong_result_lane),
        ("upper merge source mismatch", wrong_upper_source),
        ("nonzero destination clear", nonzero_clear),
        ("destination chain mismatch", wrong_destination),
        ("low insert scalar mismatch", wrong_insert_scalar),
    ];
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn vex_l1_scalar_memory_forms_lift_but_remain_at_the_interpreter_frontier() {
    for case in all_cases() {
        let mut bytes = case.bytes();
        let p1 = if bytes[0] == 0xC5 { 1 } else { 2 };
        bytes[p1] |= 0x04;
        let function = lift_bytes(&bytes);
        assert_rejected("VEX.L=1 generation-dependent scalar encoding", &function);
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

fn scalar_operands(case: ScalarMemoryCase, scenario: usize) -> (u64, u64) {
    match scenario {
        // Operation-specific boundary, rounding, quiet-NaN, and exception inputs.
        0 => match (case.format, case.operation) {
            (FpFormat::F32, FpOperation::Add) => (0x3F80_0001, 0x3380_0000),
            (FpFormat::F32, FpOperation::Mul) => (0x7F7F_FFFF, 0x4000_0000),
            (FpFormat::F32, FpOperation::Sub) => (0x0000_0000, 0x8000_0000),
            (FpFormat::F32, FpOperation::Min) => (0x7FC1_2345, 0x8000_0000),
            (FpFormat::F32, FpOperation::Div) => (0x3F80_0000, 0x0000_0000),
            (FpFormat::F32, FpOperation::Max) => (0xBF80_0000, 0x7FC5_4321),
            (FpFormat::F64, FpOperation::Add) => (0x3FF0_0000_0000_0001, 0x3CA0_0000_0000_0000),
            (FpFormat::F64, FpOperation::Mul) => (0x7FEF_FFFF_FFFF_FFFF, 0x4000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Sub) => (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Min) => (0x7FF8_2468_ACE0_1357, 0x8000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Div) => (0x3FF0_0000_0000_0000, 0x0000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Max) => (0xBFF0_0000_0000_0000, 0x7FF8_7531_0246_8ACE),
        },
        // Signaling NaNs require invalid-status handling and payload quieting.
        1 => match case.format {
            FpFormat::F32 => (0x7F81_2345, 0x3F80_0000),
            FpFormat::F64 => (0x7FF0_1234_5678_9ABC, 0x3FF0_0000_0000_0000),
        },
        // Subnormal inputs plus operation-specific exact or inexact tiny results.
        2 => match (case.format, case.operation) {
            (FpFormat::F32, FpOperation::Add) => (0x0080_0000, 0x807F_FFFF),
            (FpFormat::F32, FpOperation::Mul) => (0x0080_0000, 0x3EFF_FFFF),
            (FpFormat::F32, FpOperation::Sub) => (0x0080_0000, 0x007F_FFFF),
            (FpFormat::F32, FpOperation::Min) => (0x3F80_0000, 0x0000_0001),
            (FpFormat::F32, FpOperation::Div) => (0x0080_0000, 0x4040_0000),
            (FpFormat::F32, FpOperation::Max) => (0x8000_0001, 0x8000_0000),
            (FpFormat::F64, FpOperation::Add) => (0x0010_0000_0000_0000, 0x800F_FFFF_FFFF_FFFF),
            (FpFormat::F64, FpOperation::Mul) => (0x0010_0000_0000_0000, 0x3FDF_FFFF_FFFF_FFFF),
            (FpFormat::F64, FpOperation::Sub) => (0x0010_0000_0000_0000, 0x000F_FFFF_FFFF_FFFF),
            (FpFormat::F64, FpOperation::Min) => (0x3FF0_0000_0000_0000, 0x0000_0000_0000_0001),
            (FpFormat::F64, FpOperation::Div) => (0x0010_0000_0000_0000, 0x4008_0000_0000_0000),
            (FpFormat::F64, FpOperation::Max) => (0x8000_0000_0000_0001, 0x8000_0000_0000_0000),
        },
        // Both infinities and both signed zeros, including invalid combinations.
        3 => match (case.format, case.operation) {
            (FpFormat::F32, FpOperation::Add) => (0x7F80_0000, 0xFF80_0000),
            (FpFormat::F32, FpOperation::Mul) => (0xFF80_0000, 0x0000_0000),
            (FpFormat::F32, FpOperation::Sub) => (0xFF80_0000, 0x7F80_0000),
            (FpFormat::F32, FpOperation::Min) => (0x0000_0000, 0x8000_0000),
            (FpFormat::F32, FpOperation::Div) => (0xBF80_0000, 0x7F80_0000),
            (FpFormat::F32, FpOperation::Max) => (0x8000_0000, 0x0000_0000),
            (FpFormat::F64, FpOperation::Add) => (0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000),
            (FpFormat::F64, FpOperation::Mul) => (0xFFF0_0000_0000_0000, 0x0000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Sub) => (0xFFF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (FpFormat::F64, FpOperation::Min) => (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
            (FpFormat::F64, FpOperation::Div) => (0xBFF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (FpFormat::F64, FpOperation::Max) => (0x8000_0000_0000_0000, 0x0000_0000_0000_0000),
        },
        // A source2 quiet NaN exercises the distinct operand-selection path.
        4 => match case.format {
            FpFormat::F32 => (0x3F80_0000, 0xFFC5_4321),
            FpFormat::F64 => (0x3FF0_0000_0000_0000, 0xFFF8_7531_0246_8ACE),
        },
        _ => unreachable!("five scalar FP scenarios"),
    }
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

    let mut bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    let value = words_to_bytes(context.value);
    bytes[..size as usize].copy_from_slice(&value[..size as usize]);
    state.vector_scratch = bytes_to_words(bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ScalarMemoryCase, ordinal: usize, scenario: usize) -> GuestRegs {
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
        // All exception masks remain set. RC and prior status vary, while
        // DAZ/FTZ remain clear for native-vs-translated-host portability.
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
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
    let (source1, _) = scalar_operands(case, scenario);
    match case.format {
        FpFormat::F32 => {
            registers.zmm[usize::from(case.source1())][0] =
                (registers.zmm[usize::from(case.source1())][0] & !u64::from(u32::MAX)) | source1;
        }
        FpFormat::F64 => registers.zmm[usize::from(case.source1())][0] = source1,
    }
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] = std::array::from_fn(|word| {
            0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7 + ordinal) as u32)
        });
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreted_architecture(
    function: &SmirFunction,
    initial: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: ScalarMemoryCase,
    level: OptLevel,
) -> ([u64; 32], [[u64; 8]; 32], [u64; 8], u64, u32) {
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
    memory.load(
        address as usize,
        &bytes[..case.format.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    (x86.gpr, vectors, x86.k, x86.rflags, x86.mxcsr)
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_scalar_memory_fp_arithmetic_matches_o0_o2_interpretation_and_faults_precisely() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native scalar VEX memory-FP differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_successes = cases.len() * DIFFERENTIAL_LEVELS.len() * FP_SCENARIOS;
    let expected_faults = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut newly_raised_status = 0u32;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            for scenario in 0..FP_SCENARIOS {
                let (_, source2_scalar) = scalar_operands(case, scenario);
                let source2 = std::array::from_fn(|index| {
                    if index == 0 {
                        source2_scalar
                    } else {
                        0x5A5A_5A5A_5A5A_5A5A
                    }
                });

                let mut context = ScalarMemoryContext {
                    value: source2,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let state_ordinal = ordinal * FP_SCENARIOS + scenario;
                let mut registers = full_guest_regs(case, state_ordinal, scenario);
                let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                registers.vec_load_fn = scalar_load_helper as usize as u64;
                let initial = registers;
                let (gpr, zmm, k, rflags, mxcsr) =
                    interpreted_architecture(&function, &initial, source2, address, case, level);
                let mut expected = initial;
                expected.gpr = gpr;
                expected.zmm = zmm;
                expected.k = k;
                expected.rflags = rflags;
                expected.mxcsr = mxcsr;
                let mut scratch = [0u8; 64];
                let source_bytes = words_to_bytes(source2);
                scratch[..case.format.memory_size() as usize]
                    .copy_from_slice(&source_bytes[..case.format.memory_size() as usize]);
                expected.vector_scratch = bytes_to_words(scratch);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(
                    registers, expected,
                    "{level:?} {case:?} scenario {scenario}: success"
                );
                assert_eq!(context.calls, 1, "{level:?} {case:?} scenario {scenario}");
                assert_eq!(
                    context.last_addr, address,
                    "{level:?} {case:?} scenario {scenario}"
                );
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                    "{level:?} {case:?} scenario {scenario}"
                );
                assert_eq!(
                    context.last_size,
                    case.format.memory_size(),
                    "{level:?} {case:?} scenario {scenario}"
                );
                assert_eq!(
                    context.last_zero_upper, 1,
                    "{level:?} {case:?} scenario {scenario}"
                );
                newly_raised_status |= expected.mxcsr & !initial.mxcsr & 0x3F;
                successes += 1;
            }

            let (_, source2_scalar) = scalar_operands(case, 0);
            let source2 = std::array::from_fn(|index| {
                if index == 0 {
                    source2_scalar
                } else {
                    0x5A5A_5A5A_5A5A_5A5A
                }
            });
            let mut context = ScalarMemoryContext {
                value: source2,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, 0);
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
                case.format.memory_size(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert_eq!(successes, expected_successes);
    assert_eq!(faults, expected_faults);
    assert_eq!(
        newly_raised_status, 0x3F,
        "native differential did not newly exercise every MXCSR status flag"
    );
}
