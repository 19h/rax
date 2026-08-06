//! Exact helper-backed EVEX.128 high/low 64-bit lane load coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexHalfMoveMemoryEncoding, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitEvexHalfMoveMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_half_move_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;
mod store;

const PC: u64 = 0xE9_1216;
const MEMORY_ADDRESS: u64 = 0x3000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryLane {
    Low,
    High,
}

impl MemoryLane {
    const ALL: [Self; 2] = [Self::Low, Self::High];

    const fn index(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::High => 1,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::Low => 0x12,
            Self::High => 0x16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveFormat {
    Ps,
    Pd,
}

impl MoveFormat {
    const ALL: [Self; 2] = [Self::Ps, Self::Pd];

    const fn pp(self) -> u8 {
        match self {
            Self::Ps => 0,
            Self::Pd => 1,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::Pd)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfMoveCase {
    lane: MemoryLane,
    format: MoveFormat,
    destination: u8,
    source1: u8,
}

impl HalfMoveCase {
    fn bytes(self) -> [u8; 6] {
        assert!(self.destination < 32 && self.source1 < 32);
        [
            0x62,
            (u8::from(self.destination & 8 == 0) << 7)
                | 0x40
                | 0x20
                | (u8::from(self.destination & 16 == 0) << 4)
                | 1,
            (u8::from(self.format.w()) << 7)
                | (((!self.source1) & 0x0F) << 3)
                | 0x04
                | self.format.pp(),
            u8::from(self.source1 < 16) << 3,
            self.lane.opcode(),
            ((self.destination & 7) << 3) | 2,
        ]
    }

    fn stack_instruction(self) -> X86InstructionBytes {
        let bytes = self.bytes();
        X86InstructionBytes::new(&[
            0x62,
            (bytes[1] & 0x97) | 0x60,
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            ((self.destination & 7) << 3) | 4,
            0x24,
        ])
        .unwrap()
    }

    fn expected_encoding(self) -> X86EvexHalfMoveMemoryEncoding {
        X86EvexHalfMoveMemoryEncoding {
            destination: self.destination,
            source1: self.source1,
            memory_lane: self.lane.index(),
            w: self.format.w(),
            pp: self.format.pp(),
            opcode: self.lane.opcode(),
            stack_instruction: self.stack_instruction(),
        }
    }
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn function_from_bytes(bytes: &[u8]) -> SmirFunction {
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
        X86InstructionBytes::new(bytes).expect("EVEX half-move provenance"),
    );
    function
}

fn lift_case(case: HalfMoveCase) -> SmirFunction {
    function_from_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexHalfMoveMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_half_move_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_graph(function: &SmirFunction, case: HalfMoveCase) {
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    let ops = &function.blocks[0].ops[index..];
    assert_eq!(ops.len(), 6, "{case:?}: {ops:#?}");
    let preserved_lane = 1 - case.lane.index();
    let preserved = match ops[0].kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(vec, xmm(case.source1), "{case:?}");
            assert_eq!(lane, preserved_lane, "{case:?}");
            dst
        }
        ref other => panic!("{case:?}: preserved extraction: {other:?}"),
    };
    let loaded = match &ops[1].kind {
        OpKind::Load {
            dst,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        } => *dst,
        other => panic!("{case:?}: 8-byte load: {other:?}"),
    };
    let zero = match ops[2].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => dst,
        ref other => panic!("{case:?}: zero materialization: {other:?}"),
    };
    assert!(matches!(
        ops[3].kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes: 1,
        } if dst == xmm(case.destination) && scalar == zero
    ));
    assert!(matches!(
        ops[4].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if dst == xmm(case.destination)
            && vec == xmm(case.destination)
            && scalar == preserved
            && lane == preserved_lane
    ));
    assert!(matches!(
        ops[5].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if dst == xmm(case.destination)
            && vec == xmm(case.destination)
            && scalar == loaded
            && lane == case.lane.index()
    ));
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        sequence(function, true),
        Some(X86JitEvexHalfMoveMemorySequence {
            consumed: 6,
            address_offset: 1,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(sequence(function, false), None, "{case:?}");
}

fn configured_lowerer(avx_only: bool) -> X86_64Lowerer {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(avx_only);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
}

fn lower(function: &SmirFunction, case: HalfMoveCase) -> (Vec<u8>, usize) {
    assert_exact_graph(function, case);
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));
    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any && requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.has_k16_opmask_span, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");

    let mut lowerer = configured_lowerer(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX half-move lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer.finalize().expect("finalize EVEX half-move replay");
    let stack = case.stack_instruction();
    assert!(
        code.windows(stack.as_slice().len())
            .any(|window| window == stack.as_slice()),
        "{case:?}: missing stack replay {:02X?}",
        stack.as_slice()
    );
    (code, result.entry_offset)
}

fn representative_cases() -> Vec<HalfMoveCase> {
    let mut cases = Vec::with_capacity(16);
    let mut ordinal = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for source1 in [0, 1, 15, 31] {
                let destination = if source1 == 31 && ordinal & 1 == 0 {
                    source1
                } else {
                    [0, 9, 17, 31][ordinal & 3]
                };
                cases.push(HalfMoveCase {
                    lane,
                    format,
                    destination,
                    source1,
                });
                ordinal += 1;
            }
        }
    }
    assert_eq!(cases.len(), 16);
    cases
}

#[test]
fn sixteen_extension_alias_cells_admit_and_lower_at_every_optimizer_level() {
    let cases = representative_cases();
    let mut lowerings = 0usize;
    for case in cases {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_half_move_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(encoding, case.expected_encoding(), "{case:?}");
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            lower(&function, case);
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 16 * LEVELS.len());
}

#[test]
fn full_vector_bridge_rejects_avx_only_state_marshalling() {
    let case = representative_cases()[0];
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    let mut lowerer = configured_lowerer(true);
    assert!(matches!(
        lowerer.lower_function(&function),
        Err(LowerError::InvalidOperand { .. })
    ));
}
