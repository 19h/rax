//! Exact helper-backed VEX immediate-permute memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitVexImmediatePermuteMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_immediate_permute_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::{HashMap, HashSet};

mod semantics;

const PC: u64 = 0x1A41;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImmediatePermute {
    PermilPs,
    PermilPd,
    PermQ,
    PermPd,
}

impl ImmediatePermute {
    fn opcode(self) -> u8 {
        match self {
            Self::PermilPs => 0x04,
            Self::PermilPd => 0x05,
            Self::PermQ => 0x00,
            Self::PermPd => 0x01,
        }
    }

    fn elem(self) -> VecElementType {
        match self {
            Self::PermilPs => VecElementType::F32,
            Self::PermilPd | Self::PermPd => VecElementType::F64,
            Self::PermQ => VecElementType::I64,
        }
    }

    fn w(self) -> bool {
        matches!(self, Self::PermQ | Self::PermPd)
    }

    fn needs_avx2(self) -> bool {
        self.w()
    }

    fn supports(self, width: VecWidth) -> bool {
        matches!(width, VecWidth::V128 | VecWidth::V256)
            && (!self.needs_avx2() || width == VecWidth::V256)
    }
}

const SHAPES: [(ImmediatePermute, VecWidth); 6] = [
    (ImmediatePermute::PermilPs, VecWidth::V128),
    (ImmediatePermute::PermilPs, VecWidth::V256),
    (ImmediatePermute::PermilPd, VecWidth::V128),
    (ImmediatePermute::PermilPd, VecWidth::V256),
    (ImmediatePermute::PermQ, VecWidth::V256),
    (ImmediatePermute::PermPd, VecWidth::V256),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImmediatePermuteMemoryCase {
    operation: ImmediatePermute,
    width: VecWidth,
    destination: u8,
    base: u8,
    immediate: u8,
    clear_ignored_x: bool,
}

impl ImmediatePermuteMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination)
            .expect("one destination leaves fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.operation.supports(self.width));
        assert!(self.destination < 16 && self.base < 16);
        assert_ne!(self.base & 7, 4, "general cases use non-SIB bases");
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.operation.w()) << 7)
                | 0x78
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.operation.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
            self.immediate,
        ]
    }

    fn register_bytes(self) -> [u8; 6] {
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.operation.w()) << 7)
                | 0x78
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.operation.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
            self.immediate,
        ]
    }
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

fn expected_address(case: ImmediatePermuteMemoryCase) -> Address {
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

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexImmediatePermuteMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_immediate_permute_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(function: &SmirFunction) -> Option<X86JitVexImmediatePermuteMemorySequence> {
    classified_at(function, 0, true)
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

fn lift_case(case: ImmediatePermuteMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    let sequence = assert_exact_sequence(&function, case);
    let OpKind::VLoad { addr, width, .. } = &function.blocks[0].ops[sequence.load_offset].kind
    else {
        panic!("{case:?}: exact sequence must contain VLoad")
    };
    assert_eq!(addr, &expected_address(case), "{case:?}");
    assert_eq!(*width, case.width, "{case:?}");
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_sequence(
    function: &SmirFunction,
    case: ImmediatePermuteMemoryCase,
) -> X86JitVexImmediatePermuteMemorySequence {
    let sequence =
        classified(function).unwrap_or_else(|| panic!("{case:?}: exact sequence not classified"));
    let lanes = case.width.lanes(case.operation.elem()) as usize;
    assert_eq!(sequence.consumed, 4 + lanes * 2, "{case:?}");
    assert_eq!(sequence.load_offset, 2 + lanes * 2, "{case:?}");
    assert_eq!(sequence.consumed, function.blocks[0].ops.len(), "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(sequence.encoding.elem, case.operation.elem(), "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(
        sequence.encoding.opcode,
        case.operation.opcode(),
        "{case:?}"
    );
    assert_eq!(sequence.encoding.immediate, case.immediate, "{case:?}");
    assert_eq!(
        sequence.encoding.memory_size,
        case.width.bytes(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.needs_avx2,
        case.operation.needs_avx2(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.register_bytes(),
        "{case:?}"
    );
    assert_eq!(classified_at(function, 0, false), None, "{case:?}");
    sequence
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexImmediatePermuteMemorySequence) {
    let sequence = classified(function).expect("classified VEX immediate-permute memory sequence");
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
    assert_eq!(requirements.needs_avx2, sequence.encoding.needs_avx2);
    assert!(!requirements.needs_sse3);
    assert!(!requirements.needs_f16c);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_fma4);
    assert!(!requirements.needs_xop);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_gfni);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("helper-backed immediate-permute lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX immediate permute"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_288_destination_shape_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    let mut min_consumed = usize::MAX;
    let mut max_consumed = 0usize;
    for (shape_ordinal, (operation, width)) in SHAPES.into_iter().enumerate() {
        for destination in 0..16 {
            let case = ImmediatePermuteMemoryCase {
                operation,
                width,
                destination,
                base: if destination & 1 == 0 { 3 } else { 11 },
                immediate: (shape_ordinal as u8).wrapping_mul(41) ^ destination.wrapping_mul(29),
                clear_ignored_x: destination & 2 != 0,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let sequence = assert_exact_sequence(&function, case);
                let (code, _, lowered_sequence) = lower(&function);
                assert_eq!(sequence, lowered_sequence);
                assert!(
                    code.windows(6)
                        .any(|window| window == case.register_bytes()),
                    "{level:?} {case:?}: missing {:02X?}",
                    case.register_bytes()
                );
                min_consumed = min_consumed.min(sequence.consumed);
                max_consumed = max_consumed.max(sequence.consumed);
                lowered += 1;
            }
        }
    }
    assert_eq!(lowered, 6 * 16 * LEVELS.len());
    assert_eq!((min_consumed, max_consumed), (8, 20));
}

#[test]
fn complete_segment_sib_rip_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[&[u8]] = &[
        &[0x64, 0xC4, 0x43, 0x79, 0x04, 0x4B, 0x20, 0xA5],
        &[0x65, 0xC4, 0x03, 0xFD, 0x01, 0x74, 0xEC, 0x20, 0x1B],
        &[
            0x67, 0xC4, 0x63, 0x79, 0x05, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x3C,
        ],
        &[0xC4, 0xE3, 0xFD, 0x00, 0x0D, 0x11, 0x22, 0x33, 0x44, 0xFF],
    ];
    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified(&function)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(sequence.consumed, function.blocks[0].ops.len());
            let (code, _, _) = lower(&function);
            assert!(
                code.windows(sequence.encoding.register_instruction.as_slice().len())
                    .any(|window| window == sequence.encoding.register_instruction.as_slice()),
                "{level:?} {bytes:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified(function),
        None,
        "{name}: sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated encoding fits metadata"),
    );
}

#[test]
fn classifier_and_gate_fail_closed_for_graph_provenance_and_escape_invariants() {
    let case = ImmediatePermuteMemoryCase {
        operation: ImmediatePermute::PermilPs,
        width: VecWidth::V256,
        destination: 9,
        base: 11,
        immediate: 0xA5,
        clear_ignored_x: true,
    };
    let base = lift_case(case);
    let sequence = assert_exact_sequence(&base, case);
    let mut malformed = Vec::<(&str, SmirFunction)>::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source map", 1usize, 0x01u8),
        ("source mandatory prefix", 2, 0x03),
        ("source vector length", 2, 0x04),
        ("source reserved vvvv", 2, 0x08),
        ("source opcode", 3, 0x01),
        ("source destination", 4, 0x08),
        ("source immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut wrong_w = base.clone();
    let mut bytes = case.bytes();
    bytes[2] |= 0x80;
    replace_instruction_bytes(&mut wrong_w, &bytes);
    malformed.push(("source W", wrong_w));

    let mut register_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    replace_instruction_bytes(&mut register_metadata, &bytes);
    malformed.push(("register-source metadata", register_metadata));

    let mut missing_immediate = base.clone();
    let mut bytes = case.bytes();
    bytes.pop();
    replace_instruction_bytes(&mut missing_immediate, &bytes);
    malformed.push(("missing immediate", missing_immediate));

    let mut trailing_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    replace_instruction_bytes(&mut trailing_metadata, &bytes);
    malformed.push(("trailing source byte", trailing_metadata));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut op_hint = base.clone();
    op_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented graph hint", op_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7000), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    for index in 0..base.blocks[0].ops.len() {
        let mut function = base.clone();
        function.blocks[0].ops[index].kind = OpKind::Nop;
        malformed.push(("missing canonical graph node", function));
    }

    let loaded = match base.blocks[0].ops[sequence.load_offset].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[0].kind else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src = SrcOperand::Imm(1),
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        }
        malformed.push(("index clear field", function));
    }

    for field in 0..4 {
        let mut function = base.clone();
        let OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[1].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *scalar = loaded,
            2 => *elem = VecElementType::F64,
            3 => *lanes -= 1,
            _ => unreachable!(),
        }
        malformed.push(("index broadcast field", function));
    }

    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[2].kind else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src = SrcOperand::Imm(-1),
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        }
        malformed.push(("selector field", function));
    }

    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } = &mut function.blocks[0].ops[3].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *vec = loaded,
            2 => *scalar = loaded,
            3 => *lane ^= 1,
            4 => *elem = VecElementType::F64,
            _ => unreachable!(),
        }
        malformed.push(("selector insertion field", function));
    }

    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::VLoad { dst, addr, width } =
            &mut function.blocks[0].ops[sequence.load_offset].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = vector(8, case.width),
            1 => *addr = Address::Direct(VReg::Virtual(VirtualId(0xFF00))),
            2 => *width = VecWidth::V128,
            _ => unreachable!(),
        }
        malformed.push(("load field", function));
    }

    let permute_offset = sequence.load_offset + 1;
    for field in 0..7 {
        let mut function = base.clone();
        let OpKind::VPermute {
            dst,
            src1,
            src2,
            indices,
            elem,
            width,
            overwrite_table,
        } = &mut function.blocks[0].ops[permute_offset].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = vector(8, case.width),
            1 => *src1 = vector(8, case.width),
            2 => *src2 = Some(vector(8, case.width)),
            3 => *indices = vector(8, case.width),
            4 => *elem = VecElementType::F64,
            5 => *width = VecWidth::V128,
            6 => *overwrite_table = true,
            _ => unreachable!(),
        }
        malformed.push(("permute field", function));
    }

    let local_virtuals: HashSet<VReg> = base.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .filter(|register| matches!(register, VReg::Virtual(_)))
        .collect();
    for (ordinal, register) in local_virtuals.into_iter().enumerate() {
        let mut external_use = base.clone();
        external_use.blocks[0].ops.push(SmirOp::new(
            OpId(0x7100 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xF000 + ordinal as u32)),
                src: SrcOperand::Reg(register),
                width: OpWidth::W64,
            },
        ));
        malformed.push(("local virtual escapes sequence", external_use));

        let mut duplicate_definition = base.clone();
        duplicate_definition.blocks[0].ops.push(SmirOp::new(
            OpId(0x7200 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: register,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        malformed.push(("local virtual is redefined", duplicate_definition));
    }

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7300), PC, OpKind::Nop));
    assert_eq!(classified_at(&same_pc_head, 1, true), None);
    assert_rejected("unconsumed same-PC head", &same_pc_head);
}
