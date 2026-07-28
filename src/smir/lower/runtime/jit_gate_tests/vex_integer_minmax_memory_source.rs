//! Exact helper-backed VEX VPMIN*/VPMAX* memory-source coverage.

use std::collections::HashMap;

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
    x86_jit_vex_binary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xBD80;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MinMaxKind {
    map: u8,
    opcode: u8,
    elem: VecElementType,
    minimum: bool,
    signed: bool,
}

impl MinMaxKind {
    const fn map(self) -> X86VecMap {
        match self.map {
            1 => X86VecMap::Map0F,
            2 => X86VecMap::Map0F38,
            _ => unreachable!(),
        }
    }

    const fn supports_vex2(self) -> bool {
        self.map == 1
    }

    const fn compare_condition(self) -> VecCmpCond {
        match (self.minimum, self.signed) {
            (true, true) => VecCmpCond::Lt,
            (true, false) => VecCmpCond::Ltu,
            (false, true) => VecCmpCond::Gt,
            (false, false) => VecCmpCond::Gtu,
        }
    }
}

const KINDS: [MinMaxKind; 12] = [
    MinMaxKind {
        map: 1,
        opcode: 0xDA,
        elem: VecElementType::I8,
        minimum: true,
        signed: false,
    },
    MinMaxKind {
        map: 1,
        opcode: 0xDE,
        elem: VecElementType::I8,
        minimum: false,
        signed: false,
    },
    MinMaxKind {
        map: 1,
        opcode: 0xEA,
        elem: VecElementType::I16,
        minimum: true,
        signed: true,
    },
    MinMaxKind {
        map: 1,
        opcode: 0xEE,
        elem: VecElementType::I16,
        minimum: false,
        signed: true,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x38,
        elem: VecElementType::I8,
        minimum: true,
        signed: true,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x39,
        elem: VecElementType::I32,
        minimum: true,
        signed: true,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3A,
        elem: VecElementType::I16,
        minimum: true,
        signed: false,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3B,
        elem: VecElementType::I32,
        minimum: true,
        signed: false,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3C,
        elem: VecElementType::I8,
        minimum: false,
        signed: true,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3D,
        elem: VecElementType::I32,
        minimum: false,
        signed: true,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3E,
        elem: VecElementType::I16,
        minimum: false,
        signed: false,
    },
    MinMaxKind {
        map: 2,
        opcode: 0x3F,
        elem: VecElementType::I32,
        minimum: false,
        signed: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    Vex2,
    Vex3W0,
    Vex3W1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerMinMaxMemoryCase {
    kind: MinMaxKind,
    width: VecWidth,
    form: VexForm,
    alias: bool,
}

impl IntegerMinMaxMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match (self.form, self.alias) {
            (VexForm::Vex2, false) => (14, 9, 3),
            (VexForm::Vex2, true) => (15, 15, 3),
            (VexForm::Vex3W0, false) => (0, 1, 3),
            (VexForm::Vex3W0, true) => (0, 0, 3),
            (VexForm::Vex3W1, false) => (14, 9, 11),
            (VexForm::Vex3W1, true) => (15, 15, 11),
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

    const fn w(self) -> bool {
        matches!(self.form, VexForm::Vex3W1)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source1())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let l = u8::from(self.width == VecWidth::V256);
        match self.form {
            VexForm::Vex2 => {
                assert!(self.kind.supports_vex2());
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 })
                        | (((!source1) & 0x0F) << 3)
                        | (l << 2)
                        | 1,
                    self.kind.opcode,
                    0x40 | ((destination & 7) << 3) | base,
                    DISP as u8,
                ]
            }
            VexForm::Vex3W0 | VexForm::Vex3W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | self.kind.map,
                (u8::from(self.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode,
                0x40 | ((destination & 7) << 3) | (base & 7),
                DISP as u8,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        if self.kind.supports_vex2() {
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | 1,
                self.kind.opcode,
                0xC0 | ((destination & 7) << 3) | scratch,
            ]
        } else {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 }) | 0x40 | 0x20 | self.kind.map,
                (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode,
                0xC0 | ((destination & 7) << 3) | scratch,
            ]
        }
    }
}

fn all_cases() -> Vec<IntegerMinMaxMemoryCase> {
    let mut cases = Vec::new();
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in [VexForm::Vex2, VexForm::Vex3W0, VexForm::Vex3W1] {
                if form == VexForm::Vex2 && !kind.supports_vex2() {
                    continue;
                }
                for alias in [false, true] {
                    cases.push(IntegerMinMaxMemoryCase {
                        kind,
                        width,
                        form,
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
        _ => unreachable!("VEX packed-integer min/max has only 128-/256-bit forms"),
    })
}

fn expected_address(case: IntegerMinMaxMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: IntegerMinMaxMemoryCase) {
    let [load, compare, select_op] = ops else {
        panic!("{case:?}: expected exact VLoad + VCmp + VBitSelect chain, got {ops:?}")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let loaded = match &load.kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };

    assert_eq!(compare.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(compare.x86_hint, None, "{case:?}");
    let (mask, source1) = match &compare.kind {
        OpKind::VCmp {
            dst: mask @ VReg::Virtual(_),
            src1,
            src2,
            cond,
            elem,
            lanes,
        } => {
            assert_ne!(*mask, loaded, "{case:?}");
            assert_eq!(*src1, vector(case.source1(), case.width), "{case:?}");
            assert_eq!(*src2, loaded, "{case:?}");
            assert_eq!(*cond, case.kind.compare_condition(), "{case:?}");
            assert_eq!(*elem, case.kind.elem, "{case:?}");
            assert_eq!(*lanes, case.width.lanes(case.kind.elem) as u8, "{case:?}");
            (*mask, *src1)
        }
        other => panic!("{case:?}: expected VCmp, got {other:?}"),
    };

    assert_eq!(select_op.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(select_op.x86_hint, None, "{case:?}");
    let OpKind::VBitSelect {
        dst,
        mask: select_mask,
        src_true,
        src_false,
        width,
    } = &select_op.kind
    else {
        panic!("{case:?}: expected VBitSelect, got {select_op:?}")
    };
    assert_eq!(*dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(*select_mask, mask, "{case:?}");
    assert_eq!(*src_true, source1, "{case:?}");
    assert_eq!(*src_false, loaded, "{case:?}");
    assert_eq!(*width, case.width, "{case:?}");
}

fn lift_case(case: IntegerMinMaxMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_chain(&result.ops, case);

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

fn lower(function: &SmirFunction, case: IntegerMinMaxMemoryCase) -> (Vec<u8>, usize) {
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
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("helper-backed VEX VPMIN*/VPMAX* lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VPMIN*/VPMAX*"),
        result.entry_offset,
    )
}

#[test]
fn every_kind_width_form_alias_and_optimizer_shape_is_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 112);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);

            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(actual.consumed, 3, "{level:?} {case:?}");
            assert_eq!(actual.memory_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(actual.destination, case.destination(), "{level:?} {case:?}");
            assert_eq!(actual.source1, case.source1(), "{level:?} {case:?}");
            assert_eq!(actual.width, case.width, "{level:?} {case:?}");
            assert_eq!(actual.map, case.kind.map(), "{level:?} {case:?}");
            assert_eq!(actual.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(actual.opcode, case.kind.opcode, "{level:?} {case:?}");
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
    assert_eq!(lowered, 112 * LEVELS.len());
}

#[test]
fn register_rewrite_matches_independent_llvm_23_encodings() {
    let find = |map, opcode| {
        KINDS
            .into_iter()
            .find(|kind| kind.map == map && kind.opcode == opcode)
            .unwrap()
    };
    for (case, expected) in [
        (
            IntegerMinMaxMemoryCase {
                kind: find(1, 0xDA),
                width: VecWidth::V128,
                form: VexForm::Vex3W0,
                alias: false,
            },
            &[0xC5, 0xF1, 0xDA, 0xC2][..],
        ),
        (
            IntegerMinMaxMemoryCase {
                kind: find(1, 0xEE),
                width: VecWidth::V256,
                form: VexForm::Vex3W0,
                alias: false,
            },
            &[0xC5, 0xF5, 0xEE, 0xC2][..],
        ),
        (
            IntegerMinMaxMemoryCase {
                kind: find(2, 0x38),
                width: VecWidth::V128,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC4, 0x62, 0x31, 0x38, 0xF0][..],
        ),
        (
            IntegerMinMaxMemoryCase {
                kind: find(2, 0x3F),
                width: VecWidth::V256,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC4, 0x62, 0x35, 0x3F, 0xF0][..],
        ),
    ] {
        assert_eq!(case.emitted_bytes(), expected, "{case:?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed chain"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed chain"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed chain"
    );
}

fn assert_mutation_rejected(
    base: &SmirFunction,
    name: &str,
    mutate: impl FnOnce(&mut SmirFunction),
) {
    let mut function = base.clone();
    mutate(&mut function);
    assert_rejected(name, &function);
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_lowerer_fail_closed_for_every_chain_invariant() {
    let case = IntegerMinMaxMemoryCase {
        kind: KINDS[5],
        width: VecWidth::V128,
        form: VexForm::Vex3W0,
        alias: false,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mask = match base.blocks[0].ops[1].kind {
        OpKind::VCmp { dst, .. } => dst,
        _ => unreachable!(),
    };

    assert_mutation_rejected(&base, "loaded value used three times", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VMov {
                dst: vector(4, VecWidth::V128),
                src: loaded,
                width: VecWidth::V128,
            },
        ));
    });
    assert_mutation_rejected(&base, "loaded value defined twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VLoad {
                dst: loaded,
                addr: expected_address(case),
                width: VecWidth::V128,
            },
        ));
    });
    assert_mutation_rejected(&base, "mask used twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VMov {
                dst: vector(4, VecWidth::V128),
                src: mask,
                width: VecWidth::V128,
            },
        ));
    });
    assert_mutation_rejected(&base, "mask defined twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VCmp {
                dst: mask,
                src1: vector(1, VecWidth::V128),
                src2: loaded,
                cond: VecCmpCond::Lt,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
    });
    assert_mutation_rejected(&base, "load carries an encoding hint", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x39,
            width: VecWidth::V128,
            w: false,
        });
    });
    assert_mutation_rejected(&base, "load/consumer width mismatch", |function| {
        if let OpKind::VLoad { width, .. } = &mut function.blocks[0].ops[0].kind {
            *width = VecWidth::V256;
        }
    });
    assert_mutation_rejected(&base, "virtual address component", |function| {
        if let OpKind::VLoad { addr, .. } = &mut function.blocks[0].ops[0].kind {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
    });
    assert_mutation_rejected(&base, "compare has a different guest PC", |function| {
        function.blocks[0].ops[1].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "select has a different guest PC", |function| {
        function.blocks[0].ops[2].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "compare carries an encoding hint", |function| {
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x39,
            width: VecWidth::V128,
            w: false,
        });
    });
    assert_mutation_rejected(&base, "select carries an encoding hint", |function| {
        function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x39,
            width: VecWidth::V128,
            w: false,
        });
    });
    assert_mutation_rejected(&base, "compare bypasses loaded value", |function| {
        if let OpKind::VCmp { src2, .. } = &mut function.blocks[0].ops[1].kind {
            *src2 = vector(2, VecWidth::V128);
        }
    });
    assert_mutation_rejected(&base, "wrong comparison predicate", |function| {
        if let OpKind::VCmp { cond, .. } = &mut function.blocks[0].ops[1].kind {
            *cond = VecCmpCond::Gtu;
        }
    });
    assert_mutation_rejected(&base, "wrong comparison element", |function| {
        if let OpKind::VCmp { elem, .. } = &mut function.blocks[0].ops[1].kind {
            *elem = VecElementType::I16;
        }
    });
    assert_mutation_rejected(&base, "wrong comparison lane count", |function| {
        if let OpKind::VCmp { lanes, .. } = &mut function.blocks[0].ops[1].kind {
            *lanes -= 1;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only first source", |function| {
        if let OpKind::VCmp { src1, .. } = &mut function.blocks[0].ops[1].kind {
            *src1 = vector(16, VecWidth::V128);
        }
    });
    assert_mutation_rejected(&base, "selection uses a different mask", |function| {
        if let OpKind::VBitSelect { mask, .. } = &mut function.blocks[0].ops[2].kind {
            *mask = VReg::Virtual(VirtualId(0xFFFE));
        }
    });
    assert_mutation_rejected(&base, "selection swaps true source", |function| {
        if let OpKind::VBitSelect { src_true, .. } = &mut function.blocks[0].ops[2].kind {
            *src_true = loaded;
        }
    });
    assert_mutation_rejected(
        &base,
        "selection bypasses loaded false source",
        |function| {
            if let OpKind::VBitSelect { src_false, .. } = &mut function.blocks[0].ops[2].kind {
                *src_false = vector(2, VecWidth::V128);
            }
        },
    );
    assert_mutation_rejected(&base, "selection width mismatch", |function| {
        if let OpKind::VBitSelect { width, .. } = &mut function.blocks[0].ops[2].kind {
            *width = VecWidth::V256;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only destination", |function| {
        if let OpKind::VBitSelect { dst, .. } = &mut function.blocks[0].ops[2].kind {
            *dst = vector(16, VecWidth::V128);
        }
    });
    assert_mutation_rejected(
        &base,
        "destination register namespace mismatch",
        |function| {
            if let OpKind::VBitSelect { dst, .. } = &mut function.blocks[0].ops[2].kind {
                *dst = x86(X86Reg::Ymm(case.destination()));
            }
        },
    );
    assert_mutation_rejected(&base, "missing instruction-byte provenance", |function| {
        function.x86_instruction_bytes.clear();
    });

    let mut bytes = case.bytes();
    bytes[4] = (bytes[4] & !0x38) | 0x30;
    assert_mutation_rejected(&base, "encoded destination mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] = (bytes[2] & !0x78) | (((!2u8) & 0x0F) << 3);
    assert_mutation_rejected(&base, "encoded first-source mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] |= 0x04;
    assert_mutation_rejected(&base, "encoded width mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[3] = 0x3B;
    assert_mutation_rejected(&base, "encoded signedness mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
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

fn lane_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I8 => 0xFF,
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        _ => unreachable!("VEX packed-integer min/max element"),
    }
}

fn read_lane(bytes: &[u8], elem: VecElementType, lane: usize) -> u64 {
    let size = elem.bytes() as usize;
    let mut raw = [0; 8];
    raw[..size].copy_from_slice(&bytes[lane * size..lane * size + size]);
    u64::from_le_bytes(raw)
}

fn write_lane(bytes: &mut [u8], elem: VecElementType, lane: usize, value: u64) {
    let size = elem.bytes() as usize;
    bytes[lane * size..lane * size + size].copy_from_slice(&value.to_le_bytes()[..size]);
}

fn signed_lane(value: u64, elem: VecElementType) -> i64 {
    let shift = 64 - elem.bytes() * 8;
    ((value << shift) as i64) >> shift
}

fn operand_vectors(case: IntegerMinMaxMemoryCase) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    let mask = lane_mask(case.kind.elem);
    let sign = 1u64 << (case.kind.elem.bytes() * 8 - 1);
    let patterns = [
        (0, mask),
        (mask, 0),
        (sign, sign - 1),
        (sign - 1, sign),
        (1, 1),
        (mask, sign),
        (sign, mask),
        (0x5555_5555 & mask, 0xAAAA_AAAA & mask),
    ];
    let lanes = usize::try_from(case.width.lanes(case.kind.elem))
        .expect("packed integer lane count fits usize");
    for lane in 0..lanes {
        let (first, second) = patterns[lane % patterns.len()];
        write_lane(&mut source1, case.kind.elem, lane, first);
        write_lane(&mut source2, case.kind.elem, lane, second);
    }
    (bytes_to_words(source1), bytes_to_words(source2))
}

fn model_result(case: IntegerMinMaxMemoryCase, source1: [u64; 8], source2: [u64; 8]) -> [u64; 8] {
    let source1 = words_to_bytes(source1);
    let source2 = words_to_bytes(source2);
    let mut result = [0; 64];
    let lanes = usize::try_from(case.width.lanes(case.kind.elem))
        .expect("packed integer lane count fits usize");
    for lane in 0..lanes {
        let first = read_lane(&source1, case.kind.elem, lane);
        let second = read_lane(&source2, case.kind.elem, lane);
        let first_wins = if case.kind.signed {
            let first = signed_lane(first, case.kind.elem);
            let second = signed_lane(second, case.kind.elem);
            if case.kind.minimum {
                first < second
            } else {
                first > second
            }
        } else if case.kind.minimum {
            first < second
        } else {
            first > second
        };
        write_lane(
            &mut result,
            case.kind.elem,
            lane,
            if first_wins { first } else { second },
        );
    }
    bytes_to_words(result)
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
fn full_guest_regs(case: IntegerMinMaxMemoryCase, ordinal: usize) -> GuestRegs {
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
    case: IntegerMinMaxMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1())];
    registers.zmm[usize::from(case.destination())] = model_result(case, source1, source2);
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
    case: IntegerMinMaxMemoryCase,
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
fn native_integer_minmax_matches_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX VPMIN*/VPMAX* memory differential: host lacks AVX");
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
        "executed {successes} successful and {faults} faulting native VEX \
         VPMIN*/VPMAX* memory cases"
    );
}
