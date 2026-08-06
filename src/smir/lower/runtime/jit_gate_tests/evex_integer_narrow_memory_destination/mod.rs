//! Exact helper-backed EVEX VPMOV*/VPMOVS*/VPMOVUS* memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86NarrowMode, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitEvexIntegerNarrowMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_narrow_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0x5A5B_31E6;
pub(super) const MEMORY_ADDRESS: u64 = 0x2000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NarrowOperation {
    pub(super) src_elem: VecElementType,
    pub(super) dst_elem: VecElementType,
    pub(super) mode: X86NarrowMode,
}

impl NarrowOperation {
    pub(super) fn all() -> Vec<Self> {
        let mut operations = Vec::with_capacity(18);
        for mode in [
            X86NarrowMode::UnsignedSaturate,
            X86NarrowMode::SignedSaturate,
            X86NarrowMode::Truncate,
        ] {
            for (src_elem, dst_elem) in [
                (VecElementType::I16, VecElementType::I8),
                (VecElementType::I32, VecElementType::I8),
                (VecElementType::I64, VecElementType::I8),
                (VecElementType::I32, VecElementType::I16),
                (VecElementType::I64, VecElementType::I16),
                (VecElementType::I64, VecElementType::I32),
            ] {
                operations.push(Self {
                    src_elem,
                    dst_elem,
                    mode,
                });
            }
        }
        operations
    }

    pub(super) const fn opcode(self) -> u8 {
        let high = match self.mode {
            X86NarrowMode::UnsignedSaturate => 0x10,
            X86NarrowMode::SignedSaturate => 0x20,
            X86NarrowMode::Truncate => 0x30,
        };
        let low = match (self.src_elem, self.dst_elem) {
            (VecElementType::I16, VecElementType::I8) => 0,
            (VecElementType::I32, VecElementType::I8) => 1,
            (VecElementType::I64, VecElementType::I8) => 2,
            (VecElementType::I32, VecElementType::I16) => 3,
            (VecElementType::I64, VecElementType::I16) => 4,
            (VecElementType::I64, VecElementType::I32) => 5,
            _ => unreachable!(),
        };
        high | low
    }

    pub(super) fn needs_avx512bw(self) -> bool {
        self.src_elem == VecElementType::I16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NarrowMemoryCase {
    pub(super) operation: NarrowOperation,
    pub(super) width: VecWidth,
    pub(super) source: u8,
    pub(super) writemask: Option<u8>,
}

impl NarrowMemoryCase {
    pub(super) const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    pub(super) fn mask(self) -> u8 {
        self.writemask.unwrap_or(0)
    }

    pub(super) fn lanes(self) -> usize {
        self.width.lanes(self.operation.src_elem) as usize
    }

    pub(super) const fn lane_bytes(self) -> usize {
        self.operation.dst_elem.bytes() as usize
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    pub(super) fn expected_replay(self) -> Vec<u8> {
        stack_encoding(self)
    }
}

pub(super) fn memory_encoding(case: NarrowMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.source < 32);
    let p0 = 0x62
        | if case.source & 8 == 0 { 0x80 } else { 0 }
        | if case.source & 16 == 0 { 0x10 } else { 0 };
    let mut bytes = vec![
        0x62,
        p0,
        0x7E,
        (case.ll() << 5) | 0x08 | case.mask(),
        case.operation.opcode(),
        ((case.source & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are varied independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: NarrowMemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.source & 8 == 0 { 0x80 } else { 0 }
        | if case.source & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        0x7E,
        (case.ll() << 5) | 0x08,
        case.operation.opcode(),
        ((case.source & 7) << 3) | 4,
        0x24,
    ]
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
        X86InstructionBytes::new(bytes).expect("integer-narrow instruction provenance"),
    );
    function
}

pub(super) fn lift_case(case: NarrowMemoryCase) -> SmirFunction {
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexIntegerNarrowMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_integer_narrow_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

pub(super) fn lower(function: &SmirFunction, case: NarrowMemoryCase) -> (Vec<u8>, usize) {
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
    // The current full helper bridge deliberately retains KMOVQ marshalling.
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any && requirements.needs_avx);
    assert!(requirements.needs_avx512bw);
    assert_eq!(requirements.needs_avx512vl, case.width != VecWidth::V512);
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.writemask.is_some() && case.lanes() <= 16
    );
    assert!(!requirements.all_spans_support_avx_ymm16);

    let mut lowerer = configured_lowerer(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: integer-narrow lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize integer-narrow replay"),
        result.entry_offset,
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

pub(super) fn all_cases() -> Vec<NarrowMemoryCase> {
    let mut cases = Vec::with_capacity(108);
    for operation in NarrowOperation::all() {
        for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
            .into_iter()
            .enumerate()
        {
            for writemask in [None, Some(3)] {
                cases.push(NarrowMemoryCase {
                    operation,
                    width,
                    source: [0, 17, 31][width_index],
                    writemask,
                });
            }
        }
    }
    cases
}

#[test]
fn all_108_integer_narrow_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
    let mut lowerings = 0usize;
    for case in cases {
        let classified = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_integer_narrow_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(classified.width, case.width);
        assert_eq!(classified.src_elem, case.operation.src_elem);
        assert_eq!(classified.dst_elem, case.operation.dst_elem);
        assert_eq!(classified.mode, case.operation.mode);
        assert_eq!(classified.source, case.source);
        assert_eq!(classified.writemask, case.writemask);

        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding, classified, "{level:?} {case:?}");
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            assert_eq!(exact.address_offset, 0);
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::PredStore { .. }))
                    .count(),
                case.lanes(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 108 * LEVELS.len());
}

#[test]
fn integer_narrow_matcher_rejects_graph_provenance_and_virtual_escape_mutations() {
    let case = NarrowMemoryCase {
        operation: NarrowOperation {
            src_elem: VecElementType::I64,
            dst_elem: VecElementType::I16,
            mode: X86NarrowMode::SignedSaturate,
        },
        width: VecWidth::V512,
        source: 17,
        writemask: Some(3),
    };
    let valid = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&valid, true).is_some());

    let mut wrong_width = valid.clone();
    let store = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredStore { .. }))
        .unwrap();
    let OpKind::PredStore { width, .. } = &mut store.kind else {
        unreachable!()
    };
    *width = MemWidth::B8;
    assert!(sequence(&wrong_width, true).is_none());

    let mut wrong_mode = valid.clone();
    let narrow = wrong_mode.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::X86NarrowInt { .. }))
        .unwrap();
    let OpKind::X86NarrowInt { mode, .. } = &mut narrow.kind else {
        unreachable!()
    };
    *mode = X86NarrowMode::UnsignedSaturate;
    assert!(sequence(&wrong_mode, true).is_none());

    let mut wrong_provenance = valid.clone();
    let mut bytes = case.bytes();
    bytes[4] = 0x31;
    wrong_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(sequence(&wrong_provenance, true).is_none());

    let mut wrong_source = valid.clone();
    let extract = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    let OpKind::VExtractLane { vec, .. } = &mut extract.kind else {
        unreachable!()
    };
    *vec = VReg::Arch(ArchReg::X86(X86Reg::Zmm(18)));
    assert!(sequence(&wrong_source, true).is_none());

    let mut wrong_address = valid.clone();
    let store = wrong_address.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredStore { .. }))
        .unwrap();
    let OpKind::PredStore { addr, .. } = &mut store.kind else {
        unreachable!()
    };
    let Address::BaseOffset { offset, .. } = addr else {
        unreachable!()
    };
    *offset += 1;
    assert!(sequence(&wrong_address, true).is_none());

    let mut escaped = valid.clone();
    let leaked = escaped.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .find(|register| matches!(register, VReg::Virtual(_)))
        .unwrap();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(u16::MAX),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(u32::MAX)),
            src: SrcOperand::Reg(leaked),
            width: OpWidth::W64,
        },
    ));
    assert!(sequence(&escaped, true).is_none());
}

#[test]
fn integer_narrow_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let case = NarrowMemoryCase {
        operation: NarrowOperation {
            src_elem: VecElementType::I64,
            dst_elem: VecElementType::I16,
            mode: X86NarrowMode::SignedSaturate,
        },
        width: VecWidth::V512,
        source: 17,
        writemask: Some(3),
    };
    let mut rip = case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    for (name, bytes) in [("RIP", rip), ("addr32", addr32), ("FS", fs)] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let mut apx = memory_encoding(case, true);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let base = lift_bytes(&apx);
    assert!(matches!(base.blocks[0].ops[0].kind, OpKind::X86RequireApx));
    let mut missing_guard = base.clone();
    missing_guard.blocks[0].ops.remove(0);
    assert!(sequence(&missing_guard, true).is_none());
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        sequence(&function, true)
            .unwrap_or_else(|| panic!("APX {level:?}: {:#?}", function.blocks[0].ops));
        lower(&function, case);
    }
}

#[test]
fn integer_narrow_full_vector_bridge_rejects_avx_only_lowering() {
    let case = all_cases()
        .into_iter()
        .find(|case| case.writemask.is_some())
        .unwrap();
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    let mut lowerer = configured_lowerer(true);
    assert!(matches!(
        lowerer.lower_function(&function),
        Err(LowerError::InvalidOperand { .. })
    ));
}
