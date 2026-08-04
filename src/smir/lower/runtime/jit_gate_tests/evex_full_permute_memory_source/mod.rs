//! Exact helper-backed EVEX one-table full-permute memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexFullPermuteControl, X86EvexFullPermuteMemoryReplay,
    X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexFullPermuteMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_full_permute_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E80;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

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
            Self::Merge => (3, false),
            Self::Zero => (5, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermuteKind {
    PermB,
    PermW,
    PermD,
    PermQ,
    PermPs,
    PermPd,
    PermQImm,
    PermPdImm,
    PermilPsImm,
    PermilPdImm,
}

impl PermuteKind {
    const ALL: [Self; 10] = [
        Self::PermB,
        Self::PermW,
        Self::PermD,
        Self::PermQ,
        Self::PermPs,
        Self::PermPd,
        Self::PermQImm,
        Self::PermPdImm,
        Self::PermilPsImm,
        Self::PermilPdImm,
    ];

    const fn encoding(self) -> (u8, u8, bool) {
        match self {
            Self::PermB => (2, 0x8D, false),
            Self::PermW => (2, 0x8D, true),
            Self::PermD => (2, 0x36, false),
            Self::PermQ => (2, 0x36, true),
            Self::PermPs => (2, 0x16, false),
            Self::PermPd => (2, 0x16, true),
            Self::PermQImm => (3, 0x00, true),
            Self::PermPdImm => (3, 0x01, true),
            Self::PermilPsImm => (3, 0x04, false),
            Self::PermilPdImm => (3, 0x05, true),
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::PermB => VecElementType::I8,
            Self::PermW => VecElementType::I16,
            Self::PermD => VecElementType::I32,
            Self::PermQ | Self::PermQImm => VecElementType::I64,
            Self::PermPs | Self::PermilPsImm => VecElementType::F32,
            Self::PermPd | Self::PermPdImm | Self::PermilPdImm => VecElementType::F64,
        }
    }

    const fn variable(self) -> bool {
        matches!(
            self,
            Self::PermB | Self::PermW | Self::PermD | Self::PermQ | Self::PermPs | Self::PermPd
        )
    }

    const fn allows_128(self) -> bool {
        matches!(
            self,
            Self::PermB | Self::PermW | Self::PermilPsImm | Self::PermilPdImm
        )
    }

    const fn allows_broadcast(self) -> bool {
        !matches!(self, Self::PermB | Self::PermW)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PermuteMemoryCase {
    kind: PermuteKind,
    width: VecWidth,
    destination: u8,
    indices: u8,
    form: SourceForm,
    control: MaskControl,
    immediate: u8,
}

impl PermuteMemoryCase {
    const fn elem(self) -> VecElementType {
        self.kind.elem()
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn expected_control(self) -> X86EvexFullPermuteControl {
        if self.kind.variable() {
            X86EvexFullPermuteControl::Variable {
                indices: self.indices,
            }
        } else {
            let (domain_lanes, repeat_lanes, control_bits) = match self.kind {
                PermuteKind::PermQImm | PermuteKind::PermPdImm => (4, 4, 2),
                PermuteKind::PermilPsImm => (4, 4, 2),
                PermuteKind::PermilPdImm => (2, 8, 1),
                _ => unreachable!(),
            };
            X86EvexFullPermuteControl::Immediate {
                immediate: self.immediate,
                domain_lanes,
                repeat_lanes,
                control_bits,
            }
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 32 && self.indices < 32);
        assert!(self.kind.allows_128() || self.width != VecWidth::V128);
        assert!(self.kind.allows_broadcast() || self.form == SourceForm::Vector);
        let (map, opcode, w) = self.kind.encoding();
        let encoded_vvvv = if self.kind.variable() {
            (!self.indices) & 0x0F
        } else {
            0x0F
        };
        let encoded_v_high = !self.kind.variable() || self.indices < 16;
        let mut bytes = vec![
            0x62,
            0x60 | map
                | (u8::from(self.destination & 8 == 0) << 7)
                | (u8::from(self.destination & 16 == 0) << 4),
            (u8::from(w) << 7) | (encoded_vvvv << 3) | 0x05,
            (u8::from(self.zeroing()) << 7)
                | (self.ll() << 5)
                | (u8::from(self.form == SourceForm::Broadcast) << 4)
                | (u8::from(encoded_v_high) << 3)
                | self.mask(),
            opcode,
            ((self.destination & 7) << 3) | 3,
        ];
        if !self.kind.variable() {
            bytes.push(self.immediate);
        }
        bytes
    }
}

fn widths(kind: PermuteKind) -> &'static [VecWidth] {
    if kind.allows_128() {
        &[VecWidth::V128, VecWidth::V256, VecWidth::V512]
    } else {
        &[VecWidth::V256, VecWidth::V512]
    }
}

fn forms(kind: PermuteKind) -> &'static [SourceForm] {
    if kind.allows_broadcast() {
        &[SourceForm::Vector, SourceForm::Broadcast]
    } else {
        &[SourceForm::Vector]
    }
}

fn scanner_cases() -> Vec<PermuteMemoryCase> {
    let mut cases = Vec::new();
    for kind in PermuteKind::ALL {
        for &width in widths(kind) {
            for &form in forms(kind) {
                for control in MaskControl::ALL {
                    let indices = if kind.variable() {
                        &[0u8, 15, 31][..]
                    } else {
                        &[0u8][..]
                    };
                    for &indices in indices {
                        cases.push(PermuteMemoryCase {
                            kind,
                            width,
                            destination: 0,
                            indices,
                            form,
                            control,
                            immediate: 0xA5,
                        });
                    }
                }
            }
        }
    }
    cases
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
        X86InstructionBytes::new(bytes).expect("EVEX full-permute provenance"),
    );
    function
}

fn lift_case(case: PermuteMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
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

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexFullPermuteMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_full_permute_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexFullPermuteMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn replay_bytes(sequence: X86JitEvexFullPermuteMemorySequence) -> X86InstructionBytes {
    match sequence.encoding.replay {
        X86EvexFullPermuteMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction,
        X86EvexFullPermuteMemoryReplay::Broadcast {
            stack_instruction, ..
        } => stack_instruction,
    }
}

fn lower(function: &SmirFunction, case: PermuteMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vbmi,
        case.elem() == VecElementType::I8,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512vbmi2, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
            && (case.elem() != VecElementType::I8 || std::is_x86_feature_detected!("avx512vbmi")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: full-permute lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX full permute"),
        result.entry_offset,
    )
}

#[test]
fn all_258_family_scanner_cells_lift_optimize_admit_and_lower_at_every_level() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 258);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let bytes = case.bytes();
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                !function.blocks[0]
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
                "{level:?} {case:?}: E4NF tuple became fault-suppressing"
            );
            let matched = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: missing exact sequence"));
            assert_eq!(matched.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(matched.encoding.elem, case.elem(), "{level:?} {case:?}");
            assert_eq!(
                matched.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(
                matched.encoding.control,
                case.expected_control(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                matched.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                matched.encoding.zeroing,
                case.zeroing(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                matched.encoding.memory_size,
                if case.form == SourceForm::Broadcast {
                    case.elem().bytes()
                } else {
                    case.width.bytes()
                },
                "{level:?} {case:?}"
            );
            let replay = replay_bytes(matched);
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(replay.as_slice().len())
                    .any(|window| window == replay.as_slice()),
                "{level:?} {case:?}: exact replay bytes absent"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 258 * LEVELS.len());
}

#[test]
fn classifier_exhausts_destination_index_mask_address_extension_and_immediate_fields() {
    let mut classified = 0usize;
    for kind in PermuteKind::ALL {
        for &width in widths(kind) {
            for &form in forms(kind) {
                for destination in 0u8..32 {
                    let indices_end = if kind.variable() { 32 } else { 1 };
                    for indices in 0u8..indices_end {
                        let immediates: &[u8] = if kind.variable() {
                            &[0xA5]
                        } else {
                            &[0x00, 0xA5, 0xFF]
                        };
                        for &immediate in immediates {
                            let case = PermuteMemoryCase {
                                kind,
                                width,
                                destination,
                                indices,
                                form,
                                control: MaskControl::None,
                                immediate,
                            };
                            for mask in 0u8..8 {
                                for zeroing in [false, true] {
                                    if mask == 0 && zeroing {
                                        continue;
                                    }
                                    for b4 in [false, true] {
                                        for x4 in [false, true] {
                                            let mut bytes = case.bytes();
                                            bytes[1] = (bytes[1] & !0x08) | (u8::from(b4) << 3);
                                            bytes[2] = (bytes[2] & !0x04) | (u8::from(!x4) << 2);
                                            bytes[3] = (bytes[3] & !0x87)
                                                | (u8::from(zeroing) << 7)
                                                | mask;
                                            let instruction = X86InstructionBytes::new(&bytes)
                                                .expect("bounded EVEX instruction");
                                            let encoding = instruction
                                                .evex_full_permute_memory_encoding()
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            assert_eq!(encoding.width, width);
                                            assert_eq!(encoding.elem, case.elem());
                                            assert_eq!(encoding.destination, destination);
                                            assert_eq!(encoding.control, case.expected_control());
                                            assert_eq!(
                                                encoding.writemask,
                                                (mask != 0).then_some(mask)
                                            );
                                            assert_eq!(encoding.zeroing, zeroing);
                                            match encoding.replay {
                                                X86EvexFullPermuteMemoryReplay::Vector {
                                                    scratch,
                                                    register_instruction,
                                                } => {
                                                    assert_ne!(scratch, destination);
                                                    if kind.variable() {
                                                        assert_ne!(scratch, indices);
                                                    }
                                                    assert!(scratch < 16);
                                                    assert!(
                                                        register_instruction
                                                            .evex_full_permute_memory_encoding()
                                                            .is_none()
                                                    );
                                                }
                                                X86EvexFullPermuteMemoryReplay::Broadcast {
                                                    stack_instruction,
                                                    ..
                                                } => {
                                                    let stack = stack_instruction
                                                        .evex_full_permute_memory_encoding()
                                                        .expect("stack replay reclassifies");
                                                    assert_eq!(stack.width, width);
                                                    assert_eq!(stack.elem, case.elem());
                                                    assert_eq!(stack.destination, destination);
                                                    assert_eq!(
                                                        stack.control,
                                                        case.expected_control()
                                                    );
                                                }
                                            }
                                            classified += 1;
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
    assert_eq!(classified, 1_466_880);
}

#[test]
fn llvm_23_encoding_anchors_classify_exactly() {
    let anchors: [(
        &[u8],
        PermuteKind,
        VecWidth,
        u8,
        SourceForm,
        Option<u8>,
        bool,
    ); 10] = [
        (
            &[0x62, 0xE2, 0x6D, 0x82, 0x8D, 0x0C, 0x24],
            PermuteKind::PermB,
            VecWidth::V128,
            17,
            SourceForm::Vector,
            Some(2),
            true,
        ),
        (
            &[0x62, 0xE2, 0xD5, 0x23, 0x8D, 0x24, 0x24],
            PermuteKind::PermW,
            VecWidth::V256,
            20,
            SourceForm::Vector,
            Some(3),
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0xA9, 0x36, 0x0C, 0x24],
            PermuteKind::PermD,
            VecWidth::V256,
            1,
            SourceForm::Vector,
            Some(1),
            true,
        ),
        (
            &[0x62, 0xF2, 0xD5, 0x5C, 0x36, 0x1C, 0x24],
            PermuteKind::PermQ,
            VecWidth::V512,
            3,
            SourceForm::Broadcast,
            Some(4),
            false,
        ),
        (
            &[0x62, 0xF2, 0x45, 0xCD, 0x16, 0x34, 0x24],
            PermuteKind::PermPs,
            VecWidth::V512,
            6,
            SourceForm::Vector,
            Some(5),
            true,
        ),
        (
            &[0x62, 0x72, 0xB5, 0x3E, 0x16, 0x04, 0x24],
            PermuteKind::PermPd,
            VecWidth::V256,
            8,
            SourceForm::Broadcast,
            Some(6),
            false,
        ),
        (
            &[0x62, 0x73, 0xFD, 0xDF, 0x00, 0x14, 0x24, 0x1B],
            PermuteKind::PermQImm,
            VecWidth::V512,
            10,
            SourceForm::Broadcast,
            Some(7),
            true,
        ),
        (
            &[0x62, 0x73, 0xFD, 0x29, 0x01, 0x1C, 0x24, 0xE4],
            PermuteKind::PermPdImm,
            VecWidth::V256,
            11,
            SourceForm::Vector,
            Some(1),
            false,
        ),
        (
            &[0x62, 0x73, 0x7D, 0x9A, 0x04, 0x24, 0x24, 0x39],
            PermuteKind::PermilPsImm,
            VecWidth::V128,
            12,
            SourceForm::Broadcast,
            Some(2),
            true,
        ),
        (
            &[0x62, 0x73, 0xFD, 0x4B, 0x05, 0x2C, 0x24, 0xA5],
            PermuteKind::PermilPdImm,
            VecWidth::V512,
            13,
            SourceForm::Vector,
            Some(3),
            false,
        ),
    ];
    for (bytes, kind, width, destination, form, mask, zeroing) in anchors {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_full_permute_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(encoding.elem, kind.elem(), "{bytes:02X?}");
        assert_eq!(encoding.width, width, "{bytes:02X?}");
        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
        assert_eq!(encoding.writemask, mask, "{bytes:02X?}");
        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
        assert_eq!(
            matches!(
                encoding.replay,
                X86EvexFullPermuteMemoryReplay::Broadcast { .. }
            ),
            form == SourceForm::Broadcast,
            "{bytes:02X?}"
        );
        let function = optimize(lift_bytes(bytes), OptLevel::O2);
        sequence(&function, true).unwrap_or_else(|| panic!("{bytes:02X?}: graph"));
    }
}

fn assert_unclassified(name: &str, bytes: &[u8]) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert!(
        instruction.evex_full_permute_memory_encoding().is_none(),
        "{name}: {bytes:02X?}"
    );
}

#[test]
fn malformed_reserved_register_truncated_and_trailing_encodings_fail_closed() {
    let variable = PermuteMemoryCase {
        kind: PermuteKind::PermB,
        width: VecWidth::V128,
        destination: 17,
        indices: 18,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        immediate: 0,
    }
    .bytes();
    let immediate = PermuteMemoryCase {
        kind: PermuteKind::PermQImm,
        width: VecWidth::V256,
        destination: 9,
        indices: 0,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
        immediate: 0xA5,
    }
    .bytes();
    let mut malformed = Vec::new();
    let mut bytes = variable.clone();
    bytes[1] = (bytes[1] & !7) | 1;
    malformed.push(("wrong map", bytes));
    let mut bytes = variable.clone();
    bytes[2] = (bytes[2] & !3) | 2;
    malformed.push(("wrong mandatory prefix", bytes));
    let mut bytes = variable.clone();
    bytes[3] |= 0x10;
    malformed.push(("byte broadcast", bytes));
    let mut bytes = variable.clone();
    bytes[3] = (bytes[3] & !0x60) | 0x60;
    malformed.push(("reserved LL", bytes));
    let mut bytes = variable.clone();
    bytes[3] = (bytes[3] & !7) | 0x80;
    malformed.push(("zeroing k0", bytes));
    let mut bytes = variable.clone();
    bytes[5] |= 0xC0;
    malformed.push(("register operand", bytes));
    let mut bytes = variable.clone();
    bytes.pop();
    malformed.push(("truncated", bytes));
    let mut bytes = variable.clone();
    bytes.push(0);
    malformed.push(("trailing", bytes));
    let mut bytes = immediate.clone();
    bytes[2] ^= 0x08;
    malformed.push(("immediate reserved vvvv", bytes));
    let mut bytes = immediate.clone();
    bytes[3] &= !0x08;
    malformed.push(("immediate reserved V-prime", bytes));
    let mut bytes = immediate.clone();
    bytes[3] &= !0x60;
    malformed.push(("immediate qword LL=0", bytes));
    let mut bytes = immediate.clone();
    bytes.pop();
    malformed.push(("missing immediate", bytes));
    let mut bytes = immediate;
    bytes.push(0);
    malformed.push(("immediate trailing byte", bytes));
    for (name, bytes) in malformed {
        assert_unclassified(name, &bytes);
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted mutation"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted mutation"
    );
}

#[test]
fn matcher_fails_closed_for_selector_provenance_graph_fault_mask_and_boundary_mutations() {
    let case = PermuteMemoryCase {
        kind: PermuteKind::PermilPdImm,
        width: VecWidth::V512,
        destination: 25,
        indices: 0,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        immediate: 0xA5,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    let matched = sequence(&base, true).expect("baseline immediate full permute");
    let index = sequence_index(&base);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_provenance = base.clone();
    let mut wrong_bytes = case.bytes();
    wrong_bytes[4] = 0x04;
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );

    let mut oversized_selector = base.clone();
    let insert_index = oversized_selector.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .expect("first immediate selector insertion");
    match &mut oversized_selector.blocks[0].ops[insert_index - 1].kind {
        OpKind::Mov {
            src: SrcOperand::Imm(value),
            ..
        } => *value += 0x100,
        other => panic!("selector producer: {other:?}"),
    }

    let mut hinted = base.clone();
    hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));

    let mut virtual_address = base.clone();
    match &mut virtual_address.blocks[0].ops[index + matched.address_offset].kind {
        OpKind::VLoad { addr, .. } => {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
        other => panic!("tuple load: {other:?}"),
    }

    let mut wrong_permute = base.clone();
    match &mut wrong_permute.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VPermute { .. }))
        .unwrap()
        .kind
    {
        OpKind::VPermute { elem, .. } => *elem = VecElementType::I64,
        _ => unreachable!(),
    }

    let mut child_pc = base.clone();
    child_pc.blocks[0].ops[index + 1].guest_pc += 1;

    let mut wrong_mask = base.clone();
    match &mut wrong_mask.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::Shr {
                    amount: SrcOperand::Imm(1),
                    ..
                }
            )
        })
        .expect("mask lane-one shift")
        .kind
    {
        OpKind::Shr {
            amount: SrcOperand::Imm(amount),
            ..
        } => *amount = 2,
        _ => unreachable!(),
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F00), PC, OpKind::Nop));

    let loaded = match base.blocks[0].ops[index + matched.address_offset].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F01),
        PC + 1,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            src: loaded,
            width: VecWidth::V512,
        },
    ));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("opcode provenance differs", wrong_provenance),
        ("selector differs by 2^8", oversized_selector),
        ("semantic root has hint", hinted),
        ("address contains virtual register", virtual_address),
        ("permute element differs", wrong_permute),
        ("semantic child PC differs", child_pc),
        ("mask predicate differs", wrong_mask),
        ("same-PC operation follows sequence", same_pc_tail),
        ("loaded temporary escapes sequence", external_use),
    ] {
        assert_rejected(name, &function);
    }
    assert!(sequence(&base, false).is_none());
}

#[test]
fn segment_addr32_rip_compressed_tuple_and_apx_b4_x4_addresses_remain_exact() {
    let case = PermuteMemoryCase {
        kind: PermuteKind::PermD,
        width: VecWidth::V512,
        destination: 9,
        indices: 14,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        immediate: 0,
    };
    let vector = case.bytes();
    let mut rip = vector.clone();
    rip[5] = (rip[5] & 0x38) | 0x05;
    rip.splice(6..6, 0x20i32.to_le_bytes());
    let mut addr32 = vector.clone();
    addr32.insert(0, 0x67);
    let mut fs = vector.clone();
    fs.insert(0, 0x64);
    let mut gs_addr32 = vector.clone();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.splice(6..6, [0x8B, 0x02]);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let address_cases = [
        (
            "RIP+disp32",
            rip.clone(),
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + rip.len() as u64),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS Full Mem",
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
            "GS addr32 SIB compressed Full Mem",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 128,
            })),
        ),
    ];
    for (name, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{name} {level:?}"));
        }
    }

    let mut apx = case.bytes();
    apx[5] = (apx[5] & 0x38) | 0x04;
    apx.push(0x48);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let expected = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(
            function.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(&op.kind, OpKind::VLoad { addr, .. } if addr == &expected))
        );
        sequence(&function, true).unwrap_or_else(|| panic!("APX {level:?}"));

        let mut missing_guard = function.clone();
        missing_guard.blocks[0].ops.remove(0);
        assert!(sequence_at(&missing_guard, 0, true).is_none());
    }
}

#[test]
fn avx_only_vector_bridge_is_rejected() {
    let case = scanner_cases()
        .into_iter()
        .find(|case| case.kind == PermuteKind::PermD && case.width == VecWidth::V512)
        .unwrap();
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only bridge must reject EVEX full permute");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
