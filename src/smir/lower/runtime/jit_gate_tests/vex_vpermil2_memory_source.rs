//! Exact helper-backed AMD VEX VPERMIL2 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexVpermil2MemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_vpermil2_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod semantics;

const PC: u64 = 0x5E1E_C702;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const OPERANDS: [(u8, u8, u8, u8); 8] = [
    (1, 2, 3, 4),
    (9, 10, 11, 12),
    (1, 1, 3, 4),
    (4, 2, 3, 4),
    (3, 2, 3, 4),
    (2, 2, 3, 4),
    (4, 2, 3, 2),
    (1, 1, 3, 1),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vpermil2MemoryCase {
    opcode: u8,
    w: bool,
    width_256: bool,
    destination: u8,
    source1: u8,
    base: u8,
    is4: u8,
    ignored_low: u8,
}

impl Vpermil2MemoryCase {
    fn width(self) -> VecWidth {
        if self.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        }
    }

    fn elem(self) -> VecElementType {
        if self.opcode == 0x48 {
            VecElementType::I32
        } else {
            VecElementType::I64
        }
    }

    fn immediate(self) -> u8 {
        (self.is4 << 4) | (self.ignored_low & 0x0F)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| {
                *candidate != self.destination
                    && *candidate != self.source1
                    && *candidate != self.is4
            })
            .expect("three VPERMIL2 operands leave at least thirteen scratch registers")
    }

    fn p1(self) -> u8 {
        (u8::from(self.w) << 7)
            | (((!self.source1) & 0x0F) << 3)
            | (u8::from(self.width_256) << 2)
            | 1
    }

    fn bytes(self) -> Vec<u8> {
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            self.p1(),
            self.opcode,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
            self.immediate(),
        ]
    }

    fn register_bytes(self) -> [u8; 6] {
        let scratch = self.scratch();
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 3,
            self.p1(),
            self.opcode,
            0xC0 | ((self.destination & 7) << 3) | (scratch & 7),
            self.immediate(),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VPERMIL2 test width"),
    }))
}

fn all_cases() -> Vec<Vpermil2MemoryCase> {
    let mut cases = Vec::with_capacity(2 * 2 * 2 * OPERANDS.len());
    let mut ordinal = 0usize;
    for opcode in [0x48, 0x49] {
        for w in [false, true] {
            for width_256 in [false, true] {
                for (destination, source1, base, is4) in OPERANDS {
                    cases.push(Vpermil2MemoryCase {
                        opcode,
                        w,
                        width_256,
                        destination,
                        source1,
                        base,
                        is4,
                        ignored_low: ordinal as u8 & 0x0F,
                    });
                    ordinal += 1;
                }
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
        X86InstructionBytes::new(bytes).expect("VPERMIL2 instruction provenance"),
    );
    function
}

fn lift_case(case: Vpermil2MemoryCase) -> SmirFunction {
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
        .expect("VPERMIL2 memory load")
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
) -> Option<X86JitVexVpermil2MemorySequence> {
    let block = &function.blocks[0];
    let index = block
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))?;
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_vpermil2_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_graph(function: &SmirFunction, case: Vpermil2MemoryCase) {
    let block = &function.blocks[0];
    let index = sequence_index(function);
    assert_eq!(index, 2, "{case:?}");
    let sequence = classified_sequence(function, true)
        .unwrap_or_else(|| panic!("unclassified exact VPERMIL2 graph: {case:?}"));
    assert_eq!(sequence.consumed, block.ops.len() - index, "{case:?}");
    assert_eq!(sequence.encoding.width, case.width(), "{case:?}");
    assert_eq!(sequence.encoding.elem, case.elem(), "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.is4, case.is4, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(sequence.encoding.w, case.w, "{case:?}");
    assert_eq!(sequence.encoding.immediate, case.immediate(), "{case:?}");
    assert_eq!(
        sequence.encoding.memory_size,
        case.width().bytes(),
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
    case: Vpermil2MemoryCase,
) -> (Vec<u8>, usize, X86JitVexVpermil2MemorySequence) {
    let excluded = HashMap::new();
    let sequence = classified_sequence(function, true).expect("classified VPERMIL2 sequence");
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
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_xop, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && crate::smir::lower::runtime::x86_host_has_xop(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("{case:?}: helper-backed VPERMIL2 lowering failed: {error:?}")
    });
    assert!(lowered.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize helper-backed VPERMIL2"),
        lowered.entry_offset,
        sequence,
    )
}

#[test]
fn all_6_144_opcode_w_l_immediate_and_optimization_graphs_are_exact() {
    let mut classified = 0usize;
    for opcode in [0x48, 0x49] {
        for w in [false, true] {
            for width_256 in [false, true] {
                for immediate in u8::MIN..=u8::MAX {
                    let case = Vpermil2MemoryCase {
                        opcode,
                        w,
                        width_256,
                        destination: 9,
                        source1: 10,
                        base: 11,
                        is4: immediate >> 4,
                        ignored_low: immediate & 0x0F,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        assert_exact_graph(&function, case);
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 2 * 2 * 2 * 256 * LEVELS.len());
}

#[test]
fn all_192_family_role_alias_and_optimization_cells_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 2 * OPERANDS.len());
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, sequence) = lower(&function, case);
            assert!(
                code.windows(case.register_bytes().len())
                    .any(|window| window == case.register_bytes())
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
    let common = Vpermil2MemoryCase {
        opcode: 0x48,
        w: false,
        width_256: true,
        destination: 9,
        source1: 10,
        base: 14,
        is4: 12,
        ignored_low: 7,
    };
    let p1 = common.p1();
    for (name, case, bytes, stack_segment) in [
        (
            "FS addr32 SIB",
            common,
            vec![
                0x64,
                0x67,
                0xC4,
                0x03,
                p1,
                common.opcode,
                0x8C,
                0x7E,
                0x11,
                0x22,
                0x33,
                0x44,
                common.immediate(),
            ],
            false,
        ),
        (
            "SS addr32 SIB",
            common,
            vec![
                0x36,
                0x67,
                0xC4,
                0x03,
                p1,
                common.opcode,
                0x8C,
                0x7E,
                0x11,
                0x22,
                0x33,
                0x44,
                common.immediate(),
            ],
            true,
        ),
        (
            "RIP relative",
            Vpermil2MemoryCase {
                destination: 1,
                source1: 2,
                base: 0,
                is4: 3,
                ..common
            },
            vec![0xC4, 0xE3, 0x6D, 0x48, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x37],
            false,
        ),
        (
            "RBP default SS",
            Vpermil2MemoryCase {
                destination: 1,
                source1: 2,
                base: 5,
                is4: 3,
                ..common
            },
            vec![0xC4, 0xE3, 0x6D, 0x48, 0x4D, 0x20, 0x37],
            true,
        ),
        (
            "DS overrides RBP",
            Vpermil2MemoryCase {
                destination: 1,
                source1: 2,
                base: 5,
                is4: 3,
                ..common
            },
            vec![0x3E, 0xC4, 0xE3, 0x6D, 0x48, 0x4D, 0x20, 0x37],
            false,
        ),
    ] {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_bytes(&bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: unclassified"));
            assert_eq!(
                sequence.encoding.stack_segment, stack_segment,
                "{name} {level:?}"
            );
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.register_bytes(),
                "{name} {level:?}"
            );
            let (code, _, _) = lower(&function, case);
            assert!(
                code.windows(case.register_bytes().len())
                    .any(|window| window == case.register_bytes()),
                "{name} {level:?}"
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed VPERMIL2 graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed VPERMIL2 graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed VPERMIL2 graph"
    );
}

fn replace_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated VPERMIL2 metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_reject_every_graph_operation_and_common_invariant_mutation() {
    let case = Vpermil2MemoryCase {
        opcode: 0x48,
        w: true,
        width_256: true,
        destination: 9,
        source1: 10,
        base: 11,
        is4: 12,
        ignored_low: 3,
    };
    let base = lift_case(case);
    let index = sequence_index(&base);
    let consumed = classified_sequence(&base, true).unwrap().consumed;

    for offset in 0..consumed {
        let mut mutated = base.clone();
        mutated.blocks[0].ops[index + offset].kind = OpKind::Nop;
        assert_rejected(&format!("operation {offset} replaced"), &mutated);

        let mut hinted = base.clone();
        hinted.blocks[0].ops[index + offset].x86_hint = Some(X86OpHint::XopVpcom);
        assert_rejected(&format!("operation {offset} hinted"), &hinted);
    }

    for guard_index in 0..2 {
        let mut mutated = base.clone();
        mutated.blocks[0].ops[guard_index].kind = OpKind::Nop;
        assert_rejected(&format!("guard {guard_index} replaced"), &mutated);

        let mut hinted = base.clone();
        hinted.blocks[0].ops[guard_index].x86_hint = Some(X86OpHint::XopVpcom);
        assert_rejected(&format!("guard {guard_index} hinted"), &hinted);
    }

    let loaded = match base.blocks[0].ops[index].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaped = base.clone();
    let escaped_id = OpId(escaped.blocks[0].ops.len() as u16);
    escaped.blocks[0].ops.push(SmirOp::new(
        escaped_id,
        PC + case.bytes().len() as u64,
        OpKind::VMov {
            dst: vector(15, case.width()),
            src: loaded,
            width: case.width(),
        },
    ));
    assert_rejected("loaded virtual escapes", &escaped);

    let mut redefined = base.clone();
    let redefined_id = OpId(redefined.blocks[0].ops.len() as u16);
    redefined.blocks[0].ops.push(SmirOp::new(
        redefined_id,
        PC + case.bytes().len() as u64,
        OpKind::VMov {
            dst: loaded,
            src: vector(15, case.width()),
            width: case.width(),
        },
    ));
    assert_rejected("loaded virtual redefined", &redefined);

    let mut trailing = base.clone();
    let trailing_id = OpId(trailing.blocks[0].ops.len() as u16);
    trailing.blocks[0]
        .ops
        .push(SmirOp::new(trailing_id, PC, OpKind::Nop));
    assert_rejected("same-PC trailing operation", &trailing);

    let mut preceding = base.clone();
    preceding.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0), PC, OpKind::Nop));
    assert_rejected("same-PC preceding operation", &preceding);

    let mut wrong_alignment_address = base.clone();
    let OpKind::X86CheckAlignmentAc { addr, .. } =
        &mut wrong_alignment_address.blocks[0].ops[1].kind
    else {
        unreachable!()
    };
    *addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)));
    assert_rejected("alignment address", &wrong_alignment_address);

    for (name, mutate) in [
        ("alignment access size", 0_u8),
        ("alignment boundary", 1),
        ("natural alignment", 2),
        ("stack segment", 3),
    ] {
        let mut function = base.clone();
        let OpKind::X86CheckAlignmentAc {
            access_size,
            alignment,
            natural_alignment,
            stack_segment,
            ..
        } = &mut function.blocks[0].ops[1].kind
        else {
            unreachable!()
        };
        match mutate {
            0 => *access_size = 16,
            1 => *alignment = 32,
            2 => *natural_alignment = true,
            3 => *stack_segment = !*stack_segment,
            _ => unreachable!(),
        }
        assert_rejected(name, &function);
    }

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    for (name, byte_index, value) in [
        ("map", 1, case.register_bytes()[1] & !0x1F | 2),
        ("mandatory prefix", 2, case.register_bytes()[2] & !3 | 2),
        ("opcode", 3, 0x47),
        ("register ModRM", 4, case.register_bytes()[4]),
    ] {
        let mut bytes = case.bytes();
        bytes[byte_index] = value;
        let mut function = base.clone();
        replace_bytes(&mut function, &bytes);
        assert_rejected(name, &function);
    }
}

#[test]
fn guard_and_helper_modes_remain_mandatory() {
    let case = all_cases()[0];
    let function = lift_case(case);

    let mut no_memory_helpers = X86_64Lowerer::new();
    no_memory_helpers.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        no_memory_helpers.lower_function(&function),
        Err(LowerError::UnsupportedOp { .. }) | Err(LowerError::InvalidOperand { .. })
    ));

    let mut no_fault_guards = X86_64Lowerer::new();
    no_fault_guards.set_mem_helpers(true);
    no_fault_guards.set_preserve_vector_mem_helpers(true);
    no_fault_guards.set_avx_ymm16_vector_state(true);
    assert!(matches!(
        no_fault_guards.lower_function(&function),
        Err(LowerError::UnsupportedOp { op })
            if op == "X86RequireXop requires JIT fault-deoptimization guards"
    ));
}
