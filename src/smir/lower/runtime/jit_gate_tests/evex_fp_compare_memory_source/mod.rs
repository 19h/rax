//! Exact helper-backed EVEX VCMPPH/PS/PD memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedFpCompareMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedFpCompareMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_fp_compare_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xC2E2;
const DISP8: i32 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Masked,
}

impl MaskControl {
    const ALL: [Self; 2] = [Self::None, Self::Masked];

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Masked => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FpCompareMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    predicate: u8,
}

impl FpCompareMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn mask(self) -> u8 {
        self.control.mask()
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn memory_size(self) -> u32 {
        if self.broadcast() {
            self.elem.bytes()
        } else {
            self.width.bytes()
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, self.mask(), false, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.source1)
            .expect("one vector source leaves at least fifteen low scratch registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self, self.mask())
        } else {
            register_encoding(self, self.scratch(), 0)
        }
    }
}

fn element_fields(elem: VecElementType) -> (u8, u8, u8) {
    match elem {
        VecElementType::F16 => (3, 0, 0),
        VecElementType::F32 => (1, 0, 0),
        VecElementType::F64 => (1, 0x80, 1),
        _ => panic!("packed EVEX comparison uses binary16/32/64"),
    }
}

fn evex_fields(case: FpCompareMemoryCase, mask: u8) -> (u8, u8, u8) {
    assert!(case.destination < 8 && case.source1 < 32 && mask < 8);
    assert!(case.predicate < 32);
    let (map, w, pp) = element_fields(case.elem);
    (
        0xF0 | map,
        w | (((!case.source1) & 0x0F) << 3) | 0x04 | pp,
        (case.ll() << 5)
            | (u8::from(case.broadcast()) << 4)
            | (if case.source1 < 16 { 0x08 } else { 0 })
            | mask,
    )
}

fn memory_encoding(
    case: FpCompareMemoryCase,
    mask: u8,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    let (mut p0, mut p1, p2) = evex_fields(case, mask);
    if !apx_base && !apx_index {
        return vec![
            0x62,
            p0,
            p1,
            p2,
            0xC2,
            (case.destination << 3) | 3,
            case.predicate,
        ];
    }
    if apx_base {
        p0 |= 0x08;
    }
    if apx_index {
        p1 &= !0x04;
    }
    vec![
        0x62,
        p0,
        p1,
        p2,
        0xC2,
        0x40 | (case.destination << 3) | 0x04,
        0x48,
        DISP8 as u8,
        case.predicate,
    ]
}

fn stack_encoding(case: FpCompareMemoryCase, mask: u8) -> Vec<u8> {
    let (p0, p1, p2) = evex_fields(case, mask);
    vec![
        0x62,
        p0,
        p1,
        p2,
        0xC2,
        (case.destination << 3) | 0x04,
        0x24,
        case.predicate,
    ]
}

fn register_encoding(case: FpCompareMemoryCase, source2: u8, mask: u8) -> Vec<u8> {
    assert!(source2 < 32);
    let (map, _, _) = element_fields(case.elem);
    let (_, p1, mut p2) = evex_fields(case, mask);
    p2 &= !0x10;
    vec![
        0x62,
        0x90 | map | if source2 < 16 { 0x40 } else { 0 } | if source2 & 8 == 0 { 0x20 } else { 0 },
        p1,
        p2,
        0xC2,
        0xC0 | (case.destination << 3) | (source2 & 7),
        case.predicate,
    ]
}

fn replay_instruction(encoding: crate::smir::ir::X86EvexPackedFpCompareMemoryEncoding) -> Vec<u8> {
    match encoding.replay {
        X86EvexPackedFpCompareMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexPackedFpCompareMemoryReplay::Broadcast { stack_instruction }
        | X86EvexPackedFpCompareMemoryReplay::MaskedVector { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("packed comparison width"),
    }))
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
        X86InstructionBytes::new(bytes).expect("packed comparison instruction metadata"),
    );
    function
}

fn lift_case(case: FpCompareMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexPackedFpCompareMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexPackedFpCompareMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_fp_compare_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: FpCompareMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(
        requirements.needs_avx512fp16,
        case.elem == VecElementType::F16,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
            && (case.elem != VecElementType::F16 || std::is_x86_feature_detected!("avx512fp16")),
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
        .unwrap_or_else(|error| panic!("{case:?}: packed comparison memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed comparison memory"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FpCompareMemoryCase> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (destination, source1, predicate) in [(0, 0, 0), (3, 17, 19), (7, 31, 31)] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(FpCompareMemoryCase {
                            elem,
                            width,
                            destination,
                            source1,
                            form,
                            control,
                            predicate,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn llvm_23_byte_anchors_cover_all_formats_widths_broadcasts_and_masks() {
    let anchors = [
        (
            FpCompareMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V128,
                destination: 1,
                source1: 3,
                form: SourceForm::Vector,
                control: MaskControl::None,
                predicate: 0x1B,
            },
            vec![0x62, 0xF1, 0x64, 0x08, 0xC2, 0x0C, 0x24, 0x1B],
        ),
        (
            FpCompareMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V128,
                destination: 1,
                source1: 3,
                form: SourceForm::Broadcast,
                control: MaskControl::None,
                predicate: 0x1B,
            },
            vec![0x62, 0xF1, 0x64, 0x18, 0xC2, 0x0C, 0x24, 0x1B],
        ),
        (
            FpCompareMemoryCase {
                elem: VecElementType::F64,
                width: VecWidth::V256,
                destination: 3,
                source1: 5,
                form: SourceForm::Broadcast,
                control: MaskControl::Masked,
                predicate: 0x1F,
            },
            vec![0x62, 0xF1, 0xD5, 0x39, 0xC2, 0x1C, 0x24, 0x1F],
        ),
        (
            FpCompareMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V512,
                destination: 5,
                source1: 7,
                form: SourceForm::Broadcast,
                control: MaskControl::Masked,
                predicate: 0,
            },
            vec![0x62, 0xF3, 0x44, 0x59, 0xC2, 0x2C, 0x24, 0x00],
        ),
    ];
    for (case, llvm) in anchors {
        assert_eq!(stack_encoding(case, case.mask()), llvm, "{case:?}");
    }

    let register = FpCompareMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V256,
        destination: 3,
        source1: 5,
        form: SourceForm::Vector,
        control: MaskControl::None,
        predicate: 0x1F,
    };
    assert_eq!(
        register_encoding(register, 0, 0),
        [0x62, 0xF1, 0xD5, 0x28, 0xC2, 0xD8, 0x1F]
    );
}

#[test]
fn packed_compare_classifier_exhausts_4_718_592_semantic_and_apx_address_cells() {
    let mut accepted = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for (ll, width) in [
            (0, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            for broadcast in [false, true] {
                for destination in 0..8u8 {
                    for source1 in 0..32u8 {
                        for mask in 0..8u8 {
                            for predicate in 0..32u8 {
                                let case = FpCompareMemoryCase {
                                    elem,
                                    width,
                                    destination,
                                    source1,
                                    form: if broadcast {
                                        SourceForm::Broadcast
                                    } else {
                                        SourceForm::Vector
                                    },
                                    control: MaskControl::None,
                                    predicate,
                                };
                                for apx_base in [false, true] {
                                    for apx_index in [false, true] {
                                        let bytes =
                                            memory_encoding(case, mask, apx_base, apx_index);
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_packed_fp_compare_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, elem, "{bytes:02X?}");
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
                                        assert_eq!(encoding.predicate, predicate, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            ll != 2,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512fp16,
                                            elem == VecElementType::F16,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            replay_instruction(encoding),
                                            if broadcast || mask != 0 {
                                                stack_encoding(case, mask)
                                            } else {
                                                register_encoding(case, case.scratch(), mask)
                                            },
                                            "{bytes:02X?}"
                                        );
                                        match encoding.replay {
                                            X86EvexPackedFpCompareMemoryReplay::Broadcast {
                                                ..
                                            } => {
                                                assert!(broadcast, "{bytes:02X?}")
                                            }
                                            X86EvexPackedFpCompareMemoryReplay::MaskedVector {
                                                ..
                                            } => assert!(!broadcast && mask != 0, "{bytes:02X?}"),
                                            X86EvexPackedFpCompareMemoryReplay::Vector {
                                                scratch,
                                                ..
                                            } => {
                                                assert!(!broadcast && mask == 0, "{bytes:02X?}");
                                                assert_eq!(scratch, case.scratch(), "{bytes:02X?}");
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
    assert_eq!(accepted, 4_718_592);
}

#[test]
fn packed_compare_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = FpCompareMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        destination: 3,
        source1: 17,
        form: SourceForm::Vector,
        control: MaskControl::Masked,
        predicate: 19,
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
        (1, 0x80), // noncanonical K destination R
        (1, 0x10), // noncanonical K destination R'
        (1, 0x04), // non-owned map
        (2, 0x80), // wrong W for FP32
        (2, 0x01), // wrong mandatory prefix
        (4, 0x01), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    let mut reserved_z = valid.clone();
    reserved_z[3] |= 0x80;
    malformed.push(reserved_z);
    let mut reserved_predicate = valid.clone();
    *reserved_predicate.last_mut().unwrap() = 0x20;
    malformed.push(reserved_predicate);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fp_compare_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_packed_fp_compare_memory_encoding()
        .expect("FS/address-size prefixes belong only to helper address evaluation");
    assert_eq!(replay_instruction(encoding), stack_encoding(case, 1));
}

#[test]
fn packed_compare_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VCMPPS k1{k1},xmm17,[r16+r17*2+16],27. Disp8=1 is scaled by
    // the 16-byte full-vector tuple. APX B4/X4 affect only helper addressing.
    let bytes = [0x62, 0xF9, 0x70, 0x01, 0xC2, 0x4C, 0x48, 0x01, 0x1B];
    let base = lift_bytes(&bytes);
    let case = FpCompareMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        destination: 1,
        source1: 17,
        form: SourceForm::Vector,
        control: MaskControl::Masked,
        predicate: 0x1B,
    };
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp: 16,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                }
            )),
            "{level:?}: {:#?}",
            function.blocks[0].ops
        );
        let exact = sequence(&function, true).expect("APX-address comparison sequence");
        assert_eq!(exact.address_offset, 2, "{level:?}");
        assert_eq!(exact.encoding.destination, 1, "{level:?}");
        assert_eq!(exact.encoding.source1, 17, "{level:?}");
        let expected = [0x62, 0xF1, 0x74, 0x01, 0xC2, 0x0C, 0x24, 0x1B];
        assert_eq!(replay_instruction(exact.encoding), expected, "{level:?}");
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected)
        );

        let mut missing_guard = function.clone();
        missing_guard.blocks[0].ops.remove(0);
        assert!(sequence(&missing_guard, true).is_none(), "{level:?}");
    }
}

#[test]
fn all_108_packed_compare_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
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
            assert_eq!(exact.encoding.source1, case.source1, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.encoding.predicate, case.predicate,
                "{level:?} {case:?}"
            );
            assert_eq!(exact.memory_size, case.memory_size(), "{level:?} {case:?}");
            assert_eq!(
                exact.address_offset,
                match (case.form, case.control) {
                    (SourceForm::Vector, MaskControl::None)
                    | (SourceForm::Broadcast, MaskControl::None) => 0,
                    (SourceForm::Vector, MaskControl::Masked) => 2,
                    (SourceForm::Broadcast, MaskControl::Masked) => 5,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");

            let pred_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count();
            assert_eq!(
                pred_loads,
                match (case.form, case.control) {
                    (_, MaskControl::None) => 0,
                    (SourceForm::Vector, MaskControl::Masked) => {
                        case.width.lanes(case.elem) as usize
                    }
                    (SourceForm::Broadcast, MaskControl::Masked) => 1,
                },
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 108 * LEVELS.len());
}

#[test]
fn masked_vector_lowering_stages_all_element_widths_and_rejects_avx_only_bridge() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        let case = FpCompareMemoryCase {
            elem,
            width: VecWidth::V512,
            destination: 7,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Masked,
            predicate: 31,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
        let release_frame = [0x48, 0x8D, 0x64, 0x24, 0x50];
        assert_eq!(
            code.windows(allocate_frame.len())
                .filter(|window| *window == allocate_frame)
                .count(),
            1,
            "{elem:?}"
        );
        assert_eq!(
            code.windows(release_frame.len())
                .filter(|window| *window == release_frame)
                .count(),
            case.width.lanes(elem) as usize + 1,
            "{elem:?}"
        );

        let mut avx_only = X86_64Lowerer::new();
        avx_only.set_mem_helpers(true);
        avx_only.set_preserve_vector_mem_helpers(true);
        avx_only.set_avx_ymm16_vector_state(true);
        let error = avx_only
            .lower_function(&function)
            .expect_err("AVX-only state bridge must reject AVX-512 comparison replay");
        assert!(format!("{error:?}").contains("AVX-only vector bridge"));
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
fn packed_compare_sequence_fails_closed_for_provenance_graph_and_ssa_mutations() {
    let cases = [
        FpCompareMemoryCase {
            elem: VecElementType::F16,
            width: VecWidth::V512,
            destination: 7,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Masked,
            predicate: 31,
        },
        FpCompareMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            destination: 3,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
            predicate: 19,
        },
        FpCompareMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V256,
            destination: 5,
            source1: 31,
            form: SourceForm::Broadcast,
            control: MaskControl::Masked,
            predicate: 5,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);

        let mut missing = function.clone();
        missing.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        *bytes.last_mut().unwrap() ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong predicate provenance", &wrong_provenance);

        let compare_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86VectorFpCompare { .. }))
            .unwrap();

        let mutate_compare =
            |name: &str, mut function: SmirFunction, mutation: &dyn Fn(&mut SmirOp)| {
                mutation(&mut function.blocks[0].ops[compare_index]);
                assert_rejected(name, &function);
            };
        mutate_compare("wrong destination", function.clone(), &|op| {
            let OpKind::X86VectorFpCompare { dst, .. } = &mut op.kind else {
                unreachable!()
            };
            *dst = VReg::Arch(ArchReg::X86(X86Reg::K((case.destination + 1) & 7)));
        });
        mutate_compare("wrong source1", function.clone(), &|op| {
            let OpKind::X86VectorFpCompare { src1, .. } = &mut op.kind else {
                unreachable!()
            };
            *src1 = vector(case.source1 ^ 1, case.width);
        });
        mutate_compare("wrong writemask", function.clone(), &|op| {
            let OpKind::X86VectorFpCompare { mask, .. } = &mut op.kind else {
                unreachable!()
            };
            *mask = if case.mask() == 0 {
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))))
            } else {
                None
            };
        });
        mutate_compare("wrong predicate", function.clone(), &|op| {
            let OpKind::X86VectorFpCompare { predicate, .. } = &mut op.kind else {
                unreachable!()
            };
            *predicate ^= 1;
        });
        mutate_compare("suppressed exceptions", function.clone(), &|op| {
            let OpKind::X86VectorFpCompare {
                suppress_exceptions,
                ..
            } = &mut op.kind
            else {
                unreachable!()
            };
            *suppress_exceptions = true;
        });
        mutate_compare("wrong compare hint", function.clone(), &|op| {
            op.x86_hint = Some(X86OpHint::MovImmModRm);
        });

        let mut wrong_memory_hint = function.clone();
        let address_index = sequence(&wrong_memory_hint, true).unwrap().address_offset;
        wrong_memory_hint.blocks[0].ops[address_index].x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("wrong memory hint", &wrong_memory_hint);

        let memory_source = match function.blocks[0].ops[compare_index].kind {
            OpKind::X86VectorFpCompare { src2, .. } => src2,
            _ => unreachable!(),
        };
        let mut extra_use = function.clone();
        extra_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Reg(memory_source),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("extra SSA use", &extra_use);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &same_pc_tail);
    }
}
