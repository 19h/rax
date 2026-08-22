//! Exact helper-backed EVEX variable VPERMILPS/PD memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, X86JitEvexVariablePermuteMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_evex_variable_permute_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE70C;
const DISP8: u8 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Permil {
    Ps,
    Pd,
}

impl Permil {
    const ALL: [Self; 2] = [Self::Ps, Self::Pd];

    const fn opcode(self) -> u8 {
        match self {
            Self::Ps => 0x0C,
            Self::Pd => 0x0D,
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::Ps => VecElementType::F32,
            Self::Pd => VecElementType::F64,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::Pd)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegisterForm {
    Low,
    High,
    DestinationSourceAlias,
}

impl RegisterForm {
    const ALL: [Self; 3] = [Self::Low, Self::High, Self::DestinationSourceAlias];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PermilCase {
    operation: Permil,
    width: VecWidth,
    form: RegisterForm,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
}

impl PermilCase {
    const fn destination(self) -> u8 {
        match self.form {
            RegisterForm::Low => 0,
            RegisterForm::High => 24,
            RegisterForm::DestinationSourceAlias => 17,
        }
    }

    const fn source1(self) -> u8 {
        match self.form {
            RegisterForm::Low => 1,
            RegisterForm::High => 25,
            RegisterForm::DestinationSourceAlias => 17,
        }
    }

    const fn base(self) -> u8 {
        match self.form {
            RegisterForm::Low => 3,
            RegisterForm::High | RegisterForm::DestinationSourceAlias => 11,
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

    fn scratch(self) -> u8 {
        (0..32u8)
            .find(|index| *index != self.destination() && *index != self.source1())
            .unwrap()
    }

    fn bytes(self) -> [u8; 7] {
        encoding_bytes(
            self.operation,
            self.width,
            self.destination(),
            self.source1(),
            self.base(),
            self.mask,
            self.zeroing,
            self.broadcast,
        )
    }

    fn register_instruction(self) -> [u8; 6] {
        let bytes = self.bytes();
        let scratch = self.scratch();
        [
            0x62,
            (bytes[1] & 0x97)
                | (u8::from(scratch & 0x10 == 0) << 6)
                | (u8::from(scratch & 0x08 == 0) << 5),
            bytes[2],
            bytes[3] & !0x10,
            bytes[4],
            0xC0 | (bytes[5] & 0x38) | (scratch & 7),
        ]
    }

    fn broadcast_instruction(self) -> Option<[u8; 6]> {
        if !self.broadcast {
            return None;
        }
        let scratch = self.scratch();
        Some([
            0x62,
            (u8::from(scratch & 8 == 0) << 7)
                | (u8::from(scratch & 16 == 0) << 6)
                | (u8::from(scratch & 8 == 0) << 5)
                | (u8::from(scratch & 16 == 0) << 4)
                | 2,
            (u8::from(self.operation.w()) << 7) | 0x7D,
            (self.ll() << 5) | 0x08,
            if self.operation == Permil::Ps {
                0x58
            } else {
                0x59
            },
            0xC0 | ((scratch & 7) << 3) | (scratch & 7),
        ])
    }

    fn memory_size(self) -> u32 {
        if self.broadcast {
            self.operation.elem().bytes()
        } else {
            self.width.bytes()
        }
    }
}

fn encoding_bytes(
    operation: Permil,
    width: VecWidth,
    destination: u8,
    source1: u8,
    base: u8,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
) -> [u8; 7] {
    assert!(destination < 32 && source1 < 32 && base < 16 && mask < 8);
    assert!(!zeroing || mask != 0);
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!(),
    };
    [
        0x62,
        (u8::from(destination & 8 == 0) << 7)
            | 0x40
            | (u8::from(base & 8 == 0) << 5)
            | (u8::from(destination & 16 == 0) << 4)
            | 2,
        (u8::from(operation.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | (u8::from(broadcast) << 4)
            | (u8::from(source1 & 16 == 0) << 3)
            | mask,
        operation.opcode(),
        0x40 | ((destination & 7) << 3) | (base & 7),
        DISP8,
    ]
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!(),
    }))
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("EVEX VPERMIL memory provenance"),
    );
    function
}

fn lift_case(case: PermilCase) -> SmirFunction {
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

fn classified(function: &SmirFunction) -> Option<X86JitEvexVariablePermuteMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_variable_permute_memory_sequence(
        &function.blocks[0],
        0,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: PermilCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed EVEX VPERMIL: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX VPERMIL"),
        result.entry_offset,
    )
}

fn representative_cases() -> Vec<PermilCase> {
    let mut cases = Vec::new();
    for operation in Permil::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in RegisterForm::ALL {
                for (mask, zeroing) in [(0, false), (3, false), (7, true)] {
                    for broadcast in [false, true] {
                        cases.push(PermilCase {
                            operation,
                            width,
                            form,
                            mask,
                            zeroing,
                            broadcast,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn evex_variable_permil_memory_classifier_exhausts_184_320_field_cells() {
    let mut accepted = 0usize;
    for operation in Permil::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    for mask in 0..8u8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            for broadcast in [false, true] {
                                let bytes = encoding_bytes(
                                    operation,
                                    width,
                                    destination,
                                    source1,
                                    3,
                                    mask,
                                    zeroing,
                                    broadcast,
                                );
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_variable_permute_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                let scratch = (0..32u8)
                                    .find(|index| *index != destination && *index != source1)
                                    .unwrap();
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, operation.elem(), "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, mask, "{bytes:02X?}");
                                assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(encoding.broadcast, broadcast, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.memory_size,
                                    if broadcast {
                                        operation.elem().bytes()
                                    } else {
                                        width.bytes()
                                    },
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoding.needs_avx512vl,
                                    width != VecWidth::V512,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoding.register_instruction.as_slice(),
                                    [
                                        0x62,
                                        (bytes[1] & 0x97)
                                            | (u8::from(scratch & 0x10 == 0) << 6)
                                            | (u8::from(scratch & 0x08 == 0) << 5),
                                        bytes[2],
                                        bytes[3] & !0x10,
                                        bytes[4],
                                        0xC0 | (bytes[5] & 0x38) | (scratch & 7),
                                    ],
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
    assert_eq!(accepted, 2 * 3 * 32 * 32 * 15 * 2);
}

#[test]
fn evex_variable_permil_rewrite_matches_independent_llvm_23_encodings() {
    for (memory, register) in [
        (
            &[0x62, 0xC2, 0x5D, 0x83, 0x0C, 0x4B, 0x02][..],
            &[0x62, 0xE2, 0x5D, 0x83, 0x0C, 0xC8][..],
        ),
        (
            &[0x62, 0x52, 0xAD, 0x37, 0x0D, 0x4E, 0x08][..],
            &[0x62, 0x72, 0xAD, 0x27, 0x0D, 0xC8][..],
        ),
        (
            &[0x62, 0x42, 0x7D, 0x48, 0x0C, 0x7D, 0x02][..],
            &[0x62, 0x62, 0x7D, 0x48, 0x0C, 0xF9][..],
        ),
        (
            &[0x62, 0xF2, 0xED, 0xD9, 0x0D, 0x12][..],
            &[0x62, 0xF2, 0xED, 0xC9, 0x0D, 0xD0][..],
        ),
    ] {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_variable_permute_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), register);
    }

    for (case, broadcast) in [
        (
            PermilCase {
                operation: Permil::Ps,
                width: VecWidth::V128,
                form: RegisterForm::DestinationSourceAlias,
                mask: 0,
                zeroing: false,
                broadcast: true,
            },
            [0x62, 0xF2, 0x7D, 0x08, 0x58, 0xC0],
        ),
        (
            PermilCase {
                operation: Permil::Pd,
                width: VecWidth::V256,
                form: RegisterForm::DestinationSourceAlias,
                mask: 0,
                zeroing: false,
                broadcast: true,
            },
            [0x62, 0xF2, 0xFD, 0x28, 0x59, 0xC0],
        ),
        (
            PermilCase {
                operation: Permil::Ps,
                width: VecWidth::V512,
                form: RegisterForm::DestinationSourceAlias,
                mask: 0,
                zeroing: false,
                broadcast: true,
            },
            [0x62, 0xF2, 0x7D, 0x48, 0x58, 0xC0],
        ),
        (
            PermilCase {
                operation: Permil::Pd,
                width: VecWidth::V512,
                form: RegisterForm::DestinationSourceAlias,
                mask: 0,
                zeroing: false,
                broadcast: true,
            },
            [0x62, 0xF2, 0xFD, 0x48, 0x59, 0xC0],
        ),
    ] {
        assert_eq!(case.scratch(), 0);
        assert_eq!(case.broadcast_instruction(), Some(broadcast));
    }
}

#[test]
fn evex_variable_permil_classifier_rejects_reserved_and_incomplete_shapes() {
    let valid = PermilCase {
        operation: Permil::Ps,
        width: VecWidth::V128,
        form: RegisterForm::Low,
        mask: 0,
        zeroing: false,
        broadcast: false,
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
    for (index, xor) in [
        (1, 0x01), // map
        (2, 0x04), // EVEX.U
        (2, 0x80), // PS W
        (2, 0x01), // mandatory prefix
        (3, 0x80), // zeroing without a mask
        (4, 0x01), // opcode/W mismatch
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= xor;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut legacy_66 = valid.clone();
    legacy_66.insert(0, 0x66);
    malformed.push(legacy_66);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_variable_permute_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let prefixed_encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_variable_permute_memory_encoding()
        .expect("segment/address prefixes belong to helper address evaluation");
    let plain = X86InstructionBytes::new(&valid)
        .unwrap()
        .evex_variable_permute_memory_encoding()
        .unwrap();
    assert_eq!(
        prefixed_encoding.register_instruction,
        plain.register_instruction
    );
}

#[test]
fn all_324_representative_optimization_cells_admit_and_lower_exactly() {
    let cases = representative_cases();
    assert_eq!(cases.len(), 108);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(
                function.blocks[0]
                    .ops
                    .iter()
                    .all(|op| !matches!(op.kind, OpKind::PredLoad { .. })),
                "{level:?} {case:?}: E4NF source became fault-suppressing"
            );
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
                    .count(),
                1,
                "{level:?} {case:?}"
            );
            let sequence = classified(&function)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence not classified"));
            assert_eq!(
                sequence.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.elem,
                case.operation.elem(),
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
            assert_eq!(sequence.encoding.writemask, case.mask, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.memory_size,
                case.memory_size(),
                "{level:?} {case:?}"
            );
            let (code, _) = lower(&function, case);
            let register = case.register_instruction();
            assert!(
                code.windows(register.len())
                    .any(|window| window == register),
                "{level:?} {case:?}: missing register replay"
            );
            if let Some(broadcast) = case.broadcast_instruction() {
                assert!(
                    code.windows(broadcast.len())
                        .any(|window| window == broadcast),
                    "{level:?} {case:?}: missing control broadcast {broadcast:02X?}"
                );
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, 324);
}

fn initial_registers(case: PermilCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x2000u64
                .wrapping_add(index as u64 * 0x101)
                .wrapping_add((ordinal & 0x0F) as u64 * 0x100)
        }),
        rflags: 0x2 | ((ordinal as u64 * 0x145) & 0x8D5),
        k: std::array::from_fn(|index| 0xA55A_3CC3_F00F_9696u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x1020_3040_5060_7080u64.rotate_left((index * 11 + word * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    registers.gpr[usize::from(case.base())] = 0x4000 + ((ordinal & 0x0F) as u64) * 0x100;
    registers
}

fn memory_address(case: PermilCase, registers: &GuestRegs) -> u64 {
    registers.gpr[usize::from(case.base())] + u64::from(DISP8) * u64::from(case.memory_size())
}

fn memory_value(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|index| {
        (index as u8)
            .wrapping_mul(0x3D)
            .wrapping_add((ordinal as u8).wrapping_mul(0x27))
            ^ 0xA5
    })
}

fn manual_destination(initial: &GuestRegs, memory: &[u8; 64], case: PermilCase) -> [u64; 8] {
    let mut destination = [0u8; 64];
    let mut source = [0u8; 64];
    for word in 0..8 {
        destination[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.destination())][word].to_le_bytes());
        source[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.source1())][word].to_le_bytes());
    }
    let elem_bytes = case.operation.elem().bytes() as usize;
    let lanes = case.width.bytes() as usize / elem_bytes;
    let group_lanes = if case.operation == Permil::Ps { 4 } else { 2 };
    let mask = initial.k[usize::from(case.mask)];
    for lane in 0..lanes {
        if case.mask != 0 && mask & (1 << lane) == 0 {
            if case.zeroing {
                destination[lane * elem_bytes..(lane + 1) * elem_bytes].fill(0);
            }
            continue;
        }
        let control_lane = if case.broadcast { 0 } else { lane };
        let control_start = control_lane * elem_bytes;
        let selected = if case.operation == Permil::Ps {
            u32::from_le_bytes(memory[control_start..control_start + 4].try_into().unwrap())
                as usize
                & 3
        } else {
            ((u64::from_le_bytes(memory[control_start..control_start + 8].try_into().unwrap())
                >> 1)
                & 1) as usize
        };
        let source_lane = lane / group_lanes * group_lanes + selected;
        destination[lane * elem_bytes..(lane + 1) * elem_bytes]
            .copy_from_slice(&source[source_lane * elem_bytes..(source_lane + 1) * elem_bytes]);
    }
    destination[case.width.bytes() as usize..].fill(0);
    std::array::from_fn(|word| {
        u64::from_le_bytes(destination[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    value: &[u8; 64],
    address: u64,
) -> GuestRegs {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, vector) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(vector);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    memory.load(address as usize, value);
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
    for (index, vector) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&vector[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected
}

#[test]
fn interpreter_o0_o1_o2_matches_raw_bit_model_for_all_324_cells() {
    let mut executions = 0usize;
    for (ordinal, case) in representative_cases().into_iter().enumerate() {
        let mut initial = initial_registers(case, ordinal);
        if case.mask != 0 {
            initial.k[usize::from(case.mask)] = match ordinal % 3 {
                0 => 0,
                1 => 0xAAAA_AAAA_AAAA_AAAA,
                _ => u64::MAX,
            };
        }
        let address = memory_address(case, &initial);
        let value = memory_value(ordinal);
        let manual = manual_destination(&initial, &value, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, &value, address);
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
    }
    assert_eq!(executions, 324);
}

#[test]
fn class_e4nf_memory_faults_with_all_applicable_mask_bits_clear_without_commit() {
    for operation in Permil::ALL {
        for broadcast in [false, true] {
            for zeroing in [false, true] {
                let case = PermilCase {
                    operation,
                    width: VecWidth::V512,
                    form: RegisterForm::DestinationSourceAlias,
                    mask: 3,
                    zeroing,
                    broadcast,
                };
                let initial = initial_registers(case, usize::from(broadcast));
                let mut context = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                    x86.gpr = initial.gpr;
                    for (index, vector) in initial.zmm.iter().enumerate() {
                        x86.xmm[index][..8].copy_from_slice(vector);
                    }
                    x86.k = initial.k;
                    x86.k[usize::from(case.mask)] = 0;
                }
                let before = match &context.arch_regs {
                    ArchRegState::X86_64(x86) => x86.xmm[usize::from(case.destination())],
                    _ => unreachable!(),
                };
                let mut memory = FlatMemory::new(1);
                for level in [OptLevel::O0, OptLevel::O2] {
                    let function = optimize(lift_case(case), level);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(
                        x86.xmm[usize::from(case.destination())],
                        before,
                        "{level:?} {case:?}: fault committed destination"
                    );
                }
            }
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        classified(function).is_none(),
        "{name}: classifier admitted malformed EVEX VPERMIL graph"
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
        "{name}: lowerer admitted malformed graph"
    );
}

#[test]
fn evex_variable_permil_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = PermilCase {
        operation: Permil::Ps,
        width: VecWidth::V256,
        form: RegisterForm::High,
        mask: 7,
        zeroing: true,
        broadcast: false,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_variable_permute_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );
    let loaded = base.blocks[0].ops[0].kind.dests()[0];
    let mut mutations = Vec::new();

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing));
    let mut wrong_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    mutations.push(("memory width", wrong_width));
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    mutations.push(("virtual address", virtual_address));
    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    mutations.push(("split guest PC", wrong_pc));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    mutations.push(("same-PC tail", same_pc_tail));
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    mutations.push(("loaded control escapes", external_use));
    let mut duplicate = base;
    duplicate.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFB),
        PC + 1,
        OpKind::VLoad {
            dst: loaded,
            addr: Address::Direct(vector(5, VecWidth::V256)),
            width: VecWidth::V256,
        },
    ));
    mutations.push(("control defined twice", duplicate));

    for (name, function) in mutations {
        assert_rejected(name, &function);
    }
}

#[test]
fn avx_only_state_bridge_rejects_evex_variable_permil_memory_replay() {
    let case = PermilCase {
        operation: Permil::Pd,
        width: VecWidth::V256,
        form: RegisterForm::High,
        mask: 3,
        zeroing: false,
        broadcast: true,
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
    value: [u8; 64],
    ok: bool,
    calls: u64,
    last_addr: u64,
    last_size: u32,
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
    context.last_size = size;
    if !context.ok
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8 | 16 | 32 | 64)
    {
        return 0;
    }
    let scratch = unsafe {
        std::slice::from_raw_parts_mut(
            state.vector_scratch.as_mut_ptr().cast::<u8>(),
            std::mem::size_of_val(&state.vector_scratch),
        )
    };
    if zero_upper != 0 {
        scratch.fill(0);
    }
    scratch[..size as usize].copy_from_slice(&context.value[..size as usize]);
    1
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_evex_variable_permil_matches_interpreter_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX VPERMIL memory: host lacks AVX-512F/BW");
        return;
    }
    let selected = [
        PermilCase {
            operation: Permil::Ps,
            width: VecWidth::V512,
            form: RegisterForm::High,
            mask: 0,
            zeroing: false,
            broadcast: false,
        },
        PermilCase {
            operation: Permil::Pd,
            width: VecWidth::V512,
            form: RegisterForm::DestinationSourceAlias,
            mask: 3,
            zeroing: false,
            broadcast: true,
        },
        PermilCase {
            operation: Permil::Ps,
            width: VecWidth::V512,
            form: RegisterForm::Low,
            mask: 7,
            zeroing: true,
            broadcast: true,
        },
        PermilCase {
            operation: Permil::Pd,
            width: VecWidth::V512,
            form: RegisterForm::High,
            mask: 7,
            zeroing: true,
            broadcast: false,
        },
    ];

    let mut executions = 0usize;
    for (ordinal, case) in selected.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec = ExecMem::new(&code).expect("map EVEX VPERMIL replay");
            let mut registers = initial_registers(case, ordinal);
            if case.mask != 0 {
                registers.k[usize::from(case.mask)] = if ordinal & 1 == 0 { 0 } else { 0xA5A5 };
            }
            let address = memory_address(case, &registers);
            let value = memory_value(ordinal);
            let mut context = VectorMemoryContext {
                value,
                ok: true,
                calls: 0,
                last_addr: 0,
                last_size: 0,
            };
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, &value, address);
            expected.vector_scratch = std::array::from_fn(|word| {
                u64::from_le_bytes(value[word * 8..word * 8 + 8].try_into().unwrap())
            });

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");

            let mut fault_registers = initial_registers(case, ordinal ^ 0x55);
            if case.mask != 0 {
                fault_registers.k[usize::from(case.mask)] = 0;
            }
            let fault_address = memory_address(case, &fault_registers);
            let mut fault_context = VectorMemoryContext {
                value,
                ok: false,
                calls: 0,
                last_addr: 0,
                last_size: 0,
            };
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
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}");
            assert_eq!(fault_context.last_addr, fault_address, "{level:?} {case:?}");
            executions += 1;
        }
    }
    assert_eq!(executions, 8);
}
