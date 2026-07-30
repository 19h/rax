//! Exact helper-backed writemasked EVEX packed-logical memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexLogicMemoryKind, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_masked_logic_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE580;
const DISP8: u8 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryForm {
    Low,
    High,
    DestinationSourceAlias,
}

impl MemoryForm {
    const ALL: [Self; 3] = [Self::Low, Self::High, Self::DestinationSourceAlias];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogicKind {
    And,
    AndNot,
    Or,
    Xor,
}

impl LogicKind {
    const ALL: [Self; 4] = [Self::And, Self::AndNot, Self::Or, Self::Xor];

    const fn production(self) -> X86EvexLogicMemoryKind {
        match self {
            Self::And => X86EvexLogicMemoryKind::And,
            Self::AndNot => X86EvexLogicMemoryKind::AndNot,
            Self::Or => X86EvexLogicMemoryKind::Or,
            Self::Xor => X86EvexLogicMemoryKind::Xor,
        }
    }

    const fn opcode(self, elem: VecElementType) -> u8 {
        let integer = matches!(elem, VecElementType::I32 | VecElementType::I64);
        match (self, integer) {
            (Self::And, false) => 0x54,
            (Self::AndNot, false) => 0x55,
            (Self::Or, false) => 0x56,
            (Self::Xor, false) => 0x57,
            (Self::And, true) => 0xDB,
            (Self::AndNot, true) => 0xDF,
            (Self::Or, true) => 0xEB,
            (Self::Xor, true) => 0xEF,
        }
    }

    const fn apply_byte(self, source1: u8, source2: u8) -> u8 {
        match self {
            Self::And => source1 & source2,
            Self::AndNot => !source1 & source2,
            Self::Or => source1 | source2,
            Self::Xor => source1 ^ source2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LogicCase {
    kind: LogicKind,
    elem: VecElementType,
    width: VecWidth,
    form: MemoryForm,
    mask: u8,
    zeroing: bool,
}

impl LogicCase {
    const fn destination(self) -> u8 {
        match self.form {
            MemoryForm::Low => 0,
            MemoryForm::High => 24,
            MemoryForm::DestinationSourceAlias => 17,
        }
    }

    const fn source1(self) -> u8 {
        match self.form {
            MemoryForm::Low => 1,
            MemoryForm::High => 25,
            MemoryForm::DestinationSourceAlias => 17,
        }
    }

    const fn base(self) -> u8 {
        match self.form {
            MemoryForm::Low => 3,
            MemoryForm::High | MemoryForm::DestinationSourceAlias => 11,
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self.elem {
            VecElementType::F32 | VecElementType::I32 => MemWidth::B4,
            VecElementType::F64 | VecElementType::I64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    const fn needs_avx512dq(self) -> bool {
        matches!(self.elem, VecElementType::F32 | VecElementType::F64)
    }

    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> [u8; 7] {
        let destination = self.destination();
        let source1 = self.source1();
        let base = self.base();
        let w = matches!(self.elem, VecElementType::F64 | VecElementType::I64);
        let pp = if self.elem == VecElementType::F32 {
            0
        } else {
            1
        };
        [
            0x62,
            (if destination & 8 == 0 { 0x80 } else { 0 })
                | 0x40
                | (if base & 8 == 0 { 0x20 } else { 0 })
                | (if destination & 16 == 0 { 0x10 } else { 0 })
                | 0x01,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x04 | pp,
            (u8::from(self.zeroing) << 7)
                | (self.ll() << 5)
                | (if source1 & 16 == 0 { 0x08 } else { 0 })
                | self.mask,
            self.kind.opcode(self.elem),
            0x40 | ((destination & 7) << 3) | (base & 7),
            DISP8,
        ]
    }

    fn stack_instruction(self) -> [u8; 7] {
        let bytes = self.bytes();
        [
            0x62,
            (bytes[1] & 0x97) | 0x60,
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            (bytes[5] & 0x38) | 0x04,
            0x24,
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    match width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(index))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(index))),
        _ => unreachable!(),
    }
}

fn lift_case(case: LogicCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("masked EVEX logic provenance"),
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
    case: LogicCase,
    level: OptLevel,
) -> X86JitEvexMaskedLogicMemorySequence {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_masked_logic_memory_sequence(
        &function.blocks[0],
        0,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .unwrap_or_else(|| {
        panic!(
            "{level:?} {case:?}: exact masked EVEX logic sequence\n{:#?}",
            function.blocks[0].ops
        )
    })
}

fn lower(function: &SmirFunction, case: LogicCase) -> (Vec<u8>, usize) {
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
        requirements.needs_avx512dq,
        case.needs_avx512dq(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (!case.needs_avx512dq() || std::is_x86_feature_detected!("avx512dq"))
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
        .unwrap_or_else(|error| panic!("{case:?}: masked EVEX logic lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed masked EVEX logic"),
        result.entry_offset,
    )
}

#[test]
fn evex_masked_logic_byte_classifier_exhaustively_rewrites_688_128_operands() {
    let mut accepted = 0usize;
    for kind in LogicKind::ALL {
        for elem in [
            VecElementType::F32,
            VecElementType::F64,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for (ll, width) in [
                (0, VecWidth::V128),
                (1, VecWidth::V256),
                (2, VecWidth::V512),
            ] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for mask in 1..=7u8 {
                            for zeroing in [false, true] {
                                let w = matches!(elem, VecElementType::F64 | VecElementType::I64);
                                let pp = if elem == VecElementType::F32 { 0 } else { 1 };
                                let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                                    | 0x60
                                    | (if destination & 16 == 0 { 0x10 } else { 0 })
                                    | 0x01;
                                let p1 =
                                    (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x04 | pp;
                                let p2 = (u8::from(zeroing) << 7)
                                    | (ll << 5)
                                    | (if source1 & 16 == 0 { 0x08 } else { 0 })
                                    | mask;
                                let opcode = kind.opcode(elem);
                                let bytes = [
                                    0x62,
                                    p0,
                                    p1,
                                    p2,
                                    opcode,
                                    ((destination & 7) << 3) | 0x04,
                                    0x24,
                                ];
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_masked_logic_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(encoding.kind, kind.production(), "{bytes:02X?}");
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, mask, "{bytes:02X?}");
                                assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.memory_width,
                                    if w { MemWidth::B8 } else { MemWidth::B4 },
                                    "{bytes:02X?}"
                                );
                                assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.needs_avx512dq,
                                    matches!(elem, VecElementType::F32 | VecElementType::F64),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(encoding.stack_instruction.as_slice(), bytes);
                                accepted += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 4 * 4 * 3 * 32 * 32 * 7 * 2);
}

#[test]
fn evex_masked_logic_rewrite_matches_independent_llvm_23_encodings() {
    for (bytes, expected) in [
        (
            &[0x62, 0xF1, 0x6C, 0x09, 0x54, 0x0F][..],
            &[0x62, 0xF1, 0x6C, 0x09, 0x54, 0x0C, 0x24][..],
        ),
        (
            &[0x62, 0x41, 0x2C, 0xAB, 0x55, 0x4B, 0x08][..],
            &[0x62, 0x61, 0x2C, 0xAB, 0x55, 0x0C, 0x24][..],
        ),
        (
            &[0x62, 0x41, 0x85, 0x89, 0x56, 0x7E, 0xFF][..],
            &[0x62, 0x61, 0x85, 0x89, 0x56, 0x3C, 0x24][..],
        ),
        (
            &[0x62, 0xF1, 0xE5, 0x4A, 0x57, 0x12][..],
            &[0x62, 0xF1, 0xE5, 0x4A, 0x57, 0x14, 0x24][..],
        ),
        (
            &[0x62, 0xF1, 0x55, 0x2C, 0xDB, 0x64, 0x24, 0x01][..],
            &[0x62, 0xF1, 0x55, 0x2C, 0xDB, 0x24, 0x24][..],
        ),
        (
            &[0x62, 0xC1, 0xD5, 0xC6, 0xDF, 0x65, 0x02][..],
            &[0x62, 0xE1, 0xD5, 0xC6, 0xDF, 0x24, 0x24][..],
        ),
        (
            &[0x62, 0xD1, 0x3D, 0x0F, 0xEB, 0x3C, 0x24][..],
            &[0x62, 0xF1, 0x3D, 0x0F, 0xEB, 0x3C, 0x24][..],
        ),
        (
            &[0x62, 0x61, 0x95, 0xC2, 0xEF, 0x36][..],
            &[0x62, 0x61, 0x95, 0xC2, 0xEF, 0x34, 0x24][..],
        ),
    ] {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_masked_logic_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(encoding.stack_instruction.as_slice(), expected);
    }
}

#[test]
fn evex_masked_logic_classifier_rejects_reserved_unmasked_broadcast_and_trailing_shapes() {
    let valid = LogicCase {
        kind: LogicKind::Xor,
        elem: VecElementType::F32,
        width: VecWidth::V128,
        form: MemoryForm::Low,
        mask: 1,
        zeroing: false,
    }
    .bytes()
    .to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    register.truncate(6);
    malformed.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x04), // EVEX.U
        (2, 0x80), // W without 66
        (2, 0x01), // 66 without W
        (3, 0x10), // scalar broadcast
        (4, 0x08), // non-logical opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut unmasked = valid.clone();
    unmasked[3] &= !7;
    malformed.push(unmasked);
    let mut forbidden_legacy_prefix = valid.clone();
    forbidden_legacy_prefix.insert(0, 0x66);
    malformed.push(forbidden_legacy_prefix);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_masked_logic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_masked_logic_memory_encoding()
        .expect("FS/address-size prefixes belong only to helper address evaluation");
    assert_eq!(
        encoding.stack_instruction.as_slice(),
        X86InstructionBytes::new(&valid)
            .unwrap()
            .evex_masked_logic_memory_encoding()
            .unwrap()
            .stack_instruction
            .as_slice()
    );
}

#[test]
fn all_2_016_masked_evex_logic_shapes_optimize_admit_and_lower_exactly() {
    let mut cases = 0usize;
    let mut lowerings = 0usize;
    for kind in LogicKind::ALL {
        for elem in [
            VecElementType::F32,
            VecElementType::F64,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::ALL {
                    for mask in 1..=7 {
                        for zeroing in [false, true] {
                            let case = LogicCase {
                                kind,
                                elem,
                                width,
                                form,
                                mask,
                                zeroing,
                            };
                            cases += 1;
                            for level in LEVELS {
                                let function = optimize(lift_case(case), level);
                                let sequence = sequence(&function, case, level);
                                assert_eq!(sequence.address_offset, 2, "{level:?} {case:?}");
                                assert_eq!(sequence.encoding.kind, kind.production());
                                assert_eq!(sequence.encoding.elem, elem);
                                assert_eq!(sequence.encoding.destination, case.destination());
                                assert_eq!(sequence.encoding.source1, case.source1());
                                assert_eq!(sequence.encoding.writemask, mask);
                                assert_eq!(sequence.encoding.zeroing, zeroing);
                                assert_eq!(sequence.encoding.needs_avx512dq, case.needs_avx512dq());
                                assert_eq!(
                                    sequence.encoding.stack_instruction.as_slice(),
                                    case.stack_instruction(),
                                    "{level:?} {case:?}"
                                );

                                let (code, _) = lower(&function, case);
                                let expected = case.stack_instruction();
                                assert!(
                                    code.windows(expected.len())
                                        .any(|window| window == expected),
                                    "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
                                );
                                lowerings += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 4 * 4 * 3 * 3 * 7 * 2);
    assert_eq!(lowerings, cases * LEVELS.len());
}

#[test]
fn masked_evex_logic_lowering_has_one_live_k_guard_per_lane_and_rejects_avx_only_bridge() {
    let case = LogicCase {
        kind: LogicKind::AndNot,
        elem: VecElementType::F32,
        width: VecWidth::V512,
        form: MemoryForm::High,
        mask: 3,
        zeroing: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let (code, _) = lower(&function, case);
    let mut guards = 0usize;
    for lane in 0..16 {
        let lane_mask = (1u32 << lane).to_le_bytes();
        let guard = [
            0x9C,
            0x50,
            0xC4,
            0xE1,
            0xFB,
            0x93,
            0xC0 | case.mask,
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
            "lane {lane}: {guard:02X?}"
        );
        guards += 1;
    }
    assert_eq!(guards, 16);
    let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
    let release_frame = [0x48, 0x8D, 0x64, 0x24, 0x50];
    assert_eq!(
        code.windows(allocate_frame.len())
            .filter(|window| *window == allocate_frame)
            .count(),
        1,
        "one 80-byte staging-frame allocation"
    );
    assert_eq!(
        code.windows(release_frame.len())
            .filter(|window| *window == release_frame)
            .count(),
        17,
        "one success cleanup plus one fault cleanup per lane"
    );
    assert!(
        code.windows(5)
            .any(|window| window == [0x48, 0x89, 0x44, 0x24, 0x4C]),
        "final 4-byte lane helper must own its complete 8-byte return at stack offset 76"
    );

    let mut avx_only = X86_64Lowerer::new();
    avx_only.set_mem_helpers(true);
    avx_only.set_preserve_vector_mem_helpers(true);
    avx_only.set_avx_ymm16_vector_state(true);
    let error = avx_only
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

fn initial_registers(case: LogicCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x20)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| {
            0x8000_0000_0000_0000u64 | 0xA55Au64.rotate_left((index * 5) as u32)
        }),
        vector_active: X86_VECTOR_STATE_K64,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x100;
    registers
}

fn memory_address(case: LogicCase, registers: &GuestRegs) -> u64 {
    registers.gpr[usize::from(case.base())] + u64::from(DISP8) * u64::from(case.width.bytes())
}

fn memory_value(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|index| {
        (index as u8)
            .wrapping_mul(0x3D)
            .wrapping_add((ordinal as u8).wrapping_mul(0x27))
            ^ 0xA5
    })
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: &[u8; 64],
    address: u64,
    case: LogicCase,
) -> GuestRegs {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    memory.load(
        address as usize,
        &memory_value[..case.width.bytes() as usize],
    );
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
    expected
}

fn manual_destination(initial: &GuestRegs, memory_value: &[u8; 64], case: LogicCase) -> [u64; 8] {
    let mut destination = [0u8; 64];
    let mut source1 = [0u8; 64];
    for word in 0..8 {
        destination[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.destination())][word].to_le_bytes());
        source1[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.source1())][word].to_le_bytes());
    }
    let element_bytes = case.memory_width().bytes() as usize;
    let lanes = case.width.bytes() as usize / element_bytes;
    let mask = initial.k[usize::from(case.mask)];
    for lane in 0..lanes {
        let start = lane * element_bytes;
        if mask & (1 << lane) != 0 {
            for byte in 0..element_bytes {
                destination[start + byte] = case
                    .kind
                    .apply_byte(source1[start + byte], memory_value[start + byte]);
            }
        } else if case.zeroing {
            destination[start..start + element_bytes].fill(0);
        }
    }
    destination[case.width.bytes() as usize..].fill(0);
    std::array::from_fn(|word| {
        u64::from_le_bytes(destination[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

#[test]
fn interpreter_o0_o1_o2_matches_raw_bit_model_for_masked_full_vectors() {
    let masks = [(1, false), (3, true), (7, false)];
    let mut executions = 0usize;
    let mut ordinal = 0usize;
    for kind in LogicKind::ALL {
        for elem in [
            VecElementType::F32,
            VecElementType::F64,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::ALL {
                    for (mask, zeroing) in masks {
                        let case = LogicCase {
                            kind,
                            elem,
                            width,
                            form,
                            mask,
                            zeroing,
                        };
                        let mut initial = initial_registers(case, ordinal);
                        initial.k[usize::from(mask)] = match ordinal % 3 {
                            0 => 0,
                            1 => 0xAAAA_AAAA_AAAA_AAAA,
                            _ => u64::MAX,
                        };
                        let address = memory_address(case, &initial);
                        let memory_value = memory_value(ordinal);
                        let manual = manual_destination(&initial, &memory_value, case);
                        for level in LEVELS {
                            let function = optimize(lift_case(case), level);
                            let actual = interpreter_success(
                                &function,
                                &initial,
                                &memory_value,
                                address,
                                case,
                            );
                            assert_eq!(
                                actual.zmm[usize::from(case.destination())],
                                manual,
                                "{level:?} {case:?}"
                            );
                            for index in 0..32 {
                                if index != usize::from(case.destination()) {
                                    assert_eq!(
                                        actual.zmm[index], initial.zmm[index],
                                        "{level:?} {case:?}: clobbered ZMM{index}"
                                    );
                                }
                            }
                            assert_eq!(actual.gpr, initial.gpr, "{level:?} {case:?}");
                            assert_eq!(actual.k, initial.k, "{level:?} {case:?}");
                            assert_eq!(actual.rflags, initial.rflags, "{level:?} {case:?}");
                            executions += 1;
                        }
                        ordinal += 1;
                    }
                }
            }
        }
    }
    assert_eq!(executions, 4 * 4 * 3 * 3 * masks.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_masked_logic_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: sequence classifier admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed graph"
    );
}

#[test]
fn masked_evex_logic_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = LogicCase {
        kind: LogicKind::Xor,
        elem: VecElementType::F32,
        width: VecWidth::V128,
        form: MemoryForm::Low,
        mask: 3,
        zeroing: true,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_masked_logic_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut mutations = Vec::<(&str, SmirFunction)>::new();
    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    mutations.push(("missing metadata", missing_metadata));

    let mut metadata_source = base.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x08;
    metadata_source
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("metadata source", metadata_source));

    let mut lea_address = base.clone();
    if let OpKind::Lea { addr, .. } = &mut lea_address.blocks[0].ops[2].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFE)));
    }
    mutations.push(("virtual LEA address", lea_address));

    let load_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let mut load_width = base.clone();
    if let OpKind::PredLoad { width, .. } = &mut load_width.blocks[0].ops[load_index].kind {
        *width = MemWidth::B8;
    }
    mutations.push(("load width", load_width));

    let mut load_address = base.clone();
    if let OpKind::PredLoad { addr, .. } = &mut load_address.blocks[0].ops[load_index].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    mutations.push(("lane address", load_address));

    let xor_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VXor { .. }))
        .unwrap();
    let mut xor_hint = base.clone();
    xor_hint.blocks[0].ops[xor_index].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0x57,
        width: VecWidth::V128,
        w: false,
    });
    mutations.push(("logic hint", xor_hint));

    let mut extra_same_pc = base.clone();
    extra_same_pc.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::VMov {
            dst: vector(2, VecWidth::V128),
            src: vector(1, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    mutations.push(("trailing same-PC op", extra_same_pc));

    for (name, function) in mutations {
        assert_rejected(name, &function);
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
struct LaneMemoryContext {
    base: u64,
    value: [u8; 64],
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 16],
}

#[cfg(target_arch = "x86_64")]
extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(signed, 0);
    assert!(matches!(size, 4 | 8));
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    let size = size as usize;
    assert!(offset + size <= context.value.len());
    let mut bytes = [0u8; 8];
    bytes[..size].copy_from_slice(&context.value[offset..offset + size]);
    LoadResult {
        value: u64::from_le_bytes(bytes),
        ok: 1,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_masked_evex_logic_matches_interpretation_faults_and_lane_suppression() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native masked EVEX logic: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_dq = std::is_x86_feature_detected!("avx512dq");
    let selected = [
        (LogicKind::And, VecElementType::F32, VecWidth::V128, false),
        (LogicKind::AndNot, VecElementType::F64, VecWidth::V256, true),
        (LogicKind::Or, VecElementType::I32, VecWidth::V512, false),
        (LogicKind::Xor, VecElementType::I64, VecWidth::V512, true),
    ];
    let mut executions = 0usize;
    for (ordinal, (kind, elem, width, zeroing)) in selected
        .into_iter()
        .filter(|(_, elem, width, _)| {
            (*width == VecWidth::V512 || has_vl)
                && (!matches!(elem, VecElementType::F32 | VecElementType::F64) || has_dq)
        })
        .enumerate()
    {
        let case = LogicCase {
            kind,
            elem,
            width,
            form: if ordinal & 1 == 0 {
                MemoryForm::Low
            } else {
                MemoryForm::DestinationSourceAlias
            },
            mask: 3,
            zeroing,
        };
        let element_bytes = case.memory_width().bytes() as u64;
        let lanes = u64::from(case.width.bytes()) / element_bytes;
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let value = memory_value(ordinal);
            let mut registers = initial_registers(case, ordinal);
            registers.k[usize::from(case.mask)] = 0x5555;
            let address = memory_address(case, &registers);
            let mut context = LaneMemoryContext {
                base: address,
                value,
                fail_address: None,
                calls: 0,
                addresses: [0; 16],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, &value, address, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            let expected_addresses: Vec<u64> = (0..lanes)
                .filter(|lane| 0x5555 & (1 << lane) != 0)
                .map(|lane| address + lane * element_bytes)
                .collect();
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: active-lane order"
            );

            let mut fault_registers = initial_registers(case, ordinal ^ 0x55);
            fault_registers.k[usize::from(case.mask)] = 0b1101;
            let fault_base = memory_address(case, &fault_registers);
            let fail_lane = if lanes > 2 { 2 } else { 0 };
            let fail_address = fault_base + fail_lane * element_bytes;
            let mut fault_context = LaneMemoryContext {
                base: fault_base,
                value,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 16],
            };
            fault_registers.ctx = (&mut fault_context as *mut LaneMemoryContext) as u64;
            fault_registers.load_fn = lane_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed state"
            );
            assert_eq!(
                fault_context.addresses[fault_context.calls - 1],
                fail_address,
                "{level:?} {case:?}: fault lane"
            );

            let mut suppressed_registers = initial_registers(case, ordinal ^ 0xAA);
            suppressed_registers.k[usize::from(case.mask)] = 0;
            let suppressed_base = memory_address(case, &suppressed_registers);
            let mut suppressed_context = LaneMemoryContext {
                base: suppressed_base,
                value,
                fail_address: Some(suppressed_base),
                calls: 0,
                addresses: [0; 16],
            };
            suppressed_registers.ctx = (&mut suppressed_context as *mut LaneMemoryContext) as u64;
            suppressed_registers.load_fn = lane_load_helper as usize as u64;
            let mut suppressed_expected = interpreter_success(
                &function,
                &suppressed_registers,
                &value,
                suppressed_base,
                case,
            );

            exec.run(entry, &mut suppressed_registers);
            suppressed_expected.host_mxcsr = suppressed_registers.host_mxcsr;
            assert_eq!(
                suppressed_registers, suppressed_expected,
                "{level:?} {case:?}: all lanes suppressed"
            );
            assert_eq!(suppressed_context.calls, 0, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert!(executions >= 2);
}
