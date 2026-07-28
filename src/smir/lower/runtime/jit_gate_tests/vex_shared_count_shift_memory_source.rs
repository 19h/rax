//! Exact helper-backed VEX VPSLL*/VPSRL*/VPSRA* count-memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, ShiftOp, SignExtend, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
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

const PC: u64 = 0xBF00;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftKind {
    opcode: u8,
    elem: VecElementType,
    shift: ShiftOp,
}

const KINDS: [ShiftKind; 8] = [
    ShiftKind {
        opcode: 0xD1,
        elem: VecElementType::I16,
        shift: ShiftOp::Lsr,
    },
    ShiftKind {
        opcode: 0xD2,
        elem: VecElementType::I32,
        shift: ShiftOp::Lsr,
    },
    ShiftKind {
        opcode: 0xD3,
        elem: VecElementType::I64,
        shift: ShiftOp::Lsr,
    },
    ShiftKind {
        opcode: 0xE1,
        elem: VecElementType::I16,
        shift: ShiftOp::Asr,
    },
    ShiftKind {
        opcode: 0xE2,
        elem: VecElementType::I32,
        shift: ShiftOp::Asr,
    },
    ShiftKind {
        opcode: 0xF1,
        elem: VecElementType::I16,
        shift: ShiftOp::Lsl,
    },
    ShiftKind {
        opcode: 0xF2,
        elem: VecElementType::I32,
        shift: ShiftOp::Lsl,
    },
    ShiftKind {
        opcode: 0xF3,
        elem: VecElementType::I64,
        shift: ShiftOp::Lsl,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    Vex2,
    Vex3W0,
    Vex3W1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SharedCountShiftMemoryCase {
    kind: ShiftKind,
    width: VecWidth,
    form: VexForm,
    alias: bool,
}

impl SharedCountShiftMemoryCase {
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

    const fn source(self) -> u8 {
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
            .find(|index| *index != self.destination() && *index != self.source())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source, base) = self.operands();
        let l = u8::from(self.width == VecWidth::V256);
        match self.form {
            VexForm::Vex2 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 }) | (((!source) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode,
                0x40 | ((destination & 7) << 3) | base,
                DISP as u8,
            ],
            VexForm::Vex3W0 | VexForm::Vex3W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.w()) << 7) | (((!source) & 0x0F) << 3) | (l << 2) | 1,
                self.kind.opcode,
                0x40 | ((destination & 7) << 3) | (base & 7),
                DISP as u8,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source = self.source();
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source) & 0x0F) << 3) | (l << 2) | 1,
            self.kind.opcode,
            0xC0 | ((destination & 7) << 3) | scratch,
        ]
    }
}

fn all_cases() -> Vec<SharedCountShiftMemoryCase> {
    let mut cases = Vec::new();
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in [VexForm::Vex2, VexForm::Vex3W0, VexForm::Vex3W1] {
                for alias in [false, true] {
                    cases.push(SharedCountShiftMemoryCase {
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
        _ => unreachable!("VEX packed shared-count shifts have only 128-/256-bit destinations"),
    })
}

fn expected_address(case: SharedCountShiftMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: SharedCountShiftMemoryCase) {
    let [load, extract, consumer] = ops else {
        panic!("expected VLoad + VExtractLane + X86PackedShift for {case:?}, got {ops:?}")
    };
    assert_eq!(
        load.x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}"
    );
    let loaded = match &load.kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width: VecWidth::V128,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected virtual 128-bit VLoad, got {other:?}"),
    };

    assert_eq!(extract.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(extract.x86_hint, None, "{case:?}");
    let count = match &extract.kind {
        OpKind::VExtractLane {
            dst: count @ VReg::Virtual(_),
            vec,
            lane: 0,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, loaded, "{case:?}");
            *count
        }
        other => panic!("{case:?}: expected low-64 count extraction, got {other:?}"),
    };

    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(consumer.x86_hint, None, "{case:?}");
    let OpKind::X86PackedShift {
        dst,
        src,
        count: consumer_count,
        width,
        elem,
        shift,
    } = &consumer.kind
    else {
        panic!("{case:?}: expected X86PackedShift, got {consumer:?}")
    };
    assert_eq!(*dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(*src, vector(case.source(), case.width), "{case:?}");
    assert_eq!(*consumer_count, count, "{case:?}");
    assert_eq!(*width, case.width, "{case:?}");
    assert_eq!(*elem, case.kind.elem, "{case:?}");
    assert_eq!(*shift, case.kind.shift, "{case:?}");
}

fn lift_case(case: SharedCountShiftMemoryCase) -> SmirFunction {
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

fn lower(function: &SmirFunction, case: SharedCountShiftMemoryCase) -> (Vec<u8>, usize) {
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
        panic!("helper-backed VEX shared-count lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX shared-count shift"),
        result.entry_offset,
    )
}

#[test]
fn every_kind_width_form_alias_and_optimizer_shape_is_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 96);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);

            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(actual.consumed, 3, "{level:?} {case:?}");
            assert_eq!(
                actual.memory_size,
                VecWidth::V128.bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(actual.destination, case.destination(), "{level:?} {case:?}");
            assert_eq!(actual.source1, case.source(), "{level:?} {case:?}");
            assert_eq!(actual.width, case.width, "{level:?} {case:?}");
            assert_eq!(actual.map, X86VecMap::Map0F, "{level:?} {case:?}");
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
    assert_eq!(lowered, 96 * LEVELS.len());
}

#[test]
fn register_rewrite_matches_independent_llvm_23_encodings() {
    let find = |opcode| {
        KINDS
            .into_iter()
            .find(|kind| kind.opcode == opcode)
            .unwrap()
    };
    for (case, expected) in [
        (
            SharedCountShiftMemoryCase {
                kind: find(0xF1),
                width: VecWidth::V128,
                form: VexForm::Vex3W0,
                alias: false,
            },
            &[0xC5, 0xF1, 0xF1, 0xC2][..],
        ),
        (
            SharedCountShiftMemoryCase {
                kind: find(0xE2),
                width: VecWidth::V256,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC5, 0x35, 0xE2, 0xF0][..],
        ),
        (
            SharedCountShiftMemoryCase {
                kind: find(0xD3),
                width: VecWidth::V128,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC5, 0x31, 0xD3, 0xF0][..],
        ),
        (
            SharedCountShiftMemoryCase {
                kind: find(0xF3),
                width: VecWidth::V256,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC5, 0x35, 0xF3, 0xF0][..],
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
    let case = SharedCountShiftMemoryCase {
        kind: KINDS[4],
        width: VecWidth::V128,
        form: VexForm::Vex3W0,
        alias: false,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        ref other => panic!("expected load, got {other:?}"),
    };
    let count = match base.blocks[0].ops[1].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        ref other => panic!("expected count extraction, got {other:?}"),
    };

    assert_mutation_rejected(&base, "loaded vector used twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VExtractLane {
                dst: VReg::Virtual(VirtualId(0xFFFD)),
                vec: loaded,
                lane: 1,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
            },
        ));
    });
    assert_mutation_rejected(&base, "loaded vector defined twice", |function| {
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
    assert_mutation_rejected(&base, "scalar count used twice", |function| {
        let mut duplicate = function.blocks[0].ops[2].clone();
        duplicate.id = OpId(3);
        function.blocks[0].ops.push(duplicate);
    });
    assert_mutation_rejected(&base, "scalar count defined twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(3),
            PC,
            OpKind::VExtractLane {
                dst: count,
                vec: loaded,
                lane: 0,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
            },
        ));
    });
    assert_mutation_rejected(&base, "load has no alignment hint", |function| {
        function.blocks[0].ops[0].x86_hint = None;
    });
    assert_mutation_rejected(&base, "load has aligned semantics", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    });
    assert_mutation_rejected(&base, "load width is not mem128", |function| {
        if let OpKind::VLoad { width, .. } = &mut function.blocks[0].ops[0].kind {
            *width = VecWidth::V256;
        }
    });
    assert_mutation_rejected(&base, "virtual address component", |function| {
        if let OpKind::VLoad { addr, .. } = &mut function.blocks[0].ops[0].kind {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
    });
    assert_mutation_rejected(&base, "extract has a different guest PC", |function| {
        function.blocks[0].ops[1].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "extract has an encoding hint", |function| {
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    });
    assert_mutation_rejected(&base, "extract bypasses loaded vector", |function| {
        if let OpKind::VExtractLane { vec, .. } = &mut function.blocks[0].ops[1].kind {
            *vec = vector(2, VecWidth::V128);
        }
    });
    assert_mutation_rejected(&base, "extract uses lane one", |function| {
        if let OpKind::VExtractLane { lane, .. } = &mut function.blocks[0].ops[1].kind {
            *lane = 1;
        }
    });
    assert_mutation_rejected(&base, "extract uses 32-bit elements", |function| {
        if let OpKind::VExtractLane { elem, .. } = &mut function.blocks[0].ops[1].kind {
            *elem = VecElementType::I32;
        }
    });
    assert_mutation_rejected(&base, "extract sign-extends count", |function| {
        if let OpKind::VExtractLane { sign, .. } = &mut function.blocks[0].ops[1].kind {
            *sign = SignExtend::Sign;
        }
    });
    assert_mutation_rejected(&base, "extract writes architectural state", |function| {
        if let OpKind::VExtractLane { dst, .. } = &mut function.blocks[0].ops[1].kind {
            *dst = x86(X86Reg::Rax);
        }
    });
    assert_mutation_rejected(&base, "consumer has a different guest PC", |function| {
        function.blocks[0].ops[2].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "consumer has an encoding hint", |function| {
        function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.kind.opcode,
            width: case.width,
            w: false,
        });
    });
    assert_mutation_rejected(&base, "consumer bypasses extracted count", |function| {
        if let OpKind::X86PackedShift { count, .. } = &mut function.blocks[0].ops[2].kind {
            *count = x86(X86Reg::Xmm(2));
        }
    });
    assert_mutation_rejected(&base, "wrong packed element width", |function| {
        if let OpKind::X86PackedShift { elem, .. } = &mut function.blocks[0].ops[2].kind {
            *elem = VecElementType::I16;
        }
    });
    assert_mutation_rejected(&base, "wrong shift direction", |function| {
        if let OpKind::X86PackedShift { shift, .. } = &mut function.blocks[0].ops[2].kind {
            *shift = ShiftOp::Lsl;
        }
    });
    assert_mutation_rejected(&base, "consumer width mismatch", |function| {
        if let OpKind::X86PackedShift { width, .. } = &mut function.blocks[0].ops[2].kind {
            *width = VecWidth::V256;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only source", |function| {
        if let OpKind::X86PackedShift { src, .. } = &mut function.blocks[0].ops[2].kind {
            *src = vector(16, VecWidth::V128);
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only destination", |function| {
        if let OpKind::X86PackedShift { dst, .. } = &mut function.blocks[0].ops[2].kind {
            *dst = vector(16, VecWidth::V128);
        }
    });
    assert_mutation_rejected(
        &base,
        "destination register namespace mismatch",
        |function| {
            if let OpKind::X86PackedShift { dst, .. } = &mut function.blocks[0].ops[2].kind {
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
    assert_mutation_rejected(&base, "encoded source mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] |= 0x04;
    assert_mutation_rejected(&base, "encoded destination width mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[3] = 0xD2;
    assert_mutation_rejected(&base, "encoded shift semantics mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[1] = (bytes[1] & !0x1F) | 2;
    assert_mutation_rejected(&base, "encoded map mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] = (bytes[2] & !3) | 2;
    assert_mutation_rejected(&base, "encoded mandatory-prefix mismatch", |function| {
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

fn lane_mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn shift_lane(value: u64, bits: u32, amount: u64, shift: ShiftOp) -> u64 {
    let mask = lane_mask(bits);
    let value = value & mask;
    if amount >= u64::from(bits) {
        return match shift {
            ShiftOp::Asr if value & (1u64 << (bits - 1)) != 0 => mask,
            ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr => 0,
            _ => unreachable!("VEX shared-count shifts use only LSL/LSR/ASR"),
        };
    }
    let amount = amount as u32;
    match shift {
        ShiftOp::Lsl => (value << amount) & mask,
        ShiftOp::Lsr => value >> amount,
        ShiftOp::Asr => {
            let signed = if bits == 64 {
                value as i64
            } else {
                ((value << (64 - bits)) as i64) >> (64 - bits)
            };
            ((signed >> amount) as u64) & mask
        }
        _ => unreachable!("VEX shared-count shifts use only LSL/LSR/ASR"),
    }
}

fn count_for(case: SharedCountShiftMemoryCase, ordinal: usize) -> u64 {
    let bits = u64::from(case.kind.elem.bytes()) * 8;
    [0, 1, bits - 1, bits, bits + 1, u64::MAX][ordinal % 6]
}

fn operand_vectors(case: SharedCountShiftMemoryCase, ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let mut source = [0xC3; 64];
    let bits = u32::from(case.kind.elem.bytes()) * 8;
    let mask = lane_mask(bits);
    let lanes = usize::try_from(case.width.lanes(case.kind.elem))
        .expect("packed integer lane count fits usize");
    for lane in 0..lanes {
        let lane_u64 = lane as u64;
        let mut value = 0x0102_0408_1020_4081u64.rotate_left((lane_u64 * 7) as u32)
            ^ lane_u64.wrapping_mul(0x1111_2222_3333_4444);
        value &= mask;
        if lane & 1 != 0 {
            value |= 1u64 << (bits - 1);
        } else {
            value &= !(1u64 << (bits - 1));
        }
        write_lane(&mut source, case.kind.elem, lane, value);
    }
    let count = count_for(case, ordinal);
    let count_vector = [
        count,
        !count,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0xA55A_F00F_6996_3CC3,
        0xC3C3_5A5A_9669_F00F,
        0x1111_2222_3333_4444,
        0x8888_7777_6666_5555,
    ];
    (bytes_to_words(source), count_vector)
}

fn model_result(
    case: SharedCountShiftMemoryCase,
    source: [u64; 8],
    count_vector: [u64; 8],
) -> [u64; 8] {
    let source = words_to_bytes(source);
    let mut result = [0; 64];
    let bits = u32::from(case.kind.elem.bytes()) * 8;
    let lanes = usize::try_from(case.width.lanes(case.kind.elem))
        .expect("packed integer lane count fits usize");
    for lane in 0..lanes {
        write_lane(
            &mut result,
            case.kind.elem,
            lane,
            shift_lane(
                read_lane(&source, case.kind.elem, lane),
                bits,
                count_vector[0],
                case.kind.shift,
            ),
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
        || size != VecWidth::V128.bytes()
    {
        return 0;
    }

    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..2].copy_from_slice(&context.value[..2]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: SharedCountShiftMemoryCase, ordinal: usize) -> GuestRegs {
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
    let (source, _) = operand_vectors(case, ordinal);
    registers.zmm[usize::from(case.source())] = source;
    if case.destination() != case.source() {
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
    case: SharedCountShiftMemoryCase,
    count_vector: [u64; 8],
) -> GuestRegs {
    let source = registers.zmm[usize::from(case.source())];
    registers.zmm[usize::from(case.destination())] = model_result(case, source, count_vector);
    registers.vector_scratch =
        std::array::from_fn(|word| if word < 2 { count_vector[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    count_vector: [u64; 8],
    address: u64,
    case: SharedCountShiftMemoryCase,
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
    let bytes = words_to_bytes(count_vector);
    memory.load(address as usize, &bytes[..VecWidth::V128.bytes() as usize]);
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
fn native_shared_count_shifts_match_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX shared-count memory differential: host lacks AVX");
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
            let (_, count_vector) = operand_vectors(case, ordinal);

            let mut context = VectorMemoryContext {
                value: count_vector,
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
            let mut expected = expected_success(registers, case, count_vector);

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
            assert_eq!(
                context.last_size,
                VecWidth::V128.bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function,
                &initial,
                &expected,
                count_vector,
                address,
                case,
                level,
            );
            successes += 1;

            let mut context = VectorMemoryContext {
                value: count_vector,
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
                VecWidth::V128.bytes(),
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
        "executed {successes} successful and {faults} faulting native VEX shared-count memory cases"
    );
}
