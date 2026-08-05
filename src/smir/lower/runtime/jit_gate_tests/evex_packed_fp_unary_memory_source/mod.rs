//! Exact helper-backed EVEX packed unary floating-point memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, Avx10FP16Op, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpId,
    SourceArch, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedFpUnaryMemoryKind,
    X86EvexPackedFpUnaryMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedFpUnaryMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_fp_unary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7D40;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PackedUnaryOperation {
    SqrtF16,
    SqrtF32,
    SqrtF64,
    GetExpF16,
    GetExpF32,
    GetExpF64,
    Recip14F32,
    Recip14F64,
    Rsqrt14F32,
    Rsqrt14F64,
    RecipFp16,
    RsqrtFp16,
}

impl PackedUnaryOperation {
    const ALL: [Self; 12] = [
        Self::SqrtF16,
        Self::SqrtF32,
        Self::SqrtF64,
        Self::GetExpF16,
        Self::GetExpF32,
        Self::GetExpF64,
        Self::Recip14F32,
        Self::Recip14F64,
        Self::Rsqrt14F32,
        Self::Rsqrt14F64,
        Self::RecipFp16,
        Self::RsqrtFp16,
    ];

    const fn kind(self) -> X86EvexPackedFpUnaryMemoryKind {
        match self {
            Self::SqrtF16 | Self::SqrtF32 | Self::SqrtF64 => X86EvexPackedFpUnaryMemoryKind::Sqrt,
            Self::GetExpF16 | Self::GetExpF32 | Self::GetExpF64 => {
                X86EvexPackedFpUnaryMemoryKind::GetExponent
            }
            Self::Recip14F32 | Self::Recip14F64 => X86EvexPackedFpUnaryMemoryKind::Recip14,
            Self::Rsqrt14F32 | Self::Rsqrt14F64 => X86EvexPackedFpUnaryMemoryKind::Rsqrt14,
            Self::RecipFp16 => X86EvexPackedFpUnaryMemoryKind::RecipFp16,
            Self::RsqrtFp16 => X86EvexPackedFpUnaryMemoryKind::RsqrtFp16,
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::SqrtF16 | Self::GetExpF16 | Self::RecipFp16 | Self::RsqrtFp16 => {
                VecElementType::F16
            }
            Self::SqrtF32 | Self::GetExpF32 | Self::Recip14F32 | Self::Rsqrt14F32 => {
                VecElementType::F32
            }
            Self::SqrtF64 | Self::GetExpF64 | Self::Recip14F64 | Self::Rsqrt14F64 => {
                VecElementType::F64
            }
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::SqrtF16 => 5,
            Self::SqrtF32 | Self::SqrtF64 => 1,
            Self::GetExpF16 | Self::RecipFp16 | Self::RsqrtFp16 => 6,
            Self::GetExpF32
            | Self::GetExpF64
            | Self::Recip14F32
            | Self::Recip14F64
            | Self::Rsqrt14F32
            | Self::Rsqrt14F64 => 2,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::SqrtF16 | Self::SqrtF32 => 0,
            _ => 1,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::SqrtF16 | Self::SqrtF32 | Self::SqrtF64 => 0x51,
            Self::GetExpF16 | Self::GetExpF32 | Self::GetExpF64 => 0x42,
            Self::Recip14F32 | Self::Recip14F64 | Self::RecipFp16 => 0x4C,
            Self::Rsqrt14F32 | Self::Rsqrt14F64 | Self::RsqrtFp16 => 0x4E,
        }
    }

    const fn w(self) -> bool {
        matches!(
            self,
            Self::SqrtF64 | Self::GetExpF64 | Self::Recip14F64 | Self::Rsqrt14F64
        )
    }

    const fn needs_fp16(self) -> bool {
        matches!(
            self,
            Self::SqrtF16 | Self::GetExpF16 | Self::RecipFp16 | Self::RsqrtFp16
        )
    }

    const fn uses_k16_opmasks(self) -> bool {
        matches!(
            self,
            Self::SqrtF32
                | Self::SqrtF64
                | Self::Recip14F32
                | Self::Recip14F64
                | Self::Rsqrt14F32
                | Self::Rsqrt14F64
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceForm {
    Vector,
    Broadcast,
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
pub(super) struct PackedUnaryMemoryCase {
    pub(super) operation: PackedUnaryOperation,
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) form: SourceForm,
    pub(super) control: MaskControl,
}

impl PackedUnaryMemoryCase {
    const fn elem(self) -> VecElementType {
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

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn memory_width(self) -> MemWidth {
        match self.elem() {
            VecElementType::F16 => MemWidth::B2,
            VecElementType::F32 => MemWidth::B4,
            VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination)
            .expect("one destination leaves a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("packed unary vector width"),
    }))
}

fn memory_encoding(case: PackedUnaryMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | case.operation.map();
    let p1 = (u8::from(case.operation.w()) << 7) | 0x7C | case.operation.pp();
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | 0x08
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.operation.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: PackedUnaryMemoryCase) -> Vec<u8> {
    let mut bytes = memory_encoding(case, true);
    *bytes.last_mut().expect("SIB byte") = 0x24;
    bytes
}

pub(super) fn register_encoding(case: PackedUnaryMemoryCase, source: u8) -> Vec<u8> {
    assert!(source < 16);
    let p0 = 0x40
        | if source & 8 == 0 { 0x20 } else { 0 }
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | case.operation.map();
    vec![
        0x62,
        p0,
        (u8::from(case.operation.w()) << 7) | 0x7C | case.operation.pp(),
        (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask(),
        case.operation.opcode(),
        0xC0 | ((case.destination & 7) << 3) | (source & 7),
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
        X86InstructionBytes::new(bytes).expect("packed unary memory provenance"),
    );
    function
}

pub(super) fn lift_case(case: PackedUnaryMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexPackedFpUnaryMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_fp_unary_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

pub(super) fn lower(function: &SmirFunction, case: PackedUnaryMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(
        x86_native_vector_uses_k16_opmasks_excluding(function, &excluded),
        case.operation.uses_k16_opmasks(),
        "{case:?}"
    );

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx512bw,
        !case.operation.uses_k16_opmasks(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert_eq!(
        requirements.needs_avx512fp16,
        case.operation.needs_fp16(),
        "{case:?}"
    );
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.operation.uses_k16_opmasks(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed unary memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed unary"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<PackedUnaryMemoryCase> {
    let mut cases = Vec::new();
    for operation in PackedUnaryOperation::ALL {
        for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
            .into_iter()
            .enumerate()
        {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    cases.push(PackedUnaryMemoryCase {
                        operation,
                        width,
                        destination: [0, 9, 17][width_index],
                        form,
                        control,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn packed_unary_rewrites_match_twelve_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xE1, 0x7C, 0x08, 0x51, 0x0B],
            &[0x62, 0xE1, 0x7C, 0x08, 0x51, 0xC8],
        ),
        (
            &[0x62, 0x71, 0xFD, 0x3B, 0x51, 0x0C, 0x24],
            &[0x62, 0x71, 0xFD, 0x3B, 0x51, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE5, 0x7C, 0xCB, 0x51, 0x08],
            &[0x62, 0xE5, 0x7C, 0xCB, 0x51, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x08, 0x42, 0x0B],
            &[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xC8],
        ),
        (
            &[0x62, 0x72, 0xFD, 0x3B, 0x42, 0x0C, 0x24],
            &[0x62, 0x72, 0xFD, 0x3B, 0x42, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE6, 0x7D, 0xCB, 0x42, 0x08],
            &[0x62, 0xE6, 0x7D, 0xCB, 0x42, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x7D, 0x48, 0x4C, 0x0A],
            &[0x62, 0xE2, 0x7D, 0x48, 0x4C, 0xC8],
        ),
        (
            &[0x62, 0x72, 0xFD, 0x3B, 0x4C, 0x0C, 0x24],
            &[0x62, 0x72, 0xFD, 0x3B, 0x4C, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x8B, 0x4E, 0x0B],
            &[0x62, 0xF2, 0x7D, 0x8B, 0x4E, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0xFD, 0x5B, 0x4E, 0x0C, 0x24],
            &[0x62, 0xE2, 0xFD, 0x5B, 0x4E, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x76, 0x7D, 0xAB, 0x4C, 0x08],
            &[0x62, 0x76, 0x7D, 0xAB, 0x4C, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE6, 0x7D, 0x5B, 0x4E, 0x0C, 0x24],
            &[0x62, 0xE6, 0x7D, 0x5B, 0x4E, 0x0C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_packed_fp_unary_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexPackedFpUnaryMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexPackedFpUnaryMemoryReplay::Broadcast { stack_instruction }
            | X86EvexPackedFpUnaryMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn packed_unary_classifier_exhausts_all_16_384_selector_cells() {
    let mut accepted = Vec::new();
    for map in 0..8u8 {
        for opcode in 0..=u8::MAX {
            for pp in 0..4u8 {
                for w in [false, true] {
                    let bytes = [
                        0x62,
                        0xF0 | map,
                        (u8::from(w) << 7) | 0x7C | pp,
                        0x08,
                        opcode,
                        0x0B,
                    ];
                    if let Some(encoding) = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_packed_fp_unary_memory_encoding()
                    {
                        accepted.push((map, opcode, pp, w, encoding.kind, encoding.elem));
                    }
                }
            }
        }
    }
    assert_eq!(accepted.len(), 12, "{accepted:#?}");
    assert_eq!(
        accepted
            .iter()
            .filter(|(_, _, _, _, kind, _)| *kind == X86EvexPackedFpUnaryMemoryKind::Sqrt)
            .count(),
        3,
        "{accepted:#?}"
    );
    for operation in PackedUnaryOperation::ALL {
        assert!(
            accepted.contains(&(
                operation.map(),
                operation.opcode(),
                operation.pp(),
                operation.w(),
                operation.kind(),
                operation.elem(),
            )),
            "{operation:?}: {accepted:#?}"
        );
    }
}

#[test]
fn packed_unary_classifier_exhausts_138_240_operand_mask_and_apx_cells() {
    let mut accepted = 0usize;
    let mut sqrt_accepted = 0usize;
    for operation in PackedUnaryOperation::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
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
                            let case = PackedUnaryMemoryCase {
                                operation,
                                width,
                                destination,
                                form,
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
                                        .evex_packed_fp_unary_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.kind, operation.kind(), "{bytes:02X?}");
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, operation.elem(), "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(encoding.map, operation.map(), "{bytes:02X?}");
                                    assert_eq!(encoding.w, operation.w(), "{bytes:02X?}");
                                    assert_eq!(encoding.opcode, operation.opcode(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.needs_avx512vl,
                                        width != VecWidth::V512,
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512fp16,
                                        operation.needs_fp16(),
                                        "{bytes:02X?}"
                                    );
                                    match encoding.replay {
                                        X86EvexPackedFpUnaryMemoryReplay::Vector {
                                            scratch,
                                            ..
                                        } => {
                                            assert_eq!(mask, 0, "{bytes:02X?}");
                                            assert_eq!(form, SourceForm::Vector);
                                            assert_ne!(scratch, destination, "{bytes:02X?}");
                                        }
                                        X86EvexPackedFpUnaryMemoryReplay::Broadcast { .. } => {
                                            assert_eq!(form, SourceForm::Broadcast);
                                        }
                                        X86EvexPackedFpUnaryMemoryReplay::MaskedVector {
                                            ..
                                        } => {
                                            assert_ne!(mask, 0, "{bytes:02X?}");
                                            assert_eq!(form, SourceForm::Vector);
                                        }
                                    }
                                    accepted += 1;
                                    sqrt_accepted += usize::from(
                                        encoding.kind == X86EvexPackedFpUnaryMemoryKind::Sqrt,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 138_240);
    assert_eq!(sqrt_accepted, 34_560);
}

#[test]
fn packed_unary_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = PackedUnaryMemoryCase {
        operation: PackedUnaryOperation::Recip14F32,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
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
        (1, 0x01), // unowned map
        (2, 0x01), // mandatory prefix
        (2, 0x08), // reserved vvvv
        (3, 0x08), // reserved V'
        (4, 0x20), // non-owned opcode
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
    let mut fp16_w = PackedUnaryMemoryCase {
        operation: PackedUnaryOperation::RecipFp16,
        ..case
    }
    .bytes();
    fp16_w[2] |= 0x80;
    malformed.push(fp16_w);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fp_unary_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_packed_fp_unary_memory_encoding()
            .is_some()
    );
}

#[test]
fn all_216_packed_unary_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 216);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.operation.kind() == X86EvexPackedFpUnaryMemoryKind::Sqrt)
            .count(),
        54
    );
    let mut lowerings = 0usize;
    let mut sqrt_lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.kind, case.operation.kind());
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.elem());
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.encoding.opcode, case.operation.opcode());
            assert_eq!(exact.encoding.w, case.operation.w());
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    case.memory_width().bytes()
                } else {
                    case.width.bytes()
                }
            );
            assert_eq!(exact.consumed, function.blocks[0].ops.len());

            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
            sqrt_lowerings +=
                usize::from(case.operation.kind() == X86EvexPackedFpUnaryMemoryKind::Sqrt);
        }
    }
    assert_eq!(lowerings, 216 * LEVELS.len());
    assert_eq!(sqrt_lowerings, 54 * LEVELS.len());
}

#[test]
fn type_e2_e4_graphs_preserve_exact_access_granularity() {
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let pred_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count();
            let ordinary_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
                .count();
            let lanes = case.width.lanes(case.elem()) as usize;
            assert_eq!(
                (ordinary_loads, pred_loads),
                match (case.control, case.form) {
                    (MaskControl::None, _) => (1, 0),
                    (_, SourceForm::Broadcast) => (0, 1),
                    (_, SourceForm::Vector) => (0, lanes),
                },
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn packed_unary_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = PackedUnaryMemoryCase {
        operation: PackedUnaryOperation::GetExpF32,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    let broadcast_case = PackedUnaryMemoryCase {
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
        ..vector_case
    };

    let mut rip = vector_case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = vector_case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast_case.bytes();
    fs.insert(0, 0x64);
    let mut gs_addr32 = broadcast_case.bytes();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.push(0x8B);
    gs_addr32.push(2);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let address_cases = [
        (
            "RIP+disp32",
            vector_case,
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 base",
            vector_case,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS broadcast",
            broadcast_case,
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB broadcast",
            broadcast_case,
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 8,
            })),
        ),
    ];

    for (name, case, bytes, expected_address) in address_cases {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. }
                    | OpKind::VLoad { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::Lea { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let apx_case = PackedUnaryMemoryCase {
        operation: PackedUnaryOperation::RecipFp16,
        width: VecWidth::V512,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let mut apx = memory_encoding(apx_case, true);
    apx[1] |= 0x08; // EVEX.B4 extends SIB base RAX to R16.
    apx[2] &= !0x04; // EVEX.X4/!U extends SIB index RCX to R17.
    let expected_address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    let base = lift_bytes(&apx);
    let mut missing_guard = base.clone();
    assert!(matches!(
        missing_guard.blocks[0].ops.remove(0).kind,
        OpKind::X86RequireApx
    ));
    assert_rejected("APX address without guard", &missing_guard);
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(function.blocks[0].ops.iter().any(|op| match &op.kind {
            OpKind::Lea { addr, .. } => addr == &expected_address,
            _ => false,
        }));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {apx:02X?}"));
        lower(&function, apx_case);
    }
}

#[test]
fn packed_unary_rejects_the_avx_only_state_bridge() {
    let case = PackedUnaryMemoryCase {
        operation: PackedUnaryOperation::RsqrtFp16,
        width: VecWidth::V512,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX packed unary");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function, true).is_none(), "{name}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

fn is_packed_unary(kind: &OpKind) -> bool {
    matches!(
        kind,
        OpKind::X86GetExponent { .. }
            | OpKind::X86Recip14 { .. }
            | OpKind::X86Rsqrt14 { .. }
            | OpKind::X86RecipFp16 { .. }
            | OpKind::X86RsqrtFp16 { .. }
    )
}

#[test]
fn packed_unary_sequence_fails_closed_for_provenance_graph_and_frontier_mutations() {
    for case in [
        PackedUnaryMemoryCase {
            operation: PackedUnaryOperation::GetExpF16,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        PackedUnaryMemoryCase {
            operation: PackedUnaryOperation::Rsqrt14F64,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ] {
        for level in LEVELS {
            let canonical = optimize(lift_case(case), level);
            assert!(sequence(&canonical, true).is_some());

            let mut provenance = canonical.clone();
            provenance.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0x03]).unwrap(),
            );
            assert_rejected("mismatched provenance", &provenance);

            let mut hint = canonical.clone();
            hint.blocks[0]
                .ops
                .iter_mut()
                .find(|op| is_packed_unary(&op.kind))
                .expect("packed unary semantic")
                .x86_hint = None;
            assert_rejected("missing exact unary hint", &hint);

            let mut semantic = canonical.clone();
            let unary = semantic.blocks[0]
                .ops
                .iter_mut()
                .find(|op| is_packed_unary(&op.kind))
                .expect("packed unary semantic");
            match &mut unary.kind {
                OpKind::X86GetExponent { src, .. }
                | OpKind::X86Recip14 { src, .. }
                | OpKind::X86Rsqrt14 { src, .. }
                | OpKind::X86RecipFp16 { src, .. }
                | OpKind::X86RsqrtFp16 { src, .. } => {
                    *src = vector(case.destination, case.width);
                }
                _ => unreachable!(),
            }
            assert_rejected("wrong memory consumer", &semantic);

            let mut frontier = canonical.clone();
            frontier.blocks[0].ops.push(SmirOp::new(
                OpId(u16::MAX - 1),
                PC,
                OpKind::Mov {
                    dst: VReg::Virtual(VirtualId(u32::MAX - 1)),
                    src: SrcOperand::Imm(0),
                    width: crate::smir::ir::types::OpWidth::W64,
                },
            ));
            assert_rejected("same-PC tail", &frontier);

            let mut spurious_apx = canonical.clone();
            spurious_apx.blocks[0].ops.insert(
                0,
                SmirOp::new(OpId(u16::MAX - 2), PC, OpKind::X86RequireApx),
            );
            assert_rejected("spurious APX address guard", &spurious_apx);
        }
    }
}

#[test]
fn packed_sqrt_sequence_fails_closed_for_semantic_hint_and_mask_tail_mutations() {
    for case in [
        PackedUnaryMemoryCase {
            operation: PackedUnaryOperation::SqrtF16,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        PackedUnaryMemoryCase {
            operation: PackedUnaryOperation::SqrtF32,
            width: VecWidth::V128,
            destination: 9,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        PackedUnaryMemoryCase {
            operation: PackedUnaryOperation::SqrtF64,
            width: VecWidth::V256,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ] {
        for level in LEVELS {
            let canonical = optimize(lift_case(case), level);
            assert!(sequence(&canonical, true).is_some(), "{level:?} {case:?}");
            let semantic_index = canonical.blocks[0]
                .ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86Sqrt { .. }
                            | OpKind::VFP16Arith {
                                op: Avx10FP16Op::Sqrt,
                                ..
                            }
                    )
                })
                .expect("packed sqrt semantic operation");

            let mut provenance = canonical.clone();
            let other_destination = (case.destination + 1) & 31;
            provenance.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(
                    &PackedUnaryMemoryCase {
                        destination: other_destination,
                        ..case
                    }
                    .bytes(),
                )
                .unwrap(),
            );
            assert_rejected("packed sqrt destination provenance", &provenance);

            let mut wrong_round = canonical.clone();
            match &mut wrong_round.blocks[0].ops[semantic_index].kind {
                OpKind::X86Sqrt { round, .. } | OpKind::VFP16Arith { round, .. } => {
                    *round = FpRoundMode::RoundNearest;
                }
                _ => unreachable!(),
            }
            assert_rejected("packed sqrt static rounding", &wrong_round);

            let mut wrong_source = canonical.clone();
            match &mut wrong_source.blocks[0].ops[semantic_index].kind {
                OpKind::X86Sqrt { src, .. } => {
                    *src = vector(other_destination, case.width);
                }
                OpKind::VFP16Arith { src2, .. } => {
                    *src2 = vector(other_destination, case.width);
                }
                _ => unreachable!(),
            }
            assert_rejected("packed sqrt source provenance", &wrong_source);

            let mut wrong_hint = canonical.clone();
            if case.elem() == VecElementType::F16 {
                wrong_hint.blocks[0].ops[semantic_index].x86_hint = Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map5,
                    pp: X86SsePrefix::None,
                    opcode: 0x51,
                    width: case.width,
                    w: false,
                });
            } else {
                wrong_hint.blocks[0].ops[semantic_index].x86_hint = None;
            }
            assert_rejected("packed sqrt semantic hint", &wrong_hint);

            if case.elem() != VecElementType::F16 {
                let mut wrong_tail = canonical.clone();
                let lane = wrong_tail.blocks[0].ops[semantic_index + 1..]
                    .iter_mut()
                    .find_map(|op| match &mut op.kind {
                        OpKind::VExtractLane { lane, .. } => Some(lane),
                        _ => None,
                    })
                    .expect("masked packed sqrt result has an extract tail");
                *lane ^= 1;
                assert_rejected("packed sqrt merge/zero tail", &wrong_tail);
            }
        }
    }
}
