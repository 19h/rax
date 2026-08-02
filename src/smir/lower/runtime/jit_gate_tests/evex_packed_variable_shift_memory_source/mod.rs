//! Exact helper-backed EVEX per-element variable-shift memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, ShiftOp, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedVariableShiftMemoryReplay,
    X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedVariableShiftMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_variable_shift_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xCD00;
const DISP8: i32 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftKind {
    opcode: u8,
    elem: VecElementType,
    shift: ShiftOp,
}

impl ShiftKind {
    const ALL: [Self; 9] = [
        Self {
            opcode: 0x10,
            elem: VecElementType::I16,
            shift: ShiftOp::Lsr,
        },
        Self {
            opcode: 0x11,
            elem: VecElementType::I16,
            shift: ShiftOp::Asr,
        },
        Self {
            opcode: 0x12,
            elem: VecElementType::I16,
            shift: ShiftOp::Lsl,
        },
        Self {
            opcode: 0x45,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsr,
        },
        Self {
            opcode: 0x45,
            elem: VecElementType::I64,
            shift: ShiftOp::Lsr,
        },
        Self {
            opcode: 0x46,
            elem: VecElementType::I32,
            shift: ShiftOp::Asr,
        },
        Self {
            opcode: 0x46,
            elem: VecElementType::I64,
            shift: ShiftOp::Asr,
        },
        Self {
            opcode: 0x47,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsl,
        },
        Self {
            opcode: 0x47,
            elem: VecElementType::I64,
            shift: ShiftOp::Lsl,
        },
    ];

    const fn w(self) -> bool {
        !matches!(self.elem, VecElementType::I32)
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

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Merge | Self::Zero => 3,
        }
    }

    const fn zeroing(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftMemoryCase {
    kind: ShiftKind,
    width: VecWidth,
    destination: u8,
    source: u8,
    form: SourceForm,
    control: MaskControl,
}

impl ShiftMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn mask(self) -> u8 {
        self.control.mask()
    }

    const fn zeroing(self) -> bool {
        self.control.zeroing()
    }

    const fn compressed_displacement(self) -> i32 {
        DISP8
            * if self.broadcast() {
                self.kind.elem.bytes() as i32
            } else {
                self.width.bytes() as i32
            }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(
            self.kind,
            self.width,
            self.destination,
            self.source,
            self.mask(),
            self.zeroing(),
            self.broadcast(),
            false,
            false,
        )
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source)
            .expect("two operands leave at least fourteen low scratch registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    kind: ShiftKind,
    width: VecWidth,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    assert!(destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    assert!(!broadcast || kind.elem != VecElementType::I16);
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!("EVEX variable-shift width"),
    };
    let mut p0 = 0x62;
    if destination & 8 == 0 {
        p0 |= 0x80;
    }
    if destination & 16 == 0 {
        p0 |= 0x10;
    }
    if apx_base {
        p0 |= 0x08;
    }
    let mut p1 = (u8::from(kind.w()) << 7) | (((!source) & 0x0F) << 3) | 0x05;
    if apx_index {
        p1 &= !0x04;
    }
    let p2 = (u8::from(zeroing) << 7)
        | (ll << 5)
        | (u8::from(broadcast) << 4)
        | (if source & 16 == 0 { 0x08 } else { 0 })
        | mask;
    vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode,
        0x40 | ((destination & 7) << 3) | 0x04,
        0x48, // [RAX + RCX*2 + disp8]
        DISP8 as u8,
    ]
}

fn stack_encoding(case: ShiftMemoryCase) -> Vec<u8> {
    let mut p0 = 0x62;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(case.kind.w()) << 7) | (((!case.source) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (if case.source & 16 == 0 { 0x08 } else { 0 })
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        ((case.destination & 7) << 3) | 0x04,
        0x24,
    ]
}

fn register_encoding(case: ShiftMemoryCase, scratch: u8) -> Vec<u8> {
    let mut p0 = 0x42;
    if scratch & 8 == 0 {
        p0 |= 0x20;
    }
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(case.kind.w()) << 7) | (((!case.source) & 0x0F) << 3) | 0x05;
    let p2 = (case.ll() << 5) | (if case.source & 16 == 0 { 0x08 } else { 0 });
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
    ]
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("EVEX variable shift fits metadata"),
    );
    function
}

fn lift_case(case: ShiftMemoryCase) -> SmirFunction {
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

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexPackedVariableShiftMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexPackedVariableShiftMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_variable_shift_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: ShiftMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: variable-shift memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed variable shift"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ShiftMemoryCase> {
    let mut cases = Vec::new();
    for kind in ShiftKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (destination, source) in [(0, 0), (9, 10), (17, 18)] {
                let forms: &[SourceForm] = if kind.elem == VecElementType::I16 {
                    &[SourceForm::Vector]
                } else {
                    &[SourceForm::Vector, SourceForm::Broadcast]
                };
                for &form in forms {
                    for control in MaskControl::ALL {
                        cases.push(ShiftMemoryCase {
                            kind,
                            width,
                            destination,
                            source,
                            form,
                            control,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn llvm_23_replay_byte_anchors_cover_all_nine_variable_shift_mnemonics() {
    // Independent encodings from LLVM 23.0.0git:
    // llvm-mc -triple=x86_64 -x86-asm-syntax=intel -show-encoding
    let anchors: [(ShiftMemoryCase, &[u8]); 9] = [
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[0],
                width: VecWidth::V128,
                destination: 0,
                source: 3,
                form: SourceForm::Vector,
                control: MaskControl::None,
            },
            &[0x62, 0xF2, 0xE5, 0x08, 0x10, 0xC1],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[1],
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE2, 0xED, 0x23, 0x11, 0x0C, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[2],
                width: VecWidth::V512,
                destination: 20,
                source: 21,
                form: SourceForm::Vector,
                control: MaskControl::Zero,
            },
            &[0x62, 0xE2, 0xD5, 0xC3, 0x12, 0x24, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[3],
                width: VecWidth::V512,
                destination: 31,
                source: 30,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
            },
            &[0x62, 0x62, 0x0D, 0xD3, 0x45, 0x3C, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[4],
                width: VecWidth::V512,
                destination: 31,
                source: 30,
                form: SourceForm::Vector,
                control: MaskControl::Zero,
            },
            &[0x62, 0x62, 0x8D, 0xC3, 0x45, 0x3C, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[5],
                width: VecWidth::V512,
                destination: 20,
                source: 21,
                form: SourceForm::Vector,
                control: MaskControl::Zero,
            },
            &[0x62, 0xE2, 0x55, 0xC3, 0x46, 0x24, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[6],
                width: VecWidth::V128,
                destination: 23,
                source: 22,
                form: SourceForm::Broadcast,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE2, 0xCD, 0x13, 0x46, 0x3C, 0x24],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[7],
                width: VecWidth::V512,
                destination: 0,
                source: 3,
                form: SourceForm::Vector,
                control: MaskControl::None,
            },
            &[0x62, 0xF2, 0x65, 0x48, 0x47, 0xC1],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[8],
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                form: SourceForm::Broadcast,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE2, 0xED, 0x33, 0x47, 0x0C, 0x24],
        ),
    ];

    for (case, expected) in anchors {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_packed_variable_shift_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        let actual = match encoding.replay {
            X86EvexPackedVariableShiftMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexPackedVariableShiftMemoryReplay::Broadcast { stack_instruction }
            | X86EvexPackedVariableShiftMemoryReplay::MaskedVector { stack_instruction } => {
                stack_instruction
            }
        };
        assert_eq!(actual.as_slice(), expected, "{case:?}");
    }
}

#[test]
fn fs_gs_and_address_size_prefixes_remain_in_helper_addressing_only() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[6],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    for prefixes in [&[0x64][..], &[0x65][..], &[0x67][..], &[0x64, 0x67][..]] {
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&case.bytes());
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                sequence(&function, true).is_some(),
                "{level:?} {bytes:02X?}"
            );
            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {bytes:02X?}: helper-only prefix leaked into replay"
            );
        }
    }
}

#[test]
fn rip_relative_vector_and_broadcast_sources_admit_and_lower_at_every_level() {
    for case in [
        ShiftMemoryCase {
            kind: ShiftKind::ALL[0],
            width: VecWidth::V128,
            destination: 1,
            source: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ShiftMemoryCase {
            kind: ShiftKind::ALL[8],
            width: VecWidth::V512,
            destination: 17,
            source: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
    ] {
        let mut bytes = case.bytes();
        bytes.truncate(6);
        bytes[5] = (bytes[5] & 0x38) | 5;
        bytes.extend_from_slice(&0x20i32.to_le_bytes());
        let expected_address = Address::PcRel {
            offset: 0x20,
            disp_size: DispSize::Disp32,
            base: Some(PC + bytes.len() as u64),
        };

        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::VLoad { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{level:?} {case:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }
}

#[test]
fn variable_shift_memory_classifier_exhausts_2_764_800_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in ShiftKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source in 0..32u8 {
                    let forms: &[bool] = if kind.elem == VecElementType::I16 {
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
                                for apx_base in [false, true] {
                                    for apx_index in [false, true] {
                                        let bytes = memory_encoding(
                                            kind,
                                            width,
                                            destination,
                                            source,
                                            mask,
                                            zeroing,
                                            broadcast,
                                            apx_base,
                                            apx_index,
                                        );
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_packed_variable_shift_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                                        assert_eq!(encoding.shift, kind.shift, "{bytes:02X?}");
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
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            width != VecWidth::V512,
                                            "{bytes:02X?}"
                                        );
                                        match encoding.replay {
                                            X86EvexPackedVariableShiftMemoryReplay::Broadcast {
                                                ..
                                            } => assert!(broadcast, "{bytes:02X?}"),
                                            X86EvexPackedVariableShiftMemoryReplay::MaskedVector {
                                                ..
                                            } => {
                                                assert!(!broadcast && mask != 0, "{bytes:02X?}")
                                            }
                                            X86EvexPackedVariableShiftMemoryReplay::Vector {
                                                scratch,
                                                ..
                                            } => {
                                                assert!(!broadcast && mask == 0, "{bytes:02X?}");
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, source, "{bytes:02X?}");
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
    assert_eq!(accepted, 2_764_800);
}

#[test]
fn variable_shift_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[3],
        width: VecWidth::V128,
        destination: 1,
        source: 2,
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
    for (index, mask) in [(1, 0x01), (2, 0x01), (4, 0x01)] {
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
    let mut word_broadcast = ShiftMemoryCase {
        kind: ShiftKind::ALL[0],
        form: SourceForm::Vector,
        ..case
    }
    .bytes();
    word_broadcast[3] |= 0x10;
    malformed.push(word_broadcast);
    let mut wrong_word_w = ShiftMemoryCase {
        kind: ShiftKind::ALL[0],
        form: SourceForm::Vector,
        ..case
    }
    .bytes();
    wrong_word_w[2] &= !0x80;
    malformed.push(wrong_word_w);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_variable_shift_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_packed_variable_shift_memory_encoding()
            .is_some()
    );
    let mut repeat_prefixed = vec![0xF3];
    repeat_prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&repeat_prefixed)
            .unwrap()
            .evex_packed_variable_shift_memory_encoding()
            .is_none()
    );
}

#[test]
fn all_405_variable_shift_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 405);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(exact.encoding.elem, case.kind.elem, "{level:?} {case:?}");
            assert_eq!(exact.encoding.shift, case.kind.shift, "{level:?} {case:?}");
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
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{level:?} {case:?}");
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    case.kind.elem.bytes()
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
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");

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
    assert_eq!(lowerings, 405 * LEVELS.len());
}

#[test]
fn masked_broadcasts_have_one_aggregate_predicated_load_at_every_level() {
    for kind in ShiftKind::ALL
        .into_iter()
        .filter(|kind| kind.elem != VecElementType::I16)
    {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = ShiftMemoryCase {
                    kind,
                    width,
                    destination: 17,
                    source: 18,
                    form: SourceForm::Broadcast,
                    control,
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
                    assert!(sequence(&function, true).is_some(), "{level:?} {case:?}");
                }
            }
        }
    }
}

#[test]
fn apx_extended_sib_address_has_dynamic_guard_and_admits_after_it() {
    let kind = ShiftKind::ALL[8];
    let bytes = memory_encoding(kind, VecWidth::V512, 17, 18, 0, false, false, true, true);
    let expected_address = Address::BaseIndexScale {
        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
        scale: 2,
        disp: DISP8 * VecWidth::V512.bytes() as i32,
        disp_size: DispSize::Disp8,
    };
    let case = ShiftMemoryCase {
        kind,
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&bytes), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::VLoad { addr, .. } if addr == &expected_address
        )));
        let exact = sequence(&function, true).unwrap_or_else(|| panic!("{level:?}"));
        assert_eq!(sequence_index(&function), 1, "{level:?}");
        let (code, _) = lower(&function, case);
        let replay = match exact.encoding.replay {
            X86EvexPackedVariableShiftMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            _ => unreachable!(),
        };
        assert!(
            code.windows(replay.as_slice().len())
                .any(|window| window == replay.as_slice()),
            "{level:?}"
        );

        let mut missing_guard = function.clone();
        assert!(matches!(
            missing_guard.blocks[0].ops.remove(0).kind,
            OpKind::X86RequireApx
        ));
        assert!(
            sequence_at(&missing_guard, 0, true).is_none(),
            "{level:?}: APX address admitted without its dynamic guard"
        );
    }
}

#[test]
fn exact_sequence_rejects_mutated_semantics_provenance_and_extra_same_pc_work() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[6],
        width: VecWidth::V256,
        destination: 17,
        source: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    let reject = |name: &str, mutated: &SmirFunction| {
        assert!(
            sequence(mutated, true).is_none(),
            "{name}: {:#?}",
            mutated.blocks[0].ops
        );
    };

    let mut wrong_shift = function.clone();
    if let Some(OpKind::X86PackedShiftVariable { shift, .. }) = wrong_shift.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::X86PackedShiftVariable { .. }))
        .map(|op| &mut op.kind)
    {
        *shift = ShiftOp::Lsl;
    }
    reject("shift", &wrong_shift);

    let mut hinted = function.clone();
    let address_index = sequence_index(&hinted) + sequence(&hinted, true).unwrap().address_offset;
    hinted.blocks[0].ops[address_index].x86_hint =
        Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
    reject("hint", &hinted);

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    reject("provenance", &missing_provenance);

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
    reject("same-PC tail", &tail);

    let prefix_op = SmirOp::new(
        OpId(0xFFFE),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: crate::smir::ir::types::SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    );
    let mut same_pc_prefix = function.clone();
    same_pc_prefix.blocks[0].ops.insert(0, prefix_op.clone());
    assert!(
        sequence_at(&same_pc_prefix, 1, true).is_none(),
        "same-PC prefix admitted a semantic suffix"
    );

    let mut spurious_apx_guard = function.clone();
    spurious_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xFFFD), PC, OpKind::X86RequireApx));
    assert!(
        sequence_at(&spurious_apx_guard, 1, true).is_none(),
        "non-APX address admitted with a spurious dynamic APX guard"
    );

    let mut previous_instruction = function.clone();
    let mut previous_op = prefix_op;
    previous_op.guest_pc = PC - 1;
    previous_instruction.blocks[0].ops.insert(0, previous_op);
    assert!(
        sequence_at(&previous_instruction, 1, true).is_some(),
        "a preceding instruction must not block an exact frontier"
    );
}
