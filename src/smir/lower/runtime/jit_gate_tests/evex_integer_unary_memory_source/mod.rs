//! Exact helper-backed EVEX unary packed-integer memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SourceArch, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexIntegerUnaryMemoryKind,
    X86EvexIntegerUnaryMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexIntegerUnaryMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_unary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E20;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IntegerUnaryOperation {
    ConflictD,
    ConflictQ,
    LeadingZerosD,
    LeadingZerosQ,
    PopcntB,
    PopcntW,
    PopcntD,
    PopcntQ,
}

impl IntegerUnaryOperation {
    const ALL: [Self; 8] = [
        Self::ConflictD,
        Self::ConflictQ,
        Self::LeadingZerosD,
        Self::LeadingZerosQ,
        Self::PopcntB,
        Self::PopcntW,
        Self::PopcntD,
        Self::PopcntQ,
    ];

    const fn kind(self) -> X86EvexIntegerUnaryMemoryKind {
        match self {
            Self::ConflictD | Self::ConflictQ => X86EvexIntegerUnaryMemoryKind::Conflict,
            Self::LeadingZerosD | Self::LeadingZerosQ => {
                X86EvexIntegerUnaryMemoryKind::LeadingZeros
            }
            Self::PopcntB | Self::PopcntW | Self::PopcntD | Self::PopcntQ => {
                X86EvexIntegerUnaryMemoryKind::Popcnt
            }
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::PopcntB => VecElementType::I8,
            Self::PopcntW => VecElementType::I16,
            Self::ConflictD | Self::LeadingZerosD | Self::PopcntD => VecElementType::I32,
            Self::ConflictQ | Self::LeadingZerosQ | Self::PopcntQ => VecElementType::I64,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::ConflictD | Self::ConflictQ => 0xC4,
            Self::LeadingZerosD | Self::LeadingZerosQ => 0x44,
            Self::PopcntB | Self::PopcntW => 0x54,
            Self::PopcntD | Self::PopcntQ => 0x55,
        }
    }

    const fn w(self) -> bool {
        matches!(
            self,
            Self::ConflictQ | Self::LeadingZerosQ | Self::PopcntW | Self::PopcntQ
        )
    }

    const fn broadcast_allowed(self) -> bool {
        !matches!(self, Self::PopcntB | Self::PopcntW)
    }

    const fn needs_cd(self) -> bool {
        matches!(
            self,
            Self::ConflictD | Self::ConflictQ | Self::LeadingZerosD | Self::LeadingZerosQ
        )
    }

    const fn needs_bitalg(self) -> bool {
        matches!(self, Self::PopcntB | Self::PopcntW)
    }

    const fn needs_vpopcntdq(self) -> bool {
        matches!(self, Self::PopcntD | Self::PopcntQ)
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
pub(super) struct IntegerUnaryMemoryCase {
    pub(super) operation: IntegerUnaryOperation,
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) form: SourceForm,
    pub(super) control: MaskControl,
}

impl IntegerUnaryMemoryCase {
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
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    fn uses_k16_opmasks(self) -> bool {
        self.width.lanes(self.elem()) <= 16
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
        if self.mask() != 0 || self.broadcast() {
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
        _ => unreachable!("integer unary vector width"),
    }))
}

fn memory_encoding(case: IntegerUnaryMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && (!case.zeroing() || case.mask() != 0));
    assert!(!case.broadcast() || case.operation.broadcast_allowed());
    let mut p0 = 0x62;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(case.operation.w()) << 7) | 0x7D;
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

fn stack_encoding(case: IntegerUnaryMemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    // Masked broadcasts are deliberately replayed as a full stack vector so
    // independently materialized guest reads remain distinct.
    let replay_broadcast = case.broadcast() && case.mask() == 0;
    vec![
        0x62,
        p0,
        (u8::from(case.operation.w()) << 7) | 0x7D,
        (u8::from(case.zeroing()) << 7)
            | (case.ll() << 5)
            | (u8::from(replay_broadcast) << 4)
            | 0x08
            | case.mask(),
        case.operation.opcode(),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
}

fn register_encoding(case: IntegerUnaryMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16 && scratch != case.destination);
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
        X86InstructionBytes::new(bytes).expect("integer unary memory provenance"),
    );
    function
}

pub(super) fn lift_case(case: IntegerUnaryMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexIntegerUnaryMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_integer_unary_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

pub(super) fn lower(function: &SmirFunction, case: IntegerUnaryMemoryCase) -> (Vec<u8>, usize) {
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
        case.uses_k16_opmasks(),
        "{case:?}"
    );

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx512bw,
        !case.uses_k16_opmasks(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512cd,
        case.operation.needs_cd(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512bitalg,
        case.operation.needs_bitalg(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vpopcntdq,
        case.operation.needs_vpopcntdq(),
        "{case:?}"
    );
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.uses_k16_opmasks(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_narrow_vector_opmask_helpers(case.uses_k16_opmasks());
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: integer unary memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed integer unary"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<IntegerUnaryMemoryCase> {
    let mut cases = Vec::new();
    for operation in IntegerUnaryOperation::ALL {
        let forms: &[SourceForm] = if operation.broadcast_allowed() {
            &[SourceForm::Vector, SourceForm::Broadcast]
        } else {
            &[SourceForm::Vector]
        };
        for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
            .into_iter()
            .enumerate()
        {
            for &form in forms {
                for control in MaskControl::ALL {
                    cases.push(IntegerUnaryMemoryCase {
                        operation,
                        width,
                        destination: [1, 9, 17][width_index],
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
fn integer_unary_rewrites_match_eight_independent_llvm_23_anchor_pairs() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0x7D, 0x08, 0xC4, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x08, 0xC4, 0xC8],
        ),
        (
            &[0x62, 0x72, 0xFD, 0x2B, 0xC4, 0x0C, 0x24],
            &[0x62, 0x72, 0xFD, 0x2B, 0xC4, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x7D, 0xDB, 0x44, 0x0A],
            &[0x62, 0xE2, 0x7D, 0xCB, 0x44, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x72, 0xFD, 0x38, 0x44, 0x0A],
            &[0x62, 0x72, 0xFD, 0x38, 0x44, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x08, 0x54, 0x0A],
            &[0x62, 0xF2, 0x7D, 0x08, 0x54, 0xC8],
        ),
        (
            &[0x62, 0x72, 0xFD, 0x2B, 0x54, 0x0C, 0x24],
            &[0x62, 0x72, 0xFD, 0x2B, 0x54, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x7D, 0xDB, 0x55, 0x0A],
            &[0x62, 0xE2, 0x7D, 0xCB, 0x55, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0xFD, 0x58, 0x55, 0x0A],
            &[0x62, 0xE2, 0xFD, 0x58, 0x55, 0x0C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_unary_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexIntegerUnaryMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexIntegerUnaryMemoryReplay::Broadcast { stack_instruction }
            | X86EvexIntegerUnaryMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn integer_unary_classifier_exhausts_80_640_operand_mask_and_apx_cells() {
    let mut accepted = 0usize;
    for template in all_cases()
        .into_iter()
        .filter(|case| case.control == MaskControl::None)
    {
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
                    let case = IntegerUnaryMemoryCase {
                        destination,
                        control,
                        ..template
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
                                .evex_integer_unary_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.kind, case.operation.kind(), "{bytes:02X?}");
                            assert_eq!(encoding.width, case.width, "{bytes:02X?}");
                            assert_eq!(encoding.elem, case.elem(), "{bytes:02X?}");
                            assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                            assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                            assert_eq!(encoding.opcode, case.operation.opcode());
                            assert_eq!(encoding.w, case.operation.w());
                            assert_eq!(encoding.broadcast, case.broadcast());
                            assert_eq!(encoding.needs_avx512cd, case.operation.needs_cd());
                            assert_eq!(encoding.needs_avx512bitalg, case.operation.needs_bitalg());
                            assert_eq!(
                                encoding.needs_avx512vpopcntdq,
                                case.operation.needs_vpopcntdq()
                            );
                            match encoding.replay {
                                X86EvexIntegerUnaryMemoryReplay::Vector { scratch, .. } => {
                                    assert_eq!(mask, 0);
                                    assert_eq!(case.form, SourceForm::Vector);
                                    assert_ne!(scratch, destination);
                                }
                                X86EvexIntegerUnaryMemoryReplay::Broadcast { .. } => {
                                    assert_eq!(mask, 0);
                                    assert_eq!(case.form, SourceForm::Broadcast);
                                }
                                X86EvexIntegerUnaryMemoryReplay::MaskedVector { .. } => {
                                    assert_ne!(mask, 0);
                                }
                            }
                            accepted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 80_640);
}

#[test]
fn integer_unary_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::LeadingZerosD,
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
        (1, 0x04), // map
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
    let mut byte_broadcast = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::PopcntB,
        ..case
    }
    .bytes();
    byte_broadcast[3] |= 0x10;
    malformed.push(byte_broadcast);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_unary_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_unary_memory_encoding()
            .is_some()
    );
}

#[test]
fn all_126_integer_unary_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 126);
    let mut lowerings = 0usize;
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
            assert_eq!(exact.encoding.broadcast, case.broadcast());
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
        }
    }
    assert_eq!(lowerings, 126 * LEVELS.len());
}

#[test]
fn type_e4_integer_unary_graphs_preserve_exact_access_granularity() {
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
            assert_eq!(
                (ordinary_loads, pred_loads),
                if case.control == MaskControl::None {
                    (1, 0)
                } else {
                    (0, case.width.lanes(case.elem()) as usize)
                },
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn integer_unary_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::PopcntD,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    let broadcast_case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::LeadingZerosQ,
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
                // EVEX Full broadcast tuple scales disp8=2 by 8 bytes.
                disp: 16,
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
                "{name} {level:?}: expected {expected_address:?} in {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let apx_case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::ConflictQ,
        width: VecWidth::V512,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let mut apx = memory_encoding(apx_case, true);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
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
fn integer_unary_rejects_the_avx_only_state_bridge() {
    let case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::PopcntB,
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
        .expect_err("AVX-only state bridge must reject EVEX integer unary replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

#[test]
fn integer_unary_rejects_aggregate_masked_broadcast_graph() {
    let case = IntegerUnaryMemoryCase {
        operation: IntegerUnaryOperation::LeadingZerosQ,
        width: VecWidth::V256,
        destination: 9,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
    };
    let virtual_reg = |index| VReg::Virtual(VirtualId(index));
    let scalar = virtual_reg(0);
    let active_mask = virtual_reg(1);
    let negated = virtual_reg(2);
    let combined = virtual_reg(3);
    let predicate = virtual_reg(4);
    let loaded = virtual_reg(5);
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(case.mask())));
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = vec![
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ),
        SmirOp::new(
            OpId(1),
            PC,
            OpKind::And {
                dst: active_mask,
                src1: mask,
                src2: SrcOperand::Imm(0x0F),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        SmirOp::new(
            OpId(2),
            PC,
            OpKind::Neg {
                dst: negated,
                src: active_mask,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        SmirOp::new(
            OpId(3),
            PC,
            OpKind::Or {
                dst: combined,
                src1: active_mask,
                src2: SrcOperand::Reg(negated),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        SmirOp::new(
            OpId(4),
            PC,
            OpKind::Shr {
                dst: predicate,
                src: combined,
                amount: SrcOperand::Imm(63),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        SmirOp::new(
            OpId(5),
            PC,
            OpKind::PredLoad {
                dst: scalar,
                cond: predicate,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
                width: MemWidth::B8,
                signed: SignExtend::Zero,
            },
        ),
        SmirOp::new(
            OpId(6),
            PC,
            OpKind::VBroadcast {
                dst: loaded,
                scalar,
                elem: VecElementType::I64,
                lanes: 4,
            },
        ),
        SmirOp::new(
            OpId(7),
            PC,
            OpKind::VLeadingZeros {
                dst: vector(case.destination, case.width),
                src: loaded,
                mask: Some(mask),
                elem: VecElementType::I64,
                width: VecWidth::V256,
                zeroing: false,
            },
        ),
    ];
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&case.bytes()).expect("masked-broadcast provenance"),
    );

    assert_rejected("aggregate masked broadcast", &function);
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function, true).is_none(), "{name}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn integer_unary_sequence_fails_closed_for_provenance_graph_and_frontier_mutations() {
    for case in [
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::ConflictD,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::LeadingZerosQ,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntB,
            width: VecWidth::V128,
            destination: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
    ] {
        for level in LEVELS {
            let canonical = optimize(lift_case(case), level);
            assert!(sequence(&canonical, true).is_some());

            let mut provenance = canonical.clone();
            provenance.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&[0x62, 0xF2, 0x7D, 0x08, 0x55, 0x03]).unwrap(),
            );
            assert_rejected("mismatched provenance", &provenance);

            let mut semantic = canonical.clone();
            let unary = semantic.blocks[0]
                .ops
                .iter_mut()
                .find(|op| {
                    matches!(
                        op.kind,
                        OpKind::VConflict { .. }
                            | OpKind::VLeadingZeros { .. }
                            | OpKind::VPopcnt { .. }
                    )
                })
                .unwrap();
            match &mut unary.kind {
                OpKind::VConflict { src, .. }
                | OpKind::VLeadingZeros { src, .. }
                | OpKind::VPopcnt { src, .. } => *src = vector(case.destination, case.width),
                _ => unreachable!(),
            }
            assert_rejected("wrong memory consumer", &semantic);

            if case.control != MaskControl::None {
                let mut predicate = canonical.clone();
                let load = predicate.blocks[0]
                    .ops
                    .iter_mut()
                    .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    .unwrap();
                if let OpKind::PredLoad { ref mut width, .. } = load.kind {
                    *width = if *width == MemWidth::B8 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    };
                }
                assert_rejected("wrong predicated load width", &predicate);
            }

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
