//! Exact helper-backed VEX packed bit-test memory-source coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FunctionId, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitVexPtestMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_ptest_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::{HashMap, HashSet};

mod semantics;

const PC: u64 = 0x170E;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DEFINED_FLAG_FIXUP: [u8; 10] = [0x9C, 0x48, 0x81, 0x24, 0x24, 0x6B, 0xF7, 0xFF, 0xFF, 0x9D];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackedBitTest {
    Ptest,
    TestPs,
    TestPd,
}

impl PackedBitTest {
    fn opcode(self) -> u8 {
        match self {
            Self::Ptest => 0x17,
            Self::TestPs => 0x0E,
            Self::TestPd => 0x0F,
        }
    }

    fn tested_bits(self) -> Option<u64> {
        match self {
            Self::Ptest => None,
            Self::TestPs => Some(0x8000_0000_8000_0000),
            Self::TestPd => Some(0x8000_0000_0000_0000),
        }
    }

    fn supports_w(self, w: bool) -> bool {
        matches!(self, Self::Ptest) || !w
    }
}

const SHAPES: [(PackedBitTest, VecWidth, bool); 8] = [
    (PackedBitTest::Ptest, VecWidth::V128, false),
    (PackedBitTest::Ptest, VecWidth::V128, true),
    (PackedBitTest::Ptest, VecWidth::V256, false),
    (PackedBitTest::Ptest, VecWidth::V256, true),
    (PackedBitTest::TestPs, VecWidth::V128, false),
    (PackedBitTest::TestPs, VecWidth::V256, false),
    (PackedBitTest::TestPd, VecWidth::V128, false),
    (PackedBitTest::TestPd, VecWidth::V256, false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedBitTestMemoryCase {
    operation: PackedBitTest,
    width: VecWidth,
    w: bool,
    first_source: u8,
    base: u8,
    clear_ignored_x: bool,
}

impl PackedBitTestMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.first_source)
            .expect("one source leaves fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.operation.supports_w(self.w));
        assert!(self.first_source < 16 && self.base < 16);
        assert_ne!(self.base & 7, 4, "general cases use non-SIB bases");
        vec![
            0xC4,
            (if self.first_source < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7) | 0x78 | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode(),
            0x40 | ((self.first_source & 7) << 3) | (self.base & 7),
            DISP as u8,
        ]
    }

    fn register_bytes(self) -> [u8; 5] {
        [
            0xC4,
            (if self.first_source < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7) | 0x78 | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode(),
            0xC0 | ((self.first_source & 7) << 3) | (self.scratch() & 7),
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

fn expected_address(case: PackedBitTestMemoryCase) -> Address {
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
) -> Option<X86JitVexPtestMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_ptest_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(function: &SmirFunction) -> Option<X86JitVexPtestMemorySequence> {
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

fn lift_case(case: PackedBitTestMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_sequence(&function, case);
    let OpKind::VLoad { addr, width, .. } = &function.blocks[0].ops[0].kind else {
        panic!("{case:?}: exact sequence must start with VLoad")
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
    case: PackedBitTestMemoryCase,
) -> X86JitVexPtestMemorySequence {
    let sequence =
        classified(function).unwrap_or_else(|| panic!("{case:?}: exact sequence not classified"));
    let lanes = case.width.lanes(VecElementType::I64) as usize;
    let lane_ops = if case.operation.tested_bits().is_some() {
        8
    } else {
        6
    };
    assert_eq!(sequence.consumed, 13 + lanes * lane_ops, "{case:?}");
    assert_eq!(sequence.consumed, function.blocks[0].ops.len(), "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(
        sequence.encoding.first_source, case.first_source,
        "{case:?}"
    );
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(
        sequence.encoding.opcode,
        case.operation.opcode(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.tested_bits,
        case.operation.tested_bits(),
        "{case:?}"
    );
    assert_eq!(sequence.encoding.w, case.w, "{case:?}");
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
    assert_eq!(classified_at(function, 0, false), None, "{case:?}");
    sequence
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexPtestMemorySequence) {
    let sequence = classified(function).expect("classified VEX packed bit-test memory sequence");
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
    assert!(!requirements.needs_avx2);
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
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed packed bit-test lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer
        .finalize()
        .expect("finalize helper-backed VEX packed bit test");
    let mut expected_replay = sequence.encoding.register_instruction.as_slice().to_vec();
    expected_replay.extend_from_slice(&DEFINED_FLAG_FIXUP);
    assert!(
        code.windows(expected_replay.len())
            .any(|window| window == expected_replay),
        "missing exact packed bit-test register replay and defined-flag fixup"
    );
    (code, result.entry_offset, sequence)
}

#[test]
fn all_384_source_shape_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    let mut min_consumed = usize::MAX;
    let mut max_consumed = 0usize;
    for (shape_ordinal, (operation, width, w)) in SHAPES.into_iter().enumerate() {
        for first_source in 0..16 {
            let case = PackedBitTestMemoryCase {
                operation,
                width,
                w,
                first_source,
                base: if first_source & 1 == 0 { 3 } else { 11 },
                clear_ignored_x: (shape_ordinal + usize::from(first_source)) & 1 != 0,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let sequence = assert_exact_sequence(&function, case);
                let (code, _, lowered_sequence) = lower(&function);
                assert_eq!(sequence, lowered_sequence);
                assert!(
                    code.windows(5)
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
    assert_eq!(lowered, SHAPES.len() * 16 * LEVELS.len());
    assert_eq!((min_consumed, max_consumed), (25, 45));
}

#[test]
fn complete_segment_sib_rip_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[&[u8]] = &[
        &[0x64, 0xC4, 0x42, 0x79, 0x0E, 0x4B, 0x20],
        &[0x65, 0xC4, 0x02, 0x7D, 0x0F, 0x74, 0xEC, 0x20],
        &[
            0x67, 0xC4, 0x62, 0x79, 0x17, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44,
        ],
        &[0xC4, 0xE2, 0xFD, 0x17, 0x0D, 0x11, 0x22, 0x33, 0x44],
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
fn classifier_and_gate_fail_closed_for_graph_provenance_flags_and_escape_invariants() {
    let case = PackedBitTestMemoryCase {
        operation: PackedBitTest::TestPs,
        width: VecWidth::V256,
        w: false,
        first_source: 9,
        base: 11,
        clear_ignored_x: true,
    };
    let base = lift_case(case);
    let mut malformed = Vec::<(&str, SmirFunction)>::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source map", 1usize, 0x01u8),
        ("source mandatory prefix", 2, 0x03),
        ("source vector length", 2, 0x04),
        ("source reserved vvvv", 2, 0x08),
        ("source W", 2, 0x80),
        ("source opcode", 3, 0x01),
        ("source first register", 4, 0x08),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut register_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.pop();
    replace_instruction_bytes(&mut register_metadata, &bytes);
    malformed.push(("register-source metadata", register_metadata));

    let mut truncated_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes.pop();
    replace_instruction_bytes(&mut truncated_metadata, &bytes);
    malformed.push(("truncated source bytes", truncated_metadata));

    let mut trailing_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    replace_instruction_bytes(&mut trailing_metadata, &bytes);
    malformed.push(("trailing source byte", trailing_metadata));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing load hint", missing_load_hint));

    let mut graph_hint = base.clone();
    graph_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented graph hint", graph_hint));

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

    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::VLoad { dst, addr, width } = &mut function.blocks[0].ops[0].kind else {
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

    for offset in [1usize, 2] {
        for field in 0..3 {
            let mut function = base.clone();
            let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[offset].kind else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => *src = SrcOperand::Imm(1),
                2 => *width = OpWidth::W32,
                _ => unreachable!(),
            }
            malformed.push(("accumulator initialization field", function));
        }
    }

    for (offset, name) in [
        (3usize, "first extraction field"),
        (4, "second extraction field"),
    ] {
        for field in 0..5 {
            let mut function = base.clone();
            let OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem,
                sign,
            } = &mut function.blocks[0].ops[offset].kind
            else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => {
                    *vec = if offset == 3 {
                        loaded
                    } else {
                        vector(8, case.width)
                    }
                }
                2 => *lane ^= 1,
                3 => *elem = VecElementType::I32,
                4 => *sign = SignExtend::Sign,
                _ => unreachable!(),
            }
            malformed.push((name, function));
        }
    }

    for offset in [5usize, 6] {
        for field in 0..5 {
            let mut function = base.clone();
            let OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } = &mut function.blocks[0].ops[offset].kind
            else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => *src1 = loaded,
                2 => *src2 = SrcOperand::Imm(0),
                3 => *width = OpWidth::W32,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
            malformed.push(("tested-bit mask field", function));
        }
    }

    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } = &mut function.blocks[0].ops[7].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src1 = loaded,
            2 => *src2 = SrcOperand::Imm(0),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        }
        malformed.push(("intersection field", function));
    }

    for (offset, name) in [
        (8usize, "intersection reduction field"),
        (10, "outside reduction field"),
    ] {
        for field in 0..5 {
            let mut function = base.clone();
            let OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } = &mut function.blocks[0].ops[offset].kind
            else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => *src1 = loaded,
                2 => *src2 = SrcOperand::Imm(0),
                3 => *width = OpWidth::W32,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
            malformed.push((name, function));
        }
    }

    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::AndNot {
            dst,
            src1,
            src2,
            width,
            flags,
        } = &mut function.blocks[0].ops[9].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src1 = loaded,
            2 => *src2 = SrcOperand::Imm(0),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        }
        malformed.push(("outside field", function));
    }

    let tail = base.blocks[0].ops.len() - 10;
    let mut wrong_old_flags = base.clone();
    let OpKind::ReadFlags { dst } = &mut wrong_old_flags.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *dst = loaded;
    malformed.push(("preserved flag capture", wrong_old_flags));

    for (offset, name) in [
        (tail + 1, "ZF comparison field"),
        (tail + 3, "CF comparison field"),
    ] {
        for field in 0..3 {
            let mut function = base.clone();
            let OpKind::Cmp { src1, src2, width } = &mut function.blocks[0].ops[offset].kind else {
                unreachable!()
            };
            match field {
                0 => *src1 = loaded,
                1 => *src2 = SrcOperand::Imm(1),
                2 => *width = OpWidth::W32,
                _ => unreachable!(),
            }
            malformed.push((name, function));
        }
    }

    for (offset, name) in [
        (tail + 2, "ZF materialization field"),
        (tail + 4, "CF materialization field"),
    ] {
        for field in 0..3 {
            let mut function = base.clone();
            let OpKind::SetCC { dst, cond, width } = &mut function.blocks[0].ops[offset].kind
            else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => *cond = Condition::Ne,
                2 => *width = OpWidth::W32,
                _ => unreachable!(),
            }
            malformed.push((name, function));
        }
    }

    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::Shl {
            dst,
            src,
            amount,
            width,
            flags,
        } = &mut function.blocks[0].ops[tail + 5].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src = loaded,
            2 => *amount = SrcOperand::Imm(5),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        }
        malformed.push(("ZF flag-position field", function));
    }

    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } = &mut function.blocks[0].ops[tail + 6].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src1 = loaded,
            2 => *src2 = SrcOperand::Imm(!0x8D4),
            3 => *width = OpWidth::W32,
            4 => *flags = FlagUpdate::All,
            _ => unreachable!(),
        }
        malformed.push(("defined flag-clear field", function));
    }

    for (offset, name) in [(tail + 7, "CF merge field"), (tail + 8, "ZF merge field")] {
        for field in 0..5 {
            let mut function = base.clone();
            let OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } = &mut function.blocks[0].ops[offset].kind
            else {
                unreachable!()
            };
            match field {
                0 => *dst = loaded,
                1 => *src1 = loaded,
                2 => *src2 = SrcOperand::Imm(0),
                3 => *width = OpWidth::W32,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
            malformed.push((name, function));
        }
    }

    let mut wrong_flag_write = base.clone();
    let OpKind::WriteFlags { src } = &mut wrong_flag_write.blocks[0].ops[tail + 9].kind else {
        unreachable!()
    };
    *src = loaded;
    malformed.push(("final flag value", wrong_flag_write));

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
