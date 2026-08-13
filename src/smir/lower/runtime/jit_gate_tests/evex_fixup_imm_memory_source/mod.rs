//! Exact helper-backed EVEX VFIXUPIMM memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, VReg, VecElementType,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexFixupImmMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexFixupImmMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_fixup_imm_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0xF154;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
    Scalar { ll: u8 },
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
struct FixupMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    immediate: u8,
}

impl FixupMemoryCase {
    const fn destination(self) -> u8 {
        0
    }

    const fn scalar(self) -> bool {
        matches!(self.form, SourceForm::Scalar { .. })
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn ll(self) -> u8 {
        match self.form {
            SourceForm::Scalar { ll } => ll,
            SourceForm::Vector | SourceForm::Broadcast => match self.width {
                VecWidth::V128 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
                _ => unreachable!(),
            },
        }
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    const fn memory_width(self) -> crate::smir::ir::types::MemWidth {
        match self.elem {
            VecElementType::F32 => crate::smir::ir::types::MemWidth::B4,
            VecElementType::F64 => crate::smir::ir::types::MemWidth::B8,
            _ => unreachable!(),
        }
    }

    fn needs_avx512vl(self) -> bool {
        !self.scalar() && self.width != VecWidth::V512
    }

    fn bytes(self) -> [u8; 7] {
        memory_encoding(
            self.elem,
            self.scalar(),
            self.destination(),
            self.source1,
            self.ll(),
            self.mask(),
            self.zeroing(),
            self.broadcast(),
            3,
            self.immediate,
        )
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination() && *candidate != self.source1)
            .expect("two operands leave at least fourteen low vector registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.scalar() || self.broadcast() || self.mask() != 0 {
            stack_encoding(
                self.elem,
                self.scalar(),
                self.destination(),
                self.source1,
                self.ll(),
                self.mask(),
                self.zeroing(),
                self.broadcast(),
                self.immediate,
            )
            .to_vec()
        } else {
            register_encoding(
                self.elem,
                false,
                self.destination(),
                self.source1,
                self.scratch(),
                self.ll(),
                0,
                false,
                false,
                self.immediate,
            )
            .to_vec()
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    elem: VecElementType,
    scalar: bool,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    b: bool,
    base: u8,
    immediate: u8,
) -> [u8; 7] {
    assert!(destination < 32 && source1 < 32 && base < 16);
    assert!(ll < 4 && mask < 8 && (!zeroing || mask != 0));
    assert!(scalar || ll < 3);
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x40
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x03,
        (if elem == VecElementType::F64 { 0x80 } else { 0 })
            | (((!source1) & 0x0F) << 3)
            | 0x04
            | 0x01,
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | (u8::from(b) << 4)
            | (if source1 & 16 == 0 { 0x08 } else { 0 })
            | mask,
        if scalar { 0x55 } else { 0x54 },
        ((destination & 7) << 3) | (base & 7),
        immediate,
    ]
}

#[allow(clippy::too_many_arguments)]
fn stack_encoding(
    elem: VecElementType,
    scalar: bool,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    b: bool,
    immediate: u8,
) -> [u8; 8] {
    let mut encoding = memory_encoding(
        elem,
        scalar,
        destination,
        source1,
        ll,
        mask,
        zeroing,
        b,
        4,
        immediate,
    );
    encoding[1] |= 0x20;
    if scalar {
        encoding[3] &= !0x60;
    }
    [
        encoding[0],
        encoding[1],
        encoding[2],
        encoding[3],
        encoding[4],
        encoding[5],
        0x24,
        immediate,
    ]
}

#[allow(clippy::too_many_arguments)]
fn register_encoding(
    elem: VecElementType,
    scalar: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    sae: bool,
    immediate: u8,
) -> [u8; 7] {
    assert!(destination < 32 && source1 < 32 && source2 < 32);
    assert!(scalar || !sae);
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | (if source2 & 16 == 0 { 0x40 } else { 0 })
            | (if source2 & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x03,
        (if elem == VecElementType::F64 { 0x80 } else { 0 })
            | (((!source1) & 0x0F) << 3)
            | 0x04
            | 0x01,
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | (u8::from(sae) << 4)
            | (if source1 & 16 == 0 { 0x08 } else { 0 })
            | mask,
        if scalar { 0x55 } else { 0x54 },
        0xC0 | ((destination & 7) << 3) | (source2 & 7),
        immediate,
    ]
}

fn lift_case(case: FixupMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    lift_bytes(&bytes)
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
        X86InstructionBytes::new(bytes).expect("VFIXUPIMM provenance"),
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexFixupImmMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first(),
        Some(SmirOp {
            kind: OpKind::X86RequireApx,
            ..
        })
    ));
    x86_jit_evex_fixup_imm_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: FixupMemoryCase) -> (Vec<u8>, usize) {
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
        case.needs_avx512vl(),
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512vbmi2, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (!case.needs_avx512vl() || std::is_x86_feature_detected!("avx512vl")),
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
        .unwrap_or_else(|error| panic!("{case:?}: VFIXUPIMM memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VFIXUPIMM memory"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FixupMemoryCase> {
    let mut cases = Vec::new();
    for elem in [VecElementType::F32, VecElementType::F64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source1 in [0, 1, 17] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(FixupMemoryCase {
                            elem,
                            width,
                            source1,
                            form,
                            control,
                            immediate: 0xA5 ^ source1 ^ u8::from(width == VecWidth::V512),
                        });
                    }
                }
            }
        }
        for ll in 0..4 {
            for source1 in [0, 1, 17] {
                for control in MaskControl::ALL {
                    cases.push(FixupMemoryCase {
                        elem,
                        width: VecWidth::V128,
                        source1,
                        form: SourceForm::Scalar { ll },
                        control,
                        immediate: 0x5A ^ source1 ^ ll,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn fixup_memory_classifier_exhaustively_partitions_1_720_320_control_and_apx_address_cells() {
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for elem in [VecElementType::F32, VecElementType::F64] {
        for scalar in [false, true] {
            let ll_values: &[u8] = if scalar { &[0, 1, 2, 3] } else { &[0, 1, 2] };
            for &ll in ll_values {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for b in [false, true] {
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let immediate =
                                        destination.wrapping_mul(17) ^ source1 ^ (ll << 5);
                                    let canonical = memory_encoding(
                                        elem,
                                        scalar,
                                        destination,
                                        source1,
                                        ll,
                                        mask,
                                        zeroing,
                                        b,
                                        3,
                                        immediate,
                                    );
                                    for base_high in [false, true] {
                                        for index_high in [false, true] {
                                            let mut bytes = canonical;
                                            bytes[1] |= u8::from(base_high) << 3;
                                            if index_high {
                                                bytes[2] &= !0x04;
                                            }
                                            let classified = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_fixup_imm_memory_encoding();
                                            if scalar && b {
                                                assert!(classified.is_none(), "{bytes:02X?}");
                                                rejected += 1;
                                                continue;
                                            }
                                            let encoding = classified
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            let width = if scalar {
                                                VecWidth::V128
                                            } else {
                                                [VecWidth::V128, VecWidth::V256, VecWidth::V512]
                                                    [usize::from(ll)]
                                            };
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
                                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.immediate, immediate,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.scalar, scalar, "{bytes:02X?}");
                                            assert!(!encoding.suppress_exceptions, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.needs_avx512vl,
                                                !scalar && ll != 2,
                                                "{bytes:02X?}"
                                            );

                                            let expected_stack = stack_encoding(
                                                elem,
                                                scalar,
                                                destination,
                                                source1,
                                                ll,
                                                mask,
                                                zeroing,
                                                b,
                                                immediate,
                                            );
                                            match encoding.replay {
                                                X86EvexFixupImmMemoryReplay::Scalar {
                                                    stack_instruction,
                                                } => {
                                                    assert!(scalar, "{bytes:02X?}");
                                                    assert_eq!(
                                                        stack_instruction.as_slice(),
                                                        expected_stack,
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexFixupImmMemoryReplay::Broadcast {
                                                    stack_instruction,
                                                } => {
                                                    assert!(!scalar && b, "{bytes:02X?}");
                                                    assert_eq!(
                                                        stack_instruction.as_slice(),
                                                        expected_stack,
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexFixupImmMemoryReplay::MaskedVector {
                                                    stack_instruction,
                                                } => {
                                                    assert!(
                                                        !scalar && !b && mask != 0,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_eq!(
                                                        stack_instruction.as_slice(),
                                                        expected_stack,
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexFixupImmMemoryReplay::Vector {
                                                    scratch,
                                                    register_instruction,
                                                } => {
                                                    assert!(
                                                        !scalar && !b && mask == 0,
                                                        "{bytes:02X?}"
                                                    );
                                                    let expected_scratch = (0..16)
                                                        .find(|candidate| {
                                                            *candidate != destination
                                                                && *candidate != source1
                                                        })
                                                        .unwrap();
                                                    assert_eq!(
                                                        scratch, expected_scratch,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_eq!(
                                                        register_instruction.as_slice(),
                                                        register_encoding(
                                                            elem,
                                                            false,
                                                            destination,
                                                            source1,
                                                            scratch,
                                                            ll,
                                                            0,
                                                            false,
                                                            false,
                                                            immediate,
                                                        ),
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
    assert_eq!(accepted, 1_228_800);
    assert_eq!(rejected, 491_520);

    for immediate in 0..=u8::MAX {
        let bytes = memory_encoding(
            VecElementType::F32,
            false,
            0,
            1,
            0,
            0,
            false,
            false,
            3,
            immediate,
        );
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_fixup_imm_memory_encoding()
            .unwrap();
        assert_eq!(encoding.immediate, immediate);
    }
}

#[test]
fn fixup_encodings_match_four_independent_llvm_23_anchors() {
    for (actual, llvm) in [
        (
            memory_encoding(
                VecElementType::F32,
                false,
                0,
                1,
                0,
                0,
                false,
                false,
                3,
                0xA5,
            )
            .to_vec(),
            vec![0x62, 0xF3, 0x75, 0x08, 0x54, 0x03, 0xA5],
        ),
        (
            memory_encoding(
                VecElementType::F64,
                false,
                16,
                17,
                1,
                3,
                true,
                true,
                3,
                0x5A,
            )
            .to_vec(),
            vec![0x62, 0xE3, 0xF5, 0xB3, 0x54, 0x03, 0x5A],
        ),
        (
            memory_encoding(
                VecElementType::F32,
                true,
                31,
                30,
                0,
                7,
                false,
                false,
                3,
                0xFF,
            )
            .to_vec(),
            vec![0x62, 0x63, 0x0D, 0x07, 0x55, 0x3B, 0xFF],
        ),
        (
            stack_encoding(VecElementType::F64, true, 16, 17, 0, 1, true, false, 0xC3).to_vec(),
            vec![0x62, 0xE3, 0xF5, 0x81, 0x55, 0x04, 0x24, 0xC3],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn scalar_fixup_llig_accepts_all_four_values_and_reserves_memory_sae() {
    for ll in 0..4 {
        let bytes = memory_encoding(
            VecElementType::F32,
            true,
            17,
            18,
            ll,
            2,
            false,
            false,
            3,
            0xFF,
        );
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_fixup_imm_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert!(encoding.scalar);
        assert_eq!(encoding.width, VecWidth::V128);
        assert!(!encoding.suppress_exceptions);
        assert!(!encoding.needs_avx512vl);
        let X86EvexFixupImmMemoryReplay::Scalar { stack_instruction } = encoding.replay else {
            panic!("{bytes:02X?}: scalar selected non-scalar replay")
        };
        assert_eq!(
            stack_instruction.as_slice()[3],
            bytes[3] & !0x60,
            "guest LLIG must be accepted but canonicalized for hosted replay"
        );

        let mut reserved_memory_sae = bytes;
        reserved_memory_sae[3] |= 0x10;
        assert!(
            X86InstructionBytes::new(&reserved_memory_sae)
                .unwrap()
                .evex_fixup_imm_memory_encoding()
                .is_none(),
            "{reserved_memory_sae:02X?}"
        );

        for sae in [false, true] {
            let register = register_encoding(
                VecElementType::F32,
                true,
                17,
                18,
                3,
                ll,
                2,
                false,
                sae,
                0xFF,
            );
            assert_eq!(
                X86InstructionBytes::new(&register)
                    .unwrap()
                    .evex_register_fixup_imm_needs_vl(),
                Some(false),
                "{register:02X?}"
            );
        }
    }
}

#[test]
fn fixup_memory_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = memory_encoding(
        VecElementType::F32,
        false,
        0,
        1,
        0,
        1,
        false,
        false,
        3,
        0xA5,
    )
    .to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x02), // mandatory prefix
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
                .evex_fixup_imm_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_fixup_imm_memory_encoding()
        .expect("FS/address-size prefixes belong to helper address evaluation");
    let X86EvexFixupImmMemoryReplay::MaskedVector { stack_instruction } = encoding.replay else {
        panic!("prefixed masked vector selected wrong replay")
    };
    assert_eq!(
        stack_instruction.as_slice(),
        stack_encoding(VecElementType::F32, false, 0, 1, 0, 1, false, false, 0xA5,)
    );
}

#[test]
fn fixup_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VFIXUPIMMPS xmm16{k1},xmm17,[r16+r17*2+16],A5H. The disp8
    // value 1 is compressed by the 16-byte full-vector tuple.
    let bytes = [0x62, 0xEB, 0x71, 0x01, 0x54, 0x44, 0x48, 0x01, 0xA5];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .expect("APX-extended VFIXUPIMM memory source");
    assert_eq!(result.bytes_consumed, bytes.len());

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
        let exact = sequence(&function, true).expect("APX-address VFIXUPIMM sequence");
        assert_eq!(exact.address_offset, 2, "{level:?}");
        assert_eq!(exact.encoding.destination, 16, "{level:?}");
        assert_eq!(exact.encoding.source1, 17, "{level:?}");
        let X86EvexFixupImmMemoryReplay::MaskedVector { stack_instruction } = exact.encoding.replay
        else {
            panic!("{level:?}: APX masked vector selected wrong replay")
        };
        let expected = [0x62, 0xE3, 0x75, 0x01, 0x54, 0x04, 0x24, 0xA5];
        assert_eq!(stack_instruction.as_slice(), expected, "{level:?}");

        let case = FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            immediate: 0xA5,
        };
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{level:?}: missing APX-address VFIXUPIMM stack replay"
        );
    }
}

#[test]
fn all_180_fixup_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 180);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(sequence.encoding.elem, case.elem, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source1, case.source1,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.zeroing,
                case.zeroing(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.immediate, case.immediate,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.scalar,
                case.scalar(),
                "{level:?} {case:?}"
            );
            assert!(!sequence.encoding.suppress_exceptions, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_size,
                if case.scalar() || case.broadcast() {
                    case.memory_width().bytes()
                } else {
                    case.width.bytes()
                },
                "{level:?} {case:?}"
            );
            match (case.form, case.control) {
                (SourceForm::Vector, MaskControl::None) => {
                    assert_eq!(sequence.address_offset, 0, "{level:?} {case:?}")
                }
                (SourceForm::Vector, _) => {
                    assert_eq!(sequence.address_offset, 2, "{level:?} {case:?}")
                }
                (SourceForm::Broadcast, MaskControl::None) => {
                    assert_eq!(sequence.address_offset, 0, "{level:?} {case:?}")
                }
                (SourceForm::Broadcast, _) => {
                    assert_eq!(sequence.address_offset, 5, "{level:?} {case:?}")
                }
                (SourceForm::Scalar { .. }, MaskControl::None) => {
                    assert_eq!(sequence.address_offset, 1, "{level:?} {case:?}")
                }
                (SourceForm::Scalar { .. }, _) => {
                    assert!(
                        matches!(sequence.address_offset, 2 | 3),
                        "{level:?} {case:?}: {}",
                        sequence.address_offset
                    )
                }
            }
            assert_eq!(
                sequence.consumed,
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
    assert_eq!(lowerings, 180 * LEVELS.len());
}

#[test]
fn masked_vector_lowering_stages_disjoint_lanes_and_rejects_avx_only_bridge() {
    for (elem, staging_load, final_store) in [
        (
            VecElementType::F32,
            &[0x8B, 0x44, 0x24, 0x48][..],
            &[0x89, 0x44, 0x24, 0x44][..],
        ),
        (
            VecElementType::F64,
            &[0x48, 0x8B, 0x44, 0x24, 0x48][..],
            &[0x48, 0x89, 0x44, 0x24, 0x40][..],
        ),
    ] {
        let case = FixupMemoryCase {
            elem,
            width: VecWidth::V512,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            immediate: 0xFF,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        let lanes = case.width.lanes(case.elem);
        for lane in 0..lanes {
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
                "{elem:?} lane {lane}: {guard:02X?}"
            );
        }
        let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
        let release_frame = [0x48, 0x8D, 0x64, 0x24, 0x50];
        assert_eq!(
            code.windows(allocate_frame.len())
                .filter(|window| *window == allocate_frame)
                .count(),
            1,
            "{elem:?}"
        );
        assert!(
            code.windows(staging_load.len())
                .any(|window| window == staging_load),
            "{elem:?}: scalar helper return must use the disjoint staging slot"
        );
        assert!(
            code.windows(final_store.len())
                .any(|window| window == final_store),
            "{elem:?}: final active lane must end at payload byte 63"
        );
        assert_eq!(
            code.windows(release_frame.len())
                .filter(|window| *window == release_frame)
                .count(),
            lanes as usize + 1,
            "{elem:?}"
        );

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
fn fixup_memory_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let cases = [
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
            immediate: 0x00,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V256,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            immediate: 0xA5,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Scalar { ll: 3 },
            control: MaskControl::Merge,
            immediate: 0x5A,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(
            sequence(&function, false).is_none(),
            "{case:?}: memory-disabled admission"
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[6] ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong immediate provenance", &wrong_provenance);

        let mut wrong_immediate = function.clone();
        let fixup = wrong_immediate.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FixupImm { .. }))
            .unwrap();
        let OpKind::X86FixupImm { imm, .. } = &mut fixup.kind else {
            unreachable!()
        };
        *imm ^= 1;
        assert_rejected("wrong semantic immediate", &wrong_immediate);

        let mut wrong_source = function.clone();
        let fixup = wrong_source.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FixupImm { .. }))
            .unwrap();
        let OpKind::X86FixupImm { src1, .. } = &mut fixup.kind else {
            unreachable!()
        };
        *src1 = VReg::Arch(ArchReg::X86(match case.width {
            VecWidth::V128 => X86Reg::Xmm(case.source1 ^ 1),
            VecWidth::V256 => X86Reg::Ymm(case.source1 ^ 1),
            VecWidth::V512 => X86Reg::Zmm(case.source1 ^ 1),
            _ => unreachable!(),
        }));
        assert_rejected("wrong source1", &wrong_source);

        let mut wrong_sae = function.clone();
        let fixup = wrong_sae.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FixupImm { .. }))
            .unwrap();
        let OpKind::X86FixupImm {
            suppress_exceptions,
            ..
        } = &mut fixup.kind
        else {
            unreachable!()
        };
        *suppress_exceptions = !*suppress_exceptions;
        assert_rejected("wrong SAE", &wrong_sae);

        let mut hinted_memory = function.clone();
        let address_index = sequence(&hinted_memory, true).unwrap().address_offset;
        hinted_memory.blocks[0].ops[address_index].x86_hint =
            Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted memory", &hinted_memory);

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
