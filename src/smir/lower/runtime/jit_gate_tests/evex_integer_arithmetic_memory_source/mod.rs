//! Exact helper-backed EVEX packed integer arithmetic memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, SrcOperand,
    VLaneOp, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexIntegerArithmeticMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_arithmetic_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7C20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArithmeticKind {
    AddWrappingByte,
    AddWrappingWord,
    AddWrappingDword,
    AddWrappingQword,
    AddSignedSaturatingByte,
    AddSignedSaturatingWord,
    AddUnsignedSaturatingByte,
    AddUnsignedSaturatingWord,
    SubWrappingByte,
    SubWrappingWord,
    SubWrappingDword,
    SubWrappingQword,
    SubSignedSaturatingByte,
    SubSignedSaturatingWord,
    SubUnsignedSaturatingByte,
    SubUnsignedSaturatingWord,
    AverageByte,
    AverageWord,
    DotByte,
    DotByteSaturating,
    DotWord,
    DotWordSaturating,
}

impl ArithmeticKind {
    const ALL: [Self; 22] = [
        Self::AddWrappingByte,
        Self::AddWrappingWord,
        Self::AddWrappingDword,
        Self::AddWrappingQword,
        Self::AddSignedSaturatingByte,
        Self::AddSignedSaturatingWord,
        Self::AddUnsignedSaturatingByte,
        Self::AddUnsignedSaturatingWord,
        Self::SubWrappingByte,
        Self::SubWrappingWord,
        Self::SubWrappingDword,
        Self::SubWrappingQword,
        Self::SubSignedSaturatingByte,
        Self::SubSignedSaturatingWord,
        Self::SubUnsignedSaturatingByte,
        Self::SubUnsignedSaturatingWord,
        Self::AverageByte,
        Self::AverageWord,
        Self::DotByte,
        Self::DotByteSaturating,
        Self::DotWord,
        Self::DotWordSaturating,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::AddWrappingByte => 0xFC,
            Self::AddWrappingWord => 0xFD,
            Self::AddWrappingDword => 0xFE,
            Self::AddWrappingQword => 0xD4,
            Self::AddSignedSaturatingByte => 0xEC,
            Self::AddSignedSaturatingWord => 0xED,
            Self::AddUnsignedSaturatingByte => 0xDC,
            Self::AddUnsignedSaturatingWord => 0xDD,
            Self::SubWrappingByte => 0xF8,
            Self::SubWrappingWord => 0xF9,
            Self::SubWrappingDword => 0xFA,
            Self::SubWrappingQword => 0xFB,
            Self::SubSignedSaturatingByte => 0xE8,
            Self::SubSignedSaturatingWord => 0xE9,
            Self::SubUnsignedSaturatingByte => 0xD8,
            Self::SubUnsignedSaturatingWord => 0xD9,
            Self::AverageByte => 0xE0,
            Self::AverageWord => 0xE3,
            Self::DotByte => 0x50,
            Self::DotByteSaturating => 0x51,
            Self::DotWord => 0x52,
            Self::DotWordSaturating => 0x53,
        }
    }

    /// Element granularity used for Type E4 memory accesses and writemasking.
    const fn elem(self) -> VecElementType {
        match self {
            Self::AddWrappingByte
            | Self::AddSignedSaturatingByte
            | Self::AddUnsignedSaturatingByte
            | Self::SubWrappingByte
            | Self::SubSignedSaturatingByte
            | Self::SubUnsignedSaturatingByte
            | Self::AverageByte => VecElementType::I8,
            Self::AddWrappingWord
            | Self::AddSignedSaturatingWord
            | Self::AddUnsignedSaturatingWord
            | Self::SubWrappingWord
            | Self::SubSignedSaturatingWord
            | Self::SubUnsignedSaturatingWord
            | Self::AverageWord => VecElementType::I16,
            Self::AddWrappingDword
            | Self::SubWrappingDword
            | Self::DotByte
            | Self::DotByteSaturating
            | Self::DotWord
            | Self::DotWordSaturating => VecElementType::I32,
            Self::AddWrappingQword | Self::SubWrappingQword => VecElementType::I64,
        }
    }

    /// Multiplicand/source element granularity. VNNI accumulates groups of
    /// byte or word products into each dword result.
    const fn source_elem(self) -> VecElementType {
        match self {
            Self::DotByte | Self::DotByteSaturating => VecElementType::I8,
            Self::DotWord | Self::DotWordSaturating => VecElementType::I16,
            _ => self.elem(),
        }
    }

    const fn map(self) -> X86VecMap {
        if self.is_dot() {
            X86VecMap::Map0F38
        } else {
            X86VecMap::Map0F
        }
    }

    const fn map_bits(self) -> u8 {
        if self.is_dot() { 2 } else { 1 }
    }

    const fn is_wig(self) -> bool {
        matches!(self.elem(), VecElementType::I8 | VecElementType::I16)
    }

    const fn allows_broadcast(self) -> bool {
        matches!(self.elem(), VecElementType::I32 | VecElementType::I64)
    }

    const fn is_average(self) -> bool {
        matches!(self, Self::AverageByte | Self::AverageWord)
    }

    const fn is_dot(self) -> bool {
        matches!(
            self,
            Self::DotByte | Self::DotByteSaturating | Self::DotWord | Self::DotWordSaturating
        )
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
            Self::Merge => (3, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerArithmeticMemoryCase {
    kind: ArithmeticKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    /// Raw EVEX.W for byte/word WIG encodings. Fixed-width dword/qword/VNNI
    /// cases use W0/W1/W0 respectively.
    wig_w: bool,
}

impl IntegerArithmeticMemoryCase {
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

    const fn w(self) -> bool {
        match self.kind.elem() {
            VecElementType::I8 | VecElementType::I16 => self.wig_w,
            VecElementType::I32 => false,
            VecElementType::I64 => true,
            _ => unreachable!(),
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self.kind.elem() {
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
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave a low vector scratch")
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
        _ => unreachable!("EVEX integer-arithmetic vector width"),
    }))
}

fn memory_encoding(case: IntegerArithmeticMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    assert!(!case.broadcast() || case.kind.allows_broadcast());
    let p0 = case.kind.map_bits()
        | 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: IntegerArithmeticMemoryCase) -> Vec<u8> {
    let p0 = 0x60
        | case.kind.map_bits()
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
}

fn register_encoding(case: IntegerArithmeticMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x40
        | case.kind.map_bits()
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
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
        X86InstructionBytes::new(bytes).expect("EVEX integer-arithmetic provenance"),
    );
    function
}

fn lift_case(case: IntegerArithmeticMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexIntegerArithmeticMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_integer_arithmetic_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: IntegerArithmeticMemoryCase) -> (Vec<u8>, usize) {
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
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
            && (!case.kind.is_dot() || std::is_x86_feature_detected!("avx512vnni")),
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX integer-arithmetic lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX integer arithmetic"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<IntegerArithmeticMemoryCase> {
    let mut cases = Vec::new();
    for kind in ArithmeticKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source1) in [(0, 0), (9, 10), (17, 18)] {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                            continue;
                        }
                        for control in MaskControl::ALL {
                            cases.push(IntegerArithmeticMemoryCase {
                                kind,
                                width,
                                destination,
                                source1,
                                form,
                                control,
                                wig_w,
                            });
                        }
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn integer_arithmetic_rewrites_match_twelve_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xE1, 0x6D, 0x00, 0xFC, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x00, 0xFC, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0x4B, 0xE9, 0x0A],
            &[0x62, 0x71, 0x2D, 0x4B, 0xE9, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0xC1, 0xDC, 0x0A],
            &[0x62, 0xE1, 0x6D, 0xC1, 0xDC, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0x30, 0xFE, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x30, 0xFE, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x61, 0xAD, 0xD2, 0xFB, 0x0A],
            &[0x62, 0x61, 0xAD, 0xD2, 0xFB, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF1, 0x6D, 0x4C, 0xD8, 0x0A],
            &[0x62, 0xF1, 0x6D, 0x4C, 0xD8, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0x00, 0xE0, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x00, 0xE0, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0x2B, 0xE3, 0x0A],
            &[0x62, 0x71, 0x2D, 0x2B, 0xE3, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x6D, 0x00, 0x50, 0x0A],
            &[0x62, 0xE2, 0x6D, 0x00, 0x50, 0xC8],
        ),
        (
            &[0x62, 0x72, 0x2D, 0xBB, 0x51, 0x0A],
            &[0x62, 0x72, 0x2D, 0xBB, 0x51, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x6D, 0x42, 0x52, 0x0A],
            &[0x62, 0xE2, 0x6D, 0x42, 0x52, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0x2D, 0xD4, 0x53, 0x0A],
            &[0x62, 0x62, 0x2D, 0xD4, 0x53, 0x0C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_arithmetic_memory_encoding()
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
fn integer_arithmetic_classifier_exhausts_8_110_080_operand_control_wig_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in ArithmeticKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for form in [SourceForm::Vector, SourceForm::Broadcast] {
                            if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                                continue;
                            }
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let case = IntegerArithmeticMemoryCase {
                                        kind,
                                        width,
                                        destination,
                                        source1,
                                        form,
                                        control: if mask == 0 {
                                            MaskControl::None
                                        } else if zeroing {
                                            MaskControl::Zero
                                        } else {
                                            MaskControl::Merge
                                        },
                                        wig_w,
                                    };
                                    let mut canonical = memory_encoding(case, true);
                                    canonical[3] =
                                        (canonical[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                                    for base_high in [false, true] {
                                        for index_high in [false, true] {
                                            let mut bytes = canonical.clone();
                                            bytes[1] |= u8::from(base_high) << 3;
                                            if index_high {
                                                bytes[2] &= !0x04;
                                            }
                                            let encoding = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_integer_arithmetic_memory_encoding()
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            assert_eq!(
                                                encoding.opcode,
                                                kind.opcode(),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.w, case.w(), "{bytes:02X?}");
                                            assert_eq!(encoding.map, kind.map(), "{bytes:02X?}");
                                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                                            assert_eq!(encoding.elem, kind.elem(), "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.destination, destination,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.source1, source1, "{bytes:02X?}");
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
                                                X86EvexIntegerArithmeticMemoryReplay::Vector {
                                                    scratch,
                                                    register_instruction,
                                                } => {
                                                    assert_eq!(mask, 0, "{bytes:02X?}");
                                                    assert_eq!(form, SourceForm::Vector);
                                                    assert_ne!(
                                                        scratch, destination,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_ne!(scratch, source1, "{bytes:02X?}");
                                                    let register_needs_vl = if kind.is_average() {
                                                        register_instruction
                                                            .evex_register_packed_average_needs_vl()
                                                    } else if kind.is_dot() {
                                                        register_instruction
                                                            .evex_register_integer_dot_needs_vl()
                                                    } else {
                                                        register_instruction
                                                            .evex_register_integer_arithmetic_needs_vl()
                                                    };
                                                    assert_eq!(
                                                        register_needs_vl,
                                                        Some(width != VecWidth::V512),
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                                                    ..
                                                } => {
                                                    assert_eq!(form, SourceForm::Broadcast);
                                                }
                                                X86EvexIntegerArithmeticMemoryReplay::MaskedVector {
                                                    ..
                                                } => {
                                                    assert_ne!(mask, 0, "{bytes:02X?}");
                                                    assert_eq!(form, SourceForm::Vector);
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
    assert_eq!(accepted, 8_110_080);
}

#[test]
fn evex_integer_dot_register_classifier_exhausts_5_898_240_legal_cells() {
    let mut accepted = 0usize;
    for opcode in 0x50u8..=0x53 {
        for extensions in 0u8..16 {
            for encoded_vvvv in 0u8..16 {
                for encoded_v_prime in [false, true] {
                    for ll in 0u8..=2 {
                        for mask in 0u8..8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let p0 = (extensions << 4) | 2;
                                let p1 = (encoded_vvvv << 3) | 0x05;
                                let p2 = (u8::from(zeroing) << 7)
                                    | (ll << 5)
                                    | (u8::from(encoded_v_prime) << 3)
                                    | mask;
                                for modrm in 0xC0u8..=0xFF {
                                    let bytes = [0x62, p0, p1, p2, opcode, modrm];
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_integer_dot_needs_vl(),
                                        Some(ll != 2),
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
    assert_eq!(accepted, 5_898_240);

    let valid = [0x62, 0xE2, 0x6D, 0x42, 0x52, 0xC8];
    let mut malformed = Vec::new();
    for (index, xor) in [(1, 0x01), (2, 0x01), (4, 0x0D)] {
        let mut bytes = valid.to_vec();
        bytes[index] ^= xor;
        malformed.push(bytes);
    }
    for (index, set) in [(1, 0x08), (2, 0x80), (3, 0x10)] {
        let mut bytes = valid.to_vec();
        bytes[index] |= set;
        malformed.push(bytes);
    }
    let mut no_u = valid.to_vec();
    no_u[2] &= !0x04;
    malformed.push(no_u);
    let mut reserved_ll = valid.to_vec();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.to_vec();
    zero_k0[3] = 0x80;
    malformed.push(zero_k0);
    let mut memory = valid.to_vec();
    memory[5] &= 0x3F;
    malformed.push(memory);
    let mut trailing = valid.to_vec();
    trailing.push(0);
    malformed.push(trailing);
    malformed.push(valid[..5].to_vec());

    for bytes in malformed {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_integer_dot_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn integer_arithmetic_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AddWrappingDword,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        wig_w: false,
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
        (4, 0x08), // non-owned opcode
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
    let mut dword_w1 = valid.clone();
    dword_w1[2] |= 0x80;
    malformed.push(dword_w1);
    let mut qword_w0 = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AddWrappingQword,
        ..case
    }
    .bytes();
    qword_w0[2] &= !0x80;
    malformed.push(qword_w0);
    let mut byte_broadcast = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AddWrappingByte,
        ..case
    }
    .bytes();
    byte_broadcast[3] |= 0x10;
    malformed.push(byte_broadcast);
    let mut dot_w1 = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::DotByte,
        form: SourceForm::Broadcast,
        ..case
    }
    .bytes();
    dot_w1[2] |= 0x80;
    malformed.push(dot_w1);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_arithmetic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_arithmetic_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );

    for wig_w in [false, true] {
        let bytes = IntegerArithmeticMemoryCase {
            kind: ArithmeticKind::SubSignedSaturatingWord,
            wig_w,
            ..case
        }
        .bytes();
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_arithmetic_memory_encoding()
                .is_some(),
            "WIG byte/word form rejected W={wig_w}: {bytes:02X?}"
        );
    }
}

#[test]
fn all_1188_integer_arithmetic_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 1_188);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.map, case.kind.map());
            assert_eq!(exact.encoding.opcode, case.kind.opcode());
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.kind.elem());
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.w, case.w());
            assert_eq!(exact.encoding.zeroing, case.zeroing());
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
    assert_eq!(lowerings, 1_188 * LEVELS.len());
}

#[test]
fn type_e4_memory_graphs_preserve_exact_access_granularity() {
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
            let lanes = case.width.lanes(case.kind.elem()) as usize;
            assert_eq!(
                (ordinary_loads, pred_loads),
                match (case.control, case.form) {
                    (MaskControl::None, _) => (1, 0),
                    (_, SourceForm::Broadcast) if case.kind.is_dot() => (0, lanes),
                    (_, SourceForm::Broadcast) => (0, 1),
                    (_, SourceForm::Vector) => (0, lanes),
                },
                "{level:?} {case:?}"
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
fn integer_arithmetic_sequence_fails_closed_for_provenance_and_graph_mutations() {
    for case in [
        IntegerArithmeticMemoryCase {
            kind: ArithmeticKind::AddSignedSaturatingByte,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            wig_w: true,
        },
        IntegerArithmeticMemoryCase {
            kind: ArithmeticKind::SubWrappingQword,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            wig_w: false,
        },
        IntegerArithmeticMemoryCase {
            kind: ArithmeticKind::AverageWord,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            wig_w: true,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function, false).is_none());

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[3] = (bytes[3] & !7) | if case.mask() == 1 { 2 } else { 1 };
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong mask provenance", &wrong_provenance);

        let mut wrong_address = function.clone();
        let memory_op = wrong_address.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::Load { .. }
                        | OpKind::VLoad { .. }
                        | OpKind::PredLoad { .. }
                        | OpKind::Lea { .. }
                )
            })
            .unwrap();
        match &mut memory_op.kind {
            OpKind::Load { addr, .. }
            | OpKind::VLoad { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::Lea { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!(),
        }
        assert_rejected("virtual address", &wrong_address);

        let mut hinted = function.clone();
        hinted.blocks[0].ops[0].x86_hint = Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted first op", &hinted);

        let mut wrong_lane = function.clone();
        let extract = wrong_lane.blocks[0]
            .ops
            .iter_mut()
            .rev()
            .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .unwrap();
        let OpKind::VExtractLane { lane, .. } = &mut extract.kind else {
            unreachable!()
        };
        *lane = lane.wrapping_add(1);
        assert_rejected("wrong result lane", &wrong_lane);

        let mut tail = function.clone();
        tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &tail);
    }
}

fn mutate_dot_product(
    function: &mut SmirFunction,
    mutation: impl FnOnce(&mut crate::smir::ir::ops::OpKind),
) {
    let dot = function.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VDotProduct { .. }))
        .expect("VNNI lift owns one VDotProduct");
    mutation(&mut dot.kind);
}

fn assert_dot_mutation_rejected(
    name: &str,
    function: &SmirFunction,
    mutation: impl FnOnce(&mut crate::smir::ir::ops::OpKind),
) {
    let mut malformed = function.clone();
    mutate_dot_product(&mut malformed, mutation);
    assert_rejected(name, &malformed);
}

#[test]
fn vnni_terminal_dot_product_contract_fails_closed_for_every_semantic_axis() {
    let case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::DotByteSaturating,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
        wig_w: false,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    assert_dot_mutation_rejected("VNNI destination", &function, |kind| {
        let OpKind::VDotProduct { dst, .. } = kind else {
            unreachable!()
        };
        *dst = vector(8, case.width);
    });
    assert_dot_mutation_rejected("VNNI accumulator", &function, |kind| {
        let OpKind::VDotProduct { acc, .. } = kind else {
            unreachable!()
        };
        *acc = vector(8, case.width);
    });
    assert_dot_mutation_rejected("VNNI source1", &function, |kind| {
        let OpKind::VDotProduct { src1, .. } = kind else {
            unreachable!()
        };
        *src1 = vector(11, case.width);
    });
    assert_dot_mutation_rejected("VNNI staged source", &function, |kind| {
        let OpKind::VDotProduct { src2, .. } = kind else {
            unreachable!()
        };
        *src2 = vector(12, case.width);
    });
    assert_dot_mutation_rejected("VNNI opmask", &function, |kind| {
        let OpKind::VDotProduct { mask, .. } = kind else {
            unreachable!()
        };
        *mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(2))));
    });
    assert_dot_mutation_rejected("VNNI source element", &function, |kind| {
        let OpKind::VDotProduct { src_elem, .. } = kind else {
            unreachable!()
        };
        *src_elem = VecElementType::I16;
    });
    assert_dot_mutation_rejected("VNNI accumulator element", &function, |kind| {
        let OpKind::VDotProduct { acc_elem, .. } = kind else {
            unreachable!()
        };
        *acc_elem = VecElementType::I64;
    });
    assert_dot_mutation_rejected("VNNI width", &function, |kind| {
        let OpKind::VDotProduct { width, .. } = kind else {
            unreachable!()
        };
        *width = VecWidth::V128;
    });
    assert_dot_mutation_rejected("VNNI source signedness", &function, |kind| {
        let OpKind::VDotProduct { src1_unsigned, .. } = kind else {
            unreachable!()
        };
        *src1_unsigned = false;
    });
    assert_dot_mutation_rejected("VNNI saturation", &function, |kind| {
        let OpKind::VDotProduct { saturate, .. } = kind else {
            unreachable!()
        };
        *saturate = false;
    });
    assert_dot_mutation_rejected("VNNI zeroing", &function, |kind| {
        let OpKind::VDotProduct { zeroing, .. } = kind else {
            unreachable!()
        };
        *zeroing = false;
    });

    let mut hinted = function.clone();
    let dot = hinted.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VDotProduct { .. }))
        .unwrap();
    dot.x86_hint = Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
    assert_rejected("hinted VNNI dot product", &hinted);

    let mut wrong_map = function.clone();
    let mut bytes = case.bytes();
    bytes[1] = (bytes[1] & !7) | 1;
    wrong_map
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert_rejected("VNNI map provenance", &wrong_map);

    let mut wrong_broadcast_offset = function;
    let pred_load = wrong_broadcast_offset.blocks[0]
        .ops
        .iter_mut()
        .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .nth(1)
        .expect("masked VNNI broadcast repeats a guarded dword load");
    let OpKind::PredLoad {
        addr: Address::BaseOffset { offset, .. },
        ..
    } = &mut pred_load.kind
    else {
        panic!("masked VNNI broadcast address graph changed")
    };
    *offset = 4;
    assert_rejected("VNNI broadcast lane address", &wrong_broadcast_offset);
}

#[test]
fn packed_average_unmasked_load_compute_and_commit_fail_closed_independently() {
    let case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AverageByte,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::None,
        wig_w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    assert_eq!(function.blocks[0].ops.len(), 3);

    let mut wrong_load_hint = function.clone();
    wrong_load_hint.blocks[0].ops[0].x86_hint = None;
    assert_rejected("average load hint", &wrong_load_hint);

    let mut wrong_average = function.clone();
    let OpKind::VLane { op, .. } = &mut wrong_average.blocks[0].ops[1].kind else {
        panic!("average compute graph changed")
    };
    *op = VLaneOp::Avg;
    assert_rejected("truncating average", &wrong_average);

    let mut wrong_commit = function;
    let OpKind::VMov { src, .. } = &mut wrong_commit.blocks[0].ops[2].kind else {
        panic!("average commit graph changed")
    };
    *src = VReg::Virtual(VirtualId(0x7FFF));
    assert_rejected("average commit source", &wrong_commit);
}

#[test]
fn integer_arithmetic_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AddWrappingDword,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
        wig_w: false,
    };
    let broadcast_case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::DotByteSaturating,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
        wig_w: false,
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

    for (case, expected_address) in [
        (
            IntegerArithmeticMemoryCase {
                kind: ArithmeticKind::SubUnsignedSaturatingByte,
                width: VecWidth::V512,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                wig_w: true,
            },
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R16)),
                index: x86(X86Reg::R17),
                scale: 2,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        ),
        (
            IntegerArithmeticMemoryCase {
                kind: ArithmeticKind::DotWordSaturating,
                width: VecWidth::V512,
                destination: 25,
                source1: 26,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                wig_w: false,
            },
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R20)),
                index: x86(X86Reg::R21),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        ),
    ] {
        let mut bytes = memory_encoding(case, true);
        bytes[1] |= 0x08;
        bytes[2] &= !0x04;
        if case.destination == 25 {
            bytes[6] = 0xEC;
        }
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                matches!(
                    function.blocks[0].ops.first().map(|op| &op.kind),
                    Some(OpKind::X86RequireApx)
                ),
                "{level:?} {bytes:02X?}: APX address lost its dynamic guard"
            );
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. }
                    | OpKind::VLoad { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::Lea { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
            lower(&function, case);
        }
    }
}

#[test]
fn integer_arithmetic_rejects_the_avx_only_state_bridge() {
    let case = IntegerArithmeticMemoryCase {
        kind: ArithmeticKind::AddUnsignedSaturatingByte,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
        wig_w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX integer arithmetics");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
