//! Exact helper-backed Type-E9NF EVEX scalar-insert memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch, VReg};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86ScalarInsertMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitEvexScalarInsertMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_scalar_insert_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0xE9_0020;
pub(super) const MEMORY_ADDRESS: u64 = 0x3000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InsertShape {
    pub(super) kind: X86ScalarInsertMemoryKind,
    pub(super) w: bool,
}

impl InsertShape {
    pub(super) const fn map_opcode(self) -> (u8, u8) {
        match self.kind {
            X86ScalarInsertMemoryKind::Vpinsrw => (1, 0xC4),
            X86ScalarInsertMemoryKind::Vpinsrb => (3, 0x20),
            X86ScalarInsertMemoryKind::Vinsertps => (3, 0x21),
            X86ScalarInsertMemoryKind::Vpinsrd | X86ScalarInsertMemoryKind::Vpinsrq => (3, 0x22),
        }
    }

    pub(super) const fn needs_avx512bw(self) -> bool {
        matches!(
            self.kind,
            X86ScalarInsertMemoryKind::Vpinsrb | X86ScalarInsertMemoryKind::Vpinsrw
        )
    }

    pub(super) const fn needs_avx512dq(self) -> bool {
        matches!(
            self.kind,
            X86ScalarInsertMemoryKind::Vpinsrd | X86ScalarInsertMemoryKind::Vpinsrq
        )
    }
}

pub(super) const SHAPES: [InsertShape; 7] = [
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vinsertps,
        w: false,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrb,
        w: false,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrb,
        w: true,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrw,
        w: false,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrw,
        w: true,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrd,
        w: false,
    },
    InsertShape {
        kind: X86ScalarInsertMemoryKind::Vpinsrq,
        w: true,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InsertCase {
    pub(super) shape: InsertShape,
    pub(super) destination: u8,
    pub(super) source1: u8,
    pub(super) immediate: u8,
}

impl InsertCase {
    pub(super) fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    pub(super) fn scratch(self) -> u8 {
        (0..16u8)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .unwrap()
    }

    pub(super) fn expected_replay(self) -> X86InstructionBytes {
        let bytes = self.bytes();
        let source = if self.shape.kind == X86ScalarInsertMemoryKind::Vinsertps {
            self.scratch()
        } else {
            0
        };
        X86InstructionBytes::new(&[
            0x62,
            (bytes[1] & 0x97) | 0x40 | if source & 8 == 0 { 0x20 } else { 0 },
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            0xC0 | ((self.destination & 7) << 3) | (source & 7),
            if self.shape.kind == X86ScalarInsertMemoryKind::Vinsertps {
                self.immediate & 0x3F
            } else {
                self.immediate
            },
        ])
        .unwrap()
    }
}

pub(super) fn memory_encoding(case: InsertCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    let (map, opcode) = case.shape.map_opcode();
    let mut p0 = 0xF0 | map;
    if case.destination & 8 != 0 {
        p0 &= !0x80;
    }
    if case.destination & 16 != 0 {
        p0 &= !0x10;
    }
    let mut bytes = vec![
        0x62,
        p0,
        (u8::from(case.shape.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05,
        u8::from(case.source1 < 16) << 3,
        opcode,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        bytes.push(0x48); // [RAX + RCX*2]
    }
    bytes.push(case.immediate);
    bytes
}

pub(super) fn lift_bytes(bytes: &[u8]) -> SmirFunction {
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
        X86InstructionBytes::new(bytes).expect("EVEX scalar-insert instruction provenance"),
    );
    function
}

pub(super) fn lift_case(case: InsertCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

pub(super) fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
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

pub(super) fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexScalarInsertMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_scalar_insert_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn configured_lowerer(avx_only: bool) -> X86_64Lowerer {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(avx_only);
    lowerer.set_narrow_vector_opmask_helpers(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
}

pub(super) fn lower(function: &SmirFunction, case: InsertCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.any && requirements.needs_avx);
    assert!(requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert_eq!(requirements.needs_avx512dq, case.shape.needs_avx512dq());
    assert!(!requirements.has_k16_opmask_span);
    assert!(!requirements.all_spans_support_avx_ymm16);

    let mut lowerer = configured_lowerer(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX scalar-insert lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize EVEX scalar-insert replay"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<InsertCase> {
    let mut cases = Vec::with_capacity(10);
    for (index, shape) in SHAPES.into_iter().enumerate() {
        cases.push(InsertCase {
            shape,
            destination: [0, 9, 17, 31][index & 3],
            source1: [1, 10, 18, 30][(index + 1) & 3],
            immediate: (index as u8).wrapping_mul(0x35) ^ 0xAF,
        });
    }
    for (shape, register, immediate) in [
        (SHAPES[0], 31, 0x0F),
        (SHAPES[1], 9, 0xFF),
        (SHAPES[6], 17, 0x81),
    ] {
        cases.push(InsertCase {
            shape,
            destination: register,
            source1: register,
            immediate,
        });
    }
    cases
}

#[test]
fn all_evex_scalar_insert_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 10);
    let mut lowerings = 0usize;
    for case in cases {
        let expected = case.expected_replay();
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_scalar_insert_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(encoding.destination, case.destination);
        assert_eq!(encoding.source1, case.source1);
        assert_eq!(encoding.kind, case.shape.kind);
        assert_eq!(encoding.immediate, case.immediate);
        assert_eq!(encoding.w, case.shape.w);
        assert_eq!(encoding.scratch, case.scratch());
        assert_eq!(encoding.register_instruction, expected);
        assert_eq!(encoding.needs_avx512bw, case.shape.needs_avx512bw());
        assert_eq!(encoding.needs_avx512dq, case.shape.needs_avx512dq());

        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            assert_eq!(exact.memory_size, case.shape.kind.memory_width().bytes());
            assert_eq!(exact.encoding, encoding);
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(expected.as_slice().len())
                    .any(|window| window == expected.as_slice()),
                "{level:?} {case:?}: missing {:02X?} in {} bytes",
                expected.as_slice(),
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 10 * LEVELS.len());
}

#[test]
fn evex_scalar_insert_full_vector_bridge_rejects_avx_only_lowering() {
    let case = all_cases()[0];
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    let mut lowerer = configured_lowerer(true);
    assert!(matches!(
        lowerer.lower_function(&function),
        Err(LowerError::InvalidOperand { .. })
    ));
}
