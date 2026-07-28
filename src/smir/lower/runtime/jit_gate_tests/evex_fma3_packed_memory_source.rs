//! Exact helper-backed packed EVEX FMA3 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86FmaOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, OpId, VReg, VecElementType,
    VecWidth, VirtualId, X86FmaKind, X86FmaOrder, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedFma3MemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_fma3_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod broadcast;

const PC: u64 = 0xE3F0;
const DISP8: u8 = 1;
const DISP32: i32 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const NATIVE_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const PACKED_OPCODES: [u8; 18] = [
    0x96, 0x97, 0x98, 0x9A, 0x9C, 0x9E, 0xA6, 0xA7, 0xA8, 0xAA, 0xAC, 0xAE, 0xB6, 0xB7, 0xB8, 0xBA,
    0xBC, 0xBE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryForm {
    Low,
    High,
    DestinationSourceAlias,
    FsAddr32Sib,
    RipRelative,
    ApxR16Base,
    ApxR16R17Sib,
}

impl MemoryForm {
    const ALL: [Self; 7] = [
        Self::Low,
        Self::High,
        Self::DestinationSourceAlias,
        Self::FsAddr32Sib,
        Self::RipRelative,
        Self::ApxR16Base,
        Self::ApxR16R17Sib,
    ];

    const NATIVE: [Self; 3] = [Self::Low, Self::High, Self::DestinationSourceAlias];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FmaMemoryCase {
    opcode: u8,
    w: bool,
    width: VecWidth,
    form: MemoryForm,
}

impl FmaMemoryCase {
    const fn destination(self) -> u8 {
        match self.form {
            MemoryForm::Low | MemoryForm::FsAddr32Sib | MemoryForm::ApxR16Base => 0,
            MemoryForm::High => 24,
            MemoryForm::DestinationSourceAlias => 17,
            MemoryForm::RipRelative => 31,
            MemoryForm::ApxR16R17Sib => 16,
        }
    }

    const fn source1(self) -> u8 {
        match self.form {
            MemoryForm::Low | MemoryForm::FsAddr32Sib | MemoryForm::ApxR16Base => 1,
            MemoryForm::High => 25,
            MemoryForm::DestinationSourceAlias => 17,
            MemoryForm::RipRelative => 30,
            MemoryForm::ApxR16R17Sib => 17,
        }
    }

    const fn base(self) -> Option<u8> {
        match self.form {
            MemoryForm::Low | MemoryForm::FsAddr32Sib => Some(3),
            MemoryForm::High | MemoryForm::DestinationSourceAlias => Some(11),
            MemoryForm::RipRelative => None,
            MemoryForm::ApxR16Base | MemoryForm::ApxR16R17Sib => Some(16),
        }
    }

    const fn index(self) -> Option<u8> {
        match self.form {
            MemoryForm::FsAddr32Sib => Some(6),
            MemoryForm::ApxR16R17Sib => Some(17),
            _ => None,
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination() && *candidate != self.source1())
            .expect("two EVEX operands leave at least fourteen low scratch registers")
    }

    const fn elem(self) -> VecElementType {
        if self.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        }
    }

    const fn kind(self) -> X86FmaKind {
        match self.opcode & 0x0F {
            0x06 => X86FmaKind::AddSub,
            0x07 => X86FmaKind::SubAdd,
            0x08 => X86FmaKind::Add,
            0x0A => X86FmaKind::Sub,
            0x0C => X86FmaKind::NegativeMultiplyAdd,
            0x0E => X86FmaKind::NegativeMultiplySub,
            _ => unreachable!(),
        }
    }

    const fn order(self) -> X86FmaOrder {
        match self.opcode >> 4 {
            0x09 => X86FmaOrder::Order132,
            0x0A => X86FmaOrder::Order213,
            0x0B => X86FmaOrder::Order231,
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

    fn p0(self) -> u8 {
        let destination = self.destination();
        let base = self.base().unwrap_or(0);
        let index = self.index().unwrap_or(0);
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | (if index & 8 == 0 { 0x40 } else { 0 })
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | (if base & 16 != 0 { 0x08 } else { 0 })
            | 0x02
    }

    fn p1(self) -> u8 {
        (u8::from(self.w) << 7)
            | (((!self.source1()) & 0x0F) << 3)
            | (if self.index().is_some_and(|index| index & 16 != 0) {
                0
            } else {
                0x04
            })
            | 0x01
    }

    fn p2(self) -> u8 {
        (self.ll() << 5) | if self.source1() & 16 == 0 { 0x08 } else { 0 }
    }

    fn bytes(self) -> Vec<u8> {
        let reg = (self.destination() & 7) << 3;
        let mut bytes = match self.form {
            MemoryForm::FsAddr32Sib => vec![0x64, 0x67],
            _ => Vec::new(),
        };
        bytes.extend_from_slice(&[0x62, self.p0(), self.p1(), self.p2(), self.opcode]);
        match self.form {
            MemoryForm::FsAddr32Sib => {
                bytes.extend_from_slice(&[0x44 | reg, 0x73, DISP8]);
            }
            MemoryForm::ApxR16R17Sib => {
                bytes.extend_from_slice(&[0x44 | reg, 0x48, DISP8]);
            }
            MemoryForm::RipRelative => {
                bytes.push(reg | 0x05);
                bytes.extend_from_slice(&DISP32.to_le_bytes());
            }
            _ => {
                bytes.extend_from_slice(&[0x40 | reg | (self.base().unwrap() & 7), DISP8]);
            }
        }
        bytes
    }

    fn emitted_fma_bytes(self) -> [u8; 6] {
        let scratch = self.scratch();
        [
            0x62,
            (self.p0() & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            self.p1() | 0x04,
            self.p2(),
            self.opcode,
            0xC0 | ((self.destination() & 7) << 3) | (scratch & 7),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("packed EVEX FMA3 width"),
    }))
}

fn lift_case(case: FmaMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
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
        X86InstructionBytes::new(&bytes).expect("EVEX FMA3 instruction provenance"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("packed EVEX FMA3 memory load")
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

fn assert_exact_sequence(function: &SmirFunction, case: FmaMemoryCase) {
    let index = sequence_index(function);
    let ops = &function.blocks[0].ops[index..];
    assert_eq!(ops.len(), 3, "{case:?}: {ops:#?}");
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");

    let loaded = match &ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(*width, case.width, "{case:?}");
            assert!(addr.is_x86_state_backed_shape(), "{case:?}: {addr:?}");
            *loaded
        }
        other => panic!("{case:?}: expected VLoad, got {other:?}"),
    };
    assert_eq!(ops[0].x86_hint, None, "{case:?}");

    let raw = match &ops[1].kind {
        OpKind::X86Fma(X86FmaOp {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2,
            src3,
            mask,
            elem,
            kind,
            order,
            round,
            lanes,
        }) => {
            assert_eq!(*src1, vector(case.destination(), case.width), "{case:?}");
            assert_eq!(*src2, vector(case.source1(), case.width), "{case:?}");
            assert_eq!(*src3, loaded, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*elem, case.elem(), "{case:?}");
            assert_eq!(*kind, case.kind(), "{case:?}");
            assert_eq!(*order, case.order(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(*lanes, case.width.lanes(case.elem()) as u8, "{case:?}");
            *raw
        }
        other => panic!("{case:?}: expected X86Fma, got {other:?}"),
    };
    assert_eq!(
        ops[1].x86_hint,
        Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.width,
            w: case.w,
        }),
        "{case:?}"
    );
    assert!(
        matches!(
            ops[2].kind,
            OpKind::VMov {
                dst,
                src,
                width,
            } if dst == vector(case.destination(), case.width)
                && src == raw
                && width == case.width
        ),
        "{case:?}: {:?}",
        ops[2].kind
    );
    assert_eq!(ops[2].x86_hint, None, "{case:?}");
}

fn lower(function: &SmirFunction, case: FmaMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_fma, "{case:?}");
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
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed EVEX FMA3 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed EVEX FMA3"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FmaMemoryCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for w in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::ALL {
                    cases.push(FmaMemoryCase {
                        opcode,
                        w,
                        width,
                        form,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn packed_evex_fma3_memory_byte_classifier_exhaustively_rewrites_110_592_operands() {
    let mut accepted = 0usize;
    for opcode in PACKED_OPCODES {
        for w in [false, true] {
            for (ll, width) in [
                (0, VecWidth::V128),
                (1, VecWidth::V256),
                (2, VecWidth::V512),
            ] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                            | 0x60
                            | (if destination & 16 == 0 { 0x10 } else { 0 })
                            | 2;
                        let p1 = (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
                        let p2 = (ll << 5) | if source1 & 16 == 0 { 0x08 } else { 0 };
                        let bytes = [
                            0x62,
                            p0,
                            p1,
                            p2,
                            opcode,
                            0x40 | ((destination & 7) << 3) | 3,
                            DISP8,
                        ];
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_packed_fma3_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        let scratch = (0..16)
                            .find(|candidate| *candidate != destination && *candidate != source1)
                            .unwrap();
                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                        assert_eq!(
                            encoding.elem,
                            if w {
                                VecElementType::F64
                            } else {
                                VecElementType::F32
                            },
                            "{bytes:02X?}"
                        );
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        let (actual_scratch, register_instruction) = match encoding.replay {
                            X86EvexPackedFma3MemoryReplay::Vector {
                                scratch,
                                register_instruction,
                            } => (scratch, register_instruction),
                            X86EvexPackedFma3MemoryReplay::Broadcast { .. } => {
                                panic!(
                                    "{bytes:02X?}: non-broadcast source selected broadcast replay"
                                )
                            }
                        };
                        assert_eq!(actual_scratch, scratch, "{bytes:02X?}");
                        assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                        assert_eq!(encoding.w, w, "{bytes:02X?}");
                        assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                        assert_eq!(
                            register_instruction.evex_register_packed_fma_needs_vl(),
                            Some(ll != 2),
                            "{bytes:02X?}"
                        );
                        accepted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18 * 2 * 3 * 32 * 32);

    for opcode in 0..=u8::MAX {
        let bytes = [0x62, 0xF2, 0x75, 0x08, opcode, 0x43, DISP8];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fma3_memory_encoding()
                .is_some(),
            PACKED_OPCODES.contains(&opcode),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn packed_evex_fma3_rewrite_matches_independent_llvm_23_encodings() {
    let cases = [
        (
            FmaMemoryCase {
                opcode: 0x98,
                w: false,
                width: VecWidth::V128,
                form: MemoryForm::ApxR16R17Sib,
            },
            [0x62, 0xE2, 0x75, 0x00, 0x98, 0xC0],
        ),
        (
            FmaMemoryCase {
                opcode: 0xBA,
                w: true,
                width: VecWidth::V256,
                form: MemoryForm::High,
            },
            [0x62, 0x62, 0xB5, 0x20, 0xBA, 0xC0],
        ),
        (
            FmaMemoryCase {
                opcode: 0xAC,
                w: false,
                width: VecWidth::V512,
                form: MemoryForm::DestinationSourceAlias,
            },
            [0x62, 0xE2, 0x75, 0x40, 0xAC, 0xC8],
        ),
    ];
    for (case, llvm) in cases {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_packed_fma3_memory_encoding()
            .unwrap();
        let X86EvexPackedFma3MemoryReplay::Vector {
            register_instruction,
            ..
        } = encoding.replay
        else {
            panic!("{case:?}: non-broadcast source selected broadcast replay");
        };
        assert_eq!(register_instruction.as_slice(), llvm, "{case:?}");
        assert_eq!(case.emitted_fma_bytes(), llvm, "{case:?}");
    }
}

#[test]
fn canonical_fma3_modrm_prefix_preserves_apx_base_and_index_for_scalar_and_fp16() {
    // EVEX.B4=1 promotes SIB.base=0 to R16; EVEX.X4=!U=1 promotes
    // SIB.index=1 to R17. Disp8 compression is by the scalar element size or
    // the complete packed-vector width, respectively.
    for (name, bytes, displacement) in [
        (
            "scalar F32",
            [0x62, 0xFA, 0x71, 0x08, 0x99, 0x44, 0x48, 0x01],
            4,
        ),
        (
            "scalar F64",
            [0x62, 0xFA, 0xF1, 0x08, 0x99, 0x44, 0x48, 0x01],
            8,
        ),
        (
            "packed F16",
            [0x62, 0xFE, 0x71, 0x08, 0x98, 0x44, 0x48, 0x01],
            16,
        ),
        (
            "scalar F16",
            [0x62, 0xFE, 0x71, 0x08, 0x99, 0x44, 0x48, 0x01],
            2,
        ),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter
            .lift_insn(PC, &bytes, &mut context)
            .unwrap_or_else(|error| panic!("{name} {bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");

        let memory_addresses: Vec<_> = result
            .ops
            .iter()
            .filter_map(|op| match &op.kind {
                OpKind::Load { addr, .. }
                | OpKind::PredLoad { addr, .. }
                | OpKind::VLoad { addr, .. } => Some(addr),
                _ => None,
            })
            .collect();
        assert_eq!(memory_addresses.len(), 1, "{name}: {:#?}", result.ops);
        assert_eq!(
            memory_addresses[0],
            &Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                scale: 2,
                disp: displacement,
                disp_size: DispSize::Disp8,
            },
            "{name}: {:#?}",
            result.ops
        );
    }
}

#[test]
fn all_756_evex_packed_memory_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 18 * 2 * 3 * 7);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_evex_packed_fma3_memory_sequence(
                &function.blocks[0],
                index,
                true,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(sequence.consumed, 3, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_size,
                case.width.bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source1,
                case.source1(),
                "{level:?} {case:?}"
            );
            let X86EvexPackedFma3MemoryReplay::Vector { scratch, .. } = sequence.encoding.replay
            else {
                panic!("{level:?} {case:?}: non-broadcast source selected broadcast replay");
            };
            assert_eq!(scratch, case.scratch(), "{level:?} {case:?}");

            let (code, _) = lower(&function, case);
            let expected = case.emitted_fma_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector scratch transfer"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 756 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_packed_fma3_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: sequence classifier admitted malformed EVEX FMA3 sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed EVEX FMA3 sequence"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed EVEX FMA3 sequence"
    );
}

#[test]
fn packed_evex_fma3_memory_classifiers_reject_reserved_and_non_owned_encodings() {
    let valid = FmaMemoryCase {
        opcode: 0x98,
        w: false,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    }
    .bytes();
    let evex = 0usize;
    let mut malformed = Vec::new();
    malformed.push(valid[..valid.len() - 1].to_vec());
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[evex + 5] |= 0xC0;
    register.truncate(6);
    malformed.push(register);
    for (byte_index, mask) in [(1, 0x01), (2, 0x01), (3, 0x80), (3, 0x01)] {
        let mut bytes = valid.clone();
        bytes[evex + byte_index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[evex + 3] |= 0x60;
    malformed.push(reserved_ll);
    let mut scalar = valid.clone();
    scalar[evex + 4] = 0x99;
    malformed.push(scalar);
    let mut operand_size = valid.clone();
    operand_size.insert(0, 0x66);
    malformed.push(operand_size);
    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fma3_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn packed_evex_fma3_memory_sequence_fails_closed_for_semantic_mutations() {
    let case = FmaMemoryCase {
        opcode: 0x98,
        w: false,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let raw = match base.blocks[0].ops[1].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_packed_fma3_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    let mut metadata_destination = base.clone();
    let mut bytes = case.bytes();
    bytes[5] ^= 0x08;
    metadata_destination
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    let mut metadata_source = base.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x08;
    metadata_source
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    let mut metadata_broadcast = base.clone();
    let mut bytes = case.bytes();
    bytes[3] |= 0x10;
    metadata_broadcast
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    let mut load_arch = base.clone();
    if let OpKind::VLoad { dst, .. } = &mut load_arch.blocks[0].ops[0].kind {
        *dst = vector(2, VecWidth::V128);
    }
    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    let mut loaded_twice = base.clone();
    loaded_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: loaded,
            width: VecWidth::V128,
        },
    ));
    let mut loaded_defined_twice = base.clone();
    loaded_defined_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: loaded,
            src: vector(3, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    let mut fma_pc = base.clone();
    fma_pc.blocks[0].ops[1].guest_pc += 1;
    let mut fma_hint = base.clone();
    fma_hint.blocks[0].ops[1].x86_hint = None;
    let mut fma_map = base.clone();
    if let Some(X86OpHint::EvexOp { map, .. }) = &mut fma_map.blocks[0].ops[1].x86_hint {
        *map = X86VecMap::Map0F;
    }
    let mut fma_prefix = base.clone();
    if let Some(X86OpHint::EvexOp { pp, .. }) = &mut fma_prefix.blocks[0].ops[1].x86_hint {
        *pp = X86SsePrefix::None;
    }
    let mut fma_opcode = base.clone();
    if let Some(X86OpHint::EvexOp { opcode, .. }) = &mut fma_opcode.blocks[0].ops[1].x86_hint {
        *opcode = 0x9A;
    }
    let mut fma_hint_width = base.clone();
    if let Some(X86OpHint::EvexOp { width, .. }) = &mut fma_hint_width.blocks[0].ops[1].x86_hint {
        *width = VecWidth::V256;
    }
    let mut fma_hint_w = base.clone();
    if let Some(X86OpHint::EvexOp { w, .. }) = &mut fma_hint_w.blocks[0].ops[1].x86_hint {
        *w = true;
    }
    let mut fma_destination = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_destination.blocks[0].ops[1].kind {
        op.src1 = vector(2, VecWidth::V128);
    }
    let mut fma_source = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_source.blocks[0].ops[1].kind {
        op.src2 = vector(2, VecWidth::V128);
    }
    let mut fma_memory = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_memory.blocks[0].ops[1].kind {
        op.src3 = vector(3, VecWidth::V128);
    }
    let mut fma_mask = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_mask.blocks[0].ops[1].kind {
        op.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    }
    let mut fma_elem = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_elem.blocks[0].ops[1].kind {
        op.elem = VecElementType::F64;
        op.lanes = 2;
    }
    let mut fma_lanes = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_lanes.blocks[0].ops[1].kind {
        op.lanes -= 1;
    }
    let mut fma_round = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_round.blocks[0].ops[1].kind {
        op.round = FpRoundMode::RoundUp;
    }
    let mut fma_kind = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_kind.blocks[0].ops[1].kind {
        op.kind = X86FmaKind::Sub;
    }
    let mut fma_order = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_order.blocks[0].ops[1].kind {
        op.order = X86FmaOrder::Order231;
    }
    let mut raw_twice = base.clone();
    raw_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: raw,
            width: VecWidth::V128,
        },
    ));
    let mut raw_defined_twice = base.clone();
    raw_defined_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: raw,
            src: vector(3, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    let mut result_pc = base.clone();
    result_pc.blocks[0].ops[2].guest_pc += 1;
    let mut result_hint = base.clone();
    result_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    let mut result_dst = base.clone();
    if let OpKind::VMov { dst, .. } = &mut result_dst.blocks[0].ops[2].kind {
        *dst = vector(2, VecWidth::V128);
    }
    let mut result_src = base.clone();
    if let OpKind::VMov { src, .. } = &mut result_src.blocks[0].ops[2].kind {
        *src = loaded;
    }
    let mut result_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut result_width.blocks[0].ops[2].kind {
        *width = VecWidth::V256;
    }
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: vector(3, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    let mut missing_result = base.clone();
    missing_result.blocks[0].ops.pop();

    let malformed = [
        ("missing metadata", missing_metadata),
        ("metadata destination", metadata_destination),
        ("metadata source", metadata_source),
        ("metadata broadcast", metadata_broadcast),
        ("load hint", load_hint),
        ("architectural load", load_arch),
        ("load width", load_width),
        ("virtual address", virtual_address),
        ("loaded temporary reused", loaded_twice),
        ("loaded temporary redefined", loaded_defined_twice),
        ("FMA PC", fma_pc),
        ("FMA hint", fma_hint),
        ("FMA map", fma_map),
        ("FMA prefix", fma_prefix),
        ("FMA opcode", fma_opcode),
        ("FMA hint width", fma_hint_width),
        ("FMA hint W", fma_hint_w),
        ("FMA destination", fma_destination),
        ("FMA source", fma_source),
        ("FMA memory", fma_memory),
        ("FMA mask", fma_mask),
        ("FMA element", fma_elem),
        ("FMA lanes", fma_lanes),
        ("FMA round", fma_round),
        ("FMA kind", fma_kind),
        ("FMA order", fma_order),
        ("raw temporary reused", raw_twice),
        ("raw temporary redefined", raw_defined_twice),
        ("result PC", result_pc),
        ("result hint", result_hint),
        ("result destination", result_dst),
        ("result source", result_src),
        ("result width", result_width),
        ("same-PC tail", same_pc_tail),
        ("missing result", missing_result),
    ];
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

fn full_guest_regs(case: FmaMemoryCase, ordinal: usize, data_case: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        fs_base: 0x400,
        gs_base: 0x800,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    registers.zmm[usize::from(case.destination())] =
        super::vex_fma3_memory_source::role_vector(case.w, data_case, 0);
    if case.source1() != case.destination() {
        registers.zmm[usize::from(case.source1())] =
            super::vex_fma3_memory_source::role_vector(case.w, data_case, 1);
    }
    if let Some(base) = case.base() {
        registers.gpr[usize::from(base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x100;
    }
    if let Some(index) = case.index() {
        registers.gpr[usize::from(index)] = 0x300 + ((ordinal & 0x0F) as u64) * 0x20;
    }
    registers
}

fn memory_address(case: FmaMemoryCase, registers: &GuestRegs) -> u64 {
    let compressed_displacement = u64::from(DISP8) * u64::from(case.width.bytes());
    match case.form {
        MemoryForm::Low
        | MemoryForm::High
        | MemoryForm::DestinationSourceAlias
        | MemoryForm::ApxR16Base => {
            registers.gpr[usize::from(case.base().unwrap())].wrapping_add(compressed_displacement)
        }
        MemoryForm::FsAddr32Sib => {
            let offset = (registers.gpr[3] as u32)
                .wrapping_add((registers.gpr[6] as u32).wrapping_mul(2))
                .wrapping_add(compressed_displacement as u32);
            registers.fs_base.wrapping_add(u64::from(offset))
        }
        MemoryForm::RipRelative => {
            (PC + case.bytes().len() as u64).wrapping_add_signed(i64::from(DISP32))
        }
        MemoryForm::ApxR16R17Sib => registers.gpr[16]
            .wrapping_add(registers.gpr[17].wrapping_mul(2))
            .wrapping_add(compressed_displacement),
    }
}

fn native_cases() -> Vec<FmaMemoryCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for w in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::NATIVE {
                    cases.push(FmaMemoryCase {
                        opcode,
                        w,
                        width,
                        form,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn interpreter_o0_o1_o2_match_all_756_opcode_format_width_and_address_shapes() {
    use super::vex_fma3_memory_source::{interpreter_success, role_vector};

    let cases = all_cases();
    assert_eq!(cases.len(), 18 * 2 * 3 * 7);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = role_vector(case.w, 0, 2);
        let alternate_source = role_vector(case.w, 0, 1);
        let initial = full_guest_regs(case, ordinal, 0);
        let address = memory_address(case, &initial);
        assert!(
            address + u64::from(case.width.bytes()) <= 0x10000,
            "{case:?}: address {address:#x}"
        );
        let expected = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            source,
            address,
            case.width,
        );
        let alternate = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            alternate_source,
            address,
            case.width,
        );
        assert_ne!(
            expected.zmm[usize::from(case.destination())],
            alternate.zmm[usize::from(case.destination())],
            "{case:?}: decoded memory address did not affect the FMA result"
        );
        for level in LEVELS {
            let actual = interpreter_success(
                &optimize(lift_case(case), level),
                &initial,
                source,
                address,
                case.width,
            );
            assert_eq!(actual, expected, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert_eq!(executions, 756 * LEVELS.len());
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_packed_evex_fma3_memory_matches_interpretation_and_faults_without_commit() {
    use super::vex_fma3_memory_source::{
        VectorMemoryContext, interpreter_success, role_vector, vector_load_helper,
    };
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed EVEX FMA3 memory differential: host lacks AVX-512F/BW");
        return;
    }

    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let cases: Vec<_> = native_cases()
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert_eq!(cases.len(), 18 * 2 * if has_vl { 3 } else { 1 } * 3);
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = role_vector(case.w, ordinal, 2);

            let mut context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, ordinal);
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected =
                interpreter_success(&function, &registers, source, address, case.width);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, ordinal);
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
