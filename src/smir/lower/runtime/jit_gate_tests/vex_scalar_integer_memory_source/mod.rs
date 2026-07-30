//! Exact helper-backed VEX `VMOVD`/`VMOVQ` memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexScalarIntegerMemoryEncoding,
    X86VexScalarIntegerMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexScalarIntegerMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_scalar_integer_memory_sequence,
    x86_jit_vex_scalar_move_memory_sequence_len, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_VECTOR_SCRATCH_OFFSET,
};
use crate::smir::optimize::{OptLevel, optimize_function};
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x6E7E_D600;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

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
enum ScalarIntegerAlias {
    WidthSelectedLoad,
    WidthSelectedStore,
    WigLoad,
    WigStore,
}

impl ScalarIntegerAlias {
    const ALL: [Self; 4] = [
        Self::WidthSelectedLoad,
        Self::WidthSelectedStore,
        Self::WigLoad,
        Self::WigStore,
    ];

    const fn kind(self) -> X86VexScalarIntegerMemoryKind {
        match self {
            Self::WidthSelectedLoad | Self::WigLoad => X86VexScalarIntegerMemoryKind::Load,
            Self::WidthSelectedStore | Self::WigStore => X86VexScalarIntegerMemoryKind::Store,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::WigLoad => 2,
            _ => 1,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::WidthSelectedLoad => 0x6E,
            Self::WidthSelectedStore | Self::WigLoad => 0x7E,
            Self::WigStore => 0xD6,
        }
    }

    const fn memory_width(self, form: VexForm) -> MemWidth {
        match self {
            Self::WidthSelectedLoad | Self::WidthSelectedStore if form.w() => MemWidth::B8,
            Self::WidthSelectedLoad | Self::WidthSelectedStore => MemWidth::B4,
            Self::WigLoad | Self::WigStore => MemWidth::B8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarIntegerCase {
    alias: ScalarIntegerAlias,
    form: VexForm,
    vector: u8,
    base: u8,
}

impl ScalarIntegerCase {
    fn bytes(self) -> Vec<u8> {
        assert!(self.vector < 16 && self.base < 16);
        let modrm = 0x40 | ((self.vector & 7) << 3) | (self.base & 7);
        let mut bytes = match self.form {
            VexForm::C5 => {
                assert!(self.base < 8, "C5 has no VEX.B extension");
                vec![
                    0xC5,
                    (if self.vector < 8 { 0x80 } else { 0 }) | 0x78 | self.alias.pp(),
                    self.alias.opcode(),
                    modrm,
                ]
            }
            VexForm::C4W0 | VexForm::C4W1 => vec![
                0xC4,
                (if self.vector < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | 0x78 | self.alias.pp(),
                self.alias.opcode(),
                modrm,
            ],
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    const fn memory_width(self) -> MemWidth {
        self.alias.memory_width(self.form)
    }

    const fn element(self) -> VecElementType {
        match self.memory_width() {
            MemWidth::B4 => VecElementType::I32,
            MemWidth::B8 => VecElementType::I64,
            _ => unreachable!(),
        }
    }

    const fn expected_encoding(self) -> X86VexScalarIntegerMemoryEncoding {
        X86VexScalarIntegerMemoryEncoding {
            kind: self.alias.kind(),
            vector: self.vector,
            memory_width: self.memory_width(),
            w: self.form.w(),
            pp: self.alias.pp(),
            opcode: self.alias.opcode(),
        }
    }

    fn expected_scratch_move_bytes(self) -> Vec<u8> {
        let opcode = if self.alias.kind() == X86VexScalarIntegerMemoryKind::Load {
            0x6E
        } else {
            0x7E
        };
        let modrm = 0x80 | ((self.vector & 7) << 3);
        let mut bytes = match self.memory_width() {
            MemWidth::B4 => vec![
                0xC5,
                (if self.vector < 8 { 0x80 } else { 0 }) | 0x79,
                opcode,
                modrm,
            ],
            MemWidth::B8 => vec![
                0xC4,
                (if self.vector < 8 { 0x80 } else { 0 }) | 0x61,
                0xF9,
                opcode,
                modrm,
            ],
            _ => unreachable!(),
        };
        bytes.extend_from_slice(&X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes());
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
) -> Option<X86JitVexScalarIntegerMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_scalar_integer_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexScalarIntegerMemorySequence> {
    classified_at(function, 0, allow_mem)
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let lifted = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("architectural instruction bytes"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    optimize_function(&mut function, level);
    function
}

fn assert_exact_graph(function: &SmirFunction, case: ScalarIntegerCase) {
    let ops = &function.blocks[0].ops;
    let elem = case.element();
    match case.alias.kind() {
        X86VexScalarIntegerMemoryKind::Load => {
            assert_eq!(ops.len(), 4, "{case:?}: {ops:#?}");
            let loaded = match &ops[0].kind {
                OpKind::Load {
                    dst: value @ VReg::Virtual(_),
                    width,
                    sign: SignExtend::Zero,
                    ..
                } => {
                    assert_eq!(*width, case.memory_width(), "{case:?}");
                    *value
                }
                other => panic!("{case:?}: expected scalar load, got {other:?}"),
            };
            let zero = match &ops[1].kind {
                OpKind::Mov {
                    dst: value @ VReg::Virtual(_),
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                } => *value,
                other => panic!("{case:?}: expected zero virtual, got {other:?}"),
            };
            assert_ne!(loaded, zero, "{case:?}");
            assert!(matches!(
                &ops[2].kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: actual_elem,
                    lanes: 1,
                } if *dst == xmm(case.vector) && *scalar == zero && *actual_elem == elem
            ));
            assert!(matches!(
                &ops[3].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem: actual_elem,
                } if *dst == xmm(case.vector)
                    && *vec == xmm(case.vector)
                    && *scalar == loaded
                    && *actual_elem == elem
            ));
        }
        X86VexScalarIntegerMemoryKind::Store => {
            assert_eq!(ops.len(), 2, "{case:?}: {ops:#?}");
            let extracted = match &ops[0].kind {
                OpKind::VExtractLane {
                    dst: value @ VReg::Virtual(_),
                    vec,
                    lane: 0,
                    elem: actual_elem,
                    sign: SignExtend::Zero,
                } => {
                    assert_eq!(*vec, xmm(case.vector), "{case:?}");
                    assert_eq!(*actual_elem, elem, "{case:?}");
                    *value
                }
                other => panic!("{case:?}: expected scalar extraction, got {other:?}"),
            };
            assert!(matches!(
                &ops[1].kind,
                OpKind::Store {
                    src,
                    width,
                    ..
                } if *src == extracted && *width == case.memory_width()
            ));
        }
    }
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        classified(function, true),
        Some(X86JitVexScalarIntegerMemorySequence {
            consumed: ops.len(),
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(classified(function, false), None, "{case:?}");

    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    assert_eq!(
        x86_jit_vex_scalar_move_memory_sequence_len(
            block,
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        ),
        Some(ops.len()),
        "{case:?}"
    );
}

fn lift_case(case: ScalarIntegerCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case);
    function
}

fn assert_feature_requirements(function: &SmirFunction, case: ScalarIntegerCase) {
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

fn lower_case(function: &SmirFunction, case: ScalarIntegerCase) -> (Vec<u8>, usize) {
    assert_exact_graph(function, case);
    assert_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize scalar-integer move");
    let expected = case.expected_scratch_move_bytes();
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?}: missing exact trusted scratch transfer {expected:02X?}"
    );
    let helper = match case.alias.kind() {
        X86VexScalarIntegerMemoryKind::Load => X86_GUEST_VEC_LOAD_FN_OFFSET,
        X86VexScalarIntegerMemoryKind::Store => X86_GUEST_VEC_STORE_FN_OFFSET,
    };
    assert!(
        code.windows(4)
            .any(|window| window == (helper as u32).to_le_bytes()),
        "{case:?}: precise helper offset absent"
    );
    (code, result.entry_offset)
}

#[test]
fn all_96_scanner_cells_admit_and_lower_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for form in VexForm::ALL {
        for alias in ScalarIntegerAlias::ALL {
            for vector in 0..8 {
                let case = ScalarIntegerCase {
                    alias,
                    form,
                    vector,
                    base: if form == VexForm::C5 { 2 } else { 11 },
                };
                cells += 1;
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    lower_case(&function, case);
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(cells, 96);
    assert_eq!(lowered, 96 * LEVELS.len());
}

#[test]
fn high_vectors_aliases_and_complete_address_shapes_remain_exact() {
    let cases: &[(ScalarIntegerCase, &[u8])] = &[
        (
            ScalarIntegerCase {
                alias: ScalarIntegerAlias::WidthSelectedStore,
                form: VexForm::C5,
                vector: 9,
                base: 5,
            },
            &[0x64, 0xC5, 0x79, 0x7E, 0x4D, 0x20],
        ),
        (
            ScalarIntegerCase {
                alias: ScalarIntegerAlias::WidthSelectedLoad,
                form: VexForm::C4W1,
                vector: 14,
                base: 12,
            },
            &[0x65, 0xC4, 0x41, 0xF9, 0x6E, 0x74, 0x24, 0x20],
        ),
        (
            ScalarIntegerCase {
                alias: ScalarIntegerAlias::WigLoad,
                form: VexForm::C4W0,
                vector: 14,
                base: 5,
            },
            &[
                0x67, 0xC4, 0x61, 0x7A, 0x7E, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
            ],
        ),
    ];
    for &(case, bytes) in cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert_exact_graph(&function, case);
            lower_case(&function, case);
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified(function, true),
        None,
        "{name}: exact sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
}

#[test]
fn reserved_source_metadata_and_missing_provenance_fail_closed() {
    let case = ScalarIntegerCase {
        alias: ScalarIntegerAlias::WigStore,
        form: VexForm::C4W1,
        vector: 9,
        base: 11,
    };
    let base = lift_case(case);
    let valid = case.bytes();
    let mut invalid = Vec::new();

    let mut reserved_vvvv = valid.clone();
    reserved_vvvv[2] &= !0x08;
    invalid.push(("reserved VEX.vvvv", reserved_vvvv));
    let mut l1 = valid.clone();
    l1[2] |= 0x04;
    invalid.push(("VEX.L=1", l1));
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
    invalid.push(("wrong map", wrong_map));
    let mut wrong_opcode = valid.clone();
    wrong_opcode[3] = 0xD7;
    invalid.push(("wrong opcode", wrong_opcode));
    let mut wrong_pp = valid.clone();
    wrong_pp[2] = (wrong_pp[2] & !3) | 2;
    invalid.push(("wrong mandatory prefix", wrong_pp));
    let mut register = valid.clone();
    register[4] |= 0xC0;
    register.pop();
    invalid.push(("register form", register));
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
fn classifier_rejects_every_load_graph_field_hint_escape_and_boundary_mutation() {
    let case = ScalarIntegerCase {
        alias: ScalarIntegerAlias::WidthSelectedLoad,
        form: VexForm::C4W1,
        vector: 9,
        base: 11,
    };
    let base = lift_case(case);
    let (loaded, zero) = match (&base.blocks[0].ops[0].kind, &base.blocks[0].ops[1].kind) {
        (OpKind::Load { dst: loaded, .. }, OpKind::Mov { dst: zero, .. }) => (*loaded, *zero),
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    macro_rules! mutate_load {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Load { $field, .. } = &mut function.blocks[0].ops[0].kind else {
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
            let OpKind::Mov { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_zero!("zero destination", dst, x86(X86Reg::Rax));
    mutate_zero!("zero immediate", src, SrcOperand::Imm(1));
    mutate_zero!("zero width", width, OpWidth::W32);

    macro_rules! mutate_clear {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VBroadcast { $field, .. } = &mut function.blocks[0].ops[2].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_clear!("clear destination", dst, xmm(8));
    mutate_clear!("clear scalar", scalar, loaded);
    mutate_clear!("clear element", elem, VecElementType::I32);
    mutate_clear!("clear lanes", lanes, 2);

    macro_rules! mutate_insert {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VInsertLane { $field, .. } = &mut function.blocks[0].ops[3].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_insert!("insert destination", dst, xmm(8));
    mutate_insert!("insert vector", vec, xmm(8));
    mutate_insert!("insert scalar", scalar, zero);
    mutate_insert!("insert lane", lane, 1);
    mutate_insert!("insert element", elem, VecElementType::I32);

    for index in 0..4 {
        let mut function = base.clone();
        function.blocks[0].ops[index].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.alias.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        });
        malformed.push(("invented operation hint", function));
    }

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(("split guest provenance", split_pc));
    for (name, register, id) in [
        ("loaded value escapes", loaded, 0x7F10),
        ("zero escapes", zero, 0x7F11),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops.push(SmirOp::new(
            OpId(id),
            PC + 1,
            OpKind::Mov {
                dst: x86(X86Reg::Rax),
                src: SrcOperand::Reg(register),
                width: OpWidth::W64,
            },
        ));
        malformed.push((name, function));
    }
    for (name, register, id) in [
        ("loaded value redefined", loaded, 0x7F14),
        ("zero redefined", zero, 0x7F15),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops.push(SmirOp::new(
            OpId(id),
            PC + 1,
            OpKind::Mov {
                dst: register,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        malformed.push((name, function));
    }
    let mut alias_virtuals = base.clone();
    let OpKind::Mov { dst, .. } = &mut alias_virtuals.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *dst = loaded;
    let OpKind::VBroadcast { scalar, .. } = &mut alias_virtuals.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *scalar = loaded;
    malformed.push(("loaded and zero virtual alias", alias_virtuals));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F12), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7F13), PC, OpKind::Nop));
    assert_eq!(
        classified_at(&same_pc_head, 1, true),
        None,
        "same-PC head must prevent mid-instruction admission"
    );
}

#[test]
fn classifier_rejects_every_store_graph_field_hint_escape_and_boundary_mutation() {
    let case = ScalarIntegerCase {
        alias: ScalarIntegerAlias::WidthSelectedStore,
        form: VexForm::C4W0,
        vector: 9,
        base: 11,
    };
    let base = lift_case(case);
    let extracted = match base.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
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
    mutate_extract!("extract vector", vec, xmm(8));
    mutate_extract!("extract lane", lane, 1);
    mutate_extract!("extract element", elem, VecElementType::I64);
    mutate_extract!("extract extension", sign, SignExtend::Sign);

    macro_rules! mutate_store {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Store { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_store!("store source", src, x86(X86Reg::Rax));
    mutate_store!(
        "store address",
        addr,
        Address::Direct(VReg::Virtual(VirtualId(0xFF01)))
    );
    mutate_store!("store width", width, MemWidth::B8);

    for index in 0..2 {
        let mut function = base.clone();
        function.blocks[0].ops[index].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.alias.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        });
        malformed.push(("invented operation hint", function));
    }
    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest provenance", split_pc));
    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F20),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(extracted),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value escapes", escaped));
    let mut redefined = base.clone();
    redefined.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F23),
        PC + 1,
        OpKind::Mov {
            dst: extracted,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value redefined", redefined));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F21), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7F22), PC, OpKind::Nop));
    assert_eq!(
        classified_at(&same_pc_head, 1, true),
        None,
        "same-PC head must prevent mid-instruction admission"
    );
}
