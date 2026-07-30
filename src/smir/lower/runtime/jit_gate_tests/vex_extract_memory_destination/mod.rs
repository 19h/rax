//! Exact helper-backed VEX scalar and 128-bit chunk extraction to memory.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexChunkExtractMemoryEncoding,
    X86VexScalarExtractMemoryEncoding,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexChunkExtractMemorySequence, X86JitVexExtractMemorySequence,
    X86JitVexScalarExtractMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_extract_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_VEC_STORE_FN_OFFSET, X86_GUEST_VECTOR_SCRATCH_OFFSET,
};
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x1419_1419;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtractKind {
    Vextractf128,
    Vextracti128,
    VpextrbW0,
    VpextrbW1,
    VpextrwW0,
    VpextrwW1,
    Vpextrd,
    Vpextrq,
    VextractpsW0,
    VextractpsW1,
}

impl ExtractKind {
    const ALL: [Self; 10] = [
        Self::Vextractf128,
        Self::Vextracti128,
        Self::VpextrbW0,
        Self::VpextrbW1,
        Self::VpextrwW0,
        Self::VpextrwW1,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VextractpsW0,
        Self::VextractpsW1,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::Vextractf128 => 0x19,
            Self::Vextracti128 => 0x39,
            Self::VpextrbW0 | Self::VpextrbW1 => 0x14,
            Self::VpextrwW0 | Self::VpextrwW1 => 0x15,
            Self::Vpextrd | Self::Vpextrq => 0x16,
            Self::VextractpsW0 | Self::VextractpsW1 => 0x17,
        }
    }

    const fn w(self) -> bool {
        matches!(
            self,
            Self::VpextrbW1 | Self::VpextrwW1 | Self::Vpextrq | Self::VextractpsW1
        )
    }

    const fn is_chunk(self) -> bool {
        matches!(self, Self::Vextractf128 | Self::Vextracti128)
    }

    const fn needs_avx2(self) -> bool {
        matches!(self, Self::Vextracti128)
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::VpextrbW0 | Self::VpextrbW1 => VecElementType::I8,
            Self::VpextrwW0 | Self::VpextrwW1 => VecElementType::I16,
            Self::Vpextrd | Self::VextractpsW0 | Self::VextractpsW1 => VecElementType::I32,
            Self::Vpextrq | Self::Vextractf128 | Self::Vextracti128 => VecElementType::I64,
        }
    }

    const fn memory_width(self) -> u32 {
        match self {
            Self::VpextrbW0 | Self::VpextrbW1 => 1,
            Self::VpextrwW0 | Self::VpextrwW1 => 2,
            Self::Vpextrd | Self::VextractpsW0 | Self::VextractpsW1 => 4,
            Self::Vpextrq => 8,
            Self::Vextractf128 | Self::Vextracti128 => 16,
        }
    }

    const fn lane(self, immediate: u8) -> u8 {
        immediate
            & match self {
                Self::VpextrbW0 | Self::VpextrbW1 => 0x0F,
                Self::VpextrwW0 | Self::VpextrwW1 => 0x07,
                Self::Vpextrd | Self::VextractpsW0 | Self::VextractpsW1 => 0x03,
                Self::Vpextrq => 0x01,
                Self::Vextractf128 | Self::Vextracti128 => 0x01,
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtractCase {
    kind: ExtractKind,
    source: u8,
    base: u8,
    immediate: u8,
}

impl ExtractCase {
    fn scratch(self) -> u8 {
        (0..8)
            .find(|candidate| *candidate != self.source)
            .expect("one source leaves at least seven low scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.source < 16 && self.base < 16);
        let mut bytes = vec![
            0xC4,
            (if self.source < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.kind.w()) << 7) | 0x78 | (u8::from(self.kind.is_chunk()) << 2) | 1,
            self.kind.opcode(),
            0x40 | ((self.source & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes.push(self.immediate);
        bytes
    }

    fn expected_register_instruction(self) -> X86InstructionBytes {
        let destination = if self.kind.is_chunk() {
            self.scratch()
        } else {
            0
        };
        X86InstructionBytes::new(&[
            0xC4,
            (if self.source < 8 { 0x80 } else { 0 }) | 0x40 | 0x20 | 3,
            (u8::from(self.kind.w()) << 7) | 0x78 | (u8::from(self.kind.is_chunk()) << 2) | 1,
            self.kind.opcode(),
            0xC0 | ((self.source & 7) << 3) | destination,
            self.immediate,
        ])
        .unwrap()
    }

    fn expected_sequence(self) -> X86JitVexExtractMemorySequence {
        if self.kind.is_chunk() {
            X86JitVexExtractMemorySequence::Chunk(X86JitVexChunkExtractMemorySequence {
                consumed: 7,
                encoding: X86VexChunkExtractMemoryEncoding {
                    source: self.source,
                    first_lane: self.kind.lane(self.immediate) * 2,
                    scratch: self.scratch(),
                    needs_avx2: self.kind.needs_avx2(),
                    opcode: self.kind.opcode(),
                    immediate: self.immediate,
                    register_instruction: self.expected_register_instruction(),
                },
            })
        } else {
            let memory_width = match self.kind.memory_width() {
                1 => MemWidth::B1,
                2 => MemWidth::B2,
                4 => MemWidth::B4,
                8 => MemWidth::B8,
                _ => unreachable!(),
            };
            X86JitVexExtractMemorySequence::Scalar(X86JitVexScalarExtractMemorySequence {
                consumed: 2,
                encoding: X86VexScalarExtractMemoryEncoding {
                    source: self.source,
                    lane: self.kind.lane(self.immediate),
                    elem: self.kind.elem(),
                    memory_width,
                    w: self.kind.w(),
                    opcode: self.kind.opcode(),
                    immediate: self.immediate,
                    register_instruction: self.expected_register_instruction(),
                },
            })
        }
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn ymm(index: u8) -> VReg {
    x86(X86Reg::Ymm(index))
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
) -> Option<X86JitVexExtractMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_extract_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified(function: &SmirFunction, allow_mem: bool) -> Option<X86JitVexExtractMemorySequence> {
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits source metadata"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: ExtractCase) {
    let ops = &function.blocks[0].ops;
    if case.kind.is_chunk() {
        assert_eq!(ops.len(), 7, "{case:?}: {ops:#?}");
        let zero = match ops[0].kind {
            OpKind::Mov {
                dst: value @ VReg::Virtual(_),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => value,
            ref other => panic!("{case:?}: expected zero materialization, got {other:?}"),
        };
        let raw = match ops[1].kind {
            OpKind::VBroadcast {
                dst: value @ VReg::Virtual(_),
                scalar,
                elem: VecElementType::I64,
                lanes: 2,
            } if scalar == zero => value,
            ref other => panic!("{case:?}: expected 128-bit zero vector, got {other:?}"),
        };
        for lane in 0..2u8 {
            let extract_index = 2 + usize::from(lane) * 2;
            let scalar = match ops[extract_index].kind {
                OpKind::VExtractLane {
                    dst: value @ VReg::Virtual(_),
                    vec,
                    lane: extracted_lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                } if vec == ymm(case.source)
                    && extracted_lane == case.kind.lane(case.immediate) * 2 + lane =>
                {
                    value
                }
                ref other => panic!("{case:?}: invalid chunk extraction {other:?}"),
            };
            assert!(matches!(
                ops[extract_index + 1].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted,
                    lane: inserted_lane,
                    elem: VecElementType::I64,
                } if dst == raw && vec == raw && inserted == scalar && inserted_lane == lane
            ));
        }
        assert!(matches!(
            &ops[6].kind,
            OpKind::VStore {
                src,
                width: VecWidth::V128,
                ..
            } if *src == raw
        ));
        assert!(matches!(
            ops[6].x86_hint,
            Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        ));
        assert!(ops[..6].iter().all(|op| op.x86_hint.is_none()));
    } else {
        assert_eq!(ops.len(), 2, "{case:?}: {ops:#?}");
        let extracted = match ops[0].kind {
            OpKind::VExtractLane {
                dst: value @ VReg::Virtual(_),
                vec,
                lane,
                elem,
                sign: SignExtend::Zero,
            } if vec == xmm(case.source)
                && lane == case.kind.lane(case.immediate)
                && elem == case.kind.elem() =>
            {
                value
            }
            ref other => panic!("{case:?}: invalid scalar extraction {other:?}"),
        };
        let memory_width = match case.kind.memory_width() {
            1 => MemWidth::B1,
            2 => MemWidth::B2,
            4 => MemWidth::B4,
            8 => MemWidth::B8,
            _ => unreachable!(),
        };
        assert!(matches!(
            &ops[1].kind,
            OpKind::Store { src, width, .. }
                if *src == extracted && *width == memory_width
        ));
        assert!(ops.iter().all(|op| op.x86_hint.is_none()));
    }
    assert!(ops.iter().all(|op| op.guest_pc == PC));
    assert_eq!(classified(function, true), Some(case.expected_sequence()));
    assert_eq!(classified(function, false), None);
}

fn lift_case(case: ExtractCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn assert_feature_requirements(function: &SmirFunction, case: ExtractCase) {
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

    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_avx = true;
    expected.needs_avx2 = case.kind.needs_avx2();
    assert_eq!(
        x86_native_replay_feature_requirements(function, &excluded),
        expected,
        "{case:?}"
    );
}

fn lower(function: &SmirFunction, case: ExtractCase) -> (Vec<u8>, usize) {
    assert_exact_lift_and_sequence(function, case);
    assert_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX extraction failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize VEX extraction");
    let expected = case.expected_register_instruction();
    assert!(
        code.windows(expected.as_slice().len())
            .any(|window| window == expected.as_slice()),
        "{case:?}: rewritten extraction absent: {:02X?}",
        expected.as_slice()
    );
    for expected_offset in [
        X86_GUEST_VECTOR_SCRATCH_OFFSET as u32,
        X86_GUEST_VEC_STORE_FN_OFFSET as u32,
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == expected_offset.to_le_bytes()),
            "{case:?}: offset {expected_offset:#x} absent"
        );
    }
    (code, result.entry_offset)
}

#[test]
fn all_80_scanner_memory_destination_cells_admit_and_lower_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for kind in ExtractKind::ALL {
        for source in 0..8 {
            let case = ExtractCase {
                kind,
                source,
                base: 2,
                immediate: 0xFF,
            };
            cells += 1;
            for level in LEVELS {
                lower(&optimize(lift_case(case), level), case);
                lowered += 1;
            }
        }
    }
    assert_eq!(cells, 80);
    assert_eq!(lowered, 80 * LEVELS.len());
}

#[test]
fn high_sources_sib_bases_and_ignored_immediate_bits_remain_exact() {
    let cases = [
        ExtractCase {
            kind: ExtractKind::Vextractf128,
            source: 15,
            base: 12,
            immediate: 0xFE,
        },
        ExtractCase {
            kind: ExtractKind::Vextracti128,
            source: 8,
            base: 5,
            immediate: 0xFF,
        },
        ExtractCase {
            kind: ExtractKind::VpextrbW1,
            source: 14,
            base: 4,
            immediate: 0xDF,
        },
        ExtractCase {
            kind: ExtractKind::VpextrwW0,
            source: 9,
            base: 13,
            immediate: 0xE7,
        },
        ExtractCase {
            kind: ExtractKind::Vpextrd,
            source: 11,
            base: 5,
            immediate: 0xF3,
        },
        ExtractCase {
            kind: ExtractKind::Vpextrq,
            source: 12,
            base: 15,
            immediate: 0xF1,
        },
        ExtractCase {
            kind: ExtractKind::VextractpsW1,
            source: 13,
            base: 4,
            immediate: 0xF3,
        },
    ];
    for case in cases {
        for level in LEVELS {
            lower(&optimize(lift_case(case), level), case);
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified(function, true),
        None,
        "{name}: exact sequence classifier admitted malformed input"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed input"
    );
}

#[test]
fn reserved_source_fields_and_semantically_changed_immediates_fail_closed() {
    for case in [
        ExtractCase {
            kind: ExtractKind::Vextracti128,
            source: 9,
            base: 11,
            immediate: 0xFE,
        },
        ExtractCase {
            kind: ExtractKind::VpextrbW1,
            source: 9,
            base: 11,
            immediate: 0xFE,
        },
    ] {
        let base = lift_case(case);
        let valid = case.bytes();
        let mut invalid = Vec::new();

        let mut vvvv = valid.clone();
        vvvv[2] &= !0x08;
        invalid.push(("reserved VEX.vvvv", vvvv));
        let mut pp = valid.clone();
        pp[2] = (pp[2] & !3) | 2;
        invalid.push(("wrong mandatory prefix", pp));
        let mut map = valid.clone();
        map[1] = (map[1] & !0x1F) | 2;
        invalid.push(("wrong map", map));
        let mut opcode = valid.clone();
        opcode[3] = 0x13;
        invalid.push(("unsupported opcode", opcode));
        let mut immediate = valid.clone();
        *immediate.last_mut().unwrap() ^= 1;
        invalid.push(("different selected lane", immediate));
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.remove(register.len() - 2);
        invalid.push(("register destination", register));
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(("trailing byte", trailing));

        if case.kind.is_chunk() {
            let mut l0 = valid.clone();
            l0[2] &= !0x04;
            invalid.push(("VEX.L=0", l0));
            let mut w1 = valid.clone();
            w1[2] |= 0x80;
            invalid.push(("VEX.W=1", w1));
        } else {
            let mut l1 = valid.clone();
            l1[2] |= 0x04;
            invalid.push(("VEX.L=1", l1));
        }

        for (name, bytes) in invalid {
            let mut function = base.clone();
            function.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&bytes).expect("mutated image fits metadata"),
            );
            assert_rejected(name, &function);
        }

        let mut missing = base;
        missing.x86_instruction_bytes.clear();
        assert_rejected("missing source metadata", &missing);
    }
}

#[test]
fn every_graph_operation_boundary_hint_address_and_virtual_escape_mutation_fails_closed() {
    for case in [
        ExtractCase {
            kind: ExtractKind::Vextractf128,
            source: 9,
            base: 11,
            immediate: 1,
        },
        ExtractCase {
            kind: ExtractKind::VpextrwW1,
            source: 9,
            base: 11,
            immediate: 7,
        },
    ] {
        let base = lift_case(case);
        for index in 0..base.blocks[0].ops.len() {
            let mut function = base.clone();
            function.blocks[0].ops[index].kind = OpKind::Nop;
            assert_rejected("operation replaced", &function);
        }

        let store_index = base.blocks[0].ops.len() - 1;
        let mut bad_address = base.clone();
        match &mut bad_address.blocks[0].ops[store_index].kind {
            OpKind::Store { addr, .. } | OpKind::VStore { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0xFF00)));
            }
            _ => unreachable!(),
        }
        assert_rejected("non-state-backed address", &bad_address);

        let mut wrong_hint = base.clone();
        wrong_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
        assert_rejected("invented first hint", &wrong_hint);

        let escaped = base.blocks[0]
            .ops
            .iter()
            .flat_map(|op| op.kind.dests())
            .find(|register| matches!(register, VReg::Virtual(_)))
            .expect("extract graph has a virtual");
        let mut escape = base.clone();
        escape.blocks[0].ops.push(SmirOp::new(
            OpId(0x7FF0),
            PC + 1,
            OpKind::Mov {
                dst: x86(X86Reg::Rax),
                src: SrcOperand::Reg(escaped),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("virtual escape", &escape);

        let mut split = base.clone();
        split.blocks[0].ops[store_index].guest_pc += 1;
        assert_rejected("split guest PC", &split);

        let mut tail = base.clone();
        tail.blocks[0]
            .ops
            .push(SmirOp::new(OpId(0x7FF1), PC, OpKind::Nop));
        assert_rejected("same-PC tail", &tail);

        let mut head = base;
        head.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0x7FF2), PC, OpKind::Nop));
        assert_eq!(
            classified_at(&head, 1, true),
            None,
            "same-PC head must prevent mid-instruction admission"
        );
    }
}
