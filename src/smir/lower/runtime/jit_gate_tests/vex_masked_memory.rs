//! Exact helper-backed VEX masked-memory load/store coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{
    BlockId, FunctionId, GuestAddr, SourceArch, VReg, VecElementType, VecWidth,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitVexMaskedMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_masked_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

const PC: GuestAddr = 0x6D41_534B;
const DISP: u8 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const OPERANDS: [(u8, u8, u8); 8] = [
    (1, 2, 3),
    (9, 11, 13),
    (0, 0, 0),
    (2, 2, 2),
    (15, 15, 15),
    (4, 12, 4),
    (12, 4, 12),
    (7, 5, 5),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaskedMemoryCase {
    opcode: u8,
    w: bool,
    width: VecWidth,
    mask: u8,
    vector: u8,
    base: u8,
}

impl MaskedMemoryCase {
    fn load(self) -> bool {
        matches!(self.opcode, 0x2C | 0x2D | 0x8C)
    }

    fn elem(self) -> VecElementType {
        match (self.opcode, self.w) {
            (0x2C | 0x2E, false) => VecElementType::F32,
            (0x2D | 0x2F, false) => VecElementType::F64,
            (0x8C | 0x8E, false) => VecElementType::I32,
            (0x8C | 0x8E, true) => VecElementType::I64,
            _ => unreachable!("case constructor selects an exact masked-memory family"),
        }
    }

    fn lanes(self) -> usize {
        self.width.lanes(self.elem()) as usize
    }

    fn consumed(self) -> usize {
        if self.load() {
            4 + 5 * self.lanes()
        } else {
            1 + 4 * self.lanes()
        }
    }

    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![
            0xC4,
            (if self.vector < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7)
                | ((!self.mask & 0x0F) << 3)
                | (if self.width == VecWidth::V256 {
                    0x04
                } else {
                    0
                })
                | 1,
            self.opcode,
            0x40 | ((self.vector & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP);
        bytes
    }
}

fn families() -> [(u8, bool); 8] {
    [
        (0x2C, false),
        (0x2D, false),
        (0x2E, false),
        (0x2F, false),
        (0x8C, false),
        (0x8C, true),
        (0x8E, false),
        (0x8E, true),
    ]
}

fn all_cases() -> Vec<MaskedMemoryCase> {
    let mut cases = Vec::new();
    for (opcode, w) in families() {
        for width in [VecWidth::V128, VecWidth::V256] {
            for (mask, vector, base) in OPERANDS {
                cases.push(MaskedMemoryCase {
                    opcode,
                    w,
                    width,
                    mask,
                    vector,
                    base,
                });
            }
        }
    }
    cases
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("masked-memory instruction provenance"),
    );
    function
}

fn lift_case(case: MaskedMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
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
) -> Option<X86JitVexMaskedMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_masked_memory_sequence(
        block,
        0,
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

fn assert_exact_graph(function: &SmirFunction, case: MaskedMemoryCase) {
    let sequence = classified_sequence(function, true)
        .unwrap_or_else(|| panic!("unclassified exact masked-memory graph: {case:?}"));
    assert_eq!(sequence.consumed, case.consumed(), "{case:?}");
    assert_eq!(function.blocks[0].ops.len(), case.consumed(), "{case:?}");
    assert_eq!(sequence.encoding.load, case.load(), "{case:?}");
    assert_eq!(sequence.encoding.elem, case.elem(), "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(sequence.encoding.mask, case.mask, "{case:?}");
    assert_eq!(sequence.encoding.vector, case.vector, "{case:?}");
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(
    function: &SmirFunction,
    case: MaskedMemoryCase,
) -> (Vec<u8>, usize, X86JitVexMaskedMemorySequence) {
    let excluded = HashMap::new();
    let sequence = classified_sequence(function, true).expect("classified masked-memory sequence");
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
        lowerer.finalize().expect("finalize masked-memory lowering"),
        lowered.entry_offset,
        sequence,
    )
}

#[test]
fn all_384_family_width_alias_and_optimization_graphs_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 8 * 2 * OPERANDS.len());
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, _) = lower(&function, case);
            if case.load() {
                let helpers = code
                    .windows(5)
                    .filter(|window| {
                        **window == [0xBA, case.elem().bytes() as u8, 0x00, 0x00, 0x00]
                    })
                    .count();
                assert_eq!(helpers, case.lanes(), "{level:?} {case:?}");
            } else {
                let helpers = code
                    .windows(5)
                    .filter(|window| {
                        **window == [0xB9, case.elem().bytes() as u8, 0x00, 0x00, 0x00]
                    })
                    .count();
                assert_eq!(helpers, case.lanes(), "{level:?} {case:?}");
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, 384);
}

#[test]
fn segment_addr32_sib_rip_and_stack_default_shapes_lower_exactly() {
    for bytes in [
        vec![
            0x64, 0x67, 0xC4, 0x02, 0x71, 0x2C, 0x94, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ],
        vec![0x65, 0xC4, 0xE2, 0x75, 0x2F, 0x4D, 0x20],
        vec![0xC4, 0xE2, 0x71, 0x8C, 0x15, 0x11, 0x22, 0x33, 0x44],
        vec![0x36, 0xC4, 0xE2, 0xF1, 0x8E, 0x4C, 0x24, 0x20],
    ] {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_bytes(&bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: {:#?}", function.blocks[0].ops));
            let opcode = match (sequence.encoding.load, sequence.encoding.elem) {
                (true, VecElementType::F32) => 0x2C,
                (true, VecElementType::F64) => 0x2D,
                (false, VecElementType::F32) => 0x2E,
                (false, VecElementType::F64) => 0x2F,
                (true, VecElementType::I32 | VecElementType::I64) => 0x8C,
                (false, VecElementType::I32 | VecElementType::I64) => 0x8E,
                _ => unreachable!(),
            };
            let case = MaskedMemoryCase {
                opcode,
                w: sequence.encoding.elem == VecElementType::I64,
                width: sequence.encoding.width,
                mask: sequence.encoding.mask,
                vector: sequence.encoding.vector,
                base: 0,
            };
            let _ = lower(&function, case);
        }
    }
}

#[test]
fn every_operation_provenance_and_complete_graph_boundary_fail_closed() {
    for case in [
        MaskedMemoryCase {
            opcode: 0x2C,
            w: false,
            width: VecWidth::V256,
            mask: 9,
            vector: 11,
            base: 13,
        },
        MaskedMemoryCase {
            opcode: 0x8E,
            w: true,
            width: VecWidth::V128,
            mask: 11,
            vector: 9,
            base: 12,
        },
    ] {
        let exact = lift_case(case);
        assert_exact_graph(&exact, case);
        for index in 0..exact.blocks[0].ops.len() {
            let mut malformed = exact.clone();
            malformed.blocks[0].ops[index].x86_hint = Some(X86OpHint::MovImmModRm);
            assert_eq!(
                classified_sequence(&malformed, true),
                None,
                "{case:?}: op {index} accepted a foreign provenance hint"
            );
        }

        let mut preceding = exact.clone();
        let extra = preceding.blocks[0].ops[0].clone();
        preceding.blocks[0].ops.insert(0, extra);
        assert_eq!(classified_sequence(&preceding, true), None, "{case:?}");

        let mut trailing = exact.clone();
        let extra = trailing.blocks[0].ops[0].clone();
        trailing.blocks[0].ops.push(extra);
        assert_eq!(classified_sequence(&trailing, true), None, "{case:?}");

        let mut no_provenance = exact.clone();
        no_provenance.x86_instruction_bytes.clear();
        assert_eq!(classified_sequence(&no_provenance, true), None, "{case:?}");

        let opposite = MaskedMemoryCase {
            opcode: if case.load() { 0x8E } else { 0x8C },
            w: case.w,
            ..case
        };
        let mut wrong_bytes = exact;
        wrong_bytes.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&opposite.bytes()).unwrap(),
        );
        assert_eq!(classified_sequence(&wrong_bytes, true), None, "{case:?}");
    }
}
