//! Exact helper-backed VEX `VMOVSS`/`VMOVSD` memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexScalarFpMemoryEncoding,
    X86VexScalarFpMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexScalarFpMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_scalar_fp_memory_sequence,
    x86_jit_vex_scalar_move_memory_sequence_len, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET,
};
use crate::smir::optimize::{OptLevel, optimize_function};
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x1011_F250;
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
enum ScalarFpFamily {
    Ss,
    Sd,
}

impl ScalarFpFamily {
    const ALL: [Self; 2] = [Self::Ss, Self::Sd];

    const fn pp(self) -> u8 {
        match self {
            Self::Ss => 2,
            Self::Sd => 3,
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::Ss => MemWidth::B4,
            Self::Sd => MemWidth::B8,
        }
    }

    const fn element(self) -> VecElementType {
        match self {
            Self::Ss => VecElementType::F32,
            Self::Sd => VecElementType::F64,
        }
    }

    const fn accepts_width_256(self, width_256: bool) -> bool {
        matches!(self, Self::Sd) || !width_256
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpCase {
    family: ScalarFpFamily,
    kind: X86VexScalarFpMemoryKind,
    form: VexForm,
    width_256: bool,
    vector: u8,
    base: u8,
}

impl ScalarFpCase {
    fn bytes(self) -> Vec<u8> {
        assert!(self.family.accepts_width_256(self.width_256));
        assert!(self.vector < 16 && self.base < 16);
        let modrm = 0x40 | ((self.vector & 7) << 3) | (self.base & 7);
        let opcode = self.opcode();
        let pp = self.family.pp();
        let mut bytes = match self.form {
            VexForm::C5 => {
                assert!(self.base < 8, "C5 has no VEX.B extension");
                vec![
                    0xC5,
                    (if self.vector < 8 { 0x80 } else { 0 })
                        | 0x78
                        | (u8::from(self.width_256) << 2)
                        | pp,
                    opcode,
                    modrm,
                ]
            }
            VexForm::C4W0 | VexForm::C4W1 => vec![
                0xC4,
                (if self.vector < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | 0x78 | (u8::from(self.width_256) << 2) | pp,
                opcode,
                modrm,
            ],
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    const fn opcode(self) -> u8 {
        match self.kind {
            X86VexScalarFpMemoryKind::Load => 0x10,
            X86VexScalarFpMemoryKind::Store => 0x11,
        }
    }

    const fn expected_encoding(self) -> X86VexScalarFpMemoryEncoding {
        X86VexScalarFpMemoryEncoding {
            kind: self.kind,
            vector: self.vector,
            memory_width: self.family.memory_width(),
            width_256: self.width_256,
            w: self.form.w(),
            pp: self.family.pp(),
            opcode: self.opcode(),
        }
    }

    const fn hint(self) -> X86OpHint {
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: match self.family {
                ScalarFpFamily::Ss => X86SsePrefix::Rep,
                ScalarFpFamily::Sd => X86SsePrefix::Repne,
            },
            opcode: self.opcode(),
            width: if self.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            w: self.form.w(),
        }
    }

    fn expected_scratch_move_bytes(self) -> Vec<u8> {
        let opcode = match self.kind {
            X86VexScalarFpMemoryKind::Load => 0x6E,
            X86VexScalarFpMemoryKind::Store => 0x7E,
        };
        let modrm = 0x80 | ((self.vector & 7) << 3);
        let mut bytes = match self.family.memory_width() {
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
) -> Option<X86JitVexScalarFpMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_scalar_fp_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(function: &SmirFunction, allow_mem: bool) -> Option<X86JitVexScalarFpMemorySequence> {
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

fn assert_exact_graph(function: &SmirFunction, case: ScalarFpCase) {
    let ops = &function.blocks[0].ops;
    let source = function
        .x86_instruction_bytes
        .get(&(function.blocks[0].id, PC))
        .expect("exact scalar-move source provenance");
    let source_bytes = source.as_slice();
    let vex_offset = source_bytes
        .iter()
        .position(|byte| matches!(byte, 0xC4 | 0xC5))
        .expect("scalar move uses VEX");
    let p1_offset = if source_bytes[vex_offset] == 0xC5 {
        vex_offset + 1
    } else {
        vex_offset + 2
    };
    let mut expected_hint = case.hint();
    let X86OpHint::VexOp { width, .. } = &mut expected_hint else {
        unreachable!("scalar move always carries a VEX operation hint")
    };
    *width = if source_bytes[p1_offset] & 0x04 == 0 {
        VecWidth::V128
    } else {
        VecWidth::V256
    };
    assert_eq!(ops.len(), 2, "{case:?}: {ops:#?}");
    let intermediate = match case.kind {
        X86VexScalarFpMemoryKind::Load => {
            let loaded = match &ops[0].kind {
                OpKind::Load {
                    dst: value @ VReg::Virtual(_),
                    width,
                    sign: SignExtend::Zero,
                    ..
                } => {
                    assert_eq!(*width, case.family.memory_width(), "{case:?}");
                    *value
                }
                other => panic!("{case:?}: expected scalar load, got {other:?}"),
            };
            assert!(matches!(
                &ops[1].kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem,
                    lanes: 1,
                } if *dst == xmm(case.vector)
                    && *scalar == loaded
                    && *elem == case.family.element()
            ));
            loaded
        }
        X86VexScalarFpMemoryKind::Store => {
            let extracted = match &ops[0].kind {
                OpKind::VExtractLane {
                    dst: value @ VReg::Virtual(_),
                    vec,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                } => {
                    assert_eq!(*vec, xmm(case.vector), "{case:?}");
                    assert_eq!(*elem, case.family.element(), "{case:?}");
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
                } if *src == extracted && *width == case.family.memory_width()
            ));
            extracted
        }
    };
    assert!(matches!(intermediate, VReg::Virtual(_)));
    assert_eq!(ops[0].x86_hint, None, "{case:?}");
    assert_eq!(ops[1].x86_hint, Some(expected_hint), "{case:?}");
    assert!(ops.iter().all(|op| op.guest_pc == PC));
    assert_eq!(
        classified(function, true),
        Some(X86JitVexScalarFpMemorySequence {
            consumed: 2,
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
        Some(2),
        "{case:?}"
    );
}

fn lift_case(case: ScalarFpCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case);
    function
}

fn assert_feature_requirements(function: &SmirFunction, case: ScalarFpCase) {
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

fn lower_case(function: &SmirFunction, case: ScalarFpCase) -> (Vec<u8>, usize) {
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
    let code = lowerer.finalize().expect("finalize scalar floating move");
    let expected = case.expected_scratch_move_bytes();
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?}: missing exact trusted scratch transfer {expected:02X?}"
    );
    let helper = match case.kind {
        X86VexScalarFpMemoryKind::Load => X86_GUEST_VEC_LOAD_FN_OFFSET,
        X86VexScalarFpMemoryKind::Store => X86_GUEST_VEC_STORE_FN_OFFSET,
    };
    assert!(
        code.windows(4)
            .any(|window| window == (helper as u32).to_le_bytes()),
        "{case:?}: precise helper offset absent"
    );
    (code, result.entry_offset)
}

fn scanner_cases() -> impl Iterator<Item = ScalarFpCase> {
    VexForm::ALL.into_iter().flat_map(|form| {
        ScalarFpFamily::ALL.into_iter().flat_map(move |family| {
            [false, true]
                .into_iter()
                .filter(move |&width_256| family.accepts_width_256(width_256))
                .flat_map(move |width_256| {
                    [
                        X86VexScalarFpMemoryKind::Load,
                        X86VexScalarFpMemoryKind::Store,
                    ]
                    .into_iter()
                    .flat_map(move |kind| {
                        (0..8).map(move |vector| ScalarFpCase {
                            family,
                            kind,
                            form,
                            width_256,
                            vector,
                            base: if form == VexForm::C5 { 2 } else { 11 },
                        })
                    })
                })
        })
    })
}

#[test]
fn all_144_scanner_cells_admit_and_lower_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for case in scanner_cases() {
        cells += 1;
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            lower_case(&function, case);
            lowered += 1;
        }
    }
    assert_eq!(cells, 144);
    assert_eq!(lowered, 144 * LEVELS.len());
}

#[test]
fn high_vectors_wig_lig_and_complete_address_shapes_remain_exact() {
    let cases: &[(ScalarFpCase, &[u8])] = &[
        (
            ScalarFpCase {
                family: ScalarFpFamily::Ss,
                kind: X86VexScalarFpMemoryKind::Store,
                form: VexForm::C5,
                width_256: false,
                vector: 9,
                base: 5,
            },
            &[0x64, 0xC5, 0x7A, 0x11, 0x4D, 0x20],
        ),
        (
            ScalarFpCase {
                family: ScalarFpFamily::Sd,
                kind: X86VexScalarFpMemoryKind::Load,
                form: VexForm::C4W1,
                width_256: true,
                vector: 14,
                base: 12,
            },
            &[0x65, 0xC4, 0x41, 0xFF, 0x10, 0x74, 0x24, 0x20],
        ),
        (
            ScalarFpCase {
                family: ScalarFpFamily::Sd,
                kind: X86VexScalarFpMemoryKind::Store,
                form: VexForm::C4W0,
                width_256: false,
                vector: 14,
                base: 5,
            },
            &[
                0x67, 0xC4, 0x61, 0x7B, 0x11, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
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

#[test]
fn excluded_regions_contribute_no_features_and_aarch64_admission_stays_closed() {
    let case = ScalarFpCase {
        family: ScalarFpFamily::Sd,
        kind: X86VexScalarFpMemoryKind::Load,
        form: VexForm::C4W1,
        width_256: true,
        vector: 15,
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

    let upper = X86_GUEST_ZMM_OFFSET + i32::from(case.vector) * 64 + 32;
    let (code, _) = lower_case(&function, case);
    assert!(
        code.windows(4)
            .any(|window| window == (upper as u32).to_le_bytes())
    );
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
fn vmovss_l1_canonicalizes_while_vmovsd_lig_and_invalid_provenance_remain_exact() {
    let ss = ScalarFpCase {
        family: ScalarFpFamily::Ss,
        kind: X86VexScalarFpMemoryKind::Load,
        form: VexForm::C4W1,
        width_256: false,
        vector: 9,
        base: 11,
    };
    let mut ss_l1 = ss.bytes();
    ss_l1[2] |= 0x04;
    for level in LEVELS {
        let canonical = lower_case(&optimize(lift_case(ss), level), ss);
        let l1_function = optimize(lift_bytes(&ss_l1), level);
        assert_exact_graph(&l1_function, ss);
        assert_eq!(lower_case(&l1_function, ss), canonical, "{level:?}");
    }

    let sd_l1 = ScalarFpCase {
        family: ScalarFpFamily::Sd,
        kind: X86VexScalarFpMemoryKind::Load,
        width_256: true,
        ..ss
    };
    let sd_l1_function = lift_case(sd_l1);
    assert_eq!(
        classified(&sd_l1_function, true)
            .expect("VMOVSD is LIG")
            .encoding,
        sd_l1.expected_encoding()
    );

    let store = ScalarFpCase {
        family: ScalarFpFamily::Sd,
        kind: X86VexScalarFpMemoryKind::Store,
        form: VexForm::C4W1,
        width_256: true,
        vector: 9,
        base: 11,
    };
    let base = lift_case(store);
    let valid = store.bytes();
    let mut invalid = Vec::new();

    let mut reserved_vvvv = valid.clone();
    reserved_vvvv[2] &= !0x08;
    invalid.push(("reserved VEX.vvvv", reserved_vvvv));
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
    invalid.push(("wrong map", wrong_map));
    let mut wrong_opcode = valid.clone();
    wrong_opcode[3] = 0x12;
    invalid.push(("wrong opcode", wrong_opcode));
    let mut wrong_pp = valid.clone();
    wrong_pp[2] = (wrong_pp[2] & !3) | 1;
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
fn classifier_rejects_every_load_graph_hint_escape_and_boundary_mutation() {
    let case = ScalarFpCase {
        family: ScalarFpFamily::Sd,
        kind: X86VexScalarFpMemoryKind::Load,
        form: VexForm::C4W1,
        width_256: true,
        vector: 9,
        base: 11,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
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

    macro_rules! mutate_broadcast {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VBroadcast { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_broadcast!("broadcast destination", dst, xmm(8));
    mutate_broadcast!("broadcast scalar", scalar, x86(X86Reg::Rax));
    mutate_broadcast!("broadcast element", elem, VecElementType::F32);
    mutate_broadcast!("broadcast lanes", lanes, 2);

    for (name, hint) in [
        ("missing terminal hint", None),
        (
            "wrong hint map",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::Repne,
                opcode: 0x10,
                width: VecWidth::V256,
                w: true,
            }),
        ),
        (
            "wrong hint prefix",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x10,
                width: VecWidth::V256,
                w: true,
            }),
        ),
        (
            "wrong hint opcode",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0x11,
                width: VecWidth::V256,
                w: true,
            }),
        ),
        (
            "wrong hint width",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0x10,
                width: VecWidth::V128,
                w: true,
            }),
        ),
        (
            "wrong hint W",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0x10,
                width: VecWidth::V256,
                w: false,
            }),
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[1].x86_hint = hint;
        malformed.push((name, function));
    }
    let mut invented_first_hint = base.clone();
    invented_first_hint.blocks[0].ops[0].x86_hint = Some(case.hint());
    malformed.push(("invented first hint", invented_first_hint));
    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest provenance", split_pc));
    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F10),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value escapes", escaped));
    let mut redefined = base.clone();
    redefined.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F11),
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value redefined", redefined));
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
fn classifier_rejects_every_store_graph_hint_escape_and_boundary_mutation() {
    let case = ScalarFpCase {
        family: ScalarFpFamily::Ss,
        kind: X86VexScalarFpMemoryKind::Store,
        form: VexForm::C4W0,
        width_256: false,
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
    mutate_extract!("extract element", elem, VecElementType::F64);
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

    for (name, hint) in [
        ("missing terminal hint", None),
        (
            "wrong hint map",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::Rep,
                opcode: 0x11,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "wrong hint prefix",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0x11,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "wrong hint opcode",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x10,
                width: VecWidth::V128,
                w: false,
            }),
        ),
        (
            "wrong hint width",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x11,
                width: VecWidth::V256,
                w: false,
            }),
        ),
        (
            "wrong hint W",
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x11,
                width: VecWidth::V128,
                w: true,
            }),
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[1].x86_hint = hint;
        malformed.push((name, function));
    }
    let mut invented_first_hint = base.clone();
    invented_first_hint.blocks[0].ops[0].x86_hint = Some(case.hint());
    malformed.push(("invented first hint", invented_first_hint));
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
        OpId(0x7F21),
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
        .push(SmirOp::new(OpId(0x7F22), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7F23), PC, OpKind::Nop));
    assert_eq!(
        classified_at(&same_pc_head, 1, true),
        None,
        "same-PC head must prevent mid-instruction admission"
    );
}
