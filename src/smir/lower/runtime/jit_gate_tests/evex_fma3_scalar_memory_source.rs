//! Exact helper-backed scalar EVEX FMA3 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, X86FmaOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86FmaKind, X86FmaOrder, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_scalar_fma3_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod masked;

const PC: u64 = 0xE5A0;
const DISP8: u8 = 1;
const DISP32: i32 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const NATIVE_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const SCALAR_OPCODES: [u8; 12] = [
    0x99, 0x9B, 0x9D, 0x9F, 0xA9, 0xAB, 0xAD, 0xAF, 0xB9, 0xBB, 0xBD, 0xBF,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarFormat {
    F16,
    F32,
    F64,
}

impl ScalarFormat {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F16 => VecElementType::F16,
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::F16 => 6,
            Self::F32 | Self::F64 => 2,
        }
    }

    const fn vec_map(self) -> X86VecMap {
        match self {
            Self::F16 => X86VecMap::Map6,
            Self::F32 | Self::F64 => X86VecMap::Map0F38,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F16 => MemWidth::B2,
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn memory_size(self) -> usize {
        match self {
            Self::F16 => 2,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

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
struct ScalarFmaCase {
    opcode: u8,
    format: ScalarFormat,
    ll: u8,
    form: MemoryForm,
}

impl ScalarFmaCase {
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

    const fn hint_width(self) -> VecWidth {
        match self.ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 | 3 => VecWidth::V512,
            _ => unreachable!(),
        }
    }

    const fn kind(self) -> X86FmaKind {
        match self.opcode & 0x0F {
            0x09 => X86FmaKind::Add,
            0x0B => X86FmaKind::Sub,
            0x0D => X86FmaKind::NegativeMultiplyAdd,
            0x0F => X86FmaKind::NegativeMultiplySub,
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

    fn p0(self) -> u8 {
        let destination = self.destination();
        let base = self.base().unwrap_or(0);
        let index = self.index().unwrap_or(0);
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | (if index & 8 == 0 { 0x40 } else { 0 })
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | (if base & 16 != 0 { 0x08 } else { 0 })
            | self.format.map()
    }

    fn p1(self) -> u8 {
        (u8::from(self.format.w()) << 7)
            | (((!self.source1()) & 0x0F) << 3)
            | (if self.index().is_some_and(|index| index & 16 != 0) {
                0
            } else {
                0x04
            })
            | 0x01
    }

    fn p2(self) -> u8 {
        (self.ll << 5) | if self.source1() & 16 == 0 { 0x08 } else { 0 }
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

    fn stack_instruction(self) -> [u8; 7] {
        [
            0x62,
            (self.p0() & 0x97) | 0x60,
            self.p1() | 0x04,
            self.p2() & 0x08,
            self.opcode,
            ((self.destination() & 7) << 3) | 0x04,
            0x24,
        ]
    }
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn lift_case(case: ScalarFmaCase) -> SmirFunction {
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
        X86InstructionBytes::new(&bytes).expect("EVEX scalar FMA3 instruction provenance"),
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
        .position(|op| matches!(op.kind, OpKind::Load { .. }))
        .expect("scalar EVEX FMA3 memory load")
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
) -> Option<crate::smir::lower::runtime::X86JitEvexScalarFma3MemorySequence> {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_scalar_fma3_memory_sequence(
        &function.blocks[0],
        index,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn all_cases() -> Vec<ScalarFmaCase> {
    let mut cases = Vec::new();
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3 {
                for form in MemoryForm::ALL {
                    cases.push(ScalarFmaCase {
                        opcode,
                        format,
                        ll,
                        form,
                    });
                }
            }
        }
    }
    cases
}

fn assert_exact_sequence(function: &SmirFunction, case: ScalarFmaCase) {
    let index = sequence_index(function);
    let ops = &function.blocks[0].ops[index..];
    let elem = case.format.elem();
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    assert_eq!(ops.len(), 2 * xmm_lanes + 5, "{case:?}: {ops:#?}");
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");

    let loaded = match &ops[0].kind {
        OpKind::Load {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*width, case.format.memory_width(), "{case:?}");
            assert!(addr.is_x86_state_backed_shape(), "{case:?}: {addr:?}");
            *loaded
        }
        other => panic!("{case:?}: expected scalar Load, got {other:?}"),
    };
    let broadcast = match &ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } => {
            assert_eq!(*scalar, loaded, "{case:?}");
            assert_eq!(*broadcast_elem, elem, "{case:?}");
            *vector
        }
        other => panic!("{case:?}: expected scalar VBroadcast, got {other:?}"),
    };

    let (raw, src1, src2, src3, mask, kind, order, round, lanes) = match &ops[2].kind {
        OpKind::X86Fma(X86FmaOp {
            dst,
            src1,
            src2,
            src3,
            mask,
            elem: fma_elem,
            kind,
            order,
            round,
            lanes,
        }) if elem != VecElementType::F16 => {
            assert_eq!(*fma_elem, elem, "{case:?}");
            (
                *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
            )
        }
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        other => panic!("{case:?}: expected scalar FMA, got {other:?}"),
    };
    assert!(matches!(raw, VReg::Virtual(_)), "{case:?}");
    assert_eq!(src1, xmm(case.destination()), "{case:?}");
    assert_eq!(src2, xmm(case.source1()), "{case:?}");
    assert_eq!(src3, broadcast, "{case:?}");
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(kind, case.kind(), "{case:?}");
    assert_eq!(order, case.order(), "{case:?}");
    assert_eq!(round, FpRoundMode::Dynamic, "{case:?}");
    assert_eq!(lanes, 1, "{case:?}");
    assert_eq!(
        ops[2].x86_hint,
        Some(X86OpHint::EvexOp {
            map: case.format.vec_map(),
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.hint_width(),
            w: case.format.w(),
        }),
        "{case:?}"
    );
}

fn lower(function: &SmirFunction, case: ScalarFmaCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert_eq!(
        requirements.needs_avx512fp16,
        case.format == ScalarFormat::F16,
        "{case:?}"
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.format != ScalarFormat::F16 || std::is_x86_feature_detected!("avx512fp16")),
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
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed scalar EVEX FMA3: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar EVEX FMA3"),
        result.entry_offset,
    )
}

#[test]
fn scalar_evex_fma3_byte_classifier_exhaustively_rewrites_147_456_operands() {
    let mut accepted = 0usize;
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3u8 {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                            | 0x60
                            | (if destination & 16 == 0 { 0x10 } else { 0 })
                            | format.map();
                        let p1 = (u8::from(format.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
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
                            .evex_scalar_fma3_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(
                            encoding.hint_width,
                            match ll {
                                0 => VecWidth::V128,
                                1 => VecWidth::V256,
                                2 | 3 => VecWidth::V512,
                                _ => unreachable!(),
                            }
                        );
                        assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.writemask, None, "{bytes:02X?}");
                        assert!(!encoding.zeroing, "{bytes:02X?}");
                        assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                        assert_eq!(encoding.w, format.w(), "{bytes:02X?}");
                        assert_eq!(
                            encoding.stack_instruction.as_slice(),
                            &[
                                0x62,
                                (p0 & 0x97) | 0x60,
                                p1 | 0x04,
                                p2 & 0x08,
                                opcode,
                                ((destination & 7) << 3) | 0x04,
                                0x24,
                            ],
                            "{bytes:02X?}"
                        );
                        accepted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 12 * 3 * 4 * 32 * 32);

    for format in ScalarFormat::ALL {
        for opcode in 0..=u8::MAX {
            let p0 = 0xF0 | format.map();
            let p1 = (u8::from(format.w()) << 7) | 0x75;
            let bytes = [0x62, p0, p1, 0x08, opcode, 0x43, DISP8];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_scalar_fma3_memory_encoding()
                    .is_some(),
                SCALAR_OPCODES.contains(&opcode),
                "{format:?} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn scalar_evex_fma3_stack_rewrite_matches_independent_llvm_23_encodings() {
    let cases = [
        (
            ScalarFmaCase {
                opcode: 0x99,
                format: ScalarFormat::F32,
                ll: 3,
                form: MemoryForm::ApxR16R17Sib,
            },
            [0x62, 0xE2, 0x75, 0x00, 0x99, 0x04, 0x24],
        ),
        (
            ScalarFmaCase {
                opcode: 0xAB,
                format: ScalarFormat::F64,
                ll: 2,
                form: MemoryForm::High,
            },
            [0x62, 0x62, 0xB5, 0x00, 0xAB, 0x04, 0x24],
        ),
        (
            ScalarFmaCase {
                opcode: 0xBD,
                format: ScalarFormat::F16,
                ll: 1,
                form: MemoryForm::DestinationSourceAlias,
            },
            [0x62, 0xE6, 0x75, 0x00, 0xBD, 0x0C, 0x24],
        ),
    ];
    for (case, llvm) in cases {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_scalar_fma3_memory_encoding()
            .unwrap();
        assert_eq!(encoding.stack_instruction.as_slice(), llvm, "{case:?}");
        assert_eq!(case.stack_instruction(), llvm, "{case:?}");
    }
}

#[test]
fn all_1008_scalar_evex_memory_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 12 * 3 * 4 * 7);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let sequence =
                sequence(&function).unwrap_or_else(|| panic!("{level:?} {case:?}: rejected"));
            assert_eq!(
                sequence.consumed,
                2 * VecWidth::V128.lanes(case.format.elem()) as usize + 5,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.memory_width,
                case.format.memory_width(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.load_offset, 0, "{level:?} {case:?}");
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
            assert_eq!(
                sequence.encoding.hint_width,
                case.hint_width(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let expected = case.stack_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, case.format.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing scalar helper size"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 1008 * LEVELS.len());
}

#[test]
fn scalar_evex_fma3_classifier_rejects_reserved_and_non_owned_encodings() {
    let valid_case = ScalarFmaCase {
        opcode: 0x99,
        format: ScalarFormat::F32,
        ll: 0,
        form: MemoryForm::Low,
    };
    let valid = valid_case.bytes();
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
    for (byte_index, mask) in [(1, 0x01), (2, 0x01), (3, 0x10), (3, 0x80)] {
        let mut bytes = valid.clone();
        bytes[evex + byte_index] ^= mask;
        malformed.push(bytes);
    }
    let mut packed = valid.clone();
    packed[evex + 4] = 0x98;
    malformed.push(packed);
    let mut operand_size = valid.clone();
    operand_size.insert(0, 0x66);
    malformed.push(operand_size);
    let mut fp16_w1 = ScalarFmaCase {
        format: ScalarFormat::F16,
        ..valid_case
    }
    .bytes();
    fp16_w1[2] |= 0x80;
    malformed.push(fp16_w1);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fma3_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
    for ll in 0..=3 {
        let bytes = ScalarFmaCase { ll, ..valid_case }.bytes();
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fma3_memory_encoding()
                .is_some(),
            "Intel SDM EVEX.LLIG value {ll}"
        );
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_scalar_fma3_memory_sequence(
            &function.blocks[0],
            index,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: sequence classifier admitted malformed scalar EVEX FMA3"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed scalar EVEX FMA3"
    );
}

#[test]
fn scalar_evex_fma3_sequence_fails_closed_for_semantic_provenance_and_ssa_mutations() {
    let case = ScalarFmaCase {
        opcode: 0x99,
        format: ScalarFormat::F32,
        ll: 0,
        form: MemoryForm::Low,
    };
    let base = lift_case(case);
    assert_eq!(base.blocks[0].ops.len(), 13);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let raw = match base.blocks[0].ops[2].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };
    let scalar_result = match base.blocks[0].ops[3].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_scalar_fma3_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut malformed: Vec<(&str, SmirFunction)> = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing instruction provenance", missing_metadata));

    let mut wrong_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] = 0xA9;
    wrong_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("mismatched instruction opcode", wrong_metadata));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x10,
    });
    malformed.push(("unexpected load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("wrong load width", load_width));

    let mut signed_load = base.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("signed load", signed_load));

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(999)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut loaded_used_twice = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut loaded_used_twice.blocks[0].ops[4].kind {
        *vec = loaded;
    }
    malformed.push(("loaded temporary used twice", loaded_used_twice));

    let mut wrong_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut wrong_broadcast_scalar.blocks[0].ops[1].kind {
        *scalar = scalar_result;
    }
    malformed.push(("wrong broadcast scalar", wrong_broadcast_scalar));

    let mut wrong_broadcast_elem = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut wrong_broadcast_elem.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("wrong broadcast element", wrong_broadcast_elem));

    let mut wrong_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(("wrong broadcast lanes", wrong_broadcast_lanes));

    let mut fma_pc = base.clone();
    fma_pc.blocks[0].ops[2].guest_pc += 1;
    malformed.push(("FMA guest PC mismatch", fma_pc));

    let mut fma_hint = base.clone();
    fma_hint.blocks[0].ops[2].x86_hint = None;
    malformed.push(("missing FMA hint", fma_hint));

    let mut fma_hint_width = base.clone();
    if let Some(X86OpHint::EvexOp { width, .. }) = &mut fma_hint_width.blocks[0].ops[2].x86_hint {
        *width = VecWidth::V256;
    }
    malformed.push(("wrong LLIG hint width", fma_hint_width));

    let mut fma_hint_map = base.clone();
    if let Some(X86OpHint::EvexOp { map, .. }) = &mut fma_hint_map.blocks[0].ops[2].x86_hint {
        *map = X86VecMap::Map6;
    }
    malformed.push(("wrong FMA map", fma_hint_map));

    let mut fma_destination = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_destination.blocks[0].ops[2].kind {
        op.src1 = xmm(2);
    }
    malformed.push(("wrong destructive destination", fma_destination));

    let mut fma_source1 = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_source1.blocks[0].ops[2].kind {
        op.src2 = xmm(2);
    }
    malformed.push(("wrong EVEX.vvvv source", fma_source1));

    let mut fma_bypasses_load = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_bypasses_load.blocks[0].ops[2].kind {
        op.src3 = xmm(2);
    }
    malformed.push(("FMA bypasses load", fma_bypasses_load));

    let mut fma_masked = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_masked.blocks[0].ops[2].kind {
        op.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    }
    malformed.push(("masked FMA", fma_masked));

    let mut fma_elem = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_elem.blocks[0].ops[2].kind {
        op.elem = VecElementType::F64;
    }
    malformed.push(("wrong FMA element", fma_elem));

    let mut fma_lanes = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_lanes.blocks[0].ops[2].kind {
        op.lanes = 2;
    }
    malformed.push(("wrong FMA lane count", fma_lanes));

    let mut fma_kind = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_kind.blocks[0].ops[2].kind {
        op.kind = X86FmaKind::Sub;
    }
    malformed.push(("wrong FMA kind", fma_kind));

    let mut fma_order = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_order.blocks[0].ops[2].kind {
        op.order = X86FmaOrder::Order231;
    }
    malformed.push(("wrong FMA order", fma_order));

    let mut fma_round = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_round.blocks[0].ops[2].kind {
        op.round = FpRoundMode::RoundDown;
    }
    malformed.push(("explicit FMA rounding", fma_round));

    let mut raw_used_twice = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut raw_used_twice.blocks[0].ops[4].kind {
        *vec = raw;
    }
    malformed.push(("raw temporary used twice", raw_used_twice));

    let mut result_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut result_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }
    malformed.push(("wrong result lane", result_lane));

    let mut upper_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut upper_source.blocks[0].ops[4].kind {
        *vec = xmm(1);
    }
    malformed.push(("wrong upper-lane source", upper_source));

    let mut nonzero_clear = base.clone();
    if let OpKind::Mov { src, .. } = &mut nonzero_clear.blocks[0].ops[7].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("nonzero destination clear", nonzero_clear));

    let mut clear_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut clear_width.blocks[0].ops[7].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("wrong clear width", clear_width));

    let mut clear_destination = base.clone();
    if let OpKind::VBroadcast { dst, .. } = &mut clear_destination.blocks[0].ops[8].kind {
        *dst = xmm(2);
    }
    malformed.push(("wrong clear destination", clear_destination));

    let mut insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut insert_scalar.blocks[0].ops[9].kind {
        *scalar = loaded;
    }
    malformed.push(("wrong low insert scalar", insert_scalar));

    let mut insert_destination = base.clone();
    if let OpKind::VInsertLane { dst, vec, .. } = &mut insert_destination.blocks[0].ops[9].kind {
        *dst = xmm(2);
        *vec = xmm(2);
    }
    malformed.push(("wrong insert destination", insert_destination));

    let mut upper_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut upper_insert_lane.blocks[0].ops[10].kind {
        *lane = 2;
    }
    malformed.push(("wrong upper insert lane", upper_insert_lane));

    let mut same_pc_tail = base.clone();
    let tail = same_pc_tail.blocks[0].ops[12].clone();
    same_pc_tail.blocks[0].ops.push(tail);
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let fp16 = lift_case(ScalarFmaCase {
        format: ScalarFormat::F16,
        ..case
    });
    assert!(sequence(&fp16).is_some());
    let mut wrong_fp16_kind = fp16;
    if let OpKind::X86FP16Fma { kind, .. } = &mut wrong_fp16_kind.blocks[0].ops[2].kind {
        *kind = X86FmaKind::Sub;
    }
    assert_rejected("wrong FP16 FMA kind", &wrong_fp16_kind);
}

#[test]
fn scalar_evex_fma3_lowerer_rejects_the_avx_only_vector_bridge() {
    let case = ScalarFmaCase {
        opcode: 0xB9,
        format: ScalarFormat::F64,
        ll: 3,
        form: MemoryForm::High,
    };
    let function = lift_case(case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}

fn scalar_bits(format: ScalarFormat, data_case: usize, role: usize) -> u64 {
    const F16: [[u16; 3]; 4] = [
        [0x3E00, 0x4000, 0x4200],
        [0x3C01, 0x3BFF, 0x0001],
        [0x7E11, 0x7D22, 0x7C00],
        [0x0400, 0x7BFF, 0x4000],
    ];
    const F32: [[u32; 3]; 4] = [
        [0x3FC0_0000, 0x4000_0000, 0x4040_0000],
        [0x3F80_0001, 0x3F7F_FFFF, 0x3380_0000],
        [0x7FC0_0011, 0x7F80_0022, 0x7F80_0000],
        [0x0080_0000, 0x7F7F_FFFF, 0x4000_0000],
    ];
    const F64: [[u64; 3]; 4] = [
        [
            0x3FF8_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x4008_0000_0000_0000,
        ],
        [
            0x3FF0_0000_0000_0001,
            0x3FEF_FFFF_FFFF_FFFF,
            0x3CA0_0000_0000_0000,
        ],
        [
            0x7FF8_0000_0000_0011,
            0x7FF0_0000_0000_0022,
            0x7FF0_0000_0000_0000,
        ],
        [
            0x0010_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0x4000_0000_0000_0000,
        ],
    ];
    match format {
        ScalarFormat::F16 => u64::from(F16[data_case % F16.len()][role]),
        ScalarFormat::F32 => u64::from(F32[data_case % F32.len()][role]),
        ScalarFormat::F64 => F64[data_case % F64.len()][role],
    }
}

fn source_words(format: ScalarFormat, data_case: usize, role: usize) -> [u64; 8] {
    let mut bytes = [0xA5; 64];
    let scalar = scalar_bits(format, data_case, role);
    bytes[..format.memory_size()].copy_from_slice(&scalar.to_le_bytes()[..format.memory_size()]);
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn full_guest_regs(case: ScalarFmaCase, ordinal: usize, data_case: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        // Mask every exception. Vary status and rounding while leaving
        // DAZ/FTZ clear for interpreter/native portability.
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

    let mut set_low_scalar = |index: u8, role: usize| {
        let scalar = scalar_bits(case.format, data_case, role);
        let mask = match case.format {
            ScalarFormat::F16 => u64::from(u16::MAX),
            ScalarFormat::F32 => u64::from(u32::MAX),
            ScalarFormat::F64 => u64::MAX,
        };
        let word = &mut registers.zmm[usize::from(index)][0];
        *word = (*word & !mask) | scalar;
    };
    set_low_scalar(case.destination(), 0);
    if case.source1() != case.destination() {
        set_low_scalar(case.source1(), 1);
    }
    if let Some(base) = case.base() {
        registers.gpr[usize::from(base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x100;
    }
    if let Some(index) = case.index() {
        registers.gpr[usize::from(index)] = 0x300 + ((ordinal & 0x0F) as u64) * 0x20;
    }
    registers
}

fn memory_address(case: ScalarFmaCase, registers: &GuestRegs) -> u64 {
    let compressed_displacement =
        u64::from(DISP8) * u64::try_from(case.format.memory_size()).unwrap();
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

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    format: ScalarFormat,
) -> GuestRegs {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.fs_base = initial.fs_base;
        x86.gs_base = initial.gs_base;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(source) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    memory.load(address as usize, &bytes[..format.memory_size()]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected
}

#[test]
fn interpreter_o0_o1_o2_match_all_1008_opcode_format_llig_and_address_shapes() {
    let cases = all_cases();
    assert_eq!(cases.len(), 12 * 3 * 4 * 7);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = source_words(case.format, 0, 2);
        let alternate_source = source_words(case.format, 0, 1);
        let initial = full_guest_regs(case, ordinal, 0);
        let address = memory_address(case, &initial);
        assert!(
            address + case.format.memory_size() as u64 <= 0x10000,
            "{case:?}: address {address:#x}"
        );
        let expected = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            source,
            address,
            case.format,
        );
        let alternate = interpreter_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            alternate_source,
            address,
            case.format,
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
                case.format,
            );
            assert_eq!(actual, expected, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert_eq!(executions, 1008 * LEVELS.len());
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<ScalarFmaCase> {
    let mut cases = Vec::new();
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3 {
                for form in MemoryForm::NATIVE {
                    cases.push(ScalarFmaCase {
                        opcode,
                        format,
                        ll,
                        form,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_scalar_evex_fma3_memory_matches_interpretation_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native scalar EVEX FMA3 memory differential: host lacks AVX512F/BW");
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 12 * 3 * 4 * 3);
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        if case.format == ScalarFormat::F16 && !std::is_x86_feature_detected!("avx512fp16") {
            continue;
        }
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let data_case = ordinal;
            let source = source_words(case.format, data_case, 2);
            let scalar = scalar_bits(case.format, data_case, 2);
            let mut context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = full_guest_regs(case, ordinal, data_case);
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected =
                interpreter_success(&function, &registers, source, address, case.format);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = full_guest_regs(case, ordinal, data_case);
            fault_registers.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}");
            assert_eq!(fault_context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                fault_context.last_size,
                case.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, faults);
    assert!(successes >= 12 * 2 * 4 * 3 * NATIVE_LEVELS.len());
}
