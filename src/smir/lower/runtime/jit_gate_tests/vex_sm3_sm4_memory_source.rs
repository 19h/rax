//! Exact helper-backed Intel VEX SM3/SM4 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{Address, ArchReg, BlockId, FunctionId, OpId, VReg, VecWidth, X86Reg};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexSm3Sm4MemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexSm3Sm4MemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_sm3_sm4_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod semantics;

const PC: u64 = 0x5E13_5A40;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const OPERANDS: [(u8, u8, u8); 8] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (3, 2, 3),
    (2, 2, 3),
    (11, 10, 3),
    (4, 2, 11),
    (15, 15, 11),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sm3Sm4MemoryCase {
    kind: X86VexSm3Sm4MemoryKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl Sm3Sm4MemoryCase {
    fn map(self) -> u8 {
        if self.kind == X86VexSm3Sm4MemoryKind::Sm3Rounds2 {
            3
        } else {
            2
        }
    }

    fn pp(self) -> u8 {
        match self.kind {
            X86VexSm3Sm4MemoryKind::Sm3Msg1 => 0,
            X86VexSm3Sm4MemoryKind::Sm3Msg2 | X86VexSm3Sm4MemoryKind::Sm3Rounds2 => 1,
            X86VexSm3Sm4MemoryKind::Sm4Key4 => 2,
            X86VexSm3Sm4MemoryKind::Sm4Rounds4 => 3,
        }
    }

    fn opcode(self) -> u8 {
        if self.kind == X86VexSm3Sm4MemoryKind::Sm3Rounds2 {
            0xDE
        } else {
            0xDA
        }
    }

    fn has_immediate(self) -> bool {
        self.kind == X86VexSm3Sm4MemoryKind::Sm3Rounds2
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two SM3/SM4 operands leave at least fourteen scratch registers")
    }

    fn p1(self) -> u8 {
        (((!self.source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | self.pp()
    }

    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | self.map(),
            self.p1(),
            self.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
        ];
        if self.has_immediate() {
            bytes.push(self.immediate);
        }
        bytes
    }

    fn register_bytes(self) -> Vec<u8> {
        let scratch = self.scratch();
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | self.map(),
            self.p1(),
            self.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (scratch & 7),
        ];
        if self.has_immediate() {
            bytes.push(self.immediate);
        }
        bytes
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("SM3/SM4 test width"),
    }))
}

fn families() -> [(X86VexSm3Sm4MemoryKind, VecWidth); 7] {
    [
        (X86VexSm3Sm4MemoryKind::Sm3Msg1, VecWidth::V128),
        (X86VexSm3Sm4MemoryKind::Sm3Msg2, VecWidth::V128),
        (X86VexSm3Sm4MemoryKind::Sm3Rounds2, VecWidth::V128),
        (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V128),
        (X86VexSm3Sm4MemoryKind::Sm4Key4, VecWidth::V256),
        (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V128),
        (X86VexSm3Sm4MemoryKind::Sm4Rounds4, VecWidth::V256),
    ]
}

fn all_cases() -> Vec<Sm3Sm4MemoryCase> {
    let mut cases = Vec::with_capacity(families().len() * OPERANDS.len());
    let mut ordinal = 0usize;
    for (kind, width) in families() {
        for (destination, source1, base) in OPERANDS {
            cases.push(Sm3Sm4MemoryCase {
                kind,
                width,
                destination,
                source1,
                base,
                immediate: ordinal as u8,
            });
            ordinal += 1;
        }
    }
    cases
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
        X86InstructionBytes::new(bytes).expect("SM3/SM4 instruction provenance"),
    );
    function
}

fn lift_case(case: Sm3Sm4MemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("SM3/SM4 memory load")
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

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexSm3Sm4MemorySequence> {
    let block = &function.blocks[0];
    let index = block
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))?;
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_sm3_sm4_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn expected_requirements(case: Sm3Sm4MemoryCase) -> X86NativeReplayFeatureRequirements {
    X86NativeReplayFeatureRequirements {
        any: true,
        all_spans_support_avx_ymm16: true,
        needs_avx: true,
        needs_sm3: case.kind.needs_sm3(),
        needs_sm4: case.kind.needs_sm4(),
        ..X86NativeReplayFeatureRequirements::default()
    }
}

fn assert_exact_graph(function: &SmirFunction, case: Sm3Sm4MemoryCase) {
    let block = &function.blocks[0];
    let index = sequence_index(function);
    assert_eq!(index, 0, "{case:?}");
    let sequence = classified_sequence(function, true)
        .unwrap_or_else(|| panic!("unclassified exact SM3/SM4 graph: {case:?}"));
    assert_eq!(sequence.consumed, 2, "{case:?}");
    assert_eq!(block.ops.len(), 2, "{case:?}");
    assert_eq!(sequence.encoding.kind, case.kind, "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(
        sequence.encoding.immediate,
        case.has_immediate().then_some(case.immediate),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.memory_size,
        case.width.bytes(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.register_bytes(),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(
    function: &SmirFunction,
    case: Sm3Sm4MemoryCase,
) -> (Vec<u8>, usize, X86JitVexSm3Sm4MemorySequence) {
    let excluded = HashMap::new();
    let sequence = classified_sequence(function, true).expect("classified SM3/SM4 sequence");
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
    assert_eq!(requirements, expected_requirements(case), "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
            && (!case.kind.needs_sm3() || std::is_x86_feature_detected!("sm3"))
            && (!case.kind.needs_sm4() || std::is_x86_feature_detected!("sm4")),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed lowering failed: {error:?}"));
    assert!(lowered.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize helper-backed SM3/SM4"),
        lowered.entry_offset,
        sequence,
    )
}

#[test]
fn all_768_sm3_round_immediate_and_optimization_graphs_are_exact() {
    let mut classified = 0usize;
    for immediate in u8::MIN..=u8::MAX {
        let case = Sm3Sm4MemoryCase {
            kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
            width: VecWidth::V128,
            destination: 9,
            source1: 10,
            base: 11,
            immediate,
        };
        for level in LEVELS {
            assert_exact_graph(&optimize(lift_case(case), level), case);
            classified += 1;
        }
    }
    assert_eq!(classified, 256 * LEVELS.len());
}

#[test]
fn all_168_family_width_alias_and_optimization_cells_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 7 * OPERANDS.len());
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, sequence) = lower(&function, case);
            assert!(
                code.windows(case.register_bytes().len())
                    .any(|window| window == case.register_bytes()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.register_bytes(),
                "{level:?} {case:?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 168);
}

#[test]
fn segment_addr32_sib_rip_and_stack_default_address_shapes_lower_exactly() {
    let sm3 = Sm3Sm4MemoryCase {
        kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
        width: VecWidth::V128,
        destination: 9,
        source1: 11,
        base: 14,
        immediate: 0xFF,
    };
    let sm4 = Sm3Sm4MemoryCase {
        kind: X86VexSm3Sm4MemoryKind::Sm4Key4,
        width: VecWidth::V256,
        immediate: 0,
        ..sm3
    };
    for (name, case, bytes) in [
        (
            "FS addr32 SIB SM3RNDS2",
            sm3,
            vec![
                0x64,
                0x67,
                0xC4,
                0x03,
                sm3.p1(),
                0xDE,
                0x8C,
                0x7E,
                0x11,
                0x22,
                0x33,
                0x44,
                0xFF,
            ],
        ),
        (
            "SS addr32 SIB VSM4KEY4",
            sm4,
            vec![
                0x36,
                0x67,
                0xC4,
                0x02,
                sm4.p1(),
                0xDA,
                0x8C,
                0x7E,
                0x11,
                0x22,
                0x33,
                0x44,
            ],
        ),
        (
            "RIP relative",
            Sm3Sm4MemoryCase {
                kind: X86VexSm3Sm4MemoryKind::Sm3Msg1,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                base: 0,
                immediate: 0,
            },
            vec![0xC4, 0xE2, 0x68, 0xDA, 0x0D, 0x11, 0x22, 0x33, 0x44],
        ),
        (
            "RBP default SS",
            Sm3Sm4MemoryCase {
                kind: X86VexSm3Sm4MemoryKind::Sm4Rounds4,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                base: 5,
                immediate: 0,
            },
            vec![0xC4, 0xE2, 0x6B, 0xDA, 0x4D, 0x20],
        ),
    ] {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_bytes(&bytes), level);
            let sequence = classified_sequence(&function, true).unwrap_or_else(|| {
                panic!(
                    "{name} {level:?}: unclassified: {:#?}",
                    function.blocks[0].ops
                )
            });
            assert_eq!(sequence.encoding.kind, case.kind, "{name} {level:?}");
            assert_eq!(sequence.encoding.width, case.width, "{name} {level:?}");
            assert_eq!(
                sequence.encoding.destination, case.destination,
                "{name} {level:?}"
            );
            assert_eq!(sequence.encoding.source1, case.source1, "{name} {level:?}");
            let expected = sequence.encoding.register_instruction.as_slice().to_vec();
            let (code, _, _) = lower(&function, case);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name} {level:?}"
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed SM3/SM4 graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed SM3/SM4 graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed SM3/SM4 graph"
    );
}

#[test]
fn classifier_gate_and_lowerer_reject_graph_provenance_and_ssa_mutations() {
    let case = Sm3Sm4MemoryCase {
        kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
        width: VecWidth::V128,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0xFF,
    };
    let base = lift_case(case);
    let index = sequence_index(&base);
    assert_eq!(index, 0);

    for offset in 0..2 {
        let mut mutated = base.clone();
        mutated.blocks[0].ops[offset].kind = OpKind::Nop;
        assert_rejected(&format!("operation {offset} replaced"), &mutated);

        let mut hinted = base.clone();
        hinted.blocks[0].ops[offset].x86_hint = Some(X86OpHint::XopVpcom);
        assert_rejected(&format!("operation {offset} hinted"), &hinted);
    }

    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + case.bytes().len() as u64,
        OpKind::VMov {
            dst: vector(15, case.width),
            src: loaded,
            width: case.width,
        },
    ));
    assert_rejected("loaded virtual escapes", &escaped);

    let mut redefined = base.clone();
    redefined.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + case.bytes().len() as u64,
        OpKind::VMov {
            dst: loaded,
            src: vector(15, case.width),
            width: case.width,
        },
    ));
    assert_rejected("loaded virtual redefined", &redefined);

    let mut trailing = base.clone();
    trailing.blocks[0]
        .ops
        .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
    assert_rejected("same-PC trailing operation", &trailing);

    let mut preceding = base.clone();
    preceding.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0), PC, OpKind::Nop));
    assert_rejected("same-PC preceding operation", &preceding);

    let mut wrong_width = base.clone();
    let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = VecWidth::V256;
    assert_rejected("load width", &wrong_width);

    let mut wrong_address = base.clone();
    let OpKind::VLoad { addr, .. } = &mut wrong_address.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *addr = Address::Direct(loaded);
    assert_rejected("virtual address", &wrong_address);

    let mut wrong_immediate = base.clone();
    let OpKind::X86Sm3Rounds2 { imm, .. } = &mut wrong_immediate.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *imm = 0xFE;
    assert_rejected("semantic immediate", &wrong_immediate);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    for (name, byte_index, value) in [
        ("map", 1, case.bytes()[1] & !0x1F | 2),
        ("mandatory prefix", 2, case.bytes()[2] & !3),
        ("W", 2, case.bytes()[2] | 0x80),
        ("L", 2, case.bytes()[2] | 0x04),
        ("opcode", 3, 0xDD),
    ] {
        let mut bytes = case.bytes();
        bytes[byte_index] = value;
        let mut function = base.clone();
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected(name, &function);
    }
}

#[test]
fn memory_helpers_remain_mandatory() {
    let case = all_cases()[0];
    let function = lift_case(case);

    let mut no_memory_helpers = X86_64Lowerer::new();
    no_memory_helpers.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        no_memory_helpers.lower_function(&function),
        Err(LowerError::UnsupportedOp { .. }) | Err(LowerError::InvalidOperand { .. })
    ));
}
