//! Exact helper-backed EVEX VCOMPRESS*/VPCOMPRESS* memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexCompressMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_compress_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0x5A5B_8AE4;
pub(super) const MEMORY_ADDRESS: u64 = 0x2000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompressOperation {
    CompressPs,
    CompressPd,
    CompressB,
    CompressW,
    CompressD,
    CompressQ,
}

impl CompressOperation {
    const ALL: [Self; 6] = [
        Self::CompressPs,
        Self::CompressPd,
        Self::CompressB,
        Self::CompressW,
        Self::CompressD,
        Self::CompressQ,
    ];

    pub(super) const fn elem(self) -> VecElementType {
        match self {
            Self::CompressB => VecElementType::I8,
            Self::CompressW => VecElementType::I16,
            Self::CompressD => VecElementType::I32,
            Self::CompressQ => VecElementType::I64,
            Self::CompressPs => VecElementType::F32,
            Self::CompressPd => VecElementType::F64,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::CompressB | Self::CompressW => 0x63,
            Self::CompressPs | Self::CompressPd => 0x8A,
            Self::CompressD | Self::CompressQ => 0x8B,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::CompressPd | Self::CompressW | Self::CompressQ)
    }

    pub(super) const fn needs_vbmi2(self) -> bool {
        matches!(self, Self::CompressB | Self::CompressW)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaskControl {
    None,
    Masked(u8),
}

impl MaskControl {
    const ALL: [Self; 2] = [Self::None, Self::Masked(3)];

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Masked(mask) => mask,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CompressMemoryCase {
    pub(super) operation: CompressOperation,
    pub(super) width: VecWidth,
    pub(super) source: u8,
    pub(super) control: MaskControl,
}

impl CompressMemoryCase {
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
        self.control.mask()
    }

    pub(super) fn lanes(self) -> usize {
        self.width.lanes(self.elem()) as usize
    }

    pub(super) const fn lane_bytes(self) -> usize {
        self.elem().bytes() as usize
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn expected_replay(self) -> Vec<u8> {
        stack_encoding(self)
    }
}

fn memory_encoding(case: CompressMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.source < 32);
    let p0 = 0x62
        | if case.source & 8 == 0 { 0x80 } else { 0 }
        | if case.source & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.operation.w()) << 7) | 0x7D;
    let p2 = (case.ll() << 5) | 0x08 | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.operation.opcode(),
        ((case.source & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are varied independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: CompressMemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.source & 8 == 0 { 0x80 } else { 0 }
        | if case.source & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.operation.w()) << 7) | 0x7D,
        (case.ll() << 5) | 0x08 | case.mask(),
        case.operation.opcode(),
        ((case.source & 7) << 3) | 4,
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
        X86InstructionBytes::new(bytes).expect("packed compress instruction provenance"),
    );
    function
}

pub(super) fn lift_case(case: CompressMemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexCompressMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_compress_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn first_stack_value_load(case: CompressMemoryCase) -> &'static [u8] {
    match case.lane_bytes() {
        1 => &[0x48, 0x0F, 0xB6, 0x54, 0x24, 0x10], // movzx rdx, byte [rsp+16]
        2 => &[0x48, 0x0F, 0xB7, 0x54, 0x24, 0x10], // movzx rdx, word [rsp+16]
        4 => &[0x8B, 0x54, 0x24, 0x10],             // mov edx, dword [rsp+16]
        8 => &[0x48, 0x8B, 0x54, 0x24, 0x10],       // mov rdx, qword [rsp+16]
        _ => unreachable!("packed compress scalar lane width"),
    }
}

pub(super) fn lower(function: &SmirFunction, case: CompressMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: packed compress lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize packed compress replay"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<CompressMemoryCase> {
    let mut cases = Vec::new();
    for operation in CompressOperation::ALL {
        for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
            .into_iter()
            .enumerate()
        {
            for control in MaskControl::ALL {
                cases.push(CompressMemoryCase {
                    operation,
                    width,
                    source: [0, 17, 31][width_index],
                    control,
                });
            }
        }
    }
    cases
}

#[test]
fn compress_rewrites_match_six_independent_llvm_23_memory_anchors() {
    // Source and `[rsp]` replay encodings were produced independently by
    // llvm-mc 23.0.0git. Source disp8 values are 127 times Tuple1 Scalar N.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0x52, 0x7D, 0x0B, 0x63, 0x4A, 0x7F],
            &[0x62, 0x72, 0x7D, 0x0B, 0x63, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x52, 0xFD, 0x2C, 0x63, 0x53, 0x7F],
            &[0x62, 0x72, 0xFD, 0x2C, 0x63, 0x14, 0x24],
        ),
        (
            &[0x62, 0x52, 0x7D, 0x4D, 0x8B, 0x5C, 0x24, 0x7F],
            &[0x62, 0x72, 0x7D, 0x4D, 0x8B, 0x1C, 0x24],
        ),
        (
            &[0x62, 0x52, 0xFD, 0x0E, 0x8B, 0x65, 0x7F],
            &[0x62, 0x72, 0xFD, 0x0E, 0x8B, 0x24, 0x24],
        ),
        (
            &[0x62, 0x52, 0x7D, 0x2F, 0x8A, 0x6E, 0x7F],
            &[0x62, 0x72, 0x7D, 0x2F, 0x8A, 0x2C, 0x24],
        ),
        (
            &[0x62, 0x52, 0xFD, 0x49, 0x8A, 0x77, 0x7F],
            &[0x62, 0x72, 0xFD, 0x49, 0x8A, 0x34, 0x24],
        ),
    ];
    for (memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_compress_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *replay,
            "{memory:02X?}"
        );
    }
}

#[test]
fn compress_classifier_exhausts_18_432_operand_mask_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in CompressOperation::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source in 0..32u8 {
                for mask in 0..8u8 {
                    let control = if mask == 0 {
                        MaskControl::None
                    } else {
                        MaskControl::Masked(mask)
                    };
                    let case = CompressMemoryCase {
                        operation,
                        width,
                        source,
                        control,
                    };
                    let mut canonical = memory_encoding(case, true);
                    canonical[3] = (canonical[3] & !7) | mask;
                    for base_high in [false, true] {
                        for index_high in [false, true] {
                            let mut bytes = canonical.clone();
                            bytes[1] |= u8::from(base_high) << 3;
                            if index_high {
                                bytes[2] &= !0x04;
                            }
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_compress_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                            assert_eq!(encoding.elem, operation.elem(), "{bytes:02X?}");
                            assert_eq!(encoding.source, source, "{bytes:02X?}");
                            assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                            assert_eq!(encoding.needs_avx512vbmi2, operation.needs_vbmi2());
                            assert_eq!(
                                encoding.stack_instruction.as_slice(),
                                stack_encoding(case),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18_432);
}

#[test]
fn compress_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = CompressMemoryCase {
        operation: CompressOperation::CompressD,
        width: VecWidth::V256,
        source: 17,
        control: MaskControl::Masked(3),
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
        (3, 0x80), // reserved EVEX.z
        (4, 0x10), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_compress_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_compress_memory_encoding()
            .is_some()
    );
}

#[test]
fn all_36_compress_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 36);
    let mut lowerings = 0usize;
    for case in cases {
        let classified = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_compress_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(classified.width, case.width);
        assert_eq!(classified.elem, case.elem());
        assert_eq!(classified.source, case.source);
        assert_eq!(classified.writemask, (case.mask() != 0).then_some(3));
        assert_eq!(classified.needs_avx512vbmi2, case.operation.needs_vbmi2());

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
            let value_load = first_stack_value_load(case);
            assert!(
                code.windows(value_load.len())
                    .any(|window| window == value_load),
                "{level:?} {case:?}: missing zero-extended helper value load {value_load:02X?}"
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 36 * LEVELS.len());
}

#[test]
fn compress_matcher_rejects_graph_provenance_and_virtual_escape_mutations() {
    let case = CompressMemoryCase {
        operation: CompressOperation::CompressD,
        width: VecWidth::V512,
        source: 17,
        control: MaskControl::Masked(3),
    };
    let valid = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&valid, true).is_some());

    let mut wrong_width = valid.clone();
    let pred_store = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredStore { .. }))
        .unwrap();
    let OpKind::PredStore { width, .. } = &mut pred_store.kind else {
        unreachable!()
    };
    *width = MemWidth::B8;
    assert!(sequence(&wrong_width, true).is_none());

    let mut wrong_provenance = valid.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x62, 0xF2, 0x7D, 0x49, 0x89, 0x02]).unwrap(),
    );
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
fn compress_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let case = CompressMemoryCase {
        operation: CompressOperation::CompressQ,
        width: VecWidth::V512,
        source: 17,
        control: MaskControl::Masked(3),
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
fn compress_rejects_the_avx_only_state_bridge() {
    let case = CompressMemoryCase {
        operation: CompressOperation::CompressB,
        width: VecWidth::V512,
        source: 17,
        control: MaskControl::Masked(3),
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX packed compress replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
