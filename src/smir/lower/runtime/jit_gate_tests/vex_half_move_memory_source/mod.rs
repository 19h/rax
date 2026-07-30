//! Exact helper-backed VEX.128 high/low 64-bit lane load coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexHalfMoveMemoryEncoding,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexHalfMoveMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_half_move_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{SmirLowerer, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET};
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x1216_1216;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryLane {
    Low,
    High,
}

impl MemoryLane {
    const ALL: [Self; 2] = [Self::Low, Self::High];

    const fn index(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::High => 1,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::Low => 0x12,
            Self::High => 0x16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveFormat {
    Ps,
    Pd,
}

impl MoveFormat {
    const ALL: [Self; 2] = [Self::Ps, Self::Pd];

    const fn pp(self) -> u8 {
        match self {
            Self::Ps => 0,
            Self::Pd => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4W0,
    C4W1,
}

impl VexForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];

    const fn w(self) -> bool {
        matches!(self, Self::C4W1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfMoveCase {
    lane: MemoryLane,
    format: MoveFormat,
    form: VexForm,
    destination: u8,
    source1: u8,
    base: u8,
}

impl HalfMoveCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        let encoded_vvvv = ((!self.source1) & 15) << 3;
        let modrm = 0x40 | ((self.destination & 7) << 3) | (self.base & 7);
        let mut bytes = match self.form {
            VexForm::C5 => {
                assert!(self.base < 8, "C5 has no VEX.B extension");
                vec![
                    0xC5,
                    (if self.destination < 8 { 0x80 } else { 0 }) | encoded_vvvv | self.format.pp(),
                    self.lane.opcode(),
                    modrm,
                ]
            }
            VexForm::C4W0 | VexForm::C4W1 => vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | encoded_vvvv | self.format.pp(),
                self.lane.opcode(),
                modrm,
            ],
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    fn expected_encoding(self) -> X86VexHalfMoveMemoryEncoding {
        X86VexHalfMoveMemoryEncoding {
            destination: self.destination,
            source1: self.source1,
            memory_lane: self.lane.index(),
            w: self.form.w(),
            pp: self.format.pp(),
            opcode: self.lane.opcode(),
        }
    }

    fn vex_rrr(self, opcode: u8, destination: u8, source1: u8, source2: u8) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && source2 < 8);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 15) << 3),
            opcode,
            0xC0 | ((destination & 7) << 3) | source2,
        ]
    }

    fn expected_native_bytes(self) -> Vec<u8> {
        let scratch = self.scratch();
        let mut bytes = Vec::new();
        if self.lane == MemoryLane::Low {
            bytes.extend_from_slice(&self.vex_rrr(0x16, scratch, scratch, scratch));
            bytes.extend_from_slice(&self.vex_rrr(0x12, self.destination, self.source1, scratch));
        } else {
            bytes.extend_from_slice(&self.vex_rrr(0x16, self.destination, self.source1, scratch));
        }
        bytes
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
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

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexHalfMoveMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_half_move_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexHalfMoveMemorySequence> {
    classified_at(function, 0, allow_mem)
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits source metadata"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: HalfMoveCase) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 6, "{case:?}: {ops:#?}");
    let preserved_lane = 1 - case.lane.index();

    let preserved = match &ops[0].kind {
        OpKind::VExtractLane {
            dst: value @ VReg::Virtual(_),
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, xmm(case.source1), "{case:?}");
            assert_eq!(*lane, preserved_lane, "{case:?}");
            *value
        }
        other => panic!("{case:?}: expected preserved-lane extraction, got {other:?}"),
    };

    let loaded = match &ops[1].kind {
        OpKind::Load {
            dst: value @ VReg::Virtual(_),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        } => *value,
        other => panic!("{case:?}: expected 8-byte load, got {other:?}"),
    };

    let zero = match &ops[2].kind {
        OpKind::Mov {
            dst: value @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *value,
        other => panic!("{case:?}: expected zero materialization, got {other:?}"),
    };

    assert!(matches!(
        &ops[3].kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes: 1,
        } if *dst == xmm(case.destination) && *scalar == zero
    ));
    assert!(matches!(
        &ops[4].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if *dst == xmm(case.destination)
            && *vec == xmm(case.destination)
            && *scalar == preserved
            && *lane == preserved_lane
    ));
    assert!(matches!(
        &ops[5].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if *dst == xmm(case.destination)
            && *vec == xmm(case.destination)
            && *scalar == loaded
            && *lane == case.lane.index()
    ));
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexHalfMoveMemorySequence {
            consumed: 6,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_case(case: HalfMoveCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn assert_feature_requirements(function: &SmirFunction, case: HalfMoveCase) {
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

    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_avx = true;
    assert_eq!(
        x86_native_replay_feature_requirements(function, &excluded),
        expected,
        "{case:?}"
    );
}

fn lower(function: &SmirFunction, case: HalfMoveCase) -> (Vec<u8>, usize) {
    assert_exact_lift_and_sequence(function, case);
    assert_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX half move failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize VEX half move");
    let expected = case.expected_native_bytes();
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?}: missing native lane transfer {expected:02X?}"
    );
    let scratch_offset = X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes();
    assert!(
        code.windows(scratch_offset.len())
            .any(|window| window == scratch_offset),
        "{case:?}: helper scratch offset absent"
    );
    (code, result.entry_offset)
}

#[test]
fn all_1536_scanner_memory_source_cells_admit_and_lower_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for form in VexForm::ALL {
                for destination in 0..8 {
                    for source1 in 0..16 {
                        let case = HalfMoveCase {
                            lane,
                            format,
                            form,
                            destination,
                            source1,
                            base: 2,
                        };
                        cells += 1;
                        for level in LEVELS {
                            lower(&optimize(lift_case(case), level), case);
                            lowered += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cells, 1_536);
    assert_eq!(lowered, 1_536 * LEVELS.len());
}

#[test]
fn high_operands_aliases_and_complete_address_shapes_remain_exact() {
    let cases: &[(HalfMoveCase, &[u8])] = &[
        (
            HalfMoveCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Ps,
                form: VexForm::C5,
                destination: 9,
                source1: 2,
                base: 5,
            },
            &[0x64, 0xC5, 0x68, 0x12, 0x4D, 0x20],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::High,
                format: MoveFormat::Pd,
                form: VexForm::C4W1,
                destination: 9,
                source1: 10,
                base: 12,
            },
            &[0x65, 0xC4, 0x01, 0xA9, 0x16, 0x4C, 0xEC, 0x20],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Pd,
                form: VexForm::C4W0,
                destination: 14,
                source1: 15,
                base: 5,
            },
            &[
                0x67, 0xC4, 0x61, 0x01, 0x12, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
            ],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::High,
                format: MoveFormat::Ps,
                form: VexForm::C4W1,
                destination: 0,
                source1: 15,
                base: 5,
            },
            &[0xC4, 0xC1, 0x80, 0x16, 0x05, 0x11, 0x22, 0x33, 0x44],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::High,
                format: MoveFormat::Pd,
                form: VexForm::C4W1,
                destination: 15,
                source1: 15,
                base: 11,
            },
            &[0xC4, 0x41, 0x81, 0x16, 0x7B, 0x20],
        ),
    ];
    for &(case, bytes) in cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert_exact_lift_and_sequence(&function, case);
            lower(&function, case);
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed IR"
    );
}

fn baseline_case() -> HalfMoveCase {
    HalfMoveCase {
        lane: MemoryLane::Low,
        format: MoveFormat::Pd,
        form: VexForm::C4W1,
        destination: 9,
        source1: 10,
        base: 11,
    }
}

#[test]
fn l1_store_register_reserved_and_nonexact_source_metadata_fail_closed() {
    let case = baseline_case();
    let base = lift_case(case);
    let valid = case.bytes();
    let mut invalid = Vec::new();

    let mut l1 = valid.clone();
    l1[2] |= 0x04;
    invalid.push(("VEX.L=1", l1));
    for (name, opcode) in [
        ("low store", 0x13),
        ("high store", 0x17),
        ("different load", 0x10),
    ] {
        let mut bytes = valid.clone();
        bytes[3] = opcode;
        invalid.push((name, bytes));
    }
    for (name, pp) in [("F3 prefix", 2), ("F2 prefix", 3)] {
        let mut bytes = valid.clone();
        bytes[2] = (bytes[2] & !3) | pp;
        invalid.push((name, bytes));
    }
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
    invalid.push(("wrong map", wrong_map));
    let mut register = valid.clone();
    register[4] |= 0xC0;
    register.pop();
    invalid.push(("register source", register));
    let mut trailing = valid.clone();
    trailing.push(0);
    invalid.push(("trailing byte", trailing));
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    invalid.push(("forbidden legacy prefix", forbidden_prefix));

    for (name, bytes) in invalid {
        let mut function = base.clone();
        function.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&bytes).expect("mutated source image fits metadata"),
        );
        assert_rejected(name, &function);
    }

    let mut missing = base;
    missing.x86_instruction_bytes.clear();
    assert_rejected("missing source metadata", &missing);
}

#[test]
fn classifier_rejects_every_graph_field_provenance_and_virtual_escape_mutation() {
    let case = baseline_case();
    let base = lift_case(case);
    let preserved = match base.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let loaded = match base.blocks[0].ops[1].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let zero = match base.blocks[0].ops[2].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    macro_rules! mutate_extract {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VExtractLane { $field, .. } = &mut function.blocks[0].ops[0].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_extract!("extract destination", dst, x86(X86Reg::Rax));
    mutate_extract!("extract source", vec, xmm(11));
    mutate_extract!("extract lane", lane, 0);
    mutate_extract!("extract element", elem, VecElementType::I32);
    mutate_extract!("extract extension", sign, SignExtend::Sign);

    macro_rules! mutate_load {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Load { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_load!("load destination", dst, x86(X86Reg::Rax));
    mutate_load!(
        "load address",
        addr,
        Address::Direct(VReg::Virtual(VirtualId(0xFF00)))
    );
    mutate_load!("load width", width, MemWidth::B4);
    mutate_load!("load extension", sign, SignExtend::Sign);

    macro_rules! mutate_zero {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Mov { $field, .. } = &mut function.blocks[0].ops[2].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_zero!("zero destination", dst, x86(X86Reg::Rax));
    mutate_zero!("zero source", src, SrcOperand::Imm(1));
    mutate_zero!("zero width", width, OpWidth::W32);

    macro_rules! mutate_broadcast {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VBroadcast { $field, .. } = &mut function.blocks[0].ops[3].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_broadcast!("clear destination", dst, xmm(8));
    mutate_broadcast!("clear scalar", scalar, loaded);
    mutate_broadcast!("clear element", elem, VecElementType::I32);
    mutate_broadcast!("clear lanes", lanes, 2);

    macro_rules! mutate_insert {
        ($index:literal, $name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VInsertLane { $field, .. } = &mut function.blocks[0].ops[$index].kind
            else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_insert!(4, "preserved insert destination", dst, xmm(8));
    mutate_insert!(4, "preserved insert vector", vec, xmm(8));
    mutate_insert!(4, "preserved insert scalar", scalar, loaded);
    mutate_insert!(4, "preserved insert lane", lane, 0);
    mutate_insert!(4, "preserved insert element", elem, VecElementType::I32);
    mutate_insert!(5, "memory insert destination", dst, xmm(8));
    mutate_insert!(5, "memory insert vector", vec, xmm(8));
    mutate_insert!(5, "memory insert scalar", scalar, preserved);
    mutate_insert!(5, "memory insert lane", lane, 1);
    mutate_insert!(5, "memory insert element", elem, VecElementType::I32);

    for index in 0..6 {
        let mut function = base.clone();
        function.blocks[0].ops[index].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: case.lane.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        });
        malformed.push(("invented operation hint", function));
    }

    for index in 1..6 {
        let mut function = base.clone();
        function.blocks[0].ops[index].guest_pc += 1;
        malformed.push(("split guest provenance", function));
    }

    let mut escaped_preserved = base.clone();
    escaped_preserved.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FF0),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(preserved),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("preserved value escapes", escaped_preserved));

    let mut escaped_loaded = base.clone();
    escaped_loaded.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FF1),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value escapes", escaped_loaded));

    let mut escaped_zero = base.clone();
    escaped_zero.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FF2),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(zero),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("zero value escapes", escaped_zero));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FF3),
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value redefined", duplicate_definition));

    let mut aliased_virtuals = base.clone();
    if let OpKind::Load { dst, .. } = &mut aliased_virtuals.blocks[0].ops[1].kind {
        *dst = preserved;
    }
    if let OpKind::VInsertLane { scalar, .. } = &mut aliased_virtuals.blocks[0].ops[5].kind {
        *scalar = preserved;
    }
    malformed.push(("local virtual identities alias", aliased_virtuals));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FF4), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut rejected = 0usize;
    for (name, function) in malformed {
        assert_rejected(name, &function);
        rejected += 1;
    }
    assert_eq!(rejected, 5 + 4 + 3 + 4 + 5 + 5 + 6 + 5 + 6);

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7FF5), PC, OpKind::Nop));
    assert_eq!(classified_at(&same_pc_head, 1, true), None);
    assert_rejected("unconsumed same-PC head", &same_pc_head);
}

#[test]
fn excluded_regions_contribute_no_features_and_aarch64_admission_stays_closed() {
    let case = HalfMoveCase {
        lane: MemoryLane::High,
        format: MoveFormat::Pd,
        form: VexForm::C4W1,
        destination: 15,
        source1: 15,
        base: 11,
    };
    let function = lift_case(case);
    let excluded = HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &HashMap::new()
    ));
    assert!(is_x86_aarch64_native_clobber_safe_excluding(
        &function, &excluded
    ));

    let upper = X86_GUEST_ZMM_OFFSET + i32::from(case.destination) * 64 + 32;
    let (code, _) = lower(&function, case);
    assert!(
        code.windows(4)
            .any(|window| window == (upper as u32).to_le_bytes())
    );
}
