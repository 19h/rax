//! Exact helper-backed VEX variable-permute memory-source coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexVariablePermuteMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_variable_permute_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0xA11D;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VariablePermute {
    PermilPs,
    PermilPd,
    PermPs,
    PermD,
}

impl VariablePermute {
    const ALL: [Self; 4] = [Self::PermilPs, Self::PermilPd, Self::PermPs, Self::PermD];

    fn opcode(self) -> u8 {
        match self {
            Self::PermilPs => 0x0C,
            Self::PermilPd => 0x0D,
            Self::PermPs => 0x16,
            Self::PermD => 0x36,
        }
    }

    fn elem(self) -> VecElementType {
        match self {
            Self::PermilPs | Self::PermPs => VecElementType::F32,
            Self::PermilPd => VecElementType::F64,
            Self::PermD => VecElementType::I32,
        }
    }

    fn is_permil(self) -> bool {
        matches!(self, Self::PermilPs | Self::PermilPd)
    }

    fn needs_avx2(self) -> bool {
        matches!(self, Self::PermPs | Self::PermD)
    }

    fn supports(self, width: VecWidth) -> bool {
        matches!(width, VecWidth::V128 | VecWidth::V256)
            && (!self.needs_avx2() || width == VecWidth::V256)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VariablePermuteMemoryCase {
    operation: VariablePermute,
    width: VecWidth,
    destination: u8,
    source1: u8,
    base: u8,
    clear_ignored_x: bool,
}

impl VariablePermuteMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.operation.supports(self.width));
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        assert_ne!(self.base & 7, 4, "general cases use non-SIB bases");
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            (((!self.source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
        ]
    }

    fn emitted_bytes(self) -> [u8; 5] {
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 2,
            (((!self.source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
        ]
    }
}

fn cases() -> Vec<VariablePermuteMemoryCase> {
    let mut cases = Vec::with_capacity(1_536);
    for operation in VariablePermute::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            if !operation.supports(width) {
                continue;
            }
            for destination in 0..16u8 {
                for source1 in 0..16u8 {
                    cases.push(VariablePermuteMemoryCase {
                        operation,
                        width,
                        destination,
                        source1,
                        base: if destination.wrapping_add(source1) & 1 == 0 {
                            7
                        } else {
                            11
                        },
                        clear_ignored_x: destination.wrapping_mul(3).wrapping_add(source1) & 2 != 0,
                    });
                }
            }
        }
    }
    cases
}

fn x86(register: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(register))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!(),
    })
}

fn expected_address(case: VariablePermuteMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
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
) -> Option<X86JitVexVariablePermuteMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_variable_permute_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn expected_ops(case: VariablePermuteMemoryCase, level: OptLevel) -> usize {
    if !case.operation.is_permil() {
        return 2;
    }
    let lanes = case.width.lanes(case.operation.elem()) as usize;
    if case.operation == VariablePermute::PermilPs && level != OptLevel::O0 {
        4 + lanes * 4
    } else {
        4 + lanes * 5
    }
}

fn assert_exact_graph(function: &SmirFunction, case: VariablePermuteMemoryCase, level: OptLevel) {
    let block = &function.blocks[0];
    assert_eq!(
        block.ops.len(),
        expected_ops(case, level),
        "{level:?} {case:?}"
    );
    assert!(
        block.ops.iter().all(|op| op.guest_pc == PC),
        "{level:?} {case:?}"
    );
    assert!(
        block.ops.iter().all(|op| op.x86_hint.is_none()),
        "{level:?} {case:?}"
    );
    let loaded = match &block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{level:?} {case:?}");
            assert_eq!(*width, case.width, "{level:?} {case:?}");
            *loaded
        }
        other => panic!("{level:?} {case:?}: expected leading VLoad, got {other:?}"),
    };

    let final_op = &block.ops[block.ops.len() - 1].kind;
    if case.operation.is_permil() {
        let lanes = case.width.lanes(case.operation.elem()) as usize;
        assert_eq!(
            block
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VExtractLane { vec, .. } if vec == loaded))
                .count(),
            lanes,
            "{level:?} {case:?}"
        );
        assert_eq!(
            block
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
                .count(),
            lanes,
            "{level:?} {case:?}"
        );
        assert!(matches!(
            final_op,
            OpKind::VPermute {
                dst,
                src1,
                src2: None,
                indices: VReg::Virtual(_),
                elem,
                width,
                overwrite_table: false,
            } if *dst == vector(case.destination, case.width)
                && *src1 == vector(case.source1, case.width)
                && *elem == case.operation.elem()
                && *width == case.width
        ));
    } else {
        assert!(matches!(
            final_op,
            OpKind::VPermute {
                dst,
                src1,
                src2: None,
                indices,
                elem,
                width: VecWidth::V256,
                overwrite_table: false,
            } if *dst == vector(case.destination, case.width)
                && *src1 == loaded
                && *indices == vector(case.source1, case.width)
                && *elem == case.operation.elem()
        ));
    }

    let sequence =
        classified_sequence(function, true).expect("classified variable-permute sequence");
    assert_eq!(sequence.consumed, block.ops.len(), "{level:?} {case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
    assert_eq!(
        sequence.encoding.elem,
        case.operation.elem(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.destination, case.destination,
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.source1, case.source1,
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.scratch,
        case.scratch(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.opcode,
        case.operation.opcode(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.memory_size,
        case.width.bytes(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.needs_avx2,
        case.operation.needs_avx2(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.emitted_bytes(),
        "{level:?} {case:?}"
    );
    assert_eq!(
        classified_sequence(function, false),
        None,
        "{level:?} {case:?}"
    );
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn lift_case(case: VariablePermuteMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case, OptLevel::O0);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(
    function: &SmirFunction,
    case: VariablePermuteMemoryCase,
) -> (Vec<u8>, usize, X86JitVexVariablePermuteMemorySequence) {
    let sequence =
        classified_sequence(function, true).expect("classified variable-permute sequence");
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
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx2,
        case.operation.needs_avx2(),
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX variable permute failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX variable permute"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_4_608_family_width_operand_alias_and_optimization_cells_lower_exactly() {
    let cases = cases();
    assert_eq!(cases.len(), 1_536);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case, level);
            let (code, _, sequence) = lower(&function, case);
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.emitted_bytes(),
                "{level:?} {case:?}"
            );
            assert!(
                code.windows(5).any(|window| window == case.emitted_bytes()),
                "{level:?} {case:?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 4_608);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    for (case, memory, register) in [
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPs,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                base: 7,
                clear_ignored_x: false,
            },
            &[0xC4, 0xE2, 0x69, 0x0C, 0x4F, 0x20][..],
            &[0xC4, 0xE2, 0x69, 0x0C, 0xC8][..],
        ),
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPs,
                width: VecWidth::V256,
                destination: 9,
                source1: 10,
                base: 11,
                clear_ignored_x: false,
            },
            &[0xC4, 0x42, 0x2D, 0x0C, 0x4B, 0x20][..],
            &[0xC4, 0x62, 0x2D, 0x0C, 0xC8][..],
        ),
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPd,
                width: VecWidth::V128,
                destination: 15,
                source1: 15,
                base: 14,
                clear_ignored_x: false,
            },
            &[0xC4, 0x42, 0x01, 0x0D, 0x7E, 0x20][..],
            &[0xC4, 0x62, 0x01, 0x0D, 0xF8][..],
        ),
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPd,
                width: VecWidth::V256,
                destination: 1,
                source1: 2,
                base: 7,
                clear_ignored_x: false,
            },
            &[0xC4, 0xE2, 0x6D, 0x0D, 0x4F, 0x20][..],
            &[0xC4, 0xE2, 0x6D, 0x0D, 0xC8][..],
        ),
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermPs,
                width: VecWidth::V256,
                destination: 9,
                source1: 10,
                base: 11,
                clear_ignored_x: false,
            },
            &[0xC4, 0x42, 0x2D, 0x16, 0x4B, 0x20][..],
            &[0xC4, 0x62, 0x2D, 0x16, 0xC8][..],
        ),
        (
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermD,
                width: VecWidth::V256,
                destination: 15,
                source1: 15,
                base: 14,
                clear_ignored_x: false,
            },
            &[0xC4, 0x42, 0x05, 0x36, 0x7E, 0x20][..],
            &[0xC4, 0x62, 0x05, 0x36, 0xF8][..],
        ),
    ] {
        assert_eq!(case.bytes(), memory, "{case:?}");
        assert_eq!(case.emitted_bytes(), register, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[(&[u8], VariablePermuteMemoryCase)] = &[
        (
            &[0xC4, 0xE2, 0x75, 0x0C, 0x0D, 0x11, 0x22, 0x33, 0x44],
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPs,
                width: VecWidth::V256,
                destination: 1,
                source1: 1,
                base: 0,
                clear_ignored_x: false,
            },
        ),
        (
            &[0x64, 0x67, 0xC4, 0xE2, 0x69, 0x0D, 0x4C, 0x70, 0x01],
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermilPd,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                base: 0,
                clear_ignored_x: false,
            },
        ),
        (
            &[
                0x65, 0x67, 0xC4, 0x02, 0x2D, 0x16, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
            ],
            VariablePermuteMemoryCase {
                operation: VariablePermute::PermPs,
                width: VecWidth::V256,
                destination: 14,
                source1: 10,
                base: 0,
                clear_ignored_x: true,
            },
        ),
    ];

    let mut lowered = 0usize;
    for (bytes, case) in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (code, _, sequence) = lower(&function, *case);
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_eq!(sequence.encoding.opcode, case.operation.opcode());
            assert_eq!(sequence.encoding.width, case.width);
            let register = sequence.encoding.register_instruction.as_slice();
            assert!(
                code.windows(register.len())
                    .any(|window| window == register)
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed variable-permute graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed variable-permute graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed variable-permute graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated encoding fits metadata"),
    );
}

#[test]
fn full_width_graph_fails_closed_for_provenance_roles_and_shared_invariants() {
    let case = VariablePermuteMemoryCase {
        operation: VariablePermute::PermD,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        base: 11,
        clear_ignored_x: false,
    };
    let base = lift_case(case);
    let loaded = base.blocks[0].ops[0].kind.dests()[0];
    let mut malformed = Vec::new();

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_bytes));
    for (name, byte_index, xor) in [
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded map", 1, 0x01),
        ("encoded prefix", 2, 0x02),
        ("encoded W", 2, 0x80),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }
    let mut register_source = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    replace_instruction_bytes(&mut register_source, &bytes);
    malformed.push(("register-source provenance", register_source));

    let mut invented_hint = base.clone();
    invented_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented load hint", invented_hint));
    let mut wrong_load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_load_width));
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    for (name, field) in [
        ("permute destination", 0u8),
        ("permute table", 1),
        ("permute second table", 2),
        ("permute indices", 3),
        ("permute element", 4),
        ("permute width", 5),
        ("permute overwrite", 6),
    ] {
        let mut function = base.clone();
        if let OpKind::VPermute {
            dst,
            src1,
            src2,
            indices,
            elem,
            width,
            overwrite_table,
        } = &mut function.blocks[0].ops[1].kind
        {
            match field {
                0 => *dst = vector(8, VecWidth::V256),
                1 => *src1 = vector(11, VecWidth::V256),
                2 => *src2 = Some(vector(12, VecWidth::V256)),
                3 => *indices = vector(11, VecWidth::V256),
                4 => *elem = VecElementType::F32,
                5 => *width = VecWidth::V128,
                6 => *overwrite_table = true,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));
    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value escapes sequence", external_use));
    let mut duplicate_definition = base;
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFB),
        PC + 1,
        OpKind::VLoad {
            dst: loaded,
            addr: Address::Direct(x86(X86Reg::Rax)),
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value defined twice", duplicate_definition));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn permil_graph_fails_closed_for_every_selector_stage_and_optimizer_frontier() {
    let case = VariablePermuteMemoryCase {
        operation: VariablePermute::PermilPs,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        base: 11,
        clear_ignored_x: false,
    };
    let base = lift_case(case);
    let loaded = base.blocks[0].ops[0].kind.dests()[0];
    let zero = base.blocks[0].ops[1].kind.dests()[0];
    let indices = base.blocks[0].ops[2].kind.dests()[0];
    let control = base.blocks[0].ops[3].kind.dests()[0];
    let shifted = base.blocks[0].ops[4].kind.dests()[0];
    let selected = base.blocks[0].ops[5].kind.dests()[0];
    let absolute = base.blocks[0].ops[6].kind.dests()[0];
    let final_index = base.blocks[0].ops.len() - 1;
    let mut malformed = Vec::new();

    for (name, field) in [
        ("zero destination", 0u8),
        ("zero value", 1),
        ("zero width", 2),
    ] {
        let mut function = base.clone();
        if let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[1].kind {
            match field {
                0 => *dst = loaded,
                1 => *src = SrcOperand::Imm(1),
                2 => *width = OpWidth::W32,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("broadcast destination", 0u8),
        ("broadcast scalar", 1),
        ("broadcast element", 2),
        ("broadcast lanes", 3),
    ] {
        let mut function = base.clone();
        if let OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[2].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *scalar = loaded,
                2 => *elem = VecElementType::F64,
                3 => *lanes = 7,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("extract destination", 0u8),
        ("extract source", 1),
        ("extract lane", 2),
        ("extract element", 3),
        ("extract sign", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } = &mut function.blocks[0].ops[3].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *vec = vector(11, VecWidth::V256),
                2 => *lane = 1,
                3 => *elem = VecElementType::F64,
                4 => *sign = SignExtend::Sign,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("shift destination", 0u8),
        ("shift source", 1),
        ("shift width", 2),
    ] {
        let mut function = base.clone();
        if let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[4].kind {
            match field {
                0 => *dst = loaded,
                1 => *src = SrcOperand::Reg(zero),
                2 => *width = OpWidth::W32,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("and destination", 0u8),
        ("and source", 1),
        ("and mask", 2),
        ("and width", 3),
        ("and flags", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } = &mut function.blocks[0].ops[5].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *src1 = control,
                2 => *src2 = SrcOperand::Imm(2),
                3 => *width = OpWidth::W32,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("or destination", 0u8),
        ("or source", 1),
        ("or lane base", 2),
        ("or width", 3),
        ("or flags", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        } = &mut function.blocks[0].ops[6].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *src1 = shifted,
                2 => *src2 = SrcOperand::Imm(1),
                3 => *width = OpWidth::W32,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("insert destination", 0u8),
        ("insert vector", 1),
        ("insert scalar", 2),
        ("insert lane", 3),
        ("insert element", 4),
    ] {
        let mut function = base.clone();
        if let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } = &mut function.blocks[0].ops[7].kind
        {
            match field {
                0 => *dst = loaded,
                1 => *vec = loaded,
                2 => *scalar = selected,
                3 => *lane = 1,
                4 => *elem = VecElementType::F64,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }
    for (name, field) in [
        ("final destination", 0u8),
        ("final table", 1),
        ("final second table", 2),
        ("final indices", 3),
        ("final element", 4),
        ("final width", 5),
        ("final overwrite", 6),
    ] {
        let mut function = base.clone();
        if let OpKind::VPermute {
            dst,
            src1,
            src2,
            indices: actual_indices,
            elem,
            width,
            overwrite_table,
        } = &mut function.blocks[0].ops[final_index].kind
        {
            match field {
                0 => *dst = vector(8, VecWidth::V256),
                1 => *src1 = vector(11, VecWidth::V256),
                2 => *src2 = Some(vector(12, VecWidth::V256)),
                3 => *actual_indices = loaded,
                4 => *elem = VecElementType::F64,
                5 => *width = VecWidth::V128,
                6 => *overwrite_table = true,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let mut external_indices = base.clone();
    external_indices.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFA),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: indices,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("indices escape sequence", external_indices));
    let mut duplicate_absolute = base;
    duplicate_absolute.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FF9),
        PC + 1,
        OpKind::Mov {
            dst: absolute,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("selector defined twice", duplicate_absolute));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let pd_case = VariablePermuteMemoryCase {
        operation: VariablePermute::PermilPd,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        base: 7,
        clear_ignored_x: false,
    };
    let mut wrong_shift = lift_case(pd_case);
    if let OpKind::Shr { amount, .. } = &mut wrong_shift.blocks[0].ops[4].kind {
        *amount = SrcOperand::Imm(2);
    }
    assert_rejected("VPERMILPD control bit", &wrong_shift);
}
