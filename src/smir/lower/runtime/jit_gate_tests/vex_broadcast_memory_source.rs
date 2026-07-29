//! Exact helper-backed VEX memory-broadcast coverage.

use std::collections::HashMap;

use super::*;
#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexBroadcastMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_broadcast_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE5A0;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    opcode: u8,
    elem: VecElementType,
    source_lanes: u8,
    width: VecWidth,
    needs_avx2: bool,
}

const SHAPES: [Shape; 13] = [
    Shape {
        opcode: 0x18,
        elem: VecElementType::F32,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x18,
        elem: VecElementType::F32,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x19,
        elem: VecElementType::F64,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x1A,
        elem: VecElementType::F32,
        source_lanes: 4,
        width: VecWidth::V256,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x58,
        elem: VecElementType::I32,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x58,
        elem: VecElementType::I32,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x59,
        elem: VecElementType::I64,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x59,
        elem: VecElementType::I64,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x5A,
        elem: VecElementType::I32,
        source_lanes: 4,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x78,
        elem: VecElementType::I8,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x78,
        elem: VecElementType::I8,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x79,
        elem: VecElementType::I16,
        source_lanes: 1,
        width: VecWidth::V128,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x79,
        elem: VecElementType::I16,
        source_lanes: 1,
        width: VecWidth::V256,
        needs_avx2: true,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BroadcastCase {
    shape: Shape,
    destination: u8,
    base: u8,
}

impl BroadcastCase {
    fn memory_size(self) -> u32 {
        u32::from(self.shape.source_lanes) * self.shape.elem.bytes()
    }

    fn consumed(self) -> usize {
        if self.shape.source_lanes == 1 { 9 } else { 34 }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.base < 16);
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            0x78 | (u8::from(self.shape.width == VecWidth::V256) << 2) | 1,
            self.shape.opcode,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX broadcast has only 128-/256-bit destinations"),
    })
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexBroadcastMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_broadcast_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexBroadcastMemorySequence> {
    classified_at(function, 0, allow_mem)
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
        X86InstructionBytes::new(bytes).expect("x86 instruction fits metadata"),
    );
    function
}

fn lift_case(case: BroadcastCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_sequence(function: &SmirFunction, case: BroadcastCase) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), case.consumed(), "{case:?}");
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none()),
        "{case:?}"
    );
    assert!(matches!(
        &ops[0].kind,
        OpKind::Lea {
            dst: VReg::Virtual(_),
            addr: Address::BaseOffset {
                base,
                offset: DISP,
                disp_size: DispSize::Disp8,
            },
        } if *base == x86(X86Reg::gpr(case.base))
    ));
    assert!(matches!(
        ops.last().map(|op| &op.kind),
        Some(OpKind::VMov { dst, width, .. })
            if *dst == vector(case.destination, case.shape.width)
                && *width == case.shape.width
    ));
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexBroadcastMemorySequence {
            consumed: case.consumed(),
            memory_size: case.memory_size(),
            destination: case.destination,
            elem: case.shape.elem,
            source_lanes: case.shape.source_lanes,
            width: case.shape.width,
            opcode: case.shape.opcode,
            needs_avx2: case.shape.needs_avx2,
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(function: &SmirFunction, case: BroadcastCase) -> (Vec<u8>, usize) {
    assert_exact_sequence(function, case);
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
    assert_eq!(requirements.needs_avx2, case.shape.needs_avx2, "{case:?}");
    assert!(!requirements.needs_sse3, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512cd, "{case:?}");
    assert!(!requirements.needs_gfni, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX broadcast failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX broadcast"),
        result.entry_offset,
    )
}

fn lower_unclassified_address_sample(function: &SmirFunction) -> Vec<u8> {
    assert!(classified_sequence(function, true).is_some());
    assert!(is_native_clobber_safe_excluding(
        function,
        &HashMap::new(),
        true
    ));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer
        .lower_function(function)
        .expect("lower address-shape VEX broadcast");
    lowerer
        .finalize()
        .expect("finalize address-shape broadcast")
}

#[test]
fn all_1248_shape_destination_base_and_optimization_cells_admit_and_lower() {
    let mut lowered = 0usize;
    for shape in SHAPES {
        for destination in 0..16 {
            for base in [3, 12] {
                let case = BroadcastCase {
                    shape,
                    destination,
                    base,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let (code, _) = lower(&function, case);
                    assert!(!code.is_empty(), "{level:?} {case:?}");
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, 13 * 16 * 2 * LEVELS.len());
}

#[test]
fn llvm_23_host_sequences_are_emitted_exactly_for_all_13_forms() {
    let expected: [&[u8]; 13] = [
        &[0xC5, 0x78, 0xC6, 0xC8, 0x00],
        &[
            0xC4, 0x63, 0x7D, 0x18, 0xC8, 0x01, 0xC4, 0x41, 0x34, 0xC6, 0xC9, 0x00,
        ],
        &[
            0xC4, 0x63, 0x7D, 0x18, 0xC8, 0x01, 0xC4, 0x41, 0x35, 0xC6, 0xC9, 0x00,
        ],
        &[0xC4, 0x63, 0x7D, 0x18, 0xC8, 0x01],
        &[0xC4, 0x62, 0x79, 0x58, 0xC8],
        &[0xC4, 0x62, 0x7D, 0x58, 0xC8],
        &[0xC4, 0x62, 0x79, 0x59, 0xC8],
        &[0xC4, 0x62, 0x7D, 0x59, 0xC8],
        &[0xC4, 0x63, 0x7D, 0x38, 0xC8, 0x01],
        &[0xC4, 0x62, 0x79, 0x78, 0xC8],
        &[0xC4, 0x62, 0x7D, 0x78, 0xC8],
        &[0xC4, 0x62, 0x79, 0x79, 0xC8],
        &[0xC4, 0x62, 0x7D, 0x79, 0xC8],
    ];

    for (shape, expected) in SHAPES.into_iter().zip(expected) {
        let case = BroadcastCase {
            shape,
            destination: 9,
            base: 3,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{case:?}: missing LLVM 23 sequence {expected:02X?}"
        );
    }
}

#[test]
fn rip_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        &[0xC4, 0x62, 0x79, 0x18, 0x4B, 0x20],
        &[0xC4, 0x02, 0x7D, 0x19, 0xBC, 0xEC, 0x44, 0x33, 0x22, 0x11],
        &[
            0x64, 0xC4, 0x62, 0x7D, 0x1A, 0x34, 0x8D, 0x44, 0x33, 0x22, 0x11,
        ],
        &[0xC4, 0x62, 0x79, 0x58, 0x2D, 0x44, 0x33, 0x22, 0x11],
        &[0x65, 0xC4, 0x42, 0x7D, 0x59, 0x62, 0xE0],
        &[0x67, 0xC4, 0x82, 0x7D, 0x5A, 0x5C, 0x48, 0x20],
        &[0xC4, 0x42, 0x79, 0x78, 0x53, 0x20],
        &[0x67, 0xC4, 0xE2, 0x7D, 0x79, 0x4C, 0x77, 0x20],
    ];
    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(
                sequence.memory_size,
                u32::from(sequence.source_lanes) * sequence.elem.bytes(),
                "{level:?} {bytes:02X?}"
            );
            assert!(!lower_unclassified_address_sample(&function).is_empty());
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed IR"
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_scalar_graph_mutations() {
    let case = BroadcastCase {
        shape: SHAPES[4],
        destination: 9,
        base: 11,
    };
    let base = lift_case(case);
    assert_exact_sequence(&base, case);
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC));
    malformed.push(("missing source bytes", missing_metadata));

    let mut wrong_destination = base.clone();
    let mut bytes = case.bytes();
    bytes[4] ^= 0x08;
    wrong_destination
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte destination", wrong_destination));

    let mut wrong_opcode = base.clone();
    let mut bytes = case.bytes();
    bytes[3] = 0x59;
    wrong_opcode
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte element", wrong_opcode));

    let mut w1 = base.clone();
    let mut bytes = case.bytes();
    bytes[2] |= 0x80;
    w1.x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte W=1", w1));

    let mut nonreserved_vvvv = base.clone();
    let mut bytes = case.bytes();
    bytes[2] &= !0x08;
    nonreserved_vvvv
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte reserved vvvv", nonreserved_vvvv));

    let mut first_hint = base.clone();
    first_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: case.shape.opcode,
        width: case.shape.width,
        w: false,
    });
    malformed.push(("invented first hint", first_hint));

    let mut virtual_address = base.clone();
    if let OpKind::Lea { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xE101)));
    }
    malformed.push(("virtual helper address", virtual_address));

    let mut source_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut source_zero.blocks[0].ops[1].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("source zero", source_zero));

    let mut source_broadcast = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut source_broadcast.blocks[0].ops[2].kind {
        *lanes -= 1;
    }
    malformed.push(("source broadcast lanes", source_broadcast));

    let mut lane_load_offset = base.clone();
    if let OpKind::Load {
        addr: Address::BaseOffset { offset, .. },
        ..
    } = &mut lane_load_offset.blocks[0].ops[4].kind
    {
        *offset = 1;
    }
    malformed.push(("lane load offset", lane_load_offset));

    let mut lane_load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut lane_load_width.blocks[0].ops[4].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("lane load width", lane_load_width));

    let mut lane_load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut lane_load_sign.blocks[0].ops[4].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("lane load sign", lane_load_sign));

    let mut source_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut source_insert_lane.blocks[0].ops[5].kind {
        *lane = 1;
    }
    malformed.push(("source insert lane", source_insert_lane));

    let mut extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut extract_lane.blocks[0].ops[6].kind {
        *lane = 1;
    }
    malformed.push(("extract lane", extract_lane));

    let mut result_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut result_lanes.blocks[0].ops[7].kind {
        *lanes -= 1;
    }
    malformed.push(("result lanes", result_lanes));

    let mut result_destination = base.clone();
    if let OpKind::VMov { dst, .. } = &mut result_destination.blocks[0].ops[8].kind {
        *dst = vector(8, case.shape.width);
    }
    malformed.push(("result destination", result_destination));

    let source = match base.blocks[0].ops[2].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaping_source = base.clone();
    escaping_source.blocks[0].ops.push(SmirOp::new(
        OpId(0xE102),
        PC + 1,
        OpKind::VMov {
            dst: vector(2, VecWidth::V128),
            src: source,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("source virtual escapes", escaping_source));

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[4].guest_pc += 1;
    malformed.push(("split guest PC", split_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0xE103), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xE104), PC, OpKind::Nop));
    assert_eq!(classified_at(&same_pc_head, 1, true), None);
    assert_rejected("same-PC head", &same_pc_head);
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_block_graph_mutations() {
    let case = BroadcastCase {
        shape: SHAPES[8],
        destination: 9,
        base: 11,
    };
    let base = lift_case(case);
    assert_exact_sequence(&base, case);
    let mut malformed = Vec::new();

    let mut fourth_load_offset = base.clone();
    if let OpKind::Load {
        addr: Address::BaseOffset { offset, .. },
        ..
    } = &mut fourth_load_offset.blocks[0].ops[13].kind
    {
        *offset = 8;
    }
    malformed.push(("fourth source offset", fourth_load_offset));

    let mut source_insert_element = base.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut source_insert_element.blocks[0].ops[14].kind {
        *elem = VecElementType::I64;
    }
    malformed.push(("source insert element", source_insert_element));

    let mut result_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut result_zero.blocks[0].ops[15].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("result zero", result_zero));

    let mut result_zero_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut result_zero_lanes.blocks[0].ops[16].kind {
        *lanes -= 1;
    }
    malformed.push(("result zero lanes", result_zero_lanes));

    let mut result_extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut result_extract_lane.blocks[0].ops[23].kind {
        *lane = 0;
    }
    malformed.push(("cyclic result extract lane", result_extract_lane));

    let mut result_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut result_insert_lane.blocks[0].ops[24].kind {
        *lane = 0;
    }
    malformed.push(("result insert lane", result_insert_lane));

    let mut result_insert_vector = base.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut result_insert_vector.blocks[0].ops[24].kind {
        *vec = VReg::Virtual(VirtualId(0xE201));
    }
    malformed.push(("result insert chain", result_insert_vector));

    let mut result_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut result_insert_scalar.blocks[0].ops[32].kind {
        *scalar = VReg::Virtual(VirtualId(0xE202));
    }
    malformed.push(("result insert scalar", result_insert_scalar));

    let mut wrong_result_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut wrong_result_width.blocks[0].ops[33].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("result width", wrong_result_width));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[cfg(target_arch = "x86_64")]
fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn source_bytes(case: BroadcastCase, ordinal: usize) -> [u8; 64] {
    const BYTE_VALUES: [u8; 8] = [0x00, 0x01, 0x7F, 0x80, 0xFF, 0xFE, 0x55, 0xAA];
    const WORD_VALUES: [u16; 8] = [
        0x0000, 0x0001, 0x7FFF, 0x8000, 0xFFFF, 0xFFFE, 0x5555, 0xAAAA,
    ];
    const DWORD_VALUES: [u32; 8] = [
        0x0000_0000,
        0x8000_0000,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC1_2345,
        0x7F81_2345,
        0x0000_0001,
        0xDEAD_BEEF,
    ];
    const QWORD_VALUES: [u64; 8] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0001_2345,
        0x7FF0_0000_0001_2345,
        0x0000_0000_0000_0001,
        0xDEAD_BEEF_CAFE_BABE,
    ];
    let mut bytes = std::array::from_fn(|index| 0xA5 ^ (index as u8).wrapping_mul(0x1D));
    for lane in 0..usize::from(case.shape.source_lanes) {
        let selector = (lane + ordinal) % 8;
        let offset = lane * case.shape.elem.bytes() as usize;
        match case.shape.elem {
            VecElementType::I8 => bytes[offset] = BYTE_VALUES[selector],
            VecElementType::I16 => {
                bytes[offset..offset + 2].copy_from_slice(&WORD_VALUES[selector].to_le_bytes());
            }
            VecElementType::I32 | VecElementType::F32 => {
                bytes[offset..offset + 4].copy_from_slice(&DWORD_VALUES[selector].to_le_bytes());
            }
            VecElementType::I64 | VecElementType::F64 => {
                bytes[offset..offset + 8].copy_from_slice(&QWORD_VALUES[selector].to_le_bytes());
            }
            _ => unreachable!(),
        }
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn architectural_result(case: BroadcastCase, source: &[u8; 64]) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    let element_bytes = case.shape.elem.bytes() as usize;
    let destination_lanes = case.shape.width.bytes() as usize / element_bytes;
    for lane in 0..destination_lanes {
        let source_lane = lane % usize::from(case.shape.source_lanes);
        let source_offset = source_lane * element_bytes;
        let destination_offset = lane * element_bytes;
        bytes[destination_offset..destination_offset + element_bytes]
            .copy_from_slice(&source[source_offset..source_offset + element_bytes]);
    }
    bytes_to_words(bytes)
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u8; 64],
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
        || !matches!(size, 1 | 2 | 4 | 8 | 16)
    {
        return 0;
    }
    let mut bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    bytes[..size as usize].copy_from_slice(&context.value[..size as usize]);
    state.vector_scratch = bytes_to_words(bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: BroadcastCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
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
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreted_expected(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u8; 64],
    address: u64,
    case: BroadcastCase,
) -> GuestRegs {
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
    memory.load(address as usize, &source[..case.memory_size() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in expected.zmm.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    let mut scratch = [0u8; 64];
    scratch[..case.memory_size() as usize].copy_from_slice(&source[..case.memory_size() as usize]);
    expected.vector_scratch = bytes_to_words(scratch);
    expected
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<BroadcastCase> {
    const OPERANDS: [(u8, u8); 8] = [
        (0, 3),
        (1, 4),
        (15, 5),
        (9, 12),
        (8, 13),
        (7, 4),
        (4, 5),
        (12, 11),
    ];
    let mut cases = Vec::new();
    for shape in SHAPES {
        for (destination, base) in OPERANDS {
            cases.push(BroadcastCase {
                shape,
                destination,
                base,
            });
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: BroadcastCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, case.memory_size(), "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_broadcasts_match_manual_bits_and_interpreter_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory-broadcast differential: host lacks AVX");
        return;
    }
    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases: Vec<_> = native_cases()
        .into_iter()
        .filter(|case| !case.shape.needs_avx2 || avx2)
        .collect();
    let expected_cases = if avx2 { 13 * 8 } else { 4 * 8 };
    assert_eq!(cases.len(), expected_cases);
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    eprintln!("executing {expected_executions} native VEX broadcast success/fault pairs");

    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = source_bytes(case, ordinal);
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let mut memory_context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut memory_context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = interpreted_expected(&function, &initial, source, address, case);
            assert_eq!(
                expected.zmm[usize::from(case.destination)],
                architectural_result(case, &source),
                "{level:?} {case:?}: interpreter versus broadcast equations"
            );

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_observation(&memory_context, address, case, "success");
            successes += 1;

            let mut memory_context = VectorMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut memory_context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_helper_observation(&memory_context, address, case, "fault");
            faults += 1;
        }
    }

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
