//! Exact register and helper-backed memory coverage for AVX-512 VBMI2 packed
//! funnel shifts.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedFunnelShiftMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedFunnelShiftMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_funnel_shift_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7B40;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FunnelKind {
    ImmediateRight,
    ImmediateLeft,
    VariableRight,
    VariableLeft,
}

impl FunnelKind {
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
        if self.variable() { 2 } else { 3 }
    }

    const fn opcode(self, elem: VecElementType) -> u8 {
        match (self.left(), elem) {
            (true, VecElementType::I16) => 0x70,
            (true, VecElementType::I32 | VecElementType::I64) => 0x71,
            (false, VecElementType::I16) => 0x72,
            (false, VecElementType::I32 | VecElementType::I64) => 0x73,
            _ => unreachable!(),
        }
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
struct FunnelMemoryCase {
    kind: FunnelKind,
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source: u8,
    form: SourceForm,
    control: MaskControl,
    amount: u8,
}

impl FunnelMemoryCase {
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
            VecElementType::I16 => MemWidth::B2,
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
            .find(|candidate| *candidate != self.destination && *candidate != self.source)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self)
        } else {
            register_rewrite(self, self.scratch())
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("packed funnel-shift vector width"),
    }))
}

const fn element_w(elem: VecElementType) -> bool {
    matches!(elem, VecElementType::I16 | VecElementType::I64)
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    kind: FunnelKind,
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
    amount: u8,
    sib: bool,
) -> Vec<u8> {
    assert!(destination < 32 && source < 32 && mask < 8 && (!zeroing || mask != 0));
    assert!(!broadcast || elem != VecElementType::I16);
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!("packed funnel-shift width"),
    };
    let mut p0 = kind.map() | 0x60;
    if destination & 8 == 0 {
        p0 |= 0x80;
    }
    if destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(element_w(elem)) << 7) | (((!source) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (ll << 5)
        | (u8::from(broadcast) << 4)
        | (u8::from(source < 16) << 3)
        | mask;
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode(elem),
        ((destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently.
        bytes.push(0x48);
    }
    if !kind.variable() {
        bytes.push(amount);
    }
    bytes
}

fn stack_encoding(case: FunnelMemoryCase) -> Vec<u8> {
    let p0 = (u8::from(case.destination & 8 == 0) << 7)
        | 0x60
        | (u8::from(case.destination & 16 == 0) << 4)
        | case.kind.map();
    let p1 = (u8::from(element_w(case.elem)) << 7) | (((!case.source) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (u8::from(case.source < 16) << 3)
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(case.elem),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ];
    if !case.kind.variable() {
        bytes.push(case.amount);
    }
    bytes
}

fn register_rewrite(case: FunnelMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = (u8::from(case.destination & 8 == 0) << 7)
        | 0x40
        | (u8::from(scratch & 8 == 0) << 5)
        | (u8::from(case.destination & 16 == 0) << 4)
        | case.kind.map();
    let p1 = (u8::from(element_w(case.elem)) << 7) | (((!case.source) & 0x0F) << 3) | 0x05;
    let p2 = (case.ll() << 5) | (u8::from(case.source < 16) << 3);
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(case.elem),
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
    ];
    if !case.kind.variable() {
        bytes.push(case.amount);
    }
    bytes
}

#[allow(clippy::too_many_arguments)]
fn register_encoding(
    kind: FunnelKind,
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source: u8,
    rm: u8,
    mask: u8,
    zeroing: bool,
    amount: u8,
) -> Vec<u8> {
    let mut p0 = kind.map();
    if destination & 8 == 0 {
        p0 |= 0x80;
    }
    if destination & 16 == 0 {
        p0 |= 0x10;
    }
    if rm & 8 == 0 {
        p0 |= 0x20;
    }
    if rm & 16 == 0 {
        p0 |= 0x40;
    }
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!(),
    };
    let p1 = (u8::from(element_w(elem)) << 7) | (((!source) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(source < 16) << 3) | mask;
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode(elem),
        0xC0 | ((destination & 7) << 3) | (rm & 7),
    ];
    if !kind.variable() {
        bytes.push(amount);
    }
    bytes
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
        X86InstructionBytes::new(bytes).expect("packed funnel-shift provenance"),
    );
    function
}

fn lift_case(case: FunnelMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
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
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_funnel_shift_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: FunnelMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx512vbmi2, "{case:?}");
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
            && std::is_x86_feature_detected!("avx512vbmi2")
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
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed funnel-shift lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed funnel shift"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FunnelMemoryCase> {
    let mut cases = Vec::new();
    for kind in FunnelKind::ALL {
        for elem in [
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source) in [(0, 0), (9, 10), (17, 18)] {
                    let forms: &[SourceForm] = if elem == VecElementType::I16 {
                        &[SourceForm::Vector]
                    } else {
                        &[SourceForm::Vector, SourceForm::Broadcast]
                    };
                    let bits = (elem.bytes() * 8) as u8;
                    let amounts: &[u8] = if kind.variable() {
                        &[0]
                    } else {
                        &[0, bits - 1, 0xFF]
                    };
                    for &form in forms {
                        for control in MaskControl::ALL {
                            for &amount in amounts {
                                cases.push(FunnelMemoryCase {
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
fn register_classifier_exhausts_1_658_880_operand_control_cells() {
    let mut accepted = 0usize;
    for kind in FunnelKind::ALL {
        for elem in [
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source in 0..32u8 {
                        for rm in [0, 9, 17] {
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let bytes = register_encoding(
                                        kind,
                                        elem,
                                        width,
                                        destination,
                                        source,
                                        rm,
                                        mask,
                                        zeroing,
                                        0xA5,
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_packed_funnel_shift_needs_vl(),
                                        Some(width != VecWidth::V512),
                                        "{bytes:02X?}"
                                    );
                                    accepted += 1;
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
fn memory_classifier_exhausts_3_686_400_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in FunnelKind::ALL {
        for elem in [
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source in 0..32u8 {
                        let forms: &[bool] = if elem == VecElementType::I16 {
                            &[false]
                        } else {
                            &[false, true]
                        };
                        for &broadcast in forms {
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
                                    for base_high in [false, true] {
                                        for index_high in [false, true] {
                                            let mut bytes = canonical.clone();
                                            bytes[1] |= u8::from(base_high) << 3;
                                            if index_high {
                                                bytes[2] &= !0x04;
                                            }
                                            let encoding = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_packed_funnel_shift_memory_encoding()
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
                                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                            assert_eq!(encoding.left, kind.left(), "{bytes:02X?}");
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
                                                X86EvexPackedFunnelShiftMemoryReplay::Broadcast {
                                                    ..
                                                } => assert!(broadcast, "{bytes:02X?}"),
                                                X86EvexPackedFunnelShiftMemoryReplay::MaskedVector {
                                                    ..
                                                } => assert!(
                                                    !broadcast && mask != 0,
                                                    "{bytes:02X?}"
                                                ),
                                                X86EvexPackedFunnelShiftMemoryReplay::Vector {
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
                                                    assert_ne!(scratch, source, "{bytes:02X?}");
                                                    assert_eq!(
                                                        register_instruction
                                                            .evex_register_packed_funnel_shift_needs_vl(),
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
    assert_eq!(accepted, 3_686_400);
}

#[test]
fn rewrites_match_twelve_independent_llvm_23_anchors() {
    // Source and replay bytes were emitted independently by LLVM 23
    // `llvm-mc -triple=x86_64 --x86-asm-syntax=intel --show-encoding`.
    let anchors: &[(&str, &[u8], &[u8])] = &[
        (
            "VPSHLDW",
            &[0x62, 0xE3, 0xED, 0x40, 0x70, 0x08, 0x0F],
            &[0x62, 0xE3, 0xED, 0x40, 0x70, 0xC8, 0x0F],
        ),
        (
            "VPSHLDD",
            &[0x62, 0x73, 0x2D, 0xBB, 0x71, 0x08, 0x1F],
            &[0x62, 0x73, 0x2D, 0xBB, 0x71, 0x0C, 0x24, 0x1F],
        ),
        (
            "VPSHLDQ",
            &[0x62, 0xF3, 0xED, 0x0A, 0x71, 0x08, 0x3F],
            &[0x62, 0xF3, 0xED, 0x0A, 0x71, 0x0C, 0x24, 0x3F],
        ),
        (
            "VPSHLDVW",
            &[0x62, 0xE2, 0xED, 0x20, 0x70, 0x08],
            &[0x62, 0xE2, 0xED, 0x20, 0x70, 0xC8],
        ),
        (
            "VPSHLDVD",
            &[0x62, 0x72, 0x2D, 0x59, 0x71, 0x08],
            &[0x62, 0x72, 0x2D, 0x59, 0x71, 0x0C, 0x24],
        ),
        (
            "VPSHLDVQ",
            &[0x62, 0xE2, 0xED, 0x87, 0x71, 0x08],
            &[0x62, 0xE2, 0xED, 0x87, 0x71, 0x0C, 0x24],
        ),
        (
            "VPSHRDW",
            &[0x62, 0xE3, 0xED, 0x40, 0x72, 0x08, 0x0F],
            &[0x62, 0xE3, 0xED, 0x40, 0x72, 0xC8, 0x0F],
        ),
        (
            "VPSHRDD",
            &[0x62, 0x73, 0x2D, 0xBB, 0x73, 0x08, 0x1F],
            &[0x62, 0x73, 0x2D, 0xBB, 0x73, 0x0C, 0x24, 0x1F],
        ),
        (
            "VPSHRDQ",
            &[0x62, 0xF3, 0xED, 0x0A, 0x73, 0x08, 0x3F],
            &[0x62, 0xF3, 0xED, 0x0A, 0x73, 0x0C, 0x24, 0x3F],
        ),
        (
            "VPSHRDVW",
            &[0x62, 0xE2, 0xED, 0x20, 0x72, 0x08],
            &[0x62, 0xE2, 0xED, 0x20, 0x72, 0xC8],
        ),
        (
            "VPSHRDVD",
            &[0x62, 0x72, 0x2D, 0x59, 0x73, 0x08],
            &[0x62, 0x72, 0x2D, 0x59, 0x73, 0x0C, 0x24],
        ),
        (
            "VPSHRDVQ",
            &[0x62, 0xE2, 0xED, 0x87, 0x73, 0x08],
            &[0x62, 0xE2, 0xED, 0x87, 0x73, 0x0C, 0x24],
        ),
    ];

    for &(name, source, expected) in anchors {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_packed_funnel_shift_memory_encoding()
            .unwrap_or_else(|| panic!("{name}: {source:02X?}"));
        let actual = match encoding.replay {
            X86EvexPackedFunnelShiftMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexPackedFunnelShiftMemoryReplay::Broadcast { stack_instruction }
            | X86EvexPackedFunnelShiftMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(actual.as_slice(), expected, "{name}");
        assert_eq!(
            lift_bytes(source).blocks[0].ops.last().unwrap().guest_pc,
            PC,
            "{name}"
        );
    }
}

#[test]
fn classifiers_reject_reserved_nonowned_and_trailing_shapes() {
    let case = FunnelMemoryCase {
        kind: FunnelKind::VariableRight,
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        source: 2,
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
        (4, 0x04), // non-owned opcode
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
    let mut word_broadcast = memory_encoding(
        FunnelKind::ImmediateLeft,
        VecElementType::I16,
        VecWidth::V128,
        1,
        2,
        0,
        false,
        false,
        7,
        false,
    );
    word_broadcast[3] |= 0x10;
    malformed.push(word_broadcast);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_funnel_shift_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_packed_funnel_shift_memory_encoding()
            .is_some()
    );

    let register = register_encoding(
        FunnelKind::ImmediateLeft,
        VecElementType::I32,
        VecWidth::V128,
        1,
        2,
        3,
        0,
        false,
        7,
    );
    for (index, bit) in [(1, 0x08), (2, 0x04), (3, 0x10), (3, 0x60)] {
        let mut invalid = register.clone();
        invalid[index] ^= bit;
        assert!(
            X86InstructionBytes::new(&invalid)
                .unwrap()
                .evex_register_packed_funnel_shift_needs_vl()
                .is_none(),
            "{invalid:02X?}"
        );
    }
}

#[test]
fn all_1080_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 1080);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(exact.encoding.elem, case.elem, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.source, case.source, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.encoding.immediate,
                (!case.kind.variable()).then_some(case.amount),
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    case.memory_width().bytes()
                } else {
                    case.width.bytes()
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.address_offset,
                match (case.form, case.control) {
                    (SourceForm::Vector, MaskControl::None)
                    | (SourceForm::Broadcast, MaskControl::None) => 0,
                    (SourceForm::Vector, _) => 2,
                    (SourceForm::Broadcast, _) => 5,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.consumed,
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
    assert_eq!(lowerings, 1080 * LEVELS.len());
}

#[test]
fn zero_reduced_immediates_retain_semantics_and_provenance_at_o2() {
    for elem in [
        VecElementType::I16,
        VecElementType::I32,
        VecElementType::I64,
    ] {
        let bits = (elem.bytes() * 8) as u8;
        for amount in [0, bits, bits * 2, 0xFF] {
            let case = FunnelMemoryCase {
                kind: FunnelKind::ImmediateLeft,
                elem,
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount,
            };
            let function = optimize(lift_case(case), OptLevel::O2);
            assert!(
                function.blocks[0].ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::X86PackedFunnelShift {
                        amount: actual,
                        ..
                    } if actual == amount
                )),
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

            let register_bytes = register_encoding(
                case.kind,
                case.elem,
                case.width,
                case.destination,
                case.source,
                3,
                0,
                false,
                amount,
            );
            let register_function = optimize(lift_bytes(&register_bytes), OptLevel::O2);
            assert!(is_native_clobber_safe_excluding(
                &register_function,
                &HashMap::new(),
                true
            ));
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_native_vector_state_active(true);
            lowerer
                .lower_function(&register_function)
                .unwrap_or_else(|error| panic!("{register_bytes:02X?}: {error:?}"));
            let code = lowerer.finalize().unwrap();
            assert!(
                code.windows(register_bytes.len())
                    .any(|window| window == register_bytes),
                "{register_bytes:02X?}"
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
fn sequence_fails_closed_for_provenance_and_graph_mutations() {
    let cases = [
        FunnelMemoryCase {
            kind: FunnelKind::ImmediateRight,
            elem: VecElementType::I16,
            width: VecWidth::V128,
            destination: 1,
            source: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
            amount: 7,
        },
        FunnelMemoryCase {
            kind: FunnelKind::ImmediateLeft,
            elem: VecElementType::I64,
            width: VecWidth::V256,
            destination: 17,
            source: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            amount: 63,
        },
        FunnelMemoryCase {
            kind: FunnelKind::VariableRight,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            destination: 9,
            source: 10,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            amount: 0,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function, false).is_none(), "{case:?}");

        let mut missing = function.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing);

        let mut wrong = function.clone();
        let wrong_case = FunnelMemoryCase {
            kind: if case.kind.left() {
                FunnelKind::VariableRight
            } else {
                FunnelKind::VariableLeft
            },
            ..case
        };
        let wrong_bytes = if wrong_case.kind.variable() {
            wrong_case.bytes()
        } else {
            FunnelMemoryCase {
                kind: FunnelKind::ImmediateLeft,
                ..wrong_case
            }
            .bytes()
        };
        wrong.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&wrong_bytes).unwrap(),
        );
        assert_rejected("wrong provenance", &wrong);

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

        let mut hinted = function.clone();
        let address_index = sequence(&hinted, true).unwrap().address_offset;
        hinted.blocks[0].ops[address_index].x86_hint =
            Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted memory", &hinted);

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
fn apx_r16_r17_sib_addresses_admit_and_lower_at_every_level() {
    for kind in FunnelKind::ALL {
        let mut bytes = memory_encoding(
            kind,
            VecElementType::I32,
            VecWidth::V512,
            9,
            10,
            0,
            false,
            false,
            7,
            true,
        );
        bytes[1] |= 0x08; // EVEX.B4 promotes SIB.base=0 to R16.
        bytes[2] &= !0x04; // !EVEX.U promotes SIB.index=1 to R17.
        let expected = Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
            index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
            scale: 2,
            disp: 0,
            disp_size: DispSize::Auto,
        };
        let base = lift_bytes(&bytes);
        let mut missing_guard = base.clone();
        assert!(matches!(
            missing_guard.blocks[0].ops.remove(0).kind,
            OpKind::X86RequireApx
        ));
        assert_rejected("APX address without guard", &missing_guard);

        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                matches!(
                    function.blocks[0].ops.first().map(|op| &op.kind),
                    Some(OpKind::X86RequireApx)
                ),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                    .count(),
                1,
                "{level:?} {bytes:02X?}"
            );
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } => addr == &expected,
                    _ => false,
                }),
                "{level:?} {kind:?}: {:#?}",
                function.blocks[0].ops
            );
            let exact = sequence(&function, true).unwrap();
            let case = FunnelMemoryCase {
                kind,
                elem: VecElementType::I32,
                width: VecWidth::V512,
                destination: 9,
                source: 10,
                form: SourceForm::Vector,
                control: MaskControl::None,
                amount: 7,
            };
            let (code, _) = lower(&function, case);
            let replay = match exact.encoding.replay {
                X86EvexPackedFunnelShiftMemoryReplay::Vector {
                    register_instruction,
                    ..
                } => register_instruction,
                _ => unreachable!(),
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
fn packed_funnel_shift_memory_sequence_rejects_spurious_apx_guard() {
    let mut function = lift_case(FunnelMemoryCase {
        kind: FunnelKind::ImmediateRight,
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        source: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
        amount: 7,
    });
    function.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xFFFE), PC, OpKind::X86RequireApx));
    assert_rejected("low address with APX guard", &function);
}

#[test]
fn masked_broadcasts_have_one_aggregate_load_and_i16_vectors_have_32_lane_guards() {
    for kind in FunnelKind::ALL {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                let case = FunnelMemoryCase {
                    kind,
                    elem,
                    width,
                    destination: 17,
                    source: 18,
                    form: SourceForm::Broadcast,
                    control: MaskControl::Zero,
                    amount: 0xFF,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    assert_eq!(
                        function.blocks[0]
                            .ops
                            .iter()
                            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                            .count(),
                        1,
                        "{level:?} {case:?}"
                    );
                }
            }
        }
    }

    let case = FunnelMemoryCase {
        kind: FunnelKind::VariableLeft,
        elem: VecElementType::I16,
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
        amount: 0,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let (code, _) = lower(&function, case);
    for lane in 0..32 {
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
            "I16 lane {lane}: {guard:02X?}"
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
