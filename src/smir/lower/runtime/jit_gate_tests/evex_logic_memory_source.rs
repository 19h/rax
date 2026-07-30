//! Exact helper-backed unmasked EVEX packed-logical memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_logic_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE680;
const DISP8: u8 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogicKind {
    And,
    AndNot,
    Or,
    Xor,
}

impl LogicKind {
    const ALL: [Self; 4] = [Self::And, Self::AndNot, Self::Or, Self::Xor];

    const fn opcode(self, family: LogicFamily) -> u8 {
        match (self, family.is_integer()) {
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

    const fn apply(self, source1: u64, source2: u64) -> u64 {
        match self {
            Self::And => source1 & source2,
            Self::AndNot => !source1 & source2,
            Self::Or => source1 | source2,
            Self::Xor => source1 ^ source2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogicFamily {
    Ps,
    Pd,
    D,
    Q,
}

impl LogicFamily {
    const ALL: [Self; 4] = [Self::Ps, Self::Pd, Self::D, Self::Q];

    const fn elem(self) -> VecElementType {
        match self {
            Self::Ps => VecElementType::F32,
            Self::Pd => VecElementType::F64,
            Self::D => VecElementType::I32,
            Self::Q => VecElementType::I64,
        }
    }

    const fn is_integer(self) -> bool {
        matches!(self, Self::D | Self::Q)
    }

    const fn pp(self) -> u8 {
        if matches!(self, Self::Ps) { 0 } else { 1 }
    }

    const fn prefix(self) -> X86SsePrefix {
        if matches!(self, Self::Ps) {
            X86SsePrefix::None
        } else {
            X86SsePrefix::OpSize
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::Pd | Self::Q)
    }

    const fn needs_avx512dq(self) -> bool {
        matches!(self, Self::Ps | Self::Pd)
    }
}

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
struct LogicCase {
    kind: LogicKind,
    family: LogicFamily,
    width: VecWidth,
    form: MemoryForm,
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

    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn scratch(self) -> u8 {
        if self.destination() != 0 && self.source1() != 0 {
            0
        } else if self.destination() != 1 && self.source1() != 1 {
            1
        } else {
            2
        }
    }

    fn bytes(self) -> [u8; 7] {
        let destination = self.destination();
        let source1 = self.source1();
        let base = self.base();
        [
            0x62,
            (if destination & 8 == 0 { 0x80 } else { 0 })
                | 0x40
                | (if base & 8 == 0 { 0x20 } else { 0 })
                | (if destination & 16 == 0 { 0x10 } else { 0 })
                | 0x01,
            (u8::from(self.family.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x04 | self.family.pp(),
            (self.ll() << 5) | (if source1 & 16 == 0 { 0x08 } else { 0 }),
            self.kind.opcode(self.family),
            0x40 | ((destination & 7) << 3) | (base & 7),
            DISP8,
        ]
    }

    fn register_instruction(self) -> [u8; 6] {
        let bytes = self.bytes();
        let scratch = self.scratch();
        [
            0x62,
            (bytes[1] & 0x97)
                | (u8::from(scratch & 0x10 == 0) << 6)
                | (u8::from(scratch & 0x08 == 0) << 5),
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            0xC0 | (bytes[5] & 0x38) | (scratch & 7),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!(),
    }))
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
        X86InstructionBytes::new(&bytes).expect("EVEX logical memory provenance"),
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

fn sequence(function: &SmirFunction) -> X86JitEvexLogicMemorySequence {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_logic_memory_sequence(
        &function.blocks[0],
        0,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .expect("exact EVEX logical memory sequence")
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
        case.family.needs_avx512dq(),
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
            && (!case.family.needs_avx512dq() || std::is_x86_feature_detected!("avx512dq"))
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
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed EVEX logic: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX logic"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<LogicCase> {
    let mut cases = Vec::new();
    for kind in LogicKind::ALL {
        for family in LogicFamily::ALL {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for form in MemoryForm::ALL {
                    cases.push(LogicCase {
                        kind,
                        family,
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
fn evex_logic_memory_byte_classifier_exhaustively_rewrites_49_152_register_pairs() {
    let mut accepted = 0usize;
    for kind in LogicKind::ALL {
        for family in LogicFamily::ALL {
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
                            | 0x01;
                        let p1 = (u8::from(family.w()) << 7)
                            | (((!source1) & 0x0F) << 3)
                            | 0x04
                            | family.pp();
                        let p2 = (ll << 5) | (if source1 & 16 == 0 { 0x08 } else { 0 });
                        let bytes = [
                            0x62,
                            p0,
                            p1,
                            p2,
                            kind.opcode(family),
                            0x40 | ((destination & 7) << 3) | 3,
                            DISP8,
                        ];
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_logic_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        let scratch = (0..32u8)
                            .find(|index| *index != destination && *index != source1)
                            .unwrap();
                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                        assert_eq!(encoding.elem, family.elem(), "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                        assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                        assert_eq!(
                            encoding.needs_avx512dq,
                            family.needs_avx512dq(),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.register_instruction.as_slice(),
                            [
                                0x62,
                                (p0 & 0x97)
                                    | (u8::from(scratch & 0x10 == 0) << 6)
                                    | (u8::from(scratch & 0x08 == 0) << 5),
                                p1,
                                p2,
                                kind.opcode(family),
                                0xC0 | ((destination & 7) << 3) | (scratch & 7),
                            ],
                            "{bytes:02X?}"
                        );
                        accepted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 4 * 4 * 3 * 32 * 32);
}

#[test]
fn evex_logic_memory_rewrite_matches_independent_llvm_23_encodings() {
    for (memory, register) in [
        (
            &[0x62, 0xC1, 0x6C, 0x00, 0x54, 0x4B, 0x02][..],
            &[0x62, 0xE1, 0x6C, 0x00, 0x54, 0xC8][..],
        ),
        (
            &[0x62, 0xC1, 0xED, 0x20, 0x55, 0x4D, 0x02][..],
            &[0x62, 0xE1, 0xED, 0x20, 0x55, 0xC8][..],
        ),
        (
            &[0x62, 0x41, 0x8D, 0x40, 0xEB, 0x78, 0x02][..],
            &[0x62, 0x61, 0x8D, 0x40, 0xEB, 0xF8][..],
        ),
        (
            &[0x62, 0xF1, 0x75, 0x48, 0xEF, 0x02][..],
            &[0x62, 0xF1, 0x75, 0x48, 0xEF, 0xC2][..],
        ),
        (
            &[0x62, 0x41, 0x35, 0x00, 0xDB, 0x43, 0x01][..],
            &[0x62, 0x61, 0x35, 0x00, 0xDB, 0xC0][..],
        ),
        (
            &[0x62, 0x41, 0x8D, 0x20, 0xEF, 0x78, 0x02][..],
            &[0x62, 0x61, 0x8D, 0x20, 0xEF, 0xF8][..],
        ),
    ] {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_logic_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), register);
    }
}

#[test]
fn evex_logic_memory_classifier_rejects_mask_broadcast_reserved_and_trailing_shapes() {
    let valid = LogicCase {
        kind: LogicKind::And,
        family: LogicFamily::Ps,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    }
    .bytes()
    .to_vec();
    let mut malformed = Vec::new();
    malformed.push(valid[..valid.len() - 1].to_vec());
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
        (2, 0x80), // PS with W=1
        (2, 0x01), // PS with 66
        (3, 0x80), // zeroing
        (3, 0x10), // broadcast
        (3, 0x01), // writemask
        (4, 0x08), // nonlogical opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut forbidden_legacy_prefix = valid.clone();
    forbidden_legacy_prefix.insert(0, 0x66);
    malformed.push(forbidden_legacy_prefix);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_logic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let prefixed_encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_logic_memory_encoding()
        .expect("FS/address-size prefixes belong only to helper address evaluation");
    let plain_encoding = X86InstructionBytes::new(&valid)
        .unwrap()
        .evex_logic_memory_encoding()
        .unwrap();
    assert_eq!(
        prefixed_encoding.register_instruction,
        plain_encoding.register_instruction
    );
}

#[test]
fn all_144_evex_logic_memory_shapes_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 4 * 4 * 3 * 3);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence = sequence(&function);
            assert_eq!(sequence.consumed, 2, "{level:?} {case:?}");
            assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.elem,
                case.family.elem(),
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
            assert_eq!(
                sequence.encoding.scratch,
                case.scratch(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.register_instruction(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let expected = case.register_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector transfer slot"
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 144 * LEVELS.len());
}

fn initial_registers(case: LogicCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
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
    registers.gpr[usize::from(case.base())] = 0x4000 + ((ordinal & 0x0F) as u64) * 0x100;
    registers
}

fn source_value(ordinal: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0xFF00_0F0F_CCCC_AAAAu64.rotate_right((word * 13 + ordinal * 3) as u32)
    })
}

fn memory_address(case: LogicCase, registers: &GuestRegs) -> u64 {
    registers.gpr[usize::from(case.base())] + u64::from(DISP8) * u64::from(case.width.bytes())
}

fn expected_destination(initial: &GuestRegs, source: [u64; 8], case: LogicCase) -> [u64; 8] {
    let source1 = initial.zmm[usize::from(case.source1())];
    let words = (case.width.bytes() / 8) as usize;
    std::array::from_fn(|word| {
        if word < words {
            case.kind.apply(source1[word], source[word])
        } else {
            0
        }
    })
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
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
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(source) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
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
fn interpreter_o0_o1_o2_matches_raw_bit_model_for_all_logic_widths_and_aliases() {
    let mut executions = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        let source = source_value(ordinal);
        let address = memory_address(case, &initial);
        let manual = expected_destination(&initial, source, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, source, address, case);
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
            assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert_eq!(executions, 144 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_logic_memory_sequence(
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
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed graph"
    );
}

#[test]
fn evex_logic_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = LogicCase {
        kind: LogicKind::And,
        family: LogicFamily::Ps,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_logic_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut mutations = Vec::<(&str, SmirFunction)>::new();
    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    mutations.push(("missing metadata", missing_metadata));

    let mut masked_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[3] |= 1;
    masked_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("masked metadata", masked_metadata));

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: loaded,
            width: VecWidth::V128,
        },
    ));
    mutations.push(("loaded virtual used twice", extra_use));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }
    mutations.push(("load width", load_width));

    let mut load_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut load_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    mutations.push(("virtual load address", load_address));

    let mut wrong_source = base.clone();
    if let OpKind::VAnd { src2, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src2 = vector(2, VecWidth::V128);
    }
    mutations.push(("consumer bypasses load", wrong_source));

    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0x55,
        width: VecWidth::V128,
        w: false,
    });
    mutations.push(("consumer opcode hint", wrong_hint));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    mutations.push(("different guest PCs", wrong_pc));

    let mut extra_same_pc = base.clone();
    extra_same_pc.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: vector(3, VecWidth::V128),
            width: VecWidth::V128,
        },
    ));
    mutations.push(("trailing same-PC operation", extra_same_pc));

    for (name, function) in mutations {
        assert_rejected(name, &function);
    }
}

#[test]
fn evex_logic_memory_rejects_the_avx_only_vector_bridge() {
    let case = LogicCase {
        kind: LogicKind::Xor,
        family: LogicFamily::Q,
        width: VecWidth::V256,
        form: MemoryForm::High,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32 | 64)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_evex_logic_memory_matches_interpretation_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
        || !std::is_x86_feature_detected!("avx512vl")
    {
        eprintln!("skipping native EVEX logic memory: host lacks AVX-512F/BW/DQ/VL");
        return;
    }

    let selected = [
        LogicCase {
            kind: LogicKind::And,
            family: LogicFamily::Ps,
            width: VecWidth::V128,
            form: MemoryForm::Low,
        },
        LogicCase {
            kind: LogicKind::AndNot,
            family: LogicFamily::Pd,
            width: VecWidth::V256,
            form: MemoryForm::High,
        },
        LogicCase {
            kind: LogicKind::Or,
            family: LogicFamily::D,
            width: VecWidth::V512,
            form: MemoryForm::DestinationSourceAlias,
        },
        LogicCase {
            kind: LogicKind::Xor,
            family: LogicFamily::Q,
            width: VecWidth::V512,
            form: MemoryForm::High,
        },
    ];
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in selected.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_value(ordinal);

            let mut context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = initial_registers(case, ordinal);
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = interpreter_success(&function, &initial, source, address, case);
            expected.vector_scratch = std::array::from_fn(|word| {
                if word < (case.width.bytes() / 8) as usize {
                    source[word]
                } else {
                    0
                }
            });

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

            let mut fault_context = VectorMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut fault_registers = initial_registers(case, ordinal ^ 0x55);
            let fault_address = memory_address(case, &fault_registers);
            fault_registers.ctx = (&mut fault_context as *mut VectorMemoryContext) as u64;
            fault_registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault");
            assert_eq!(
                fault_context.last_addr, fault_address,
                "{level:?} {case:?}: fault"
            );
            assert_eq!(
                fault_context.last_size,
                case.width.bytes(),
                "{level:?} {case:?}: fault"
            );
            faults += 1;
        }
    }
    assert_eq!(successes, selected.len() * 2);
    assert_eq!(faults, selected.len() * 2);
}
