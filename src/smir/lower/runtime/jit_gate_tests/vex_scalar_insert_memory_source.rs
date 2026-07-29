//! Exact helper-backed VEX scalar-insert memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexScalarInsertMemoryFields,
    X86VexScalarInsertMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexScalarInsertMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_scalar_insert_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::{HashMap, HashSet};

const PC: u64 = 0x51A7;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const KINDS: [X86VexScalarInsertMemoryKind; 5] = [
    X86VexScalarInsertMemoryKind::Vpinsrb,
    X86VexScalarInsertMemoryKind::Vpinsrw,
    X86VexScalarInsertMemoryKind::Vpinsrd,
    X86VexScalarInsertMemoryKind::Vpinsrq,
    X86VexScalarInsertMemoryKind::Vinsertps,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InsertMemoryCase {
    kind: X86VexScalarInsertMemoryKind,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
    wig_w: bool,
    compact: bool,
}

impl InsertMemoryCase {
    fn w(self) -> bool {
        match self.kind {
            X86VexScalarInsertMemoryKind::Vpinsrd => false,
            X86VexScalarInsertMemoryKind::Vpinsrq => true,
            _ => self.wig_w,
        }
    }

    fn map_opcode(self) -> (u8, u8) {
        match self.kind {
            X86VexScalarInsertMemoryKind::Vpinsrw => (1, 0xC4),
            X86VexScalarInsertMemoryKind::Vpinsrb => (3, 0x20),
            X86VexScalarInsertMemoryKind::Vinsertps => (3, 0x21),
            X86VexScalarInsertMemoryKind::Vpinsrd | X86VexScalarInsertMemoryKind::Vpinsrq => {
                (3, 0x22)
            }
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        let (map, opcode) = self.map_opcode();
        let mut bytes = if self.compact {
            assert_eq!(self.kind, X86VexScalarInsertMemoryKind::Vpinsrw);
            assert!(!self.w());
            assert!(self.base < 8);
            vec![
                0xC5,
                (if self.destination < 8 { 0x80 } else { 0 }) | (((!self.source1) & 0x0F) << 3) | 1,
                opcode,
                0x40 | ((self.destination & 7) << 3) | self.base,
            ]
        } else {
            vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | map,
                (u8::from(self.w()) << 7) | (((!self.source1) & 0x0F) << 3) | 1,
                opcode,
                0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            ]
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes.push(self.immediate);
        bytes
    }

    fn fields(self) -> X86VexScalarInsertMemoryFields {
        X86VexScalarInsertMemoryFields {
            destination: self.destination,
            source1: self.source1,
            kind: self.kind,
            immediate: self.immediate,
            w: self.w(),
        }
    }

    fn emitted_register_bytes(self) -> Vec<u8> {
        let (map, opcode) = self.map_opcode();
        let (source2, immediate) = if self.kind == X86VexScalarInsertMemoryKind::Vinsertps {
            (self.scratch(), self.immediate & 0x3F)
        } else {
            (0, self.immediate)
        };
        if map == 1 && !self.w() && source2 < 8 {
            return vec![
                0xC5,
                (if self.destination < 8 { 0x80 } else { 0 }) | (((!self.source1) & 0x0F) << 3) | 1,
                opcode,
                0xC0 | ((self.destination & 7) << 3) | source2,
                immediate,
            ];
        }
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if source2 < 8 { 0x20 } else { 0 })
                | map,
            (u8::from(self.w()) << 7) | (((!self.source1) & 0x0F) << 3) | 1,
            opcode,
            0xC0 | ((self.destination & 7) << 3) | (source2 & 7),
            immediate,
        ]
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn architectural_memory_width(kind: X86VexScalarInsertMemoryKind) -> MemWidth {
    match kind {
        X86VexScalarInsertMemoryKind::Vpinsrb => MemWidth::B1,
        X86VexScalarInsertMemoryKind::Vpinsrw => MemWidth::B2,
        X86VexScalarInsertMemoryKind::Vpinsrd | X86VexScalarInsertMemoryKind::Vinsertps => {
            MemWidth::B4
        }
        X86VexScalarInsertMemoryKind::Vpinsrq => MemWidth::B8,
    }
}

fn architectural_destination_lane(case: InsertMemoryCase) -> usize {
    usize::from(match case.kind {
        X86VexScalarInsertMemoryKind::Vpinsrb => case.immediate & 0x0F,
        X86VexScalarInsertMemoryKind::Vpinsrw => case.immediate & 0x07,
        X86VexScalarInsertMemoryKind::Vpinsrd => case.immediate & 0x03,
        X86VexScalarInsertMemoryKind::Vpinsrq => case.immediate & 0x01,
        X86VexScalarInsertMemoryKind::Vinsertps => (case.immediate >> 4) & 0x03,
    })
}

fn expected_address(case: InsertMemoryCase) -> Address {
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
) -> Option<X86JitVexScalarInsertMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_scalar_insert_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(function: &SmirFunction) -> Option<X86JitVexScalarInsertMemorySequence> {
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

fn lift_case(case: InsertMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    let OpKind::Load {
        dst: VReg::Virtual(_),
        addr,
        width,
        sign: SignExtend::Zero,
    } = &function.blocks[0].ops[0].kind
    else {
        panic!("{case:?}: scalar insert must begin with a precise scalar Load")
    };
    assert_eq!(addr, &expected_address(case), "{case:?}");
    assert_eq!(*width, architectural_memory_width(case.kind), "{case:?}");
    assert_eq!(function.blocks[0].ops[0].x86_hint, None, "{case:?}");
    assert_exact_sequence(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_sequence(function: &SmirFunction, case: InsertMemoryCase) {
    let sequence =
        classified(function).unwrap_or_else(|| panic!("{case:?}: exact sequence not classified"));
    assert_eq!(sequence.consumed, function.blocks[0].ops.len(), "{case:?}");
    assert_eq!(
        sequence.memory_size,
        architectural_memory_width(case.kind).bytes()
    );
    assert_eq!(sequence.encoding, case.fields(), "{case:?}");
    assert_eq!(classified_at(function, 0, false), None, "{case:?}");
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexScalarInsertMemorySequence) {
    let sequence = classified(function).expect("classified VEX scalar-insert memory sequence");
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
        .unwrap_or_else(|error| panic!("helper-backed scalar-insert lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX scalar insert"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_3456_scanner_encoding_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    let mut min_consumed = usize::MAX;
    let mut max_consumed = 0usize;
    for kind in KINDS {
        let wig_values: &[bool] = match kind {
            X86VexScalarInsertMemoryKind::Vpinsrd => &[false],
            X86VexScalarInsertMemoryKind::Vpinsrq => &[true],
            _ => &[false, true],
        };
        for &wig_w in wig_values {
            for destination in 0..8 {
                for source1 in 0..16 {
                    let case = InsertMemoryCase {
                        kind,
                        destination,
                        source1,
                        base: 2,
                        immediate: destination.wrapping_mul(17) ^ source1.wrapping_mul(29),
                        wig_w,
                        compact: false,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        assert_exact_sequence(&function, case);
                        let (code, _, sequence) = lower(&function);
                        min_consumed = min_consumed.min(sequence.consumed);
                        max_consumed = max_consumed.max(sequence.consumed);
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
    for destination in 0..8 {
        for source1 in 0..16 {
            let case = InsertMemoryCase {
                kind: X86VexScalarInsertMemoryKind::Vpinsrw,
                destination,
                source1,
                base: 2,
                immediate: destination.wrapping_mul(31) ^ source1.wrapping_mul(13),
                wig_w: false,
                compact: true,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                assert_exact_sequence(&function, case);
                let (code, _, sequence) = lower(&function);
                min_consumed = min_consumed.min(sequence.consumed);
                max_consumed = max_consumed.max(sequence.consumed);
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
    assert_eq!(lowered, 1_152 * LEVELS.len());
    assert_eq!((min_consumed, max_consumed), (7, 35));
}

#[test]
fn high_alias_wig_compact_segment_rip_sib_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[&[u8]] = &[
        // vpinsrw xmm1,xmm2,word ptr [rbx+0x20],0xa5 (compact C5).
        &[0xC5, 0xE9, 0xC4, 0x4B, 0x20, 0xA5],
        // vpinsrb xmm9,xmm10,byte ptr fs:[r11+0x20],0x0f.
        &[0x64, 0xC4, 0x43, 0x29, 0x20, 0x4B, 0x20, 0x0F],
        // VINSERTPS W1 alias with destination/merge aliases and GS:SIB.
        &[0x65, 0xC4, 0x43, 0xA9, 0x21, 0x4C, 0xEC, 0x20, 0xA5],
        // vpinsrd xmm9,xmm2,dword ptr addr32 [ecx*4+0x44332211],3.
        &[
            0x67, 0xC4, 0x63, 0x69, 0x22, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x03,
        ],
        // vpinsrq xmm14,xmm10,qword ptr [r12+r13*8+0x20],1.
        &[0xC4, 0x03, 0xA9, 0x22, 0x74, 0xEC, 0x20, 0x01],
        // vinsertps xmm1,xmm2,dword ptr [rip+0x44332211],0x3c.
        &[0xC4, 0xE3, 0x69, 0x21, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x3C],
    ];
    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified(&function)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(sequence.consumed, function.blocks[0].ops.len());
            let (code, _, _) = lower(&function);
            let fields = sequence.encoding;
            let case = InsertMemoryCase {
                kind: fields.kind,
                destination: fields.destination,
                source1: fields.source1,
                base: 0,
                immediate: fields.immediate,
                wig_w: fields.w,
                compact: false,
            };
            let expected = case.emitted_register_bytes();
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
fn classifier_and_gate_fail_closed_for_every_graph_provenance_and_escape_invariant() {
    let case = InsertMemoryCase {
        kind: X86VexScalarInsertMemoryKind::Vinsertps,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0xA5,
        wig_w: true,
        compact: false,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::<(&str, SmirFunction)>::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source map", 1usize, 0x01u8),
        ("source mandatory prefix", 2, 0x03),
        ("source vector length", 2, 0x04),
        ("source opcode", 3, 0x01),
        ("source destination", 4, 0x08),
        ("source first operand", 2, 0x08),
        ("source immediate", 6, 0x01),
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
    bytes.remove(5);
    replace_instruction_bytes(&mut register_metadata, &bytes);
    malformed.push(("register-source metadata", register_metadata));

    let mut trailing_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    replace_instruction_bytes(&mut trailing_metadata, &bytes);
    malformed.push(("trailing source byte", trailing_metadata));

    let mut wrong_load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("load width", wrong_load_width));

    let mut signed_load = base.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("load sign", signed_load));

    let mut architectural_load = base.clone();
    if let OpKind::Load { dst, .. } = &mut architectural_load.blocks[0].ops[0].kind {
        *dst = x86(X86Reg::Rax);
    }
    malformed.push(("architectural load destination", architectural_load));

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFF00)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x21,
        width: VecWidth::V128,
        w: true,
    });
    malformed.push(("invented load hint", load_hint));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut op_hint = base.clone();
    op_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::MovImmModRm);
    malformed.push(("invented graph hint", op_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7000), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    for index in 1..base.blocks[0].ops.len() {
        let mut function = base.clone();
        function.blocks[0].ops[index].kind = OpKind::Nop;
        malformed.push(("missing canonical graph node", function));
    }

    let extract_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } = &mut function.blocks[0].ops[extract_index].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *vec = x86(X86Reg::Xmm(8)),
            2 => *lane ^= 1,
            3 => *elem = VecElementType::I16,
            4 => *sign = SignExtend::Sign,
            _ => unreachable!(),
        }
        malformed.push(("extract field", function));
    }

    let mov_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Mov { .. }))
        .unwrap();
    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::Mov { dst, src, width } = &mut function.blocks[0].ops[mov_index].kind else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *src = SrcOperand::Imm(1),
            2 => *width = OpWidth::W32,
            _ => unreachable!(),
        }
        malformed.push(("zero field", function));
    }

    let broadcast_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
        .unwrap();
    for field in 0..4 {
        let mut function = base.clone();
        let OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[broadcast_index].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *scalar = loaded,
            2 => *elem = VecElementType::I16,
            3 => *lanes = 3,
            _ => unreachable!(),
        }
        malformed.push(("broadcast field", function));
    }

    let insert_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .unwrap();
    for field in 0..5 {
        let mut function = base.clone();
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } = &mut function.blocks[0].ops[insert_index].kind
        else {
            unreachable!()
        };
        match field {
            0 => *dst = loaded,
            1 => *vec = loaded,
            2 => *scalar = loaded,
            3 => *lane ^= 1,
            4 => *elem = VecElementType::I16,
            _ => unreachable!(),
        }
        malformed.push(("insert field", function));
    }

    let final_index = base.blocks[0].ops.len() - 1;
    for field in 0..3 {
        let mut function = base.clone();
        let OpKind::VMov { dst, src, width } = &mut function.blocks[0].ops[final_index].kind else {
            unreachable!()
        };
        match field {
            0 => *dst = x86(X86Reg::Xmm(8)),
            1 => *src = loaded,
            2 => *width = VecWidth::V256,
            _ => unreachable!(),
        }
        malformed.push(("final move field", function));
    }

    let local_virtuals: HashSet<VReg> = base.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .filter(|reg| matches!(reg, VReg::Virtual(_)))
        .collect();
    for (ordinal, reg) in local_virtuals.into_iter().enumerate() {
        let mut external_use = base.clone();
        external_use.blocks[0].ops.push(SmirOp::new(
            OpId(0x7100 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xF000 + ordinal as u32)),
                src: SrcOperand::Reg(reg),
                width: OpWidth::W64,
            },
        ));
        malformed.push(("local virtual escapes sequence", external_use));

        let mut duplicate_definition = base.clone();
        duplicate_definition.blocks[0].ops.push(SmirOp::new(
            OpId(0x7200 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: reg,
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

    // A fully zero-masked VINSERTPS retains its faulting load at O2 even
    // though the loaded virtual has no canonical uses. Its zero-use escape
    // must still be detected.
    let dead_case = InsertMemoryCase {
        immediate: 0x0F,
        ..case
    };
    let dead = optimize(lift_case(dead_case), OptLevel::O2);
    let dead_loaded = match dead.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    assert_eq!(
        virtual_counts(&dead.blocks[0])
            .1
            .get(&dead_loaded)
            .copied()
            .unwrap_or(0),
        0
    );
    assert_exact_sequence(&dead, dead_case);
    let mut escaped = dead;
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(0x7400),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFEED)),
            src: SrcOperand::Reg(dead_loaded),
            width: OpWidth::W64,
        },
    ));
    assert_rejected("zero-use loaded value escapes sequence", &escaped);
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

fn initial_vectors(ordinal: usize) -> [[u64; 8]; 32] {
    std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xA55A_6996_F00F_3CC3u64
                .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
        })
    })
}

/// Independent transcription of Intel scalar-insert lane selection, INSERTPS
/// zero masking, and VEX.128 upper-state clearing.
fn architectural_destination(case: InsertMemoryCase, source1: [u64; 8], scalar: u64) -> [u64; 8] {
    let source = words_to_bytes(source1);
    let mut result = [0u8; 64];
    result[..16].copy_from_slice(&source[..16]);
    let scalar = scalar.to_le_bytes();
    let width = architectural_memory_width(case.kind).bytes() as usize;
    let lane = architectural_destination_lane(case);
    result[lane * width..lane * width + width].copy_from_slice(&scalar[..width]);
    if case.kind == X86VexScalarInsertMemoryKind::Vinsertps {
        for lane in 0..4 {
            if case.immediate & (1 << lane) != 0 {
                result[lane * 4..lane * 4 + 4].fill(0);
            }
        }
    }
    bytes_to_words(result)
}

#[test]
fn interpreter_matches_intel_all_256_immediates_five_kinds_and_o0_o2() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut checked = 0usize;
    for (kind_ordinal, kind) in KINDS.into_iter().enumerate() {
        for immediate in u8::MIN..=u8::MAX {
            let destination = if immediate & 1 == 0 { 1 } else { 9 };
            let source1 = if immediate & 2 == 0 { destination } else { 10 };
            let case = InsertMemoryCase {
                kind,
                destination,
                source1,
                base: 3,
                immediate,
                wig_w: immediate & 0x80 != 0,
                compact: kind == X86VexScalarInsertMemoryKind::Vpinsrw && immediate & 0x84 == 0,
            };
            let scalar = 0xFEDC_BA98_7654_3210u64 ^ (u64::from(immediate) * 0x0101_0101_0101_0101);
            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let initial = initial_vectors(kind_ordinal * 256 + usize::from(immediate));
                let expected =
                    architectural_destination(case, initial[usize::from(source1)], scalar);
                let gprs = std::array::from_fn(|index| {
                    0x1000u64 + (index as u64) * 0x101 + u64::from(immediate)
                });
                let rflags = 0x2 | (u64::from(immediate) & 0xD5);
                let masks = std::array::from_fn(|index| {
                    0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)
                });
                let mxcsr = 0x1F80 | u32::from(immediate & 0x1F);
                let mut context = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                    x86.gpr = gprs;
                    x86.gpr[usize::from(case.base)] = 0x2000;
                    for (index, value) in initial.iter().enumerate() {
                        x86.xmm[index][..8].copy_from_slice(value);
                    }
                    x86.k = masks;
                    x86.rflags = rflags;
                    x86.mxcsr = mxcsr;
                }
                context.flags.materialized = MaterializedFlags::from_rflags(rflags);
                context.flags.lazy = None;
                let mut memory = FlatMemory::new(0x10000);
                memory.load(
                    0x2000 + DISP as usize,
                    &scalar.to_le_bytes()[..architectural_memory_width(case.kind).bytes() as usize],
                );
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    &function.blocks[0],
                );
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                assert_eq!(x86.gpr, {
                    let mut expected_gprs = gprs;
                    expected_gprs[usize::from(case.base)] = 0x2000;
                    expected_gprs
                });
                assert_eq!(
                    &x86.xmm[usize::from(destination)][..8],
                    &expected,
                    "{level:?} {case:?}"
                );
                assert_eq!(
                    &x86.xmm[usize::from(destination)][8..],
                    &[0; 8],
                    "{level:?} {case:?}: upper state"
                );
                if source1 != destination {
                    assert_eq!(
                        &x86.xmm[usize::from(source1)][..8],
                        &initial[usize::from(source1)],
                        "{level:?} {case:?}: merge source"
                    );
                }
                assert_eq!(x86.k, masks, "{level:?} {case:?}: masks");
                assert_eq!(x86.rflags, rflags, "{level:?} {case:?}: RFLAGS");
                assert_eq!(x86.mxcsr, mxcsr, "{level:?} {case:?}: MXCSR");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 5 * 256 * DIFFERENTIAL_LEVELS.len());
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct ScalarMemoryContext {
    value: u64,
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
    let context = unsafe { &mut *(state.ctx as *mut ScalarMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 1 | 2 | 4 | 8)
    {
        return 0;
    }
    let mut bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    bytes[..size as usize].copy_from_slice(&context.value.to_le_bytes()[..size as usize]);
    state.vector_scratch = bytes_to_words(bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: InsertMemoryCase, ordinal: usize) -> GuestRegs {
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
    registers.zmm = initial_vectors(ordinal);
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(mut registers: GuestRegs, case: InsertMemoryCase, scalar: u64) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1)];
    registers.zmm[usize::from(case.destination)] = architectural_destination(case, source1, scalar);
    let mut scratch = [0u8; 64];
    let size = architectural_memory_width(case.kind).bytes() as usize;
    scratch[..size].copy_from_slice(&scalar.to_le_bytes()[..size]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<InsertMemoryCase> {
    let immediates = [
        0x00, 0x01, 0x02, 0x03, 0x07, 0x0F, 0x10, 0x20, 0x30, 0x3F, 0x40, 0x80, 0xA5, 0xF0, 0xFF,
    ];
    let operands = [(0, 0, 0), (15, 0, 11), (9, 9, 3), (1, 15, 0)];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in KINDS {
        for immediate in immediates {
            let (destination, source1, base) = operands[ordinal % operands.len()];
            cases.push(InsertMemoryCase {
                kind,
                destination,
                source1,
                base,
                immediate,
                wig_w: ordinal & 1 != 0,
                compact: kind == X86VexScalarInsertMemoryKind::Vpinsrw
                    && ordinal & 2 == 0
                    && base < 8,
            });
            ordinal += 1;
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_INSERT_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[InsertMemoryCase], range: std::ops::Range<usize>) {
    use crate::smir::lower::runtime::ExecMem;

    assert!(range.start < range.end && range.end <= cases.len());
    let expected_executions = range.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for ordinal in range {
        let case = cases[ordinal];
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let expected_instruction = case.emitted_register_bytes();
            assert!(
                code.windows(expected_instruction.len())
                    .any(|window| window == expected_instruction),
                "{level:?} {case:?}: missing {expected_instruction:02X?}"
            );
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let scalar =
                0xFEDC_BA98_7654_3210u64 ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);

            let mut context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = expected_success(registers, case, scalar);

            eprintln!("native success case {ordinal}: {level:?} {case:?}");
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
                architectural_memory_width(case.kind).bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut context = ScalarMemoryContext {
                value: scalar,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            eprintln!("native fault case {ordinal}: {level:?} {case:?}");
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
                architectural_memory_width(case.kind).bytes(),
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
        "executed {successes} successful and {faults} faulting native VEX scalar-insert memory cases"
    );
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX scalar-insert memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = semantic_cases();
    assert!(!cases.is_empty());
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    panic!(
        "isolated native VEX scalar-insert memory failure at case {start}/{}: \
         {case:?}; whole status {}; singleton status {}; singleton stdout: {}; \
         singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_scalar_inserts_match_model_and_precise_noncommitting_faults() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar-insert memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_insert_memory_source::\
         native_scalar_inserts_match_model_and_precise_noncommitting_faults",
    );
}
