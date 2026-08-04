//! Exact helper-backed EVEX packed-integer comparison/test memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, SrcOperand, VReg,
    VecCmpCond, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedIntegerMaskMemoryReplay,
    X86EvexPackedIntegerMaskOperation, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedIntegerMaskMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_integer_mask_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0x26E4;
const DISP8: i32 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WControl {
    Fixed(bool),
    Ignored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntegerMaskSemantic {
    FixedCompare(VecCmpCond),
    ImmediateCompare { signed: bool },
    Test { inverted: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerMaskKind {
    name: &'static str,
    map: u8,
    pp: u8,
    opcode: u8,
    elem: VecElementType,
    w: WControl,
    semantic: IntegerMaskSemantic,
}

const KINDS: [IntegerMaskKind; 24] = [
    IntegerMaskKind {
        name: "VPCMPGTB",
        map: 1,
        pp: 1,
        opcode: 0x64,
        elem: VecElementType::I8,
        w: WControl::Ignored,
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Gt),
    },
    IntegerMaskKind {
        name: "VPCMPGTW",
        map: 1,
        pp: 1,
        opcode: 0x65,
        elem: VecElementType::I16,
        w: WControl::Ignored,
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Gt),
    },
    IntegerMaskKind {
        name: "VPCMPGTD",
        map: 1,
        pp: 1,
        opcode: 0x66,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Gt),
    },
    IntegerMaskKind {
        name: "VPCMPEQB",
        map: 1,
        pp: 1,
        opcode: 0x74,
        elem: VecElementType::I8,
        w: WControl::Ignored,
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Eq),
    },
    IntegerMaskKind {
        name: "VPCMPEQW",
        map: 1,
        pp: 1,
        opcode: 0x75,
        elem: VecElementType::I16,
        w: WControl::Ignored,
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Eq),
    },
    IntegerMaskKind {
        name: "VPCMPEQD",
        map: 1,
        pp: 1,
        opcode: 0x76,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Eq),
    },
    IntegerMaskKind {
        name: "VPCMPEQQ",
        map: 2,
        pp: 1,
        opcode: 0x29,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Eq),
    },
    IntegerMaskKind {
        name: "VPCMPGTQ",
        map: 2,
        pp: 1,
        opcode: 0x37,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::FixedCompare(VecCmpCond::Gt),
    },
    IntegerMaskKind {
        name: "VPCMPUD",
        map: 3,
        pp: 1,
        opcode: 0x1E,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: false },
    },
    IntegerMaskKind {
        name: "VPCMPUQ",
        map: 3,
        pp: 1,
        opcode: 0x1E,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: false },
    },
    IntegerMaskKind {
        name: "VPCMPD",
        map: 3,
        pp: 1,
        opcode: 0x1F,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: true },
    },
    IntegerMaskKind {
        name: "VPCMPQ",
        map: 3,
        pp: 1,
        opcode: 0x1F,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: true },
    },
    IntegerMaskKind {
        name: "VPCMPUB",
        map: 3,
        pp: 1,
        opcode: 0x3E,
        elem: VecElementType::I8,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: false },
    },
    IntegerMaskKind {
        name: "VPCMPUW",
        map: 3,
        pp: 1,
        opcode: 0x3E,
        elem: VecElementType::I16,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: false },
    },
    IntegerMaskKind {
        name: "VPCMPB",
        map: 3,
        pp: 1,
        opcode: 0x3F,
        elem: VecElementType::I8,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: true },
    },
    IntegerMaskKind {
        name: "VPCMPW",
        map: 3,
        pp: 1,
        opcode: 0x3F,
        elem: VecElementType::I16,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::ImmediateCompare { signed: true },
    },
    IntegerMaskKind {
        name: "VPTESTMB",
        map: 2,
        pp: 1,
        opcode: 0x26,
        elem: VecElementType::I8,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::Test { inverted: false },
    },
    IntegerMaskKind {
        name: "VPTESTMW",
        map: 2,
        pp: 1,
        opcode: 0x26,
        elem: VecElementType::I16,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::Test { inverted: false },
    },
    IntegerMaskKind {
        name: "VPTESTMD",
        map: 2,
        pp: 1,
        opcode: 0x27,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::Test { inverted: false },
    },
    IntegerMaskKind {
        name: "VPTESTMQ",
        map: 2,
        pp: 1,
        opcode: 0x27,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::Test { inverted: false },
    },
    IntegerMaskKind {
        name: "VPTESTNMB",
        map: 2,
        pp: 2,
        opcode: 0x26,
        elem: VecElementType::I8,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::Test { inverted: true },
    },
    IntegerMaskKind {
        name: "VPTESTNMW",
        map: 2,
        pp: 2,
        opcode: 0x26,
        elem: VecElementType::I16,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::Test { inverted: true },
    },
    IntegerMaskKind {
        name: "VPTESTNMD",
        map: 2,
        pp: 2,
        opcode: 0x27,
        elem: VecElementType::I32,
        w: WControl::Fixed(false),
        semantic: IntegerMaskSemantic::Test { inverted: true },
    },
    IntegerMaskKind {
        name: "VPTESTNMQ",
        map: 2,
        pp: 2,
        opcode: 0x27,
        elem: VecElementType::I64,
        w: WControl::Fixed(true),
        semantic: IntegerMaskSemantic::Test { inverted: true },
    },
];

impl IntegerMaskKind {
    fn w_values(self) -> &'static [bool] {
        match self.w {
            WControl::Fixed(false) => &[false],
            WControl::Fixed(true) => &[true],
            WControl::Ignored => &[false, true],
        }
    }

    const fn has_immediate(self) -> bool {
        matches!(self.semantic, IntegerMaskSemantic::ImmediateCompare { .. })
    }

    const fn permits_broadcast(self) -> bool {
        matches!(self.elem, VecElementType::I32 | VecElementType::I64)
    }

    fn expected_operation(self, immediate: u8) -> X86EvexPackedIntegerMaskOperation {
        match self.semantic {
            IntegerMaskSemantic::FixedCompare(condition) => {
                X86EvexPackedIntegerMaskOperation::Compare {
                    condition: Some(condition),
                    constant: None,
                    predicate: None,
                }
            }
            IntegerMaskSemantic::ImmediateCompare { signed } => {
                let predicate = immediate & 7;
                let (condition, constant) = match predicate {
                    0 => (Some(VecCmpCond::Eq), None),
                    1 => (
                        Some(if signed {
                            VecCmpCond::Lt
                        } else {
                            VecCmpCond::Ltu
                        }),
                        None,
                    ),
                    2 => (
                        Some(if signed {
                            VecCmpCond::Le
                        } else {
                            VecCmpCond::Leu
                        }),
                        None,
                    ),
                    3 => (None, Some(false)),
                    4 => (Some(VecCmpCond::Ne), None),
                    5 => (
                        Some(if signed {
                            VecCmpCond::Ge
                        } else {
                            VecCmpCond::Geu
                        }),
                        None,
                    ),
                    6 => (
                        Some(if signed {
                            VecCmpCond::Gt
                        } else {
                            VecCmpCond::Gtu
                        }),
                        None,
                    ),
                    7 => (None, Some(true)),
                    _ => unreachable!(),
                };
                X86EvexPackedIntegerMaskOperation::Compare {
                    condition,
                    constant,
                    predicate: Some(predicate),
                }
            }
            IntegerMaskSemantic::Test { inverted } => {
                X86EvexPackedIntegerMaskOperation::Test { inverted }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerMaskMemoryCase {
    kind: IntegerMaskKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    w: bool,
    form: SourceForm,
    mask: u8,
    immediate: u8,
}

impl IntegerMaskMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn memory_size(self) -> u32 {
        if self.broadcast() {
            self.kind.elem.bytes()
        } else {
            self.width.bytes()
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.source1)
            .expect("one vector source leaves at least fifteen low scratch registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask != 0 {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch(), self.mask)
        }
    }
}

fn evex_fields(case: IntegerMaskMemoryCase) -> (u8, u8, u8) {
    assert!(case.destination < 8 && case.source1 < 32 && case.mask < 8);
    (
        0xF0 | case.kind.map,
        (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x04 | case.kind.pp,
        (case.ll() << 5)
            | (u8::from(case.broadcast()) << 4)
            | (if case.source1 < 16 { 0x08 } else { 0 })
            | case.mask,
    )
}

fn memory_encoding(case: IntegerMaskMemoryCase, apx_base: bool, apx_index: bool) -> Vec<u8> {
    let (mut p0, mut p1, p2) = evex_fields(case);
    let mut bytes = if !apx_base && !apx_index {
        vec![
            0x62,
            p0,
            p1,
            p2,
            case.kind.opcode,
            (case.destination << 3) | 3,
        ]
    } else {
        if apx_base {
            p0 |= 0x08;
        }
        if apx_index {
            p1 &= !0x04;
        }
        vec![
            0x62,
            p0,
            p1,
            p2,
            case.kind.opcode,
            0x40 | (case.destination << 3) | 0x04,
            0x48,
            DISP8 as u8,
        ]
    };
    if case.kind.has_immediate() {
        bytes.push(case.immediate);
    }
    bytes
}

fn stack_encoding(case: IntegerMaskMemoryCase) -> Vec<u8> {
    let (p0, p1, p2) = evex_fields(case);
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        (case.destination << 3) | 0x04,
        0x24,
    ];
    if case.kind.has_immediate() {
        bytes.push(case.immediate);
    }
    bytes
}

fn register_encoding(case: IntegerMaskMemoryCase, source2: u8, mask: u8) -> Vec<u8> {
    assert!(source2 < 32 && mask < 8);
    let (_, p1, mut p2) = evex_fields(case);
    p2 = (p2 & !0x17) | mask;
    let mut bytes = vec![
        0x62,
        0x90 | case.kind.map
            | if source2 < 16 { 0x40 } else { 0 }
            | if source2 & 8 == 0 { 0x20 } else { 0 },
        p1,
        p2,
        case.kind.opcode,
        0xC0 | (case.destination << 3) | (source2 & 7),
    ];
    if case.kind.has_immediate() {
        bytes.push(case.immediate);
    }
    bytes
}

fn replay_instruction(
    encoding: crate::smir::ir::X86EvexPackedIntegerMaskMemoryEncoding,
) -> Vec<u8> {
    match encoding.replay {
        X86EvexPackedIntegerMaskMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexPackedIntegerMaskMemoryReplay::Broadcast { stack_instruction }
        | X86EvexPackedIntegerMaskMemoryReplay::MaskedVector { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
        }
    }
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("packed integer mask instruction metadata"),
    );
    function
}

fn lift_case(case: IntegerMaskMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
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

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexPackedIntegerMaskMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_integer_mask_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: IntegerMaskMemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(
        !x86_native_vector_features_supported_excluding(function, &excluded),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed integer mask lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed integer mask memory"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<IntegerMaskMemoryCase> {
    let mut cases = Vec::new();
    for kind in KINDS {
        for &w in kind.w_values() {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    if form == SourceForm::Broadcast && !kind.permits_broadcast() {
                        continue;
                    }
                    for source1 in [0, 1, 15] {
                        for mask in [0, 1] {
                            cases.push(IntegerMaskMemoryCase {
                                kind,
                                width,
                                destination: 0,
                                source1,
                                w,
                                form,
                                mask,
                                immediate: 0,
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 720);
    cases
}

fn semantic_cases() -> Vec<IntegerMaskMemoryCase> {
    let mut cases = Vec::new();
    for kind in KINDS {
        let immediates: &[u8] = if kind.has_immediate() {
            &[0, 1, 2, 3, 4, 5, 6, 7]
        } else {
            &[0]
        };
        for &w in kind.w_values().iter().take(1) {
            for width in [VecWidth::V128, VecWidth::V512] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    if form == SourceForm::Broadcast && !kind.permits_broadcast() {
                        continue;
                    }
                    for mask in [0, 1] {
                        for &immediate in immediates {
                            cases.push(IntegerMaskMemoryCase {
                                kind,
                                width,
                                destination: 7,
                                source1: 17,
                                w,
                                form,
                                mask,
                                immediate,
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 480);
    cases
}

#[test]
fn llvm_23_byte_anchors_cover_compare_test_width_broadcast_mask_and_immediate() {
    let anchors = [
        ("VPCMPEQB", [0x62, 0xF1, 0x75, 0x08, 0x74, 0x0B].as_slice()),
        ("VPCMPGTQ", [0x62, 0xF2, 0xF5, 0x48, 0x37, 0x1B].as_slice()),
        (
            "VPCMPUD broadcast",
            [0x62, 0xF3, 0x75, 0x39, 0x1E, 0x2B, 0x06].as_slice(),
        ),
        ("VPTESTMW", [0x62, 0xF2, 0xF5, 0x28, 0x26, 0x33].as_slice()),
        (
            "VPTESTNMD broadcast",
            [0x62, 0xF2, 0x76, 0x59, 0x27, 0x3B].as_slice(),
        ),
    ];
    for (name, bytes) in anchors {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_packed_integer_mask_memory_encoding();
        assert!(encoding.is_some(), "{name}: {bytes:02X?}");
    }
}

#[test]
fn packed_integer_mask_classifier_exhausts_1_867_776_semantic_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in KINDS {
        let immediates: &[u8] = if kind.has_immediate() {
            &[0, 7, 0xF8, 0xFF]
        } else {
            &[0]
        };
        for &w in kind.w_values() {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    if form == SourceForm::Broadcast && !kind.permits_broadcast() {
                        continue;
                    }
                    for destination in 0..8u8 {
                        for source1 in 0..32u8 {
                            for mask in 0..8u8 {
                                for &immediate in immediates {
                                    let case = IntegerMaskMemoryCase {
                                        kind,
                                        width,
                                        destination,
                                        source1,
                                        w,
                                        form,
                                        mask,
                                        immediate,
                                    };
                                    for apx_base in [false, true] {
                                        for apx_index in [false, true] {
                                            let bytes = memory_encoding(case, apx_base, apx_index);
                                            let encoding = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_packed_integer_mask_memory_encoding()
                                                .unwrap_or_else(|| panic!("{case:?} {bytes:02X?}"));
                                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                                            assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.destination, destination,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.writemask,
                                                (mask != 0).then_some(mask),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.operation,
                                                kind.expected_operation(immediate),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.needs_avx512vl,
                                                width != VecWidth::V512,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.needs_avx512bw,
                                                matches!(
                                                    kind.elem,
                                                    VecElementType::I8 | VecElementType::I16
                                                ),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                replay_instruction(encoding),
                                                case.expected_replay(),
                                                "{bytes:02X?}"
                                            );
                                            match encoding.replay {
                                                X86EvexPackedIntegerMaskMemoryReplay::Broadcast {
                                                    ..
                                                } => assert!(case.broadcast(), "{bytes:02X?}"),
                                                X86EvexPackedIntegerMaskMemoryReplay::MaskedVector {
                                                    ..
                                                } => assert!(
                                                    !case.broadcast() && mask != 0,
                                                    "{bytes:02X?}"
                                                ),
                                                X86EvexPackedIntegerMaskMemoryReplay::Vector {
                                                    scratch,
                                                    ..
                                                } => {
                                                    assert!(
                                                        !case.broadcast() && mask == 0,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_eq!(
                                                        scratch,
                                                        case.scratch(),
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                            }
                                            accepted += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 1_867_776);
}

#[test]
fn immediate_classifier_preserves_all_256_bytes_and_decodes_low_three_bits() {
    let mut checked = 0usize;
    for kind in KINDS.into_iter().filter(|kind| kind.has_immediate()) {
        for immediate in u8::MIN..=u8::MAX {
            let case = IntegerMaskMemoryCase {
                kind,
                width: VecWidth::V512,
                destination: 7,
                source1: 31,
                w: kind.w_values()[0],
                form: if kind.permits_broadcast() {
                    SourceForm::Broadcast
                } else {
                    SourceForm::Vector
                },
                mask: 1,
                immediate,
            };
            let encoding = X86InstructionBytes::new(&case.bytes())
                .unwrap()
                .evex_packed_integer_mask_memory_encoding()
                .unwrap();
            assert_eq!(encoding.operation, kind.expected_operation(immediate));
            assert_eq!(replay_instruction(encoding).last(), Some(&immediate));
            checked += 1;
        }
    }
    assert_eq!(checked, 8 * 256);
}

#[test]
fn classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let base = scanner_cases()[0];
    let mut invalids = Vec::<(&str, Vec<u8>)>::new();
    let mut bytes = base.bytes();
    bytes[3] |= 0x80;
    invalids.push(("EVEX.z", bytes));
    let mut bytes = base.bytes();
    bytes[3] = (bytes[3] & !0x60) | 0x60;
    invalids.push(("reserved LL", bytes));
    let mut bytes = base.bytes();
    bytes[5] |= 0xC0;
    invalids.push(("register source", bytes));
    let mut bytes = base.bytes();
    bytes.push(0);
    invalids.push(("trailing byte", bytes));
    let byte_kind = KINDS
        .into_iter()
        .find(|kind| kind.elem == VecElementType::I8)
        .unwrap();
    let byte_case = IntegerMaskMemoryCase {
        kind: byte_kind,
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        w: byte_kind.w_values()[0],
        form: SourceForm::Vector,
        mask: 0,
        immediate: 0,
    };
    let mut bytes = byte_case.bytes();
    bytes[3] |= 0x10;
    invalids.push(("E4.nb broadcast", bytes));
    let immediate_kind = KINDS.into_iter().find(|kind| kind.has_immediate()).unwrap();
    let immediate_case = IntegerMaskMemoryCase {
        kind: immediate_kind,
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        w: immediate_kind.w_values()[0],
        form: SourceForm::Vector,
        mask: 0,
        immediate: 7,
    };
    let mut bytes = immediate_case.bytes();
    bytes.pop();
    invalids.push(("missing immediate", bytes));
    for (name, bytes) in invalids {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_integer_mask_memory_encoding()
                .is_none(),
            "{name}: {bytes:02X?}"
        );
    }
}

#[test]
fn apx_r16_r17_sib_address_lifts_admits_and_lowers_with_exact_frontier() {
    let kind = KINDS
        .into_iter()
        .find(|kind| kind.name == "VPCMPUD")
        .unwrap();
    let case = IntegerMaskMemoryCase {
        kind,
        width: VecWidth::V128,
        destination: 1,
        source1: 17,
        w: false,
        form: SourceForm::Vector,
        mask: 1,
        immediate: 6,
    };
    let bytes = memory_encoding(case, true, true);
    assert_eq!(
        bytes,
        [0x62, 0xFB, 0x71, 0x01, 0x1E, 0x4C, 0x48, 0x01, 0x06]
    );
    let base = lift_bytes(&bytes);
    let expected_replay = [0x62, 0xF3, 0x75, 0x01, 0x1E, 0x0C, 0x24, 0x06];
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp: 16,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                }
            )),
            "{level:?}: {:#?}",
            function.blocks[0].ops
        );
        let exact = sequence(&function, true).expect("APX-address integer-mask sequence");
        assert_eq!(exact.address_offset, 2, "{level:?}");
        assert_eq!(exact.encoding.destination, 1, "{level:?}");
        assert_eq!(exact.encoding.source1, 17, "{level:?}");
        assert_eq!(replay_instruction(exact.encoding), expected_replay);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected_replay.len())
                .any(|window| window == expected_replay),
            "{level:?}"
        );

        let mut missing_guard = function.clone();
        missing_guard.blocks[0].ops.remove(0);
        assert!(sequence(&missing_guard, true).is_none(), "{level:?}");
    }
}

#[test]
fn all_720_scanner_memory_cells_match_and_lower_at_o0_o1_o2() {
    let cases = scanner_cases();
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: no exact sequence"));
            assert_eq!(exact.encoding.operation, case.kind.expected_operation(0));
            assert_eq!(exact.memory_size, case.memory_size(), "{level:?} {case:?}");
            assert_eq!(
                exact.address_offset,
                match (case.form, case.mask != 0) {
                    (SourceForm::Vector, false) | (SourceForm::Broadcast, false) => 0,
                    (SourceForm::Vector, true) => 2,
                    (SourceForm::Broadcast, true) => 5,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    .count(),
                match (case.form, case.mask != 0) {
                    (_, false) => 0,
                    (SourceForm::Vector, true) => {
                        case.width.lanes(case.kind.elem) as usize
                    }
                    (SourceForm::Broadcast, true) => 1,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::X86MovMask { .. }))
                    .count(),
                1,
                "{level:?} {case:?}"
            );
            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 720 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_graph_frontier_and_ssa_mutations() {
    let selected = [
        semantic_cases()
            .into_iter()
            .find(|case| {
                matches!(
                    case.kind.semantic,
                    IntegerMaskSemantic::ImmediateCompare { .. }
                ) && case.immediate == 3
                    && case.mask != 0
                    && case.form == SourceForm::Broadcast
            })
            .unwrap(),
        scanner_cases()
            .into_iter()
            .find(|case| {
                matches!(case.kind.semantic, IntegerMaskSemantic::Test { .. })
                    && case.kind.elem == VecElementType::I8
                    && case.mask != 0
            })
            .unwrap(),
    ];
    for case in selected {
        let function = optimize(lift_case(case), OptLevel::O2);

        let mut missing = function.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        if case.kind.has_immediate() {
            *bytes.last_mut().unwrap() ^= 4;
        } else {
            bytes[2] ^= 3;
        }
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong semantic provenance", &wrong_provenance);

        let compare_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VCmp { .. }))
            .unwrap();
        let mut wrong_compare = function.clone();
        let OpKind::VCmp { cond, .. } = &mut wrong_compare.blocks[0].ops[compare_index].kind else {
            unreachable!()
        };
        *cond = if *cond == VecCmpCond::Eq {
            VecCmpCond::Ne
        } else {
            VecCmpCond::Eq
        };
        assert_rejected("wrong comparison condition", &wrong_compare);

        let mov_mask_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86MovMask { .. }))
            .unwrap();
        let mut wrong_mov_mask = function.clone();
        let OpKind::X86MovMask { elem, .. } =
            &mut wrong_mov_mask.blocks[0].ops[mov_mask_index].kind
        else {
            unreachable!()
        };
        *elem = if *elem == VecElementType::I8 {
            VecElementType::I16
        } else {
            VecElementType::I8
        };
        assert_rejected("wrong sign-bit reduction", &wrong_mov_mask);

        let mut wrong_memory_hint = function.clone();
        let address_index = sequence(&wrong_memory_hint, true).unwrap().address_offset;
        wrong_memory_hint.blocks[0].ops[address_index].x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("wrong memory hint", &wrong_memory_hint);

        let memory_source = match function.blocks[0].ops[compare_index].kind {
            OpKind::VCmp { src2, .. } => src2,
            _ => unreachable!(),
        };
        let mut extra_use = function.clone();
        extra_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Reg(memory_source),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("extra SSA use", &extra_use);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &same_pc_tail);
    }
}

#[test]
fn masked_vector_lowering_stages_all_integer_widths_and_rejects_avx_only_bridge() {
    for elem in [
        VecElementType::I8,
        VecElementType::I16,
        VecElementType::I32,
        VecElementType::I64,
    ] {
        let case = scanner_cases()
            .into_iter()
            .find(|case| {
                case.kind.elem == elem
                    && case.width == VecWidth::V512
                    && case.form == SourceForm::Vector
                    && case.mask != 0
            })
            .unwrap();
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
        assert_eq!(
            code.windows(allocate_frame.len())
                .filter(|window| *window == allocate_frame)
                .count(),
            1,
            "{elem:?}"
        );
        if elem == VecElementType::I8 {
            let load_k1 = [0xC4, 0xE1, 0xFB, 0x93, 0xC1];
            assert_eq!(
                code.windows(load_k1.len())
                    .filter(|window| *window == load_k1)
                    .count(),
                64,
                "every byte lane requires one live K1 guard"
            );
            for lane in 32..64u8 {
                let high_lane_shift = [0x48, 0xC1, 0xE8, lane];
                assert!(
                    code.windows(high_lane_shift.len())
                        .any(|window| window == high_lane_shift),
                    "missing high byte-lane guard {lane}"
                );
            }
        }

        let mut avx_only = X86_64Lowerer::new();
        avx_only.set_mem_helpers(true);
        avx_only.set_preserve_vector_mem_helpers(true);
        avx_only.set_avx_ymm16_vector_state(true);
        let error = avx_only
            .lower_function(&function)
            .expect_err("AVX-only state bridge must reject AVX-512 integer-mask replay");
        assert!(format!("{error:?}").contains("AVX-only vector bridge"));
    }
}
