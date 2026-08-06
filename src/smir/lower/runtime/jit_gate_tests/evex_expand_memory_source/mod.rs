//! Exact helper-backed EVEX VEXPAND*/VPEXPAND* memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexExpandMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexExpandMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_expand_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0x5A5B_88E4;
pub(super) const MEMORY_ADDRESS: u64 = 0x2000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpandOperation {
    ExpandPs,
    ExpandPd,
    ExpandB,
    ExpandW,
    ExpandD,
    ExpandQ,
}

impl ExpandOperation {
    const ALL: [Self; 6] = [
        Self::ExpandPs,
        Self::ExpandPd,
        Self::ExpandB,
        Self::ExpandW,
        Self::ExpandD,
        Self::ExpandQ,
    ];

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::ExpandPs => "VEXPANDPS",
            Self::ExpandPd => "VEXPANDPD",
            Self::ExpandB => "VPEXPANDB",
            Self::ExpandW => "VPEXPANDW",
            Self::ExpandD => "VPEXPANDD",
            Self::ExpandQ => "VPEXPANDQ",
        }
    }

    pub(super) const fn elem(self) -> VecElementType {
        match self {
            Self::ExpandB => VecElementType::I8,
            Self::ExpandW => VecElementType::I16,
            Self::ExpandD => VecElementType::I32,
            Self::ExpandQ => VecElementType::I64,
            Self::ExpandPs => VecElementType::F32,
            Self::ExpandPd => VecElementType::F64,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::ExpandB | Self::ExpandW => 0x62,
            Self::ExpandPs | Self::ExpandPd => 0x88,
            Self::ExpandD | Self::ExpandQ => 0x89,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::ExpandPd | Self::ExpandW | Self::ExpandQ)
    }

    pub(super) const fn needs_vbmi2(self) -> bool {
        matches!(self, Self::ExpandB | Self::ExpandW)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (3, false),
            Self::Zero => (3, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExpandMemoryCase {
    pub(super) operation: ExpandOperation,
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) control: MaskControl,
}

impl ExpandMemoryCase {
    pub(super) const fn elem(self) -> VecElementType {
        self.operation.elem()
    }

    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    pub(super) const fn mask(self) -> u8 {
        self.control.fields().0
    }

    pub(super) const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    pub(super) fn lanes(self) -> usize {
        self.width.lanes(self.elem()) as usize
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination)
            .expect("one destination leaves a low vector scratch")
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.mask() == 0 {
            register_encoding(self, self.scratch())
        } else {
            stack_encoding(self)
        }
    }
}

fn memory_encoding(case: ExpandMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.operation.w()) << 7) | 0x7D;
    let p2 = (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.operation.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are varied independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn register_encoding(case: ExpandMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16 && scratch != case.destination && case.mask() == 0);
    let p0 = 0x42
        | if scratch & 8 == 0 { 0x20 } else { 0 }
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.operation.w()) << 7) | 0x7D,
        (case.ll() << 5) | 0x08,
        case.operation.opcode(),
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
    ]
}

fn stack_encoding(case: ExpandMemoryCase) -> Vec<u8> {
    assert!(case.mask() != 0);
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.operation.w()) << 7) | 0x7D,
        (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask(),
        case.operation.opcode(),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
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
        X86InstructionBytes::new(bytes).expect("packed expand instruction provenance"),
    );
    function
}

pub(super) fn lift_case(case: ExpandMemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexExpandMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_expand_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

pub(super) fn lower(function: &SmirFunction, case: ExpandMemoryCase) -> (Vec<u8>, usize) {
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
    // The current helper bridge deliberately retains full KMOVQ marshalling.
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any && requirements.needs_avx);
    assert!(requirements.needs_avx512bw);
    assert_eq!(requirements.needs_avx512vl, case.width != VecWidth::V512);
    assert_eq!(requirements.needs_avx512vbmi2, case.operation.needs_vbmi2());
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.mask() != 0 && case.lanes() <= 16
    );
    assert!(!requirements.all_spans_support_avx_ymm16);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_narrow_vector_opmask_helpers(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed expand lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize packed expand replay"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<ExpandMemoryCase> {
    let mut cases = Vec::new();
    for operation in ExpandOperation::ALL {
        for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
            .into_iter()
            .enumerate()
        {
            for control in MaskControl::ALL {
                cases.push(ExpandMemoryCase {
                    operation,
                    width,
                    destination: [0, 17, 31][width_index],
                    control,
                });
            }
        }
    }
    cases
}

#[test]
fn expand_rewrites_match_six_independent_llvm_23_memory_anchors() {
    // Source and `[rsp]` replay encodings were produced independently by
    // llvm-mc 23.0.0git. Source disp8 values are 127 times Tuple1 Scalar N.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xC2, 0x7D, 0xCB, 0x62, 0x4A, 0x7F],
            &[0x62, 0xE2, 0x7D, 0xCB, 0x62, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xC2, 0xFD, 0x2C, 0x62, 0x53, 0x7F],
            &[0x62, 0xE2, 0xFD, 0x2C, 0x62, 0x14, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7D, 0x8D, 0x89, 0x5C, 0x24, 0x7F],
            &[0x62, 0xE2, 0x7D, 0x8D, 0x89, 0x1C, 0x24],
        ),
        (
            &[0x62, 0xC2, 0xFD, 0x4E, 0x89, 0x65, 0x7F],
            &[0x62, 0xE2, 0xFD, 0x4E, 0x89, 0x24, 0x24],
        ),
        (
            &[0x62, 0xC2, 0x7D, 0xAF, 0x88, 0x6E, 0x7F],
            &[0x62, 0xE2, 0x7D, 0xAF, 0x88, 0x2C, 0x24],
        ),
        (
            &[0x62, 0xC2, 0xFD, 0x09, 0x88, 0x77, 0x7F],
            &[0x62, 0xE2, 0xFD, 0x09, 0x88, 0x34, 0x24],
        ),
    ];
    for (memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_expand_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let X86EvexExpandMemoryReplay::MaskedVector { stack_instruction } = encoding.replay else {
            panic!("masked source selected unmasked replay: {memory:02X?}")
        };
        assert_eq!(stack_instruction.as_slice(), *replay, "{memory:02X?}");
    }
}

#[test]
fn expand_classifier_exhausts_34_560_operand_mask_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in ExpandOperation::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for mask in 0..8u8 {
                    for zeroing in [false, true] {
                        if zeroing && mask == 0 {
                            continue;
                        }
                        let control = if mask == 0 {
                            MaskControl::None
                        } else if zeroing {
                            MaskControl::Zero
                        } else {
                            MaskControl::Merge
                        };
                        let case = ExpandMemoryCase {
                            operation,
                            width,
                            destination,
                            control,
                        };
                        let mut canonical = memory_encoding(case, true);
                        canonical[3] = (canonical[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                        for base_high in [false, true] {
                            for index_high in [false, true] {
                                let mut bytes = canonical.clone();
                                bytes[1] |= u8::from(base_high) << 3;
                                if index_high {
                                    bytes[2] &= !0x04;
                                }
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_expand_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, operation.elem(), "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                                assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                                assert_eq!(encoding.needs_avx512vbmi2, operation.needs_vbmi2());
                                match encoding.replay {
                                    X86EvexExpandMemoryReplay::Vector { scratch, .. } => {
                                        assert_eq!(mask, 0);
                                        assert_ne!(scratch, destination);
                                    }
                                    X86EvexExpandMemoryReplay::MaskedVector { .. } => {
                                        assert_ne!(mask, 0)
                                    }
                                }
                                accepted += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 34_560);
}

#[test]
fn expand_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = ExpandMemoryCase {
        operation: ExpandOperation::ExpandD,
        width: VecWidth::V256,
        destination: 17,
        control: MaskControl::Merge,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x01), // mandatory prefix
        (2, 0x08), // reserved vvvv
        (3, 0x08), // reserved V'
        (3, 0x10), // reserved EVEX.b
        (4, 0x10), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_expand_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_expand_memory_encoding()
            .is_some()
    );
}

#[test]
fn all_54_expand_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut lowerings = 0usize;
    for case in cases {
        let classified = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_expand_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(classified.width, case.width);
        assert_eq!(classified.elem, case.elem());
        assert_eq!(classified.destination, case.destination);
        assert_eq!(classified.writemask, (case.mask() != 0).then_some(3));
        assert_eq!(classified.zeroing, case.zeroing());
        assert_eq!(classified.needs_avx512vbmi2, case.operation.needs_vbmi2());

        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding, classified, "{level:?} {case:?}");
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            assert_eq!(exact.address_offset, 0);
            assert_eq!(exact.memory_size, case.width.bytes());
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
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
    assert_eq!(lowerings, 54 * LEVELS.len());
}

#[test]
fn expand_matcher_rejects_graph_provenance_and_virtual_escape_mutations() {
    let case = ExpandMemoryCase {
        operation: ExpandOperation::ExpandD,
        width: VecWidth::V512,
        destination: 17,
        control: MaskControl::Merge,
    };
    let valid = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&valid, true).is_some());

    let mut wrong_width = valid.clone();
    let pred_load = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let OpKind::PredLoad { width, .. } = &mut pred_load.kind else {
        unreachable!()
    };
    *width = MemWidth::B8;
    assert!(sequence(&wrong_width, true).is_none());

    let mut wrong_provenance = valid.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x62, 0xF2, 0x7D, 0x49, 0x8B, 0x02]).unwrap(),
    );
    assert!(sequence(&wrong_provenance, true).is_none());

    let mut missing_commit_hint = valid.clone();
    missing_commit_hint.blocks[0]
        .ops
        .last_mut()
        .unwrap()
        .x86_hint = None;
    assert!(sequence(&missing_commit_hint, true).is_none());

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
fn expand_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let case = ExpandMemoryCase {
        operation: ExpandOperation::ExpandQ,
        width: VecWidth::V512,
        destination: 17,
        control: MaskControl::Merge,
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
    assert!(matches!(
        base.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    let mut missing_guard = base.clone();
    missing_guard.blocks[0].ops.remove(0);
    assert!(sequence(&missing_guard, true).is_none());
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        sequence(&function, true).unwrap_or_else(|| panic!("APX {level:?}: {apx:02X?}"));
        lower(&function, case);
    }
}

#[test]
fn expand_rejects_the_avx_only_state_bridge() {
    let case = ExpandMemoryCase {
        operation: ExpandOperation::ExpandB,
        width: VecWidth::V512,
        destination: 17,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX packed expand replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
