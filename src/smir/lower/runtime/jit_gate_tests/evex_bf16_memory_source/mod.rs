//! Exact helper-backed EVEX AVX512_BF16 memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexBf16MemoryKind, X86EvexBf16MemoryReplay,
    X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexBf16MemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_bf16_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7D20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bf16Kind {
    ConvertOne,
    ConvertTwo,
    DotProduct,
}

impl Bf16Kind {
    const ALL: [Self; 3] = [Self::ConvertOne, Self::ConvertTwo, Self::DotProduct];

    const fn opcode(self) -> u8 {
        match self {
            Self::ConvertOne | Self::ConvertTwo => 0x72,
            Self::DotProduct => 0x52,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::ConvertOne => 2,
            Self::ConvertTwo => 3,
            Self::DotProduct => 2,
        }
    }

    const fn classified(self) -> X86EvexBf16MemoryKind {
        match self {
            Self::ConvertOne => X86EvexBf16MemoryKind::ConvertOne,
            Self::ConvertTwo => X86EvexBf16MemoryKind::ConvertTwo,
            Self::DotProduct => X86EvexBf16MemoryKind::DotProduct,
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
            Self::Merge => (3, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bf16MemoryCase {
    kind: Bf16Kind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
}

impl Bf16MemoryCase {
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

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || (self.kind != Bf16Kind::ConvertTwo && self.mask() != 0) {
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
        _ => unreachable!("AVX512_BF16 vector width"),
    }))
}

fn memory_encoding(case: Bf16MemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(case.kind != Bf16Kind::ConvertOne || case.source1 == 0);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x02
        | 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x04 | case.kind.pp();
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

fn stack_encoding(case: Bf16MemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x04 | case.kind.pp();
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

fn register_encoding(case: Bf16MemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x42
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x04 | case.kind.pp();
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
        X86InstructionBytes::new(bytes).expect("AVX512_BF16 provenance"),
    );
    function
}

fn lift_case(case: Bf16MemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexBf16MemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_bf16_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: Bf16MemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx512bf16, "{case:?}");
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
            && std::is_x86_feature_detected!("avx512bf16")
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
        .unwrap_or_else(|error| panic!("{case:?}: AVX512_BF16 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed AVX512_BF16"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<Bf16MemoryCase> {
    let mut cases = Vec::new();
    for kind in Bf16Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in [0, 9, 17] {
                let source1 = if kind == Bf16Kind::ConvertOne {
                    0
                } else {
                    destination + u8::from(destination != 0)
                };
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(Bf16MemoryCase {
                            kind,
                            width,
                            destination,
                            source1,
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
fn bf16_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0x7E, 0x08, 0x72, 0x0A],
            &[0x62, 0xF2, 0x7E, 0x08, 0x72, 0xCA],
        ),
        (
            &[0x62, 0x62, 0x7E, 0xDA, 0x72, 0x0A],
            &[0x62, 0x62, 0x7E, 0xDA, 0x72, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0x6F, 0x08, 0x72, 0x0A],
            &[0x62, 0xF2, 0x6F, 0x08, 0x72, 0xC8],
        ),
        (
            &[0x62, 0x72, 0x2F, 0xBB, 0x72, 0x0A],
            &[0x62, 0x72, 0x2F, 0xBB, 0x72, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0x6E, 0x41, 0x52, 0x0A],
            &[0x62, 0xE2, 0x6E, 0x41, 0x52, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0x2E, 0xD2, 0x52, 0x0A],
            &[0x62, 0x62, 0x2E, 0xD2, 0x52, 0x0C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_bf16_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexBf16MemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexBf16MemoryReplay::Broadcast { stack_instruction }
            | X86EvexBf16MemoryReplay::MaskedVector { stack_instruction } => stack_instruction,
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn bf16_classifier_exhausts_748_800_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in Bf16Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    if kind == Bf16Kind::ConvertOne && source1 != 0 {
                        continue;
                    }
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let case = Bf16MemoryCase {
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
                                            .evex_bf16_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(
                                            encoding.kind,
                                            kind.classified(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
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
                                            X86EvexBf16MemoryReplay::Vector { scratch, .. } => {
                                                assert_eq!(form, SourceForm::Vector);
                                                assert!(
                                                    kind == Bf16Kind::ConvertTwo || mask == 0,
                                                    "{bytes:02X?}"
                                                );
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, source1, "{bytes:02X?}");
                                            }
                                            X86EvexBf16MemoryReplay::Broadcast { .. } => {
                                                assert_eq!(form, SourceForm::Broadcast);
                                            }
                                            X86EvexBf16MemoryReplay::MaskedVector { .. } => {
                                                assert_ne!(kind, Bf16Kind::ConvertTwo);
                                                assert_eq!(form, SourceForm::Vector);
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
            }
        }
    }
    assert_eq!(accepted, 748_800);
}

#[test]
fn bf16_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = Bf16MemoryCase {
        kind: Bf16Kind::ConvertTwo,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
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
        (1, 0x01), // map
        (2, 0x80), // W
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
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_bf16_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_bf16_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );

    let single = Bf16MemoryCase {
        kind: Bf16Kind::ConvertOne,
        source1: 0,
        ..case
    }
    .bytes();
    for (index, mask) in [
        (2, 0x08), // reserved EVEX.vvvv
        (3, 0x08), // reserved EVEX.V'
    ] {
        let mut bytes = single.clone();
        bytes[index] ^= mask;
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_bf16_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_162_bf16_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 162);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.kind, case.kind.classified());
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    MemWidth::B4.bytes()
                } else {
                    case.width.bytes()
                }
            );
            let guard = usize::from(matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ));
            assert_eq!(exact.consumed + guard, function.blocks[0].ops.len());

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
    assert_eq!(lowerings, 162 * LEVELS.len());
}

#[test]
fn e4_and_e4nf_memory_graphs_preserve_exact_access_granularity() {
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
            let lanes = case.width.lanes(VecElementType::F32) as usize;
            assert_eq!(
                (ordinary_loads, pred_loads),
                match (case.kind, case.control, case.form) {
                    (Bf16Kind::ConvertTwo, _, _) | (_, MaskControl::None, _) => (1, 0),
                    (_, _, SourceForm::Broadcast) => (0, 1),
                    (_, _, SourceForm::Vector) => (0, lanes),
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
fn bf16_sequence_fails_closed_for_provenance_and_graph_mutations() {
    for case in [
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V512,
            destination: 17,
            source1: 0,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V256,
            destination: 9,
            source1: 0,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
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

        let mut wrong_result = function.clone();
        let result = wrong_result.blocks[0].ops.last_mut().unwrap();
        match &mut result.kind {
            OpKind::VCvtFP32ToBF16 { width, .. } | OpKind::VDotProductBF16 { width, .. } => {
                *width = VecWidth::V128;
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong result width", &wrong_result);

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

#[test]
fn bf16_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = Bf16MemoryCase {
        kind: Bf16Kind::ConvertTwo,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let broadcast_case = Bf16MemoryCase {
        kind: Bf16Kind::DotProduct,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
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

    for (case, expected_address) in [
        (
            Bf16MemoryCase {
                kind: Bf16Kind::ConvertTwo,
                width: VecWidth::V512,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
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
            Bf16MemoryCase {
                kind: Bf16Kind::DotProduct,
                width: VecWidth::V512,
                destination: 25,
                source1: 26,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
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
fn bf16_rejects_the_avx_only_state_bridge() {
    let case = Bf16MemoryCase {
        kind: Bf16Kind::DotProduct,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
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
        .expect_err("AVX-only state bridge must reject AVX512_BF16");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
