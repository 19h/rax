//! Exact helper-backed EVEX VALIGND/Q memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexVectorAlignMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexVectorAlignMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_vector_align_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
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
struct AlignMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    high: u8,
    form: SourceForm,
    control: MaskControl,
    immediate: u8,
}

impl AlignMemoryCase {
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
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.high)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        match self.form {
            SourceForm::Vector => register_encoding(self, self.scratch()),
            SourceForm::Broadcast => stack_encoding(self),
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("VALIGN vector width"),
    }))
}

fn memory_encoding(case: AlignMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.high < 32);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x03
        | 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.elem == VecElementType::I64 {
        0x80
    } else {
        0
    } | (((!case.high) & 0x0F) << 3)
        | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.high & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0x03,
        ((case.destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes.push(case.immediate);
    bytes
}

fn stack_encoding(case: AlignMemoryCase) -> Vec<u8> {
    let p0 = 0x63
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.elem == VecElementType::I64 {
        0x80
    } else {
        0
    } | (((!case.high) & 0x0F) << 3)
        | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | 0x10
        | if case.high & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x03,
        ((case.destination & 7) << 3) | 4,
        0x24,
        case.immediate,
    ]
}

fn register_encoding(case: AlignMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x43
        | if scratch & 8 == 0 { 0x20 } else { 0 }
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.elem == VecElementType::I64 {
        0x80
    } else {
        0
    } | (((!case.high) & 0x0F) << 3)
        | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.high & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x03,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
        case.immediate,
    ]
}

fn lift_case(case: AlignMemoryCase) -> SmirFunction {
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
        X86InstructionBytes::new(&bytes).expect("VALIGN memory provenance"),
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
) -> Option<X86JitEvexVectorAlignMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_vector_align_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: AlignMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: VALIGN memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize helper-backed VALIGN"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<AlignMemoryCase> {
    let mut cases = Vec::new();
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            let lanes = width.lanes(elem) as u8;
            for (destination, high) in [(0, 0), (9, 10), (17, 18)] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        // Every imm8 is reduced modulo the architectural lane
                        // count. Exercise each distinct shift class plus the
                        // maximum encoded byte to cover the reduction boundary.
                        for immediate in (0..lanes).chain([u8::MAX]) {
                            cases.push(AlignMemoryCase {
                                elem,
                                width,
                                destination,
                                high,
                                form,
                                control,
                                immediate,
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
fn valign_rewrites_match_six_independent_llvm_23_anchors() {
    let cases = [
        (
            AlignMemoryCase {
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 1,
                high: 2,
                form: SourceForm::Vector,
                control: MaskControl::None,
                immediate: 7,
            },
            vec![0x62, 0xF3, 0x6D, 0x08, 0x03, 0xC8, 0x07],
        ),
        (
            AlignMemoryCase {
                elem: VecElementType::I64,
                width: VecWidth::V256,
                destination: 9,
                high: 10,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                immediate: 0xFF,
            },
            vec![0x62, 0x73, 0xAD, 0x2B, 0x03, 0xC8, 0xFF],
        ),
        (
            AlignMemoryCase {
                elem: VecElementType::I32,
                width: VecWidth::V512,
                destination: 17,
                high: 18,
                form: SourceForm::Vector,
                control: MaskControl::Zero,
                immediate: 15,
            },
            vec![0x62, 0xE3, 0x6D, 0xC1, 0x03, 0xC8, 0x0F],
        ),
        (
            AlignMemoryCase {
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 1,
                high: 2,
                form: SourceForm::Broadcast,
                control: MaskControl::None,
                immediate: 7,
            },
            vec![0x62, 0xF3, 0x6D, 0x18, 0x03, 0x0C, 0x24, 0x07],
        ),
        (
            AlignMemoryCase {
                elem: VecElementType::I64,
                width: VecWidth::V256,
                destination: 9,
                high: 10,
                form: SourceForm::Broadcast,
                control: MaskControl::Merge,
                immediate: 0xFF,
            },
            vec![0x62, 0x73, 0xAD, 0x3B, 0x03, 0x0C, 0x24, 0xFF],
        ),
        (
            AlignMemoryCase {
                elem: VecElementType::I64,
                width: VecWidth::V512,
                destination: 17,
                high: 18,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                immediate: 15,
            },
            vec![0x62, 0xE3, 0xED, 0xD1, 0x03, 0x0C, 0x24, 0x0F],
        ),
    ];
    for (case, llvm) in cases {
        assert_eq!(case.expected_replay(), llvm, "{case:?}");
    }
}

#[test]
fn valign_memory_classifier_exhausts_737_280_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for high in 0..32u8 {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let case = AlignMemoryCase {
                                    elem,
                                    width,
                                    destination,
                                    high,
                                    form,
                                    control: if mask == 0 {
                                        MaskControl::None
                                    } else if zeroing {
                                        MaskControl::Zero
                                    } else {
                                        MaskControl::Merge
                                    },
                                    immediate: 0xA5,
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
                                            .evex_vector_align_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.high, high, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(encoding.immediate, 0xA5, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            width != VecWidth::V512,
                                            "{bytes:02X?}"
                                        );
                                        match encoding.replay {
                                            X86EvexVectorAlignMemoryReplay::Broadcast {
                                                ..
                                            } => {
                                                assert!(case.broadcast(), "{bytes:02X?}");
                                            }
                                            X86EvexVectorAlignMemoryReplay::Vector {
                                                scratch,
                                                register_instruction,
                                            } => {
                                                assert!(!case.broadcast(), "{bytes:02X?}");
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, high, "{bytes:02X?}");
                                                assert_eq!(
                                                    register_instruction
                                                        .evex_register_vector_align_needs_vl(),
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
    assert_eq!(accepted, 737_280);
}

#[test]
fn valign_memory_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = AlignMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        high: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        immediate: 7,
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
        (4, 0x01), // non-owned opcode
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
                .evex_vector_align_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_vector_align_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );
}

#[test]
fn all_864_valign_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 864);
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
            assert_eq!(exact.encoding.high, case.high, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.immediate, case.immediate,
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
            assert_eq!(exact.address_offset, 0, "{level:?} {case:?}");
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
    assert_eq!(lowerings, 864 * LEVELS.len());
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
fn valign_memory_sequence_fails_closed_for_provenance_and_graph_mutations() {
    for case in [
        AlignMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V128,
            destination: 1,
            high: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
            immediate: 7,
        },
        AlignMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V256,
            destination: 17,
            high: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            immediate: 0xFF,
        },
        AlignMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V512,
            destination: 9,
            high: 10,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            immediate: 15,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(
            sequence(&function, false).is_none(),
            "{case:?}: memory-disabled admission"
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        wrong_provenance.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(
                &AlignMemoryCase {
                    high: case.high ^ 1,
                    immediate: case.immediate.wrapping_add(1),
                    ..case
                }
                .bytes(),
            )
            .unwrap(),
        );
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut wrong_address = function.clone();
        match &mut wrong_address.blocks[0].ops[0].kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!(),
        }
        assert_rejected("virtual address", &wrong_address);

        let mut hinted_memory = function.clone();
        hinted_memory.blocks[0].ops[0].x86_hint =
            Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted memory", &hinted_memory);

        let mut wrong_lane = function.clone();
        let extract = wrong_lane.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .expect("VALIGN lane extraction");
        let OpKind::VExtractLane { lane, .. } = &mut extract.kind else {
            unreachable!()
        };
        *lane = lane.wrapping_add(1);
        assert_rejected("wrong extract lane", &wrong_lane);

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
fn valign_segment_addr32_rip_and_sib_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = AlignMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        high: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
        immediate: 7,
    };
    let broadcast_case = AlignMemoryCase {
        form: SourceForm::Broadcast,
        ..vector_case
    };
    let address_cases = [
        (
            "RIP+disp32",
            vector_case,
            vec![0x62, 0xF3, 0x6D, 0x08, 0x03, 0x0D, 0x20, 0, 0, 0, 0x07],
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 11),
            },
        ),
        (
            "addr32 base",
            vector_case,
            vec![0x67, 0x62, 0xF3, 0x6D, 0x08, 0x03, 0x0B, 0x07],
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS base broadcast",
            broadcast_case,
            vec![0x64, 0x62, 0xF3, 0x6D, 0x18, 0x03, 0x0B, 0x07],
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
            vec![
                0x65, 0x67, 0x62, 0xF3, 0x6D, 0x18, 0x03, 0x4C, 0x8B, 0x02, 0x07,
            ],
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
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let result = lifter
            .lift_insn(PC, &bytes, &mut context)
            .unwrap_or_else(|error| panic!("{name} {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
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
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } =>
                        addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.address_offset, 0, "{name} {level:?}");
            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name} {level:?}: missing {expected:02X?}"
            );
        }
    }
}

#[test]
fn valign_apx_r16_r17_addresses_admit_and_lower_at_every_level() {
    for (bytes, expected_address, expected_form) in [
        (
            vec![0x62, 0xEB, 0x69, 0x20, 0x03, 0x4C, 0x88, 0x01, 0x07],
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                scale: 4,
                disp: 32,
                disp_size: DispSize::Disp8,
            },
            SourceForm::Vector,
        ),
        (
            vec![
                0x62, 0x7B, 0xA9, 0x59, 0x03, 0x8C, 0xEC, 0x30, 0x00, 0x00, 0x00, 0xFF,
            ],
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R20))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R21)),
                scale: 8,
                disp: 48,
                disp_size: DispSize::Disp32,
            },
            SourceForm::Broadcast,
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
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } =>
                        addr == &expected_address,
                    _ => false,
                }),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            let exact =
                sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
            let case = AlignMemoryCase {
                elem: exact.encoding.elem,
                width: exact.encoding.width,
                destination: exact.encoding.destination,
                high: exact.encoding.high,
                form: expected_form,
                control: if exact.encoding.writemask.is_none() {
                    MaskControl::None
                } else if exact.encoding.zeroing {
                    MaskControl::Zero
                } else {
                    MaskControl::Merge
                },
                immediate: exact.encoding.immediate,
            };
            let (code, _) = lower(&function, case);
            let replay = match exact.encoding.replay {
                X86EvexVectorAlignMemoryReplay::Vector {
                    register_instruction,
                    ..
                } => register_instruction,
                X86EvexVectorAlignMemoryReplay::Broadcast { stack_instruction } => {
                    stack_instruction
                }
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
fn valign_rejects_the_avx_only_state_bridge() {
    let case = AlignMemoryCase {
        elem: VecElementType::I64,
        width: VecWidth::V512,
        destination: 17,
        high: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
        immediate: 7,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject AVX-512 replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
