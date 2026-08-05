//! Exact helper-backed EVEX packed integer absolute-value memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedAbsMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_abs_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7B20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

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
pub(super) struct PackedAbsMemoryCase {
    pub(super) elem: VecElementType,
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) form: SourceForm,
    pub(super) control: MaskControl,
    pub(super) w: bool,
}

impl PackedAbsMemoryCase {
    const fn opcode(self) -> u8 {
        match self.elem {
            VecElementType::I8 => 0x1C,
            VecElementType::I16 => 0x1D,
            VecElementType::I32 => 0x1E,
            VecElementType::I64 => 0x1F,
            _ => unreachable!(),
        }
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
        match self.elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
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
        _ => unreachable!("packed-abs vector width"),
    }))
}

fn memory_encoding(case: PackedAbsMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && (!case.zeroing() || case.mask() != 0));
    assert!(!case.broadcast() || matches!(case.elem, VecElementType::I32 | VecElementType::I64));
    assert!(
        matches!(case.elem, VecElementType::I8 | VecElementType::I16)
            || (case.elem == VecElementType::I32 && !case.w)
            || (case.elem == VecElementType::I64 && case.w)
    );
    let mut p0 = 0x62;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(case.w) << 7) | 0x7D;
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
        case.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: PackedAbsMemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.w) << 7) | 0x7D,
        (u8::from(case.zeroing()) << 7)
            | (case.ll() << 5)
            | (u8::from(case.broadcast()) << 4)
            | 0x08
            | case.mask(),
        case.opcode(),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
}

fn register_encoding(case: PackedAbsMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16 && scratch != case.destination);
    let p0 = 0x42
        | if scratch & 8 == 0 { 0x20 } else { 0 }
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.w) << 7) | 0x7D,
        (case.ll() << 5) | 0x08,
        case.opcode(),
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
        X86InstructionBytes::new(bytes).expect("packed-abs memory provenance"),
    );
    function
}

pub(super) fn lift_case(case: PackedAbsMemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexPackedAbsMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_abs_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

pub(super) fn lower(function: &SmirFunction, case: PackedAbsMemoryCase) -> (Vec<u8>, usize) {
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

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: packed-abs memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed abs"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<PackedAbsMemoryCase> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::I8,
        VecElementType::I16,
        VecElementType::I32,
        VecElementType::I64,
    ] {
        let w_values: &[bool] = match elem {
            VecElementType::I8 | VecElementType::I16 => &[false, true],
            VecElementType::I32 => &[false],
            VecElementType::I64 => &[true],
            _ => unreachable!(),
        };
        let forms: &[SourceForm] = match elem {
            VecElementType::I8 | VecElementType::I16 => &[SourceForm::Vector],
            VecElementType::I32 | VecElementType::I64 => {
                &[SourceForm::Vector, SourceForm::Broadcast]
            }
            _ => unreachable!(),
        };
        for &w in w_values {
            for (width_index, width) in [VecWidth::V128, VecWidth::V256, VecWidth::V512]
                .into_iter()
                .enumerate()
            {
                for &form in forms {
                    for control in MaskControl::ALL {
                        cases.push(PackedAbsMemoryCase {
                            elem,
                            width,
                            destination: [0, 9, 17][width_index],
                            form,
                            control,
                            w,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn packed_abs_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xE2, 0x7D, 0x08, 0x1C, 0x0A],
            &[0x62, 0xE2, 0x7D, 0x08, 0x1C, 0xC8],
        ),
        (
            &[0x62, 0x72, 0x7D, 0x2B, 0x1D, 0x0C, 0x24],
            &[0x62, 0x72, 0x7D, 0x2B, 0x1D, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x7D, 0x99, 0x1E, 0x0C, 0x24],
            &[0x62, 0xE2, 0x7D, 0x99, 0x1E, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0x1E, 0x12],
            &[0x62, 0xF2, 0x7D, 0x48, 0x1E, 0xD0],
        ),
        (
            &[0x62, 0x62, 0xFD, 0x3C, 0x1F, 0x0C, 0x24],
            &[0x62, 0x62, 0xFD, 0x3C, 0x1F, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0xFD, 0xCF, 0x1F, 0x3C, 0x24],
            &[0x62, 0x62, 0xFD, 0xCF, 0x1F, 0x3C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_packed_abs_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexIntegerArithmeticMemoryReplay::Broadcast { stack_instruction }
            | X86EvexIntegerArithmeticMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn packed_abs_classifier_exhausts_46_080_operand_control_wig_and_apx_cells() {
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
                    let case = PackedAbsMemoryCase {
                        destination,
                        control: if mask == 0 {
                            MaskControl::None
                        } else if zeroing {
                            MaskControl::Zero
                        } else {
                            MaskControl::Merge
                        },
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
                                .evex_packed_abs_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.width, case.width, "{bytes:02X?}");
                            assert_eq!(encoding.elem, case.elem, "{bytes:02X?}");
                            assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                            assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                            assert_eq!(encoding.opcode, case.opcode(), "{bytes:02X?}");
                            assert_eq!(encoding.w, case.w, "{bytes:02X?}");
                            assert_eq!(
                                encoding.needs_avx512vl,
                                case.width != VecWidth::V512,
                                "{bytes:02X?}"
                            );
                            match encoding.replay {
                                X86EvexIntegerArithmeticMemoryReplay::Vector {
                                    scratch,
                                    register_instruction,
                                } => {
                                    assert_eq!(mask, 0, "{bytes:02X?}");
                                    assert_eq!(case.form, SourceForm::Vector);
                                    assert_ne!(scratch, destination, "{bytes:02X?}");
                                    assert_eq!(
                                        register_instruction.evex_register_packed_abs_needs_vl(),
                                        Some(case.width != VecWidth::V512),
                                        "{bytes:02X?}"
                                    );
                                }
                                X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. } => {
                                    assert_eq!(case.form, SourceForm::Broadcast);
                                }
                                X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => {
                                    assert_ne!(mask, 0, "{bytes:02X?}");
                                    assert_eq!(case.form, SourceForm::Vector);
                                }
                            }
                            accepted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 46_080);
}

#[test]
fn packed_abs_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = PackedAbsMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        w: false,
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
    for elem in [VecElementType::I8, VecElementType::I16] {
        let mut bytes = PackedAbsMemoryCase { elem, ..case }.bytes();
        bytes[3] |= 0x10;
        malformed.push(bytes);
    }
    let mut wrong_dword_w = case.bytes();
    wrong_dword_w[2] |= 0x80;
    malformed.push(wrong_dword_w);
    let mut wrong_qword_w = PackedAbsMemoryCase {
        elem: VecElementType::I64,
        w: true,
        ..case
    }
    .bytes();
    wrong_qword_w[2] &= !0x80;
    malformed.push(wrong_qword_w);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_abs_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_packed_abs_memory_encoding()
            .is_some()
    );
    for elem in [VecElementType::I8, VecElementType::I16] {
        for w in [false, true] {
            let bytes = PackedAbsMemoryCase { elem, w, ..case }.bytes();
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_packed_abs_memory_encoding()
                    .is_some(),
                "WIG form rejected W={w}: {bytes:02X?}"
            );
        }
    }
}

#[test]
fn all_72_packed_abs_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 72);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.elem);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.encoding.opcode, case.opcode());
            assert_eq!(exact.encoding.w, case.w);
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
    assert_eq!(lowerings, 72 * LEVELS.len());
}

#[test]
fn type_e4_packed_abs_graphs_preserve_exact_access_granularity() {
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
            let lanes = case.width.lanes(case.elem) as usize;
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
fn packed_abs_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = PackedAbsMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
        control: MaskControl::None,
        w: false,
    };
    let broadcast_case = PackedAbsMemoryCase {
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

    let apx_case = PackedAbsMemoryCase {
        elem: VecElementType::I16,
        width: VecWidth::V512,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        w: true,
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
        assert!(
            matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ),
            "{level:?} {apx:02X?}: APX address lost its dynamic guard"
        );
        assert!(
            function.blocks[0].ops.iter().any(|op| match &op.kind {
                OpKind::Lea { addr, .. } => addr == &expected_address,
                _ => false,
            }),
            "{level:?} {apx:02X?}: {:#?}",
            function.blocks[0].ops
        );
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {apx:02X?}"));
        lower(&function, apx_case);
    }
}

#[test]
fn packed_abs_rejects_the_avx_only_state_bridge() {
    let case = PackedAbsMemoryCase {
        elem: VecElementType::I8,
        width: VecWidth::V512,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
        w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX packed abs");
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

#[test]
fn packed_abs_sequence_fails_closed_for_provenance_graph_and_frontier_mutations() {
    for case in [
        PackedAbsMemoryCase {
            elem: VecElementType::I8,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
        PackedAbsMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            w: true,
        },
    ] {
        for level in LEVELS {
            let canonical = optimize(lift_case(case), level);
            assert!(sequence(&canonical, true).is_some());

            let mut provenance = canonical.clone();
            provenance.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                X86InstructionBytes::new(&[0x62, 0xF2, 0x7D, 0x08, 0x1D, 0x03]).unwrap(),
            );
            assert_rejected("mismatched provenance", &provenance);

            let mut hint = canonical.clone();
            let unary = hint.blocks[0]
                .ops
                .iter_mut()
                .find(|op| matches!(op.kind, OpKind::VUnary { .. }))
                .unwrap();
            unary.x86_hint = None;
            assert_rejected("missing exact unary hint", &hint);

            let mut semantic = canonical.clone();
            let unary = semantic.blocks[0]
                .ops
                .iter_mut()
                .find(|op| matches!(op.kind, OpKind::VUnary { .. }))
                .unwrap();
            if let OpKind::VUnary { ref mut src, .. } = unary.kind {
                *src = vector(case.destination, case.width);
            }
            assert_rejected("wrong memory consumer", &semantic);

            let mut frontier = canonical.clone();
            frontier.blocks[0].ops.push(SmirOp::new(
                OpId(u16::MAX - 1),
                PC,
                OpKind::Mov {
                    dst: VReg::Virtual(VirtualId(u32::MAX - 1)),
                    src: crate::smir::ir::types::SrcOperand::Imm(0),
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
