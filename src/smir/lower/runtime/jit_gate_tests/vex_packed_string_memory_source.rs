//! Exact helper-backed Intel VEX packed-string memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86PackedStringKind};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, OpWidth, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexPackedStringMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_packed_string_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod semantics;

const PC: u64 = 0x6063_5A40;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const OPERANDS: [(u8, u8); 8] = [
    (1, 3),
    (9, 11),
    (0, 0),
    (1, 2),
    (15, 1),
    (2, 2),
    (3, 4),
    (4, 5),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedStringMemoryCase {
    kind: X86PackedStringKind,
    w: bool,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl PackedStringMemoryCase {
    fn opcode(self) -> u8 {
        match self.kind {
            X86PackedStringKind::ExplicitMask => 0x60,
            X86PackedStringKind::ExplicitIndex => 0x61,
            X86PackedStringKind::ImplicitMask => 0x62,
            X86PackedStringKind::ImplicitIndex => 0x63,
        }
    }

    fn scratch(self) -> u8 {
        (1..16)
            .find(|candidate| *candidate != self.source1)
            .expect("one source leaves at least fourteen nonzero scratch registers")
    }

    fn length_width(self) -> OpWidth {
        if self.kind.is_explicit() && self.w {
            OpWidth::W64
        } else {
            OpWidth::W32
        }
    }

    fn p0(self, rm: u8) -> u8 {
        (if self.source1 < 8 { 0x80 } else { 0 }) | 0x40 | (if rm < 8 { 0x20 } else { 0 }) | 3
    }

    fn p1(self) -> u8 {
        (if self.w { 0x80 } else { 0 }) | 0x79
    }

    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![
            0xC4,
            self.p0(self.base),
            self.p1(),
            self.opcode(),
            0x40 | ((self.source1 & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.extend([DISP as u8, self.immediate]);
        bytes
    }

    fn register_bytes(self) -> [u8; 6] {
        let scratch = self.scratch();
        [
            0xC4,
            self.p0(scratch),
            self.p1(),
            self.opcode(),
            0xC0 | ((self.source1 & 7) << 3) | (scratch & 7),
            self.immediate,
        ]
    }
}

fn families() -> [X86PackedStringKind; 4] {
    [
        X86PackedStringKind::ExplicitMask,
        X86PackedStringKind::ExplicitIndex,
        X86PackedStringKind::ImplicitMask,
        X86PackedStringKind::ImplicitIndex,
    ]
}

fn all_cases() -> Vec<PackedStringMemoryCase> {
    let mut cases = Vec::with_capacity(families().len() * 2 * OPERANDS.len());
    let mut ordinal = 0usize;
    for kind in families() {
        for w in [false, true] {
            for (source1, base) in OPERANDS {
                cases.push(PackedStringMemoryCase {
                    kind,
                    w,
                    source1,
                    base,
                    immediate: ordinal as u8,
                });
                ordinal += 1;
            }
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
        X86InstructionBytes::new(bytes).expect("packed-string instruction provenance"),
    );
    function
}

fn lift_case(case: PackedStringMemoryCase) -> SmirFunction {
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
        .expect("packed-string memory load")
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
) -> Option<X86JitVexPackedStringMemorySequence> {
    let block = &function.blocks[0];
    let index = block
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))?;
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_packed_string_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn expected_requirements() -> X86NativeReplayFeatureRequirements {
    X86NativeReplayFeatureRequirements {
        any: true,
        all_spans_support_avx_ymm16: true,
        needs_avx: true,
        ..X86NativeReplayFeatureRequirements::default()
    }
}

fn assert_exact_graph(function: &SmirFunction, case: PackedStringMemoryCase) {
    let block = &function.blocks[0];
    let index = sequence_index(function);
    assert_eq!(index, 0, "{case:?}");
    let sequence = classified_sequence(function, true)
        .unwrap_or_else(|| panic!("unclassified exact packed-string graph: {case:?}"));
    assert_eq!(sequence.consumed, 2, "{case:?}");
    assert_eq!(block.ops.len(), 2, "{case:?}");
    assert_eq!(sequence.encoding.kind, case.kind, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(sequence.encoding.immediate, case.immediate, "{case:?}");
    assert_eq!(
        sequence.encoding.length_width,
        case.length_width(),
        "{case:?}"
    );
    assert_eq!(sequence.encoding.memory_size, 16, "{case:?}");
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.register_bytes(),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(
    function: &SmirFunction,
    case: PackedStringMemoryCase,
) -> (Vec<u8>, usize, X86JitVexPackedStringMemorySequence) {
    let excluded = HashMap::new();
    let sequence = classified_sequence(function, true).expect("classified packed-string sequence");
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
    assert_eq!(requirements, expected_requirements(), "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx"),
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
        lowerer
            .finalize()
            .expect("finalize helper-backed packed string"),
        lowered.entry_offset,
        sequence,
    )
}

#[test]
fn all_6_144_kind_w_immediate_and_optimization_graphs_are_exact() {
    let mut classified = 0usize;
    for kind in families() {
        for w in [false, true] {
            for immediate in u8::MIN..=u8::MAX {
                let case = PackedStringMemoryCase {
                    kind,
                    w,
                    source1: 9,
                    base: 11,
                    immediate,
                };
                for level in LEVELS {
                    assert_exact_graph(&optimize(lift_case(case), level), case);
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 4 * 2 * 256 * LEVELS.len());
}

#[test]
fn all_192_kind_w_alias_address_and_optimization_cells_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 4 * 2 * OPERANDS.len());
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
    assert_eq!(lowered, 192);
}

#[test]
fn segment_addr32_sib_rip_and_stack_default_address_shapes_lower_exactly() {
    let explicit_mask = PackedStringMemoryCase {
        kind: X86PackedStringKind::ExplicitMask,
        w: false,
        source1: 9,
        base: 14,
        immediate: 0xFF,
    };
    let implicit_index = PackedStringMemoryCase {
        kind: X86PackedStringKind::ImplicitIndex,
        w: true,
        immediate: 0x80,
        ..explicit_mask
    };
    for (name, case, bytes) in [
        (
            "FS addr32 extended SIB explicit mask",
            explicit_mask,
            vec![
                0x64,
                0x67,
                0xC4,
                0x03,
                explicit_mask.p1(),
                0x60,
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
            "SS addr32 extended SIB implicit index",
            implicit_index,
            vec![
                0x36,
                0x67,
                0xC4,
                0x03,
                implicit_index.p1(),
                0x63,
                0x8C,
                0x7E,
                0x11,
                0x22,
                0x33,
                0x44,
                0x80,
            ],
        ),
        (
            "RIP relative implicit mask",
            PackedStringMemoryCase {
                kind: X86PackedStringKind::ImplicitMask,
                w: false,
                source1: 1,
                base: 0,
                immediate: 0x40,
            },
            vec![0xC4, 0xE3, 0x79, 0x62, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x40],
        ),
        (
            "RBP default SS explicit 64-bit index",
            PackedStringMemoryCase {
                kind: X86PackedStringKind::ExplicitIndex,
                w: true,
                source1: 1,
                base: 5,
                immediate: 0,
            },
            vec![0xC4, 0xE3, 0xF9, 0x61, 0x4D, 0x20, 0],
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
            assert_eq!(sequence.encoding.source1, case.source1, "{name} {level:?}");
            assert_eq!(
                sequence.encoding.length_width,
                case.length_width(),
                "{name} {level:?}"
            );
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
        "{name}: classifier admitted malformed packed-string graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed packed-string graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed packed-string graph"
    );
}

#[test]
fn classifier_gate_and_lowerer_reject_graph_provenance_and_ssa_mutations() {
    let case = PackedStringMemoryCase {
        kind: X86PackedStringKind::ExplicitMask,
        w: true,
        source1: 9,
        base: 11,
        immediate: 0xFF,
    };
    let base = lift_case(case);
    assert_eq!(sequence_index(&base), 0);

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
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(15))),
            src: loaded,
            width: VecWidth::V128,
        },
    ));
    assert_rejected("loaded virtual escapes", &escaped);

    let mut redefined = base.clone();
    redefined.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + case.bytes().len() as u64,
        OpKind::VMov {
            dst: loaded,
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(15))),
            width: VecWidth::V128,
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

    for (name, mutate) in [
        ("semantic immediate", 0usize),
        ("semantic kind", 1),
        ("semantic length width", 2),
        ("semantic zero upper", 3),
        ("semantic destination", 4),
        ("semantic source", 5),
        ("semantic length register", 6),
    ] {
        let mut function = base.clone();
        let OpKind::X86PackedStringCompare {
            dst,
            src1,
            len1,
            length_width,
            kind,
            imm,
            zero_upper,
            ..
        } = &mut function.blocks[0].ops[1].kind
        else {
            unreachable!()
        };
        match mutate {
            0 => *imm ^= 1,
            1 => *kind = X86PackedStringKind::ExplicitIndex,
            2 => *length_width = OpWidth::W32,
            3 => *zero_upper = false,
            4 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            5 => *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            6 => *len1 = Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
            _ => unreachable!(),
        }
        assert_rejected(name, &function);
    }

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    for (name, byte_index, value) in [
        ("map", 1, case.bytes()[1] & !0x1F | 2),
        ("mandatory prefix", 2, case.bytes()[2] & !1),
        ("L", 2, case.bytes()[2] | 0x04),
        ("vvvv", 2, case.bytes()[2] & !0x08),
        ("opcode", 3, 0x5F),
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
    no_memory_helpers.set_avx_ymm16_vector_state(true);
    assert!(matches!(
        no_memory_helpers.lower_function(&function),
        Err(LowerError::UnsupportedOp { .. }) | Err(LowerError::InvalidOperand { .. })
    ));
}
