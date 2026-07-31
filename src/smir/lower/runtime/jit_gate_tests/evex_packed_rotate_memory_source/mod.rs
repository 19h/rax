//! Exact helper-backed EVEX packed rotate memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedRotateMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedRotateMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_rotate_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7A40;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RotateKind {
    ImmediateRight,
    ImmediateLeft,
    VariableRight,
    VariableLeft,
}

impl RotateKind {
    const ALL: [Self; 4] = [
        Self::ImmediateRight,
        Self::ImmediateLeft,
        Self::VariableRight,
        Self::VariableLeft,
    ];

    const fn variable(self) -> bool {
        matches!(self, Self::VariableRight | Self::VariableLeft)
    }

    const fn left(self) -> bool {
        matches!(self, Self::ImmediateLeft | Self::VariableLeft)
    }

    const fn map(self) -> u8 {
        if self.variable() { 2 } else { 1 }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::ImmediateRight | Self::ImmediateLeft => 0x72,
            Self::VariableRight => 0x14,
            Self::VariableLeft => 0x15,
        }
    }

    const fn immediate_group(self) -> u8 {
        if self.left() { 1 } else { 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (1, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RotateMemoryCase {
    kind: RotateKind,
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source: Option<u8>,
    form: SourceForm,
    control: MaskControl,
    amount: u8,
}

impl RotateMemoryCase {
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
        match self.elem {
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(
            self.kind,
            self.elem,
            self.width,
            self.destination,
            self.source,
            self.mask(),
            self.zeroing(),
            self.broadcast(),
            self.amount,
            false,
        )
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| {
                *candidate != self.destination
                    && self.source.is_none_or(|source| *candidate != source)
            })
            .expect("at most two operands leave a low vector scratch")
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
        _ => unreachable!("packed rotate vector width"),
    }))
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    kind: RotateKind,
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source: Option<u8>,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
    amount: u8,
    sib: bool,
) -> Vec<u8> {
    assert!(destination < 32 && mask < 8 && (!zeroing || mask != 0));
    assert_eq!(source.is_some(), kind.variable());
    let source_or_destination = source.unwrap_or(destination);
    assert!(source_or_destination < 32);
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!("packed rotate width"),
    };
    let mut p0 = kind.map() | 0x40 | 0x20;
    if kind.variable() {
        if destination & 8 == 0 {
            p0 |= 0x80;
        }
        if destination & 16 == 0 {
            p0 |= 0x10;
        }
    } else {
        // R/R' are ignored by the immediate group's opcode extension.
        p0 |= 0x90;
    }
    let p1 = (if elem == VecElementType::I64 { 0x80 } else { 0 })
        | (((!source_or_destination) & 0x0F) << 3)
        | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (ll << 5)
        | (u8::from(broadcast) << 4)
        | (if source_or_destination & 16 == 0 {
            0x08
        } else {
            0
        })
        | mask;
    let reg = if kind.variable() {
        destination & 7
    } else {
        kind.immediate_group()
    };
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode(),
        (reg << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    if !kind.variable() {
        bytes.push(amount);
    }
    bytes
}

fn stack_encoding(case: RotateMemoryCase) -> Vec<u8> {
    let source_or_destination = case.source.unwrap_or(case.destination);
    let p0 = (if case.kind.variable() && case.destination & 8 == 0 {
        0x80
    } else if !case.kind.variable() {
        0x80
    } else {
        0
    }) | 0x60
        | (if case.kind.variable() && case.destination & 16 == 0 {
            0x10
        } else if !case.kind.variable() {
            0x10
        } else {
            0
        })
        | case.kind.map();
    let p1 = (if case.elem == VecElementType::I64 {
        0x80
    } else {
        0
    }) | (((!source_or_destination) & 0x0F) << 3)
        | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (if source_or_destination & 16 == 0 {
            0x08
        } else {
            0
        })
        | case.mask();
    let reg = if case.kind.variable() {
        case.destination & 7
    } else {
        case.kind.immediate_group()
    };
    let mut bytes = vec![0x62, p0, p1, p2, case.kind.opcode(), (reg << 3) | 4, 0x24];
    if !case.kind.variable() {
        bytes.push(case.amount);
    }
    bytes
}

fn register_encoding(case: RotateMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let source_or_destination = case.source.unwrap_or(case.destination);
    let mut p0 = case.kind.map() | 0x40;
    if scratch & 8 == 0 {
        p0 |= 0x20;
    }
    if case.kind.variable() {
        if case.destination & 8 == 0 {
            p0 |= 0x80;
        }
        if case.destination & 16 == 0 {
            p0 |= 0x10;
        }
    } else {
        p0 |= 0x90;
    }
    let p1 = (if case.elem == VecElementType::I64 {
        0x80
    } else {
        0
    }) | (((!source_or_destination) & 0x0F) << 3)
        | 0x05;
    let p2 = (case.ll() << 5)
        | (if source_or_destination & 16 == 0 {
            0x08
        } else {
            0
        });
    let reg = if case.kind.variable() {
        case.destination & 7
    } else {
        case.kind.immediate_group()
    };
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        0xC0 | (reg << 3) | (scratch & 7),
    ];
    if !case.kind.variable() {
        bytes.push(case.amount);
    }
    bytes
}

fn lift_case(case: RotateMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("packed rotate memory provenance"),
    );
    function
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexPackedRotateMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_rotate_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: RotateMemoryCase) -> (Vec<u8>, usize) {
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

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(
        !x86_native_vector_features_supported_excluding(function, &excluded),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed rotate memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed rotate"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<RotateMemoryCase> {
    let mut cases = Vec::new();
    for kind in RotateKind::ALL {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                let operands: &[(u8, Option<u8>)] = if kind.variable() {
                    &[(0, Some(0)), (9, Some(10)), (17, Some(18))]
                } else {
                    &[(0, None), (9, None), (17, None)]
                };
                let amounts: &[u8] = if kind.variable() {
                    &[0]
                } else if elem == VecElementType::I32 {
                    &[0, 31, 0xFF]
                } else {
                    &[0, 63, 0xFF]
                };
                for &(destination, source) in operands {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        for control in MaskControl::ALL {
                            for &amount in amounts {
                                cases.push(RotateMemoryCase {
                                    kind,
                                    elem,
                                    width,
                                    destination,
                                    source,
                                    form,
                                    control,
                                    amount,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn packed_rotate_rewrites_match_six_independent_llvm_23_anchors() {
    let cases = [
        (
            RotateMemoryCase {
                kind: RotateKind::ImmediateRight,
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 2,
                source: None,
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount: 7,
            },
            vec![0x62, 0xF1, 0x6D, 0x08, 0x72, 0xC0, 0x07],
        ),
        (
            RotateMemoryCase {
                kind: RotateKind::ImmediateLeft,
                elem: VecElementType::I64,
                width: VecWidth::V512,
                destination: 20,
                source: None,
                form: SourceForm::Vector,
                control: MaskControl::Zero,
                amount: 63,
            },
            vec![0x62, 0xF1, 0xDD, 0xC1, 0x72, 0x0C, 0x24, 0x3F],
        ),
        (
            RotateMemoryCase {
                kind: RotateKind::VariableRight,
                elem: VecElementType::I32,
                width: VecWidth::V256,
                destination: 23,
                source: Some(22),
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount: 0,
            },
            vec![0x62, 0xE2, 0x4D, 0x20, 0x14, 0xF8],
        ),
        (
            RotateMemoryCase {
                kind: RotateKind::VariableLeft,
                elem: VecElementType::I64,
                width: VecWidth::V512,
                destination: 27,
                source: Some(26),
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                amount: 0,
            },
            vec![0x62, 0x62, 0xAD, 0xD1, 0x15, 0x1C, 0x24],
        ),
        (
            RotateMemoryCase {
                kind: RotateKind::ImmediateRight,
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 17,
                source: None,
                form: SourceForm::Broadcast,
                control: MaskControl::Merge,
                amount: 0xFF,
            },
            vec![0x62, 0xF1, 0x75, 0x11, 0x72, 0x04, 0x24, 0xFF],
        ),
        (
            RotateMemoryCase {
                kind: RotateKind::VariableLeft,
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 31,
                source: Some(16),
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                amount: 0,
            },
            vec![0x62, 0x62, 0x7D, 0x01, 0x15, 0x3C, 0x24],
        ),
    ];
    for (case, llvm) in cases {
        assert_eq!(case.expected_replay(), llvm, "{case:?}");
    }
}

#[test]
fn packed_rotate_memory_classifier_exhausts_1_658_880_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in RotateKind::ALL {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    let sources: &[Option<u8>] = if kind.variable() {
                        // Materialized below to avoid a 32-element static.
                        &[]
                    } else {
                        &[None]
                    };
                    let source_values: Vec<Option<u8>> = if kind.variable() {
                        (0..32u8).map(Some).collect()
                    } else {
                        sources.to_vec()
                    };
                    for source in source_values {
                        for broadcast in [false, true] {
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let canonical = memory_encoding(
                                        kind,
                                        elem,
                                        width,
                                        destination,
                                        source,
                                        mask,
                                        zeroing,
                                        broadcast,
                                        0xA5,
                                        true,
                                    );
                                    let ignored_r_patterns: &[u8] = if kind.variable() {
                                        &[canonical[1] & 0x90]
                                    } else {
                                        &[0x00, 0x10, 0x80, 0x90]
                                    };
                                    for &ignored_r in ignored_r_patterns {
                                        for base_high in [false, true] {
                                            for index_high in [false, true] {
                                                let mut bytes = canonical.clone();
                                                if !kind.variable() {
                                                    bytes[1] = (bytes[1] & !0x90) | ignored_r;
                                                }
                                                bytes[1] |= u8::from(base_high) << 3;
                                                if index_high {
                                                    bytes[2] &= !0x04;
                                                }
                                                let encoding = X86InstructionBytes::new(&bytes)
                                                    .unwrap()
                                                    .evex_packed_rotate_memory_encoding()
                                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                                assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                                assert_eq!(
                                                    encoding.destination, destination,
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(encoding.source, source, "{bytes:02X?}");
                                                assert_eq!(
                                                    encoding.writemask,
                                                    (mask != 0).then_some(mask),
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    encoding.zeroing, zeroing,
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    encoding.left,
                                                    kind.left(),
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    encoding.immediate,
                                                    (!kind.variable()).then_some(0xA5),
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    encoding.needs_avx512vl,
                                                    width != VecWidth::V512,
                                                    "{bytes:02X?}"
                                                );
                                                match encoding.replay {
                                                    X86EvexPackedRotateMemoryReplay::Broadcast {
                                                        ..
                                                    } => assert!(broadcast, "{bytes:02X?}"),
                                                    X86EvexPackedRotateMemoryReplay::MaskedVector {
                                                        ..
                                                    } => assert!(
                                                        !broadcast && mask != 0,
                                                        "{bytes:02X?}"
                                                    ),
                                                    X86EvexPackedRotateMemoryReplay::Vector {
                                                        scratch,
                                                        register_instruction,
                                                    } => {
                                                        assert!(
                                                            !broadcast && mask == 0,
                                                            "{bytes:02X?}"
                                                        );
                                                        assert_ne!(
                                                            scratch, destination,
                                                            "{bytes:02X?}"
                                                        );
                                                        assert!(
                                                            source
                                                                .is_none_or(|source| scratch
                                                                    != source),
                                                            "{bytes:02X?}"
                                                        );
                                                        assert_eq!(
                                                            register_instruction
                                                                .evex_register_packed_rotate_needs_vl(),
                                                            Some(width != VecWidth::V512),
                                                            "{bytes:02X?}"
                                                        );
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
                }
            }
        }
    }
    assert_eq!(accepted, 1_658_880);
}

#[test]
fn packed_rotate_memory_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = RotateMemoryCase {
        kind: RotateKind::VariableRight,
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        source: Some(2),
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        amount: 0,
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
        (4, 0x02), // non-owned opcode
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

    let immediate = RotateMemoryCase {
        kind: RotateKind::ImmediateRight,
        source: None,
        ..case
    }
    .bytes();
    let mut invalid_group = immediate.clone();
    invalid_group[5] = (invalid_group[5] & !0x38) | (2 << 3);
    malformed.push(invalid_group);
    let mut missing_immediate = immediate.clone();
    missing_immediate.pop();
    malformed.push(missing_immediate);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_rotate_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_packed_rotate_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );
}

#[test]
fn all_864_packed_rotate_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 864);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(sequence.encoding.elem, case.elem, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.source, case.source, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.zeroing,
                case.zeroing(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.left,
                case.kind.left(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.immediate,
                (!case.kind.variable()).then_some(case.amount),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.memory_size,
                if case.broadcast() {
                    case.memory_width().bytes()
                } else {
                    case.width.bytes()
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.address_offset,
                match (case.form, case.control) {
                    (SourceForm::Vector, MaskControl::None)
                    | (SourceForm::Broadcast, MaskControl::None) => 0,
                    (SourceForm::Vector, _) => 2,
                    (SourceForm::Broadcast, _) => 5,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );

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
    assert_eq!(lowerings, 864 * LEVELS.len());
}

#[test]
fn reduced_zero_immediate_memory_rotates_retain_exact_provenance() {
    for elem in [VecElementType::I32, VecElementType::I64] {
        let bits = (elem.bytes() * 8) as u8;
        for amount in [0, bits, bits * 2, 0xFF] {
            let case = RotateMemoryCase {
                kind: RotateKind::ImmediateLeft,
                elem,
                width: VecWidth::V256,
                destination: 17,
                source: None,
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount,
            };
            let function = optimize(lift_case(case), OptLevel::O2);
            let identity = amount % bits == 0;
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::VMov { .. })),
                identity,
                "{case:?}"
            );
            let exact = sequence(&function, true).unwrap_or_else(|| panic!("{case:?}"));
            assert_eq!(exact.encoding.immediate, Some(amount), "{case:?}");
            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{case:?}"
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn packed_rotate_memory_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let cases = [
        RotateMemoryCase {
            kind: RotateKind::ImmediateRight,
            elem: VecElementType::I32,
            width: VecWidth::V128,
            destination: 1,
            source: None,
            form: SourceForm::Vector,
            control: MaskControl::None,
            amount: 7,
        },
        RotateMemoryCase {
            kind: RotateKind::ImmediateLeft,
            elem: VecElementType::I64,
            width: VecWidth::V256,
            destination: 17,
            source: None,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            amount: 63,
        },
        RotateMemoryCase {
            kind: RotateKind::VariableRight,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            destination: 9,
            source: Some(10),
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            amount: 0,
        },
        RotateMemoryCase {
            kind: RotateKind::VariableLeft,
            elem: VecElementType::I64,
            width: VecWidth::V128,
            destination: 17,
            source: Some(18),
            form: SourceForm::Broadcast,
            control: MaskControl::None,
            amount: 0,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(
            sequence(&function, false).is_none(),
            "{case:?}: memory-disabled admission"
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let wrong_kind = match case.kind {
            RotateKind::ImmediateRight => RotateKind::ImmediateLeft,
            RotateKind::ImmediateLeft => RotateKind::ImmediateRight,
            RotateKind::VariableRight => RotateKind::VariableLeft,
            RotateKind::VariableLeft => RotateKind::VariableRight,
        };
        let wrong_case = RotateMemoryCase {
            kind: wrong_kind,
            ..case
        };
        wrong_provenance.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&wrong_case.bytes()).unwrap(),
        );
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut wrong_direction = function.clone();
        let rotate = wrong_direction.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86PackedRotate { .. }));
        if let Some(rotate) = rotate {
            let OpKind::X86PackedRotate { left, .. } = &mut rotate.kind else {
                unreachable!()
            };
            *left = !*left;
            assert_rejected("wrong direction", &wrong_direction);
        }

        let mut wrong_address = function.clone();
        let address_index = sequence(&wrong_address, true).unwrap().address_offset;
        match &mut wrong_address.blocks[0].ops[address_index].kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::VLoad { addr, .. }
            | OpKind::Lea { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!(),
        }
        assert_rejected("virtual address", &wrong_address);

        let mut hinted_memory = function.clone();
        let address_index = sequence(&hinted_memory, true).unwrap().address_offset;
        hinted_memory.blocks[0].ops[address_index].x86_hint =
            Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted memory", &hinted_memory);

        let mut tail = function.clone();
        tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: crate::smir::ir::types::SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &tail);
    }
}

#[test]
fn packed_rotate_apx_r16_r17_addresses_admit_and_lower_at_every_level() {
    for (kind, bytes, expected_address) in [
        (
            RotateKind::ImmediateRight,
            vec![0x62, 0xF9, 0x69, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07],
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                scale: 4,
                disp: 16,
                disp_size: DispSize::Disp8,
            },
        ),
        (
            RotateKind::VariableRight,
            vec![
                0x62, 0xEA, 0x49, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
            ],
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R20))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R21)),
                scale: 8,
                disp: 48,
                disp_size: DispSize::Disp32,
            },
        ),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let result = lifter
            .lift_insn(PC, &bytes, &mut context)
            .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut base = SmirFunction::new(FunctionId(0), block.id, PC);
        base.add_block(block);
        base.x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {kind:?} {bytes:02X?}"));
            assert_eq!(exact.address_offset, 0, "{level:?} {bytes:02X?}");

            let case = RotateMemoryCase {
                kind,
                elem: if bytes[2] & 0x80 != 0 {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                width: match (bytes[3] >> 5) & 3 {
                    0 => VecWidth::V128,
                    1 => VecWidth::V256,
                    2 => VecWidth::V512,
                    _ => unreachable!(),
                },
                destination: exact.encoding.destination,
                source: exact.encoding.source,
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount: exact.encoding.immediate.unwrap_or(0),
            };
            let (code, _) = lower(&function, case);
            let replay = match exact.encoding.replay {
                X86EvexPackedRotateMemoryReplay::Vector {
                    register_instruction,
                    ..
                } => register_instruction,
                _ => unreachable!("unmasked APX vector replay"),
            };
            assert!(
                code.windows(replay.as_slice().len())
                    .any(|window| window == replay.as_slice()),
                "{level:?} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn masked_vector_lowering_has_one_live_opmask_guard_per_lane_and_rejects_avx_only_bridge() {
    for elem in [VecElementType::I32, VecElementType::I64] {
        let case = RotateMemoryCase {
            kind: RotateKind::VariableLeft,
            elem,
            width: VecWidth::V512,
            destination: 17,
            source: Some(18),
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            amount: 0,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        for lane in 0..case.width.lanes(case.elem) {
            let lane_mask = (1u32 << lane).to_le_bytes();
            let guard = [
                0x9C,
                0x50,
                0xC4,
                0xE1,
                0xFB,
                0x93,
                0xC0 | case.mask(),
                0xF7,
                0xC0,
                lane_mask[0],
                lane_mask[1],
                lane_mask[2],
                lane_mask[3],
                0x0F,
                0x84,
            ];
            assert!(
                code.windows(guard.len()).any(|window| window == guard),
                "{elem:?} lane {lane}: {guard:02X?}"
            );
        }

        let mut avx_only = X86_64Lowerer::new();
        avx_only.set_mem_helpers(true);
        avx_only.set_preserve_vector_mem_helpers(true);
        avx_only.set_avx_ymm16_vector_state(true);
        let error = avx_only
            .lower_function(&function)
            .expect_err("AVX-only state bridge must reject AVX-512 replay");
        assert!(
            format!("{error:?}").contains("AVX-only vector bridge"),
            "{error:?}"
        );
    }
}
