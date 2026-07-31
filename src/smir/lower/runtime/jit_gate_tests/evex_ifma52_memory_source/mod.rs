//! Exact helper-backed EVEX AVX-512IFMA memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
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

const PC: u64 = 0x7D20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ifma52Kind {
    Low,
    High,
}

impl Ifma52Kind {
    const ALL: [Self; 2] = [Self::Low, Self::High];

    const fn opcode(self) -> u8 {
        match self {
            Self::Low => 0xB4,
            Self::High => 0xB5,
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
struct Ifma52MemoryCase {
    kind: Ifma52Kind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
}

impl Ifma52MemoryCase {
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

    const fn lanes(self) -> usize {
        self.width.lanes(VecElementType::I64) as usize
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding_with_controls(self, false, self.mask(), self.zeroing())
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave one low vector scratch")
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
        _ => unreachable!("EVEX IFMA52 vector width"),
    }))
}

fn memory_encoding_with_controls(
    case: Ifma52MemoryCase,
    sib: bool,
    mask: u8,
    zeroing: bool,
) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(mask < 8 && (!zeroing || mask != 0));
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = 0x85 | (((!case.source1) & 0x0F) << 3);
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | mask;
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 tests independently extend both inputs.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: Ifma52MemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = 0x85 | (((!case.source1) & 0x0F) << 3);
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

fn register_encoding(case: Ifma52MemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x42
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = 0x85 | (((!case.source1) & 0x0F) << 3);
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
        X86InstructionBytes::new(bytes).expect("EVEX IFMA52 provenance"),
    );
    function
}

fn lift_case(case: Ifma52MemoryCase) -> SmirFunction {
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

fn lower(function: &SmirFunction, case: Ifma52MemoryCase) -> (Vec<u8>, usize) {
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
    // AVX_IFMA is the distinct VEX feature. EVEX IFMA52 is checked through
    // the terminal VMultiplyAdd52 operation as AVX-512IFMA.
    assert!(!requirements.needs_avx_ifma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512ifma")
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX IFMA52 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX IFMA52"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<Ifma52MemoryCase> {
    let mut cases = Vec::new();
    for kind in Ifma52Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (destination, source1) in [(0, 0), (9, 10), (17, 18)] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(Ifma52MemoryCase {
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
fn ifma52_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0xED, 0x08, 0xB4, 0x0A],
            &[0x62, 0xF2, 0xED, 0x08, 0xB4, 0xC8],
        ),
        (
            &[0x62, 0x72, 0xAD, 0x3B, 0xB5, 0x0A],
            &[0x62, 0x72, 0xAD, 0x3B, 0xB5, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE2, 0xED, 0xC1, 0xB4, 0x0A],
            &[0x62, 0xE2, 0xED, 0xC1, 0xB4, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF2, 0xDD, 0x18, 0xB5, 0x1A],
            &[0x62, 0xF2, 0xDD, 0x18, 0xB5, 0x1C, 0x24],
        ),
        (
            &[0x62, 0x72, 0xAD, 0x2D, 0xB4, 0x0A],
            &[0x62, 0x72, 0xAD, 0x2D, 0xB4, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0xAD, 0x40, 0xB5, 0x0A],
            &[0x62, 0x62, 0xAD, 0x40, 0xB5, 0xC8],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_arithmetic_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert!(encoding.is_ifma52(), "{memory:02X?}");
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
fn ifma52_memory_classifier_exhausts_737_280_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in Ifma52Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let case = Ifma52MemoryCase {
                                    kind,
                                    width,
                                    destination,
                                    source1,
                                    form,
                                    control: MaskControl::None,
                                };
                                let canonical =
                                    memory_encoding_with_controls(case, true, mask, zeroing);
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
                                        assert!(encoding.is_ifma52(), "{bytes:02X?}");
                                        assert_eq!(encoding.opcode, kind.opcode(), "{bytes:02X?}");
                                        assert_eq!(encoding.w, true, "{bytes:02X?}");
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.elem,
                                            VecElementType::I64,
                                            "{bytes:02X?}"
                                        );
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
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, source1, "{bytes:02X?}");
                                                assert_eq!(
                                                    register_instruction
                                                        .evex_register_ifma52_needs_vl(),
                                                    Some(width != VecWidth::V512),
                                                    "{bytes:02X?}"
                                                );
                                            }
                                            X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                                                ..
                                            } => assert_eq!(form, SourceForm::Broadcast),
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
    assert_eq!(accepted, 737_280);
}

#[test]
fn ifma52_register_classifier_exhausts_2_949_120_legal_cells() {
    let mut accepted = 0usize;
    for opcode in [0xB4, 0xB5] {
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
                                let p1 = 0x85 | (encoded_vvvv << 3);
                                let p2 = (u8::from(zeroing) << 7)
                                    | (ll << 5)
                                    | (u8::from(encoded_v_prime) << 3)
                                    | mask;
                                for modrm in 0xC0u8..=0xFF {
                                    let bytes = [0x62, p0, p1, p2, opcode, modrm];
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_ifma52_needs_vl(),
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
    assert_eq!(accepted, 2_949_120);

    let valid = [0x62, 0xF2, 0xED, 0x08, 0xB4, 0xC8];
    let mut malformed = Vec::new();
    for (index, xor) in [(1, 0x01), (2, 0x01), (4, 0x02)] {
        let mut bytes = valid.to_vec();
        bytes[index] ^= xor;
        malformed.push(bytes);
    }
    for (index, clear) in [(2, 0x80), (2, 0x04)] {
        let mut bytes = valid.to_vec();
        bytes[index] &= !clear;
        malformed.push(bytes);
    }
    let mut embedded_control = valid.to_vec();
    embedded_control[3] |= 0x10;
    malformed.push(embedded_control);
    let mut reserved_ll = valid.to_vec();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.to_vec();
    zero_k0[3] |= 0x80;
    malformed.push(zero_k0);
    let mut memory = valid.to_vec();
    memory[5] &= 0x3F;
    malformed.push(memory);
    let mut trailing = valid.to_vec();
    trailing.push(0);
    malformed.push(trailing);
    malformed.push(valid[..5].to_vec());
    let mut legacy = valid.to_vec();
    legacy.insert(0, 0x66);
    malformed.push(legacy);

    for bytes in malformed {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_ifma52_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn ifma52_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = Ifma52MemoryCase {
        kind: Ifma52Kind::Low,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
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
        (2, 0x80), // W=1
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
}

#[test]
fn all_108_ifma52_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert!(exact.encoding.is_ifma52());
            assert_eq!(exact.encoding.opcode, case.kind.opcode());
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, VecElementType::I64);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert!(exact.encoding.w);
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    MemWidth::B8.bytes()
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
    assert_eq!(lowerings, 108 * LEVELS.len());
}

#[test]
fn ifma52_type_e4_graphs_preserve_vector_lane_and_single_broadcast_accesses() {
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
                match (case.control, case.form) {
                    (MaskControl::None, _) => (1, 0),
                    (_, SourceForm::Broadcast) => (0, 1),
                    (_, SourceForm::Vector) => (0, case.lanes()),
                },
                "{level:?} {case:?}: {:#?}",
                function.blocks[0].ops
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

fn assert_terminal_mutation_rejected(
    name: &str,
    function: &SmirFunction,
    mutation: impl FnOnce(&mut crate::smir::ir::ops::OpKind),
) {
    let mut malformed = function.clone();
    let operation = malformed.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VMultiplyAdd52 { .. }))
        .expect("IFMA52 lift owns one terminal operation");
    mutation(&mut operation.kind);
    assert_rejected(name, &malformed);
}

#[test]
fn ifma52_sequence_fails_closed_for_provenance_graph_and_every_semantic_axis() {
    let case = Ifma52MemoryCase {
        kind: Ifma52Kind::High,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    assert!(sequence(&function, false).is_none());

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    let mut wrong_provenance = function.clone();
    let mut bytes = case.bytes();
    bytes[4] = 0xB4;
    wrong_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert_rejected("wrong high/low provenance", &wrong_provenance);

    let mut wrong_address = function.clone();
    let memory_op = wrong_address.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let OpKind::PredLoad { addr, .. } = &mut memory_op.kind else {
        unreachable!()
    };
    *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
    assert_rejected("virtual address", &wrong_address);

    let mut wrong_width = function.clone();
    let pred_load = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let OpKind::PredLoad { width, .. } = &mut pred_load.kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    assert_rejected("wrong broadcast memory width", &wrong_width);

    assert_terminal_mutation_rejected("destination", &function, |kind| {
        let OpKind::VMultiplyAdd52 { dst, .. } = kind else {
            unreachable!()
        };
        *dst = vector(8, case.width);
    });
    assert_terminal_mutation_rejected("accumulator", &function, |kind| {
        let OpKind::VMultiplyAdd52 { acc, .. } = kind else {
            unreachable!()
        };
        *acc = vector(8, case.width);
    });
    assert_terminal_mutation_rejected("source1", &function, |kind| {
        let OpKind::VMultiplyAdd52 { src1, .. } = kind else {
            unreachable!()
        };
        *src1 = vector(11, case.width);
    });
    assert_terminal_mutation_rejected("staged source", &function, |kind| {
        let OpKind::VMultiplyAdd52 { src2, .. } = kind else {
            unreachable!()
        };
        *src2 = vector(12, case.width);
    });
    assert_terminal_mutation_rejected("opmask", &function, |kind| {
        let OpKind::VMultiplyAdd52 { mask, .. } = kind else {
            unreachable!()
        };
        *mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(2))));
    });
    assert_terminal_mutation_rejected("width", &function, |kind| {
        let OpKind::VMultiplyAdd52 { width, .. } = kind else {
            unreachable!()
        };
        *width = VecWidth::V128;
    });
    assert_terminal_mutation_rejected("high/low", &function, |kind| {
        let OpKind::VMultiplyAdd52 { high, .. } = kind else {
            unreachable!()
        };
        *high = false;
    });
    assert_terminal_mutation_rejected("zeroing", &function, |kind| {
        let OpKind::VMultiplyAdd52 { zeroing, .. } = kind else {
            unreachable!()
        };
        *zeroing = false;
    });

    let mut hinted = function.clone();
    let operation = hinted.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VMultiplyAdd52 { .. }))
        .unwrap();
    operation.x86_hint = Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
    assert_rejected("hinted IFMA52", &hinted);

    let mut tail = function;
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

#[test]
fn ifma52_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = Ifma52MemoryCase {
        kind: Ifma52Kind::Low,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    let broadcast_case = Ifma52MemoryCase {
        kind: Ifma52Kind::High,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
    };

    let mut rip = vector_case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = vector_case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast_case.bytes();
    fs.insert(0, 0x64);
    let addresses = [
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
            "addr32 RDX",
            vector_case,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS broadcast",
            broadcast_case,
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
    ];
    for (name, case, bytes, expected_address) in addresses {
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

    for case in [vector_case, broadcast_case] {
        let mut bytes = memory_encoding_with_controls(case, true, case.mask(), case.zeroing());
        bytes[1] |= 0x08; // APX B4 extends SIB base RAX to R16.
        bytes[2] &= !0x04; // APX X4 extends SIB index RCX to R17.
        let expected_address = Address::BaseIndexScale {
            base: Some(x86(X86Reg::R16)),
            index: x86(X86Reg::R17),
            scale: 2,
            disp: 0,
            disp_size: DispSize::Auto,
        };
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
fn ifma52_rejects_the_avx_only_state_bridge() {
    let case = Ifma52MemoryCase {
        kind: Ifma52Kind::Low,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(
        lowerer.lower_function(&function).is_err(),
        "AVX-only bridge admitted EVEX IFMA52 state"
    );
}
