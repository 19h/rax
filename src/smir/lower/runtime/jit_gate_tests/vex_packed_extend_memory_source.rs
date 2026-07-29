//! Exact helper-backed VEX packed sign/zero-extension memory-source coverage.

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
    X86JitVexPackedExtendMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_packed_extend_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE470;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operation {
    opcode: u8,
    source_element: VecElementType,
    destination_element: VecElementType,
    signed: bool,
}

const OPERATIONS: [Operation; 12] = [
    Operation {
        opcode: 0x20,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I16,
        signed: true,
    },
    Operation {
        opcode: 0x21,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I32,
        signed: true,
    },
    Operation {
        opcode: 0x22,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I64,
        signed: true,
    },
    Operation {
        opcode: 0x23,
        source_element: VecElementType::I16,
        destination_element: VecElementType::I32,
        signed: true,
    },
    Operation {
        opcode: 0x24,
        source_element: VecElementType::I16,
        destination_element: VecElementType::I64,
        signed: true,
    },
    Operation {
        opcode: 0x25,
        source_element: VecElementType::I32,
        destination_element: VecElementType::I64,
        signed: true,
    },
    Operation {
        opcode: 0x30,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I16,
        signed: false,
    },
    Operation {
        opcode: 0x31,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I32,
        signed: false,
    },
    Operation {
        opcode: 0x32,
        source_element: VecElementType::I8,
        destination_element: VecElementType::I64,
        signed: false,
    },
    Operation {
        opcode: 0x33,
        source_element: VecElementType::I16,
        destination_element: VecElementType::I32,
        signed: false,
    },
    Operation {
        opcode: 0x34,
        source_element: VecElementType::I16,
        destination_element: VecElementType::I64,
        signed: false,
    },
    Operation {
        opcode: 0x35,
        source_element: VecElementType::I32,
        destination_element: VecElementType::I64,
        signed: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtendCase {
    operation: Operation,
    width: VecWidth,
    w: bool,
    destination: u8,
    base: u8,
}

impl ExtendCase {
    fn lanes(self) -> usize {
        self.width.lanes(self.operation.destination_element) as usize
    }

    fn memory_size(self) -> u32 {
        (self.lanes() as u32) * self.operation.source_element.bytes()
    }

    fn source_width(self) -> VecWidth {
        if self.memory_size() <= 8 {
            VecWidth::V64
        } else {
            VecWidth::V128
        }
    }

    fn consumed(self) -> usize {
        5 + 5 * self.lanes()
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination)
            .expect("one VEX destination leaves fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.base < 16);
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7) | 0x78 | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    fn emitted_register_bytes(self) -> [u8; 5] {
        let scratch = self.scratch();
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7) | 0x78 | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.operation.opcode,
            0xC0 | ((self.destination & 7) << 3) | (scratch & 7),
        ]
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX packed extension has only 128-/256-bit destinations"),
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
) -> Option<X86JitVexPackedExtendMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_packed_extend_memory_sequence(
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
) -> Option<X86JitVexPackedExtendMemorySequence> {
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

fn lift_case(case: ExtendCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: ExtendCase) {
    let ops = &function.blocks[0].ops;
    let lanes = case.lanes();
    assert_eq!(ops.len(), case.consumed(), "{case:?}");
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );

    let source_zero = match ops[0].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => zero,
        ref other => panic!("{case:?}: source zero {other:?}"),
    };
    let source = match ops[1].kind {
        OpKind::VBroadcast {
            dst: source @ VReg::Virtual(_),
            scalar,
            elem,
            lanes,
        } => {
            assert_eq!(scalar, source_zero, "{case:?}");
            assert_eq!(elem, case.operation.source_element, "{case:?}");
            assert_eq!(
                lanes,
                case.source_width().lanes(case.operation.source_element) as u8,
                "{case:?}"
            );
            source
        }
        ref other => panic!("{case:?}: source broadcast {other:?}"),
    };
    let address_base = match &ops[2].kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr:
                Address::BaseOffset {
                    base: architectural_base,
                    offset: DISP,
                    disp_size: DispSize::Disp8,
                },
        } => {
            assert_eq!(*architectural_base, x86(X86Reg::gpr(case.base)), "{case:?}");
            *base
        }
        other => panic!("{case:?}: address materialization {other:?}"),
    };

    for lane in 0..lanes {
        let offset = 3 + lane * 3;
        let scalar = match ops[offset].kind {
            OpKind::Mov {
                dst: scalar @ VReg::Virtual(_),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => scalar,
            ref other => panic!("{case:?} lane {lane}: scalar zero {other:?}"),
        };
        let expected_width = match case.operation.source_element {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            _ => unreachable!(),
        };
        assert!(matches!(
            &ops[offset + 1].kind,
            OpKind::Load {
                dst,
                addr: Address::BaseOffset {
                    base,
                    offset: lane_offset,
                    disp_size: DispSize::Auto,
                },
                width,
                sign: SignExtend::Zero,
            } if *dst == scalar
                && *base == address_base
                && *lane_offset
                    == (lane as i64) * i64::from(case.operation.source_element.bytes())
                && *width == expected_width
        ));
        assert!(matches!(
            &ops[offset + 2].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem,
            } if *dst == source
                && *vec == source
                && *inserted == scalar
                && usize::from(*inserted_lane) == lane
                && *elem == case.operation.source_element
        ));
    }

    let extract_start = 3 + lanes * 3;
    let mut extracted = Vec::new();
    for lane in 0..lanes {
        let scalar = match ops[extract_start + lane].kind {
            OpKind::VExtractLane {
                dst: scalar @ VReg::Virtual(_),
                vec,
                lane: extracted_lane,
                elem,
                sign,
            } => {
                assert_eq!(vec, source, "{case:?}");
                assert_eq!(usize::from(extracted_lane), lane, "{case:?}");
                assert_eq!(elem, case.operation.source_element, "{case:?}");
                assert_eq!(
                    sign,
                    if case.operation.signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                    "{case:?}"
                );
                scalar
            }
            ref other => panic!("{case:?} lane {lane}: extract {other:?}"),
        };
        extracted.push(scalar);
    }

    let result_zero_offset = extract_start + lanes;
    let result_zero = match ops[result_zero_offset].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => zero,
        ref other => panic!("{case:?}: result zero {other:?}"),
    };
    assert!(matches!(
        ops[result_zero_offset + 1].kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: result_lanes,
        } if dst == vector(case.destination, case.width)
            && scalar == result_zero
            && elem == case.operation.destination_element
            && usize::from(result_lanes) == lanes
    ));
    for (lane, scalar) in extracted.into_iter().enumerate() {
        assert!(matches!(
            ops[result_zero_offset + 2 + lane].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem,
            } if dst == vector(case.destination, case.width)
                && vec == vector(case.destination, case.width)
                && inserted == scalar
                && usize::from(inserted_lane) == lane
                && elem == case.operation.destination_element
        ));
    }

    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexPackedExtendMemorySequence {
            consumed: case.consumed(),
            memory_size: case.memory_size(),
            destination: case.destination,
            source_element: case.operation.source_element,
            destination_element: case.operation.destination_element,
            width: case.width,
            signed: case.operation.signed,
            opcode: case.operation.opcode,
            w: case.w,
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(function: &SmirFunction, case: ExtendCase) -> (Vec<u8>, usize) {
    assert_exact_lift_and_sequence(function, case);
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
        case.width == VecWidth::V256,
        "{case:?}"
    );
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
        .unwrap_or_else(|error| panic!("helper-backed VEX packed extension failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX packed extension"),
        result.entry_offset,
    )
}

#[test]
fn all_2304_destination_shape_width_w_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    for operation in OPERATIONS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for w in [false, true] {
                for destination in 0..16 {
                    let case = ExtendCase {
                        operation,
                        width,
                        w,
                        destination,
                        base: 3,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        assert_exact_lift_and_sequence(&function, case);
                        let (code, _) = lower(&function, case);
                        let expected = case.emitted_register_bytes();
                        assert!(
                            code.windows(expected.len())
                                .any(|window| window == expected),
                            "{level:?} {case:?}: missing {expected:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 12 * 2 * 2 * 16 * LEVELS.len());
}

#[test]
fn llvm_23_rip_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        &[0xC4, 0x42, 0x79, 0x20, 0x4B, 0x20],
        &[0xC4, 0x02, 0x7D, 0x21, 0xBC, 0xEC, 0x44, 0x33, 0x22, 0x11],
        &[
            0x64, 0xC4, 0x62, 0x79, 0x22, 0x34, 0x8D, 0x44, 0x33, 0x22, 0x11,
        ],
        &[0xC4, 0x62, 0x7D, 0x23, 0x2D, 0x44, 0x33, 0x22, 0x11],
        &[0x65, 0xC4, 0x42, 0x79, 0x24, 0x62, 0xE0],
        &[0xC4, 0x02, 0x7D, 0x25, 0x5C, 0x48, 0x20],
        &[0xC4, 0x42, 0x7D, 0x30, 0x53, 0x20],
        &[0xC4, 0x02, 0x79, 0x31, 0x8C, 0xEC, 0x44, 0x33, 0x22, 0x11],
        &[
            0x64, 0xC4, 0x62, 0x7D, 0x32, 0x04, 0x8D, 0x44, 0x33, 0x22, 0x11,
        ],
        &[0xC4, 0xE2, 0x79, 0x33, 0x3D, 0x44, 0x33, 0x22, 0x11],
        &[0x65, 0xC4, 0xC2, 0x7D, 0x34, 0x72, 0xE0],
        &[0xC4, 0x82, 0x79, 0x35, 0x6C, 0x48, 0x20],
        &[0x67, 0xC4, 0xE2, 0x79, 0x20, 0x4C, 0x77, 0x20],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert!(is_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                true
            ));

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer.set_preserve_vector_mem_helpers(true);
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            let scratch = (0..16u8)
                .find(|candidate| *candidate != sequence.destination)
                .unwrap();
            let expected = [
                0xC4,
                (if sequence.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if scratch < 8 { 0x20 } else { 0 })
                    | 2,
                (u8::from(sequence.w) << 7)
                    | 0x78
                    | (u8::from(sequence.width == VecWidth::V256) << 2)
                    | 1,
                sequence.opcode,
                0xC0 | ((sequence.destination & 7) << 3) | (scratch & 7),
            ];
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {bytes:02X?}: missing {expected:02X?}"
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
fn classifier_gate_and_lowerer_fail_closed_for_graph_ssa_and_provenance_mutations() {
    let case = ExtendCase {
        operation: OPERATIONS[1],
        width: VecWidth::V128,
        w: true,
        destination: 9,
        base: 11,
    };
    let base = lift_case(case);
    assert_exact_lift_and_sequence(&base, case);
    let lanes = case.lanes();
    let extract_start = 3 + lanes * 3;
    let result_zero = extract_start + lanes;
    let result_broadcast = result_zero + 1;
    let result_insert = result_zero + 2;
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC));
    malformed.push(("missing source bytes", missing_metadata));

    let mut wrong_destination_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] ^= 0x08;
    wrong_destination_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte destination", wrong_destination_metadata));

    let mut wrong_operation_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[3] = 0x31;
    wrong_operation_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte signedness", wrong_operation_metadata));

    let mut nonreserved_vvvv = base.clone();
    let mut bytes = case.bytes();
    bytes[2] &= !0x08;
    nonreserved_vvvv
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("source-byte reserved vvvv", nonreserved_vvvv));

    let mut register_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.pop();
    register_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("register-source metadata", register_metadata));

    let mut first_hint = base.clone();
    first_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: case.operation.opcode,
        width: case.width,
        w: case.w,
    });
    malformed.push(("invented first hint", first_hint));

    let mut source_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut source_zero.blocks[0].ops[0].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("source zero constant", source_zero));

    let mut source_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut source_broadcast_scalar.blocks[0].ops[1].kind {
        *scalar = VReg::Virtual(VirtualId(0xE001));
    }
    malformed.push(("source broadcast scalar", source_broadcast_scalar));

    let mut source_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut source_broadcast_element.blocks[0].ops[1].kind {
        *elem = VecElementType::I16;
    }
    malformed.push(("source broadcast element", source_broadcast_element));

    let mut source_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut source_broadcast_lanes.blocks[0].ops[1].kind {
        *lanes += 1;
    }
    malformed.push(("source broadcast lanes", source_broadcast_lanes));

    let mut virtual_address = base.clone();
    if let OpKind::Lea { addr, .. } = &mut virtual_address.blocks[0].ops[2].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xE002)));
    }
    malformed.push(("virtual helper address", virtual_address));

    let mut lane_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut lane_zero.blocks[0].ops[3].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("lane zero constant", lane_zero));

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
        *width = MemWidth::B2;
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

    let mut source_insert_element = base.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut source_insert_element.blocks[0].ops[5].kind {
        *elem = VecElementType::I16;
    }
    malformed.push(("source insert element", source_insert_element));

    let mut extract_vector = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut extract_vector.blocks[0].ops[extract_start].kind
    {
        *vec = vector(2, VecWidth::V128);
    }
    malformed.push(("extract vector", extract_vector));

    let mut extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut extract_lane.blocks[0].ops[extract_start].kind {
        *lane = 1;
    }
    malformed.push(("extract lane", extract_lane));

    let mut extract_element = base.clone();
    if let OpKind::VExtractLane { elem, .. } =
        &mut extract_element.blocks[0].ops[extract_start].kind
    {
        *elem = VecElementType::I16;
    }
    malformed.push(("extract element", extract_element));

    let mut extract_sign = base.clone();
    if let OpKind::VExtractLane { sign, .. } = &mut extract_sign.blocks[0].ops[extract_start].kind {
        *sign = SignExtend::Zero;
    }
    malformed.push(("extract sign", extract_sign));

    let mut result_zero_constant = base.clone();
    if let OpKind::Mov { src, .. } = &mut result_zero_constant.blocks[0].ops[result_zero].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("result zero constant", result_zero_constant));

    let mut result_destination = base.clone();
    if let OpKind::VBroadcast { dst, .. } =
        &mut result_destination.blocks[0].ops[result_broadcast].kind
    {
        *dst = vector(8, case.width);
    }
    malformed.push(("result destination", result_destination));

    let mut result_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } =
        &mut result_element.blocks[0].ops[result_broadcast].kind
    {
        *elem = VecElementType::I64;
    }
    malformed.push(("result element", result_element));

    let mut result_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut result_lanes.blocks[0].ops[result_broadcast].kind
    {
        *lanes += 1;
    }
    malformed.push(("result lanes", result_lanes));

    let mut result_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } =
        &mut result_insert_scalar.blocks[0].ops[result_insert].kind
    {
        *scalar = VReg::Virtual(VirtualId(0xE003));
    }
    malformed.push(("result insert scalar", result_insert_scalar));

    let mut result_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } =
        &mut result_insert_lane.blocks[0].ops[result_insert].kind
    {
        *lane = 1;
    }
    malformed.push(("result insert lane", result_insert_lane));

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[4].guest_pc += 1;
    malformed.push(("split guest PC", split_pc));

    let source = match base.blocks[0].ops[1].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaping_source = base.clone();
    escaping_source.blocks[0].ops.push(SmirOp::new(
        OpId(0xE004),
        PC + 1,
        OpKind::VMov {
            dst: vector(2, VecWidth::V128),
            src: source,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("source virtual escapes", escaping_source));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0xE005), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xE006), PC, OpKind::Nop));
    assert_eq!(classified_at(&same_pc_head, 1, true), None);
    assert_rejected("same-PC head", &same_pc_head);
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
fn source_bytes(case: ExtendCase, ordinal: usize) -> [u8; 64] {
    const BYTE_VALUES: [u8; 8] = [0x00, 0x01, 0x7F, 0x80, 0xFF, 0xFE, 0x55, 0xAA];
    const WORD_VALUES: [u16; 8] = [
        0x0000, 0x0001, 0x7FFF, 0x8000, 0xFFFF, 0xFFFE, 0x5555, 0xAAAA,
    ];
    const DWORD_VALUES: [u32; 8] = [
        0x0000_0000,
        0x0000_0001,
        0x7FFF_FFFF,
        0x8000_0000,
        0xFFFF_FFFF,
        0xFFFF_FFFE,
        0x5555_5555,
        0xAAAA_AAAA,
    ];
    let mut bytes = std::array::from_fn(|index| 0xA5 ^ (index as u8).wrapping_mul(0x1D));
    for lane in 0..case.lanes() {
        let selector = (lane + ordinal) % 8;
        let offset = lane * case.operation.source_element.bytes() as usize;
        match case.operation.source_element {
            VecElementType::I8 => bytes[offset] = BYTE_VALUES[selector],
            VecElementType::I16 => {
                bytes[offset..offset + 2].copy_from_slice(&WORD_VALUES[selector].to_le_bytes());
            }
            VecElementType::I32 => {
                bytes[offset..offset + 4].copy_from_slice(&DWORD_VALUES[selector].to_le_bytes());
            }
            _ => unreachable!(),
        }
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn architectural_result(case: ExtendCase, source: &[u8; 64]) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    let source_bytes = case.operation.source_element.bytes() as usize;
    let destination_bytes = case.operation.destination_element.bytes() as usize;
    for lane in 0..case.lanes() {
        let source_offset = lane * source_bytes;
        let raw = match case.operation.source_element {
            VecElementType::I8 => u64::from(source[source_offset]),
            VecElementType::I16 => u64::from(u16::from_le_bytes(
                source[source_offset..source_offset + 2].try_into().unwrap(),
            )),
            VecElementType::I32 => u64::from(u32::from_le_bytes(
                source[source_offset..source_offset + 4].try_into().unwrap(),
            )),
            _ => unreachable!(),
        };
        let extended = if case.operation.signed {
            match case.operation.source_element {
                VecElementType::I8 => (raw as u8 as i8 as i64) as u64,
                VecElementType::I16 => (raw as u16 as i16 as i64) as u64,
                VecElementType::I32 => (raw as u32 as i32 as i64) as u64,
                _ => unreachable!(),
            }
        } else {
            raw
        };
        let destination_offset = lane * destination_bytes;
        bytes[destination_offset..destination_offset + destination_bytes]
            .copy_from_slice(&extended.to_le_bytes()[..destination_bytes]);
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
        || !matches!(size, 2 | 4 | 8 | 16)
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
fn full_guest_regs(case: ExtendCase, ordinal: usize) -> GuestRegs {
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
    case: ExtendCase,
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
fn native_cases() -> Vec<ExtendCase> {
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
    for operation in OPERATIONS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for w in [false, true] {
                for (destination, base) in OPERANDS {
                    cases.push(ExtendCase {
                        operation,
                        width,
                        w,
                        destination,
                        base,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: ExtendCase,
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
fn native_memory_packed_extensions_match_intel_interpreter_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping native VEX packed-extension memory differential: host lacks AVX2");
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 384);
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    eprintln!("executing {expected_executions} native VEX packed-extension success/fault pairs");
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
                "{level:?} {case:?}: interpreter versus Intel equations"
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
