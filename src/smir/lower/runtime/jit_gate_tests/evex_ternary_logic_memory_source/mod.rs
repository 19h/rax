//! Exact helper-backed EVEX VPTERNLOGD/Q memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexTernaryLogicMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexTernaryLogicMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_ternary_logic_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xCE00;
const DISP8: i32 = 2;
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

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Merge | Self::Zero => 1,
        }
    }

    const fn zeroing(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TernaryMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source2: u8,
    form: SourceForm,
    control: MaskControl,
    immediate: u8,
}

impl TernaryMemoryCase {
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
                self.elem.bytes() as i32
            } else {
                self.width.bytes() as i32
            }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, self.mask(), self.zeroing(), false, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source2)
            .expect("two operands leave at least fourteen low scratch registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self, self.mask(), self.zeroing())
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn memory_encoding(
    case: TernaryMemoryCase,
    mask: u8,
    zeroing: bool,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    assert!(case.destination < 32 && case.source2 < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0x63;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    if apx_base {
        p0 |= 0x08;
    }
    let mut p1 =
        (u8::from(case.elem == VecElementType::I64) << 7) | (((!case.source2) & 0x0F) << 3) | 0x05;
    if apx_index {
        p1 &= !0x04;
    }
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (if case.source2 & 16 == 0 { 0x08 } else { 0 })
        | mask;
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x25,
        0x40 | ((case.destination & 7) << 3) | 0x04,
        0x48,
        DISP8 as u8,
        case.immediate,
    ]
}

fn stack_encoding(case: TernaryMemoryCase, mask: u8, zeroing: bool) -> Vec<u8> {
    let mut p0 = 0x63;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 =
        (u8::from(case.elem == VecElementType::I64) << 7) | (((!case.source2) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (if case.source2 & 16 == 0 { 0x08 } else { 0 })
        | mask;
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x25,
        ((case.destination & 7) << 3) | 0x04,
        0x24,
        case.immediate,
    ]
}

fn register_encoding(case: TernaryMemoryCase, scratch: u8) -> Vec<u8> {
    let mut p0 = 0x43;
    if scratch & 8 == 0 {
        p0 |= 0x20;
    }
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 =
        (u8::from(case.elem == VecElementType::I64) << 7) | (((!case.source2) & 0x0F) << 3) | 0x05;
    let p2 = (case.ll() << 5) | (if case.source2 & 16 == 0 { 0x08 } else { 0 });
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x25,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
        case.immediate,
    ]
}

fn replay_instruction(encoding: crate::smir::ir::X86EvexTernaryLogicMemoryEncoding) -> Vec<u8> {
    match encoding.replay {
        X86EvexTernaryLogicMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexTernaryLogicMemoryReplay::Broadcast { stack_instruction }
        | X86EvexTernaryLogicMemoryReplay::MaskedVector { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
        }
    }
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
        X86InstructionBytes::new(bytes).expect("VPTERNLOG encoding fits instruction metadata"),
    );
    function
}

fn lift_case(case: TernaryMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexTernaryLogicMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexTernaryLogicMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_ternary_logic_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: TernaryMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: VPTERNLOG memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VPTERNLOG"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<TernaryMemoryCase> {
    let mut cases = Vec::new();
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source2 in [0, 1, 15] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(TernaryMemoryCase {
                            elem,
                            width,
                            destination: 0,
                            source2,
                            form,
                            control,
                            immediate: match source2 {
                                0 => 0x00,
                                1 => 0x96,
                                _ => 0xFF,
                            },
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn llvm_23_replay_byte_anchors_cover_dword_qword_vector_broadcast_and_masks() {
    let anchors: [(TernaryMemoryCase, &[u8]); 4] = [
        (
            TernaryMemoryCase {
                elem: VecElementType::I32,
                width: VecWidth::V128,
                destination: 0,
                source2: 3,
                form: SourceForm::Vector,
                control: MaskControl::None,
                immediate: 0x96,
            },
            &[0x62, 0xF3, 0x65, 0x08, 0x25, 0xC1, 0x96],
        ),
        (
            TernaryMemoryCase {
                elem: VecElementType::I64,
                width: VecWidth::V256,
                destination: 17,
                source2: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                immediate: 0xE1,
            },
            &[0x62, 0xE3, 0xED, 0x21, 0x25, 0x0C, 0x24, 0xE1],
        ),
        (
            TernaryMemoryCase {
                elem: VecElementType::I32,
                width: VecWidth::V512,
                destination: 20,
                source2: 21,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                immediate: 0x69,
            },
            &[0x62, 0xE3, 0x55, 0xD1, 0x25, 0x24, 0x24, 0x69],
        ),
        (
            TernaryMemoryCase {
                elem: VecElementType::I64,
                width: VecWidth::V512,
                destination: 31,
                source2: 30,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                immediate: 0xFF,
            },
            &[0x62, 0x63, 0x8D, 0xD1, 0x25, 0x3C, 0x24, 0xFF],
        ),
    ];

    for (case, expected) in anchors {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_ternary_logic_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(replay_instruction(encoding), expected, "{case:?}");
    }
}

#[test]
fn ternary_logic_memory_classifier_exhausts_737_280_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source2 in 0..32u8 {
                    for broadcast in [false, true] {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                for apx_base in [false, true] {
                                    for apx_index in [false, true] {
                                        let case = TernaryMemoryCase {
                                            elem,
                                            width,
                                            destination,
                                            source2,
                                            form: if broadcast {
                                                SourceForm::Broadcast
                                            } else {
                                                SourceForm::Vector
                                            },
                                            control: MaskControl::None,
                                            immediate: destination
                                                .wrapping_mul(17)
                                                .wrapping_add(source2.wrapping_mul(29)),
                                        };
                                        let bytes = memory_encoding(
                                            case, mask, zeroing, apx_base, apx_index,
                                        );
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_ternary_logic_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.source2, source2, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.immediate, case.immediate,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            width != VecWidth::V512,
                                            "{bytes:02X?}"
                                        );
                                        match encoding.replay {
                                            X86EvexTernaryLogicMemoryReplay::Broadcast {
                                                ..
                                            } => {
                                                assert!(broadcast, "{bytes:02X?}")
                                            }
                                            X86EvexTernaryLogicMemoryReplay::MaskedVector {
                                                ..
                                            } => {
                                                assert!(!broadcast && mask != 0, "{bytes:02X?}")
                                            }
                                            X86EvexTernaryLogicMemoryReplay::Vector {
                                                scratch,
                                                ..
                                            } => {
                                                assert!(!broadcast && mask == 0, "{bytes:02X?}");
                                                assert_ne!(scratch, destination, "{bytes:02X?}");
                                                assert_ne!(scratch, source2, "{bytes:02X?}");
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
fn all_256_truth_tables_survive_every_semantic_shape_and_rewrite() {
    let mut checked = 0usize;
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    for immediate in u8::MIN..=u8::MAX {
                        let case = TernaryMemoryCase {
                            elem,
                            width,
                            destination: 17,
                            source2: 18,
                            form,
                            control,
                            immediate,
                        };
                        let encoding = X86InstructionBytes::new(&case.bytes())
                            .unwrap()
                            .evex_ternary_logic_memory_encoding()
                            .unwrap_or_else(|| panic!("{case:?}"));
                        assert_eq!(encoding.immediate, immediate, "{case:?}");
                        assert_eq!(
                            replay_instruction(encoding).last(),
                            Some(&immediate),
                            "{case:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 2 * 3 * 2 * 3 * 256);
}

#[test]
fn ternary_logic_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = TernaryMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V128,
        destination: 1,
        source2: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        immediate: 0x96,
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

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_ternary_logic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_ternary_logic_memory_encoding()
            .is_some()
    );
    let mut repeat_prefixed = vec![0xF3];
    repeat_prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&repeat_prefixed)
            .unwrap()
            .evex_ternary_logic_memory_encoding()
            .is_none()
    );
}

#[test]
fn all_108_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = scanner_cases();
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
            assert_eq!(exact.encoding.source2, case.source2, "{level:?} {case:?}");
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
                    case.elem.bytes()
                } else {
                    case.width.bytes()
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
    assert_eq!(lowerings, 108 * LEVELS.len());
}

#[test]
fn masked_broadcasts_use_one_aggregate_predicated_load_at_every_level() {
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = TernaryMemoryCase {
                    elem,
                    width,
                    destination: 17,
                    source2: 18,
                    form: SourceForm::Broadcast,
                    control,
                    immediate: 0x96,
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
fn fs_gs_address_size_rip_relative_and_apx_addresses_remain_helper_only() {
    let case = TernaryMemoryCase {
        elem: VecElementType::I64,
        width: VecWidth::V512,
        destination: 17,
        source2: 18,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
        immediate: 0xE1,
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
                "{level:?} {bytes:02X?}"
            );
        }
    }

    let mut rip = case.bytes();
    let immediate = rip.pop().unwrap();
    rip.truncate(6);
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    rip.push(immediate);
    let expected_rip = Address::PcRel {
        offset: 0x20,
        disp_size: DispSize::Disp32,
        base: Some(PC + rip.len() as u64),
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&rip), level);
        assert!(function.blocks[0].ops.iter().any(|op| match &op.kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::VLoad { addr, .. } => addr == &expected_rip,
            _ => false,
        }));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} RIP-relative"));
        lower(&function, case);
    }

    let apx_bytes = memory_encoding(case, case.mask(), case.zeroing(), true, true);
    let expected_apx = Address::BaseIndexScale {
        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
        scale: 2,
        disp: case.compressed_displacement(),
        disp_size: DispSize::Disp8,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx_bytes), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(
            function.blocks[0].ops.iter().any(
                |op| matches!(&op.kind, OpKind::PredLoad { addr, .. } if addr == &expected_apx)
            )
        );
        assert_eq!(sequence_index(&function), 1, "{level:?}");
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} APX"));
        lower(&function, case);

        let mut missing_guard = function.clone();
        assert!(matches!(
            missing_guard.blocks[0].ops.remove(0).kind,
            OpKind::X86RequireApx
        ));
        assert!(sequence_at(&missing_guard, 0, true).is_none(), "{level:?}");
    }
}

#[test]
fn exact_sequence_rejects_mutated_semantics_provenance_and_extra_same_pc_work() {
    let case = TernaryMemoryCase {
        elem: VecElementType::I64,
        width: VecWidth::V256,
        destination: 17,
        source2: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
        immediate: 0x96,
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
    let mutate_terminal =
        |function: &SmirFunction, mutation: &dyn Fn(&mut crate::smir::ir::ops::OpKind)| {
            let mut changed = function.clone();
            let terminal = changed.blocks[0]
                .ops
                .iter_mut()
                .find(|op| matches!(op.kind, OpKind::X86TernaryLogic { .. }))
                .expect("ternary terminal");
            mutation(&mut terminal.kind);
            changed
        };

    reject(
        "immediate",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86TernaryLogic { imm, .. } = kind {
                *imm ^= 1
            }
        }),
    );
    reject(
        "source2",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86TernaryLogic { src2, .. } = kind {
                *src2 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(19)))
            }
        }),
    );
    reject(
        "src1 alias",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86TernaryLogic { src1, .. } = kind {
                *src1 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(16)))
            }
        }),
    );
    reject(
        "element",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86TernaryLogic { elem, .. } = kind {
                *elem = VecElementType::I32
            }
        }),
    );

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
}
