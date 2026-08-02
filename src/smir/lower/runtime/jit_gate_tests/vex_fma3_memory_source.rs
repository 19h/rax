//! Exact helper-backed packed VEX FMA3 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86FmaOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FpRoundMode, FunctionId, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86FmaKind, X86FmaOrder, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_binary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xF3C0;
const DISP: i64 = 0x20;
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
}

impl MemoryForm {
    const ALL: [Self; 5] = [
        Self::Low,
        Self::High,
        Self::DestinationSourceAlias,
        Self::FsAddr32Sib,
        Self::RipRelative,
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
            MemoryForm::Low | MemoryForm::FsAddr32Sib => 0,
            MemoryForm::High => 15,
            MemoryForm::DestinationSourceAlias => 9,
            MemoryForm::RipRelative => 7,
        }
    }

    const fn source2(self) -> u8 {
        match self.form {
            MemoryForm::Low | MemoryForm::FsAddr32Sib => 1,
            MemoryForm::High => 14,
            MemoryForm::DestinationSourceAlias => 9,
            MemoryForm::RipRelative => 8,
        }
    }

    const fn base(self) -> Option<u8> {
        match self.form {
            MemoryForm::Low | MemoryForm::FsAddr32Sib => Some(3),
            MemoryForm::High | MemoryForm::DestinationSourceAlias => Some(11),
            MemoryForm::RipRelative => None,
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination() && *candidate != self.source2())
            .expect("two VEX register operands leave at least fourteen scratch registers")
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

    fn vex_p0(self) -> u8 {
        let destination = self.destination();
        let base = self.base().unwrap_or(0);
        (if destination < 8 { 0x80 } else { 0 })
            | 0x40
            | (if base < 8 || self.base().is_none() {
                0x20
            } else {
                0
            })
            | 0x02
    }

    fn vex_p1(self) -> u8 {
        (u8::from(self.w) << 7)
            | (((!self.source2()) & 0x0F) << 3)
            | (u8::from(self.width == VecWidth::V256) << 2)
            | 0x01
    }

    fn bytes(self) -> Vec<u8> {
        let reg = (self.destination() & 7) << 3;
        match self.form {
            MemoryForm::Low | MemoryForm::High | MemoryForm::DestinationSourceAlias => vec![
                0xC4,
                self.vex_p0(),
                self.vex_p1(),
                self.opcode,
                0x40 | reg | (self.base().unwrap() & 7),
                DISP as u8,
            ],
            MemoryForm::FsAddr32Sib => vec![
                0x64,
                0x67,
                0xC4,
                self.vex_p0(),
                self.vex_p1(),
                self.opcode,
                0x44 | reg,
                0x73,
                DISP as u8,
            ],
            MemoryForm::RipRelative => {
                let mut bytes = vec![0xC4, self.vex_p0(), self.vex_p1(), self.opcode, reg | 0x05];
                bytes.extend_from_slice(&(DISP as i32).to_le_bytes());
                bytes
            }
        }
    }

    fn emitted_fma_bytes(self) -> [u8; 5] {
        let destination = self.destination();
        let source2 = self.source2();
        let scratch = self.scratch();
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 0x02,
            (u8::from(self.w) << 7)
                | (((!source2) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 0x01,
            self.opcode,
            0xC0 | ((destination & 7) << 3) | (scratch & 7),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("packed VEX FMA3 width"),
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
        X86InstructionBytes::new(&bytes).expect("VEX FMA3 instruction provenance"),
    );
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("packed VEX FMA3 memory load")
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
    let [load, fma, result] = &function.blocks[0].ops[index..index + 3] else {
        unreachable!()
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let loaded = match load.kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            width,
            ..
        } => {
            assert_eq!(width, case.width, "{case:?}");
            loaded
        }
        ref other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };
    assert_eq!(fma.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(
        fma.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.width,
            w: case.w,
        }),
        "{case:?}"
    );
    let OpKind::X86Fma(X86FmaOp {
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
    }) = fma.kind
    else {
        panic!("{case:?}: expected X86Fma, got {:?}", fma.kind)
    };
    assert_eq!(src1, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(src2, vector(case.source2(), case.width), "{case:?}");
    assert_eq!(src3, loaded, "{case:?}");
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(elem, case.elem(), "{case:?}");
    assert_eq!(kind, case.kind(), "{case:?}");
    assert_eq!(order, case.order(), "{case:?}");
    assert_eq!(round, FpRoundMode::Dynamic, "{case:?}");
    assert_eq!(lanes, case.width.lanes(case.elem()) as u8, "{case:?}");
    assert_eq!(result.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(result.x86_hint, None, "{case:?}");
    assert!(
        matches!(
            result.kind,
            OpKind::VMov {
                dst,
                src,
                width,
            } if dst == vector(case.destination(), case.width)
                && src == raw
                && width == case.width
        ),
        "{case:?}: unexpected result {:?}",
        result.kind
    );
    assert!(
        function.blocks[0]
            .ops
            .get(index + 3)
            .is_none_or(|op| op.guest_pc != PC),
        "{case:?}: same-PC operation follows exact sequence"
    );
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
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
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma"),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed FMA3 lowering failed: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed VEX FMA3"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FmaMemoryCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for w in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256] {
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
fn packed_fma3_memory_byte_classifier_is_exhaustive_and_exact() {
    let mut accepted = 0usize;
    for opcode in 0..=u8::MAX {
        for w in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256] {
                for destination in 0..16 {
                    for source2 in 0..16 {
                        let p0 = (if destination < 8 { 0x80 } else { 0 }) | 0x62;
                        let p1 = (u8::from(w) << 7)
                            | (((!source2) & 0x0F) << 3)
                            | (u8::from(width == VecWidth::V256) << 2)
                            | 1;
                        let bytes = [
                            0xC4,
                            p0,
                            p1,
                            opcode,
                            0x40 | ((destination & 7) << 3) | 3,
                            0x20,
                        ];
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let fields = instruction.vex_memory_packed_fma3_fields();
                        if PACKED_OPCODES.contains(&opcode) {
                            assert_eq!(
                                fields,
                                Some((destination, source2, opcode, width, w)),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        } else {
                            assert_eq!(fields, None, "{bytes:02X?}");
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18 * 2 * 2 * 16 * 16);

    let valid = FmaMemoryCase {
        opcode: 0x98,
        w: false,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    }
    .bytes();
    let mut malformed = Vec::new();
    malformed.push((&valid[..valid.len() - 1]).to_vec());
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[4] |= 0xC0;
    register.truncate(5);
    malformed.push(register);
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
    malformed.push(wrong_map);
    let mut wrong_prefix = valid.clone();
    wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
    malformed.push(wrong_prefix);
    let mut scalar = valid.clone();
    scalar[3] = 0x99;
    malformed.push(scalar);
    let mut legacy_operand_size = valid.clone();
    legacy_operand_size.insert(0, 0x66);
    malformed.push(legacy_operand_size);
    for bytes in malformed {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_memory_packed_fma3_fields(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_360_packed_memory_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 18 * 2 * 2 * 5);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_vex_binary_memory_sequence(
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
                sequence.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.source1, case.source2(), "{level:?} {case:?}");
            assert_eq!(sequence.width, case.width, "{level:?} {case:?}");
            assert_eq!(sequence.map, X86VecMap::Map0F38, "{level:?} {case:?}");
            assert_eq!(sequence.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(sequence.opcode, case.opcode, "{level:?} {case:?}");
            assert_eq!(sequence.w, case.w, "{level:?} {case:?}");
            assert!(!sequence.needs_avx2, "{level:?} {case:?}");
            assert!(sequence.needs_fma, "{level:?} {case:?}");

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
    assert_eq!(lowered, 360 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed FMA3 sequence"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed FMA3 sequence"
    );
}

#[test]
fn packed_fma3_memory_sequence_fails_closed_for_every_structural_invariant() {
    let case = FmaMemoryCase {
        opcode: 0x98,
        w: false,
        width: VecWidth::V128,
        form: MemoryForm::Low,
    };
    let base = lift_case(case);
    let index = sequence_index(&base);
    assert_eq!(index, 0);
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
        x86_jit_vex_binary_memory_sequence(
            &base.blocks[0],
            index,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();

    let mut truncated_metadata = base.clone();
    let bytes = case.bytes();
    truncated_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes[..bytes.len() - 1]).unwrap(),
    );

    let mut mismatched_destination_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] ^= 0x08;
    mismatched_destination_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut mismatched_source_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x08;
    mismatched_source_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut mismatched_width_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x04;
    mismatched_width_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut mismatched_w_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x80;
    mismatched_w_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));

    let mut architectural_load_destination = base.clone();
    if let OpKind::VLoad { dst, .. } = &mut architectural_load_destination.blocks[0].ops[0].kind {
        *dst = vector(2, VecWidth::V128);
    }

    let mut invalid_load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut invalid_load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V64;
    }

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut loaded_used_twice = base.clone();
    loaded_used_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: loaded,
            width: VecWidth::V128,
        },
    ));

    let mut fma_wrong_pc = base.clone();
    fma_wrong_pc.blocks[0].ops[1].guest_pc += 1;

    let mut missing_fma_hint = base.clone();
    missing_fma_hint.blocks[0].ops[1].x86_hint = None;

    let mut wrong_map = base.clone();
    if let Some(X86OpHint::VexOp { map, .. }) = &mut wrong_map.blocks[0].ops[1].x86_hint {
        *map = X86VecMap::Map0F;
    }

    let mut wrong_prefix = base.clone();
    if let Some(X86OpHint::VexOp { pp, .. }) = &mut wrong_prefix.blocks[0].ops[1].x86_hint {
        *pp = X86SsePrefix::Rep;
    }

    let mut wrong_opcode = base.clone();
    if let Some(X86OpHint::VexOp { opcode, .. }) = &mut wrong_opcode.blocks[0].ops[1].x86_hint {
        *opcode = 0x9A;
    }

    let mut wrong_hint_width = base.clone();
    if let Some(X86OpHint::VexOp { width, .. }) = &mut wrong_hint_width.blocks[0].ops[1].x86_hint {
        *width = VecWidth::V256;
    }

    let mut wrong_hint_w = base.clone();
    if let Some(X86OpHint::VexOp { w, .. }) = &mut wrong_hint_w.blocks[0].ops[1].x86_hint {
        *w = true;
    }

    let mut architectural_raw = base.clone();
    if let OpKind::X86Fma(op) = &mut architectural_raw.blocks[0].ops[1].kind {
        op.dst = vector(4, VecWidth::V128);
    }

    let mut wrong_destination_source = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_destination_source.blocks[0].ops[1].kind {
        op.src1 = vector(2, VecWidth::V128);
    }

    let mut wrong_vvvv_source = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_vvvv_source.blocks[0].ops[1].kind {
        op.src2 = vector(2, VecWidth::V128);
    }

    let mut bypassed_load = base.clone();
    if let OpKind::X86Fma(op) = &mut bypassed_load.blocks[0].ops[1].kind {
        op.src3 = vector(3, VecWidth::V128);
    }

    let mut masked = base.clone();
    if let OpKind::X86Fma(op) = &mut masked.blocks[0].ops[1].kind {
        op.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    }

    let mut wrong_element = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_element.blocks[0].ops[1].kind {
        op.elem = VecElementType::F64;
        op.lanes = 2;
    }

    let mut wrong_lanes = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_lanes.blocks[0].ops[1].kind {
        op.lanes -= 1;
    }

    let mut explicit_round = base.clone();
    if let OpKind::X86Fma(op) = &mut explicit_round.blocks[0].ops[1].kind {
        op.round = FpRoundMode::RoundUp;
    }

    let mut wrong_kind = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_kind.blocks[0].ops[1].kind {
        op.kind = X86FmaKind::Sub;
    }

    let mut wrong_order = base.clone();
    if let OpKind::X86Fma(op) = &mut wrong_order.blocks[0].ops[1].kind {
        op.order = X86FmaOrder::Order231;
    }

    let mut raw_used_twice = base.clone();
    raw_used_twice.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: raw,
            width: VecWidth::V128,
        },
    ));

    let mut result_wrong_pc = base.clone();
    result_wrong_pc.blocks[0].ops[2].guest_pc += 1;

    let mut result_hint = base.clone();
    result_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Aligned,
    ));

    let mut result_wrong_destination = base.clone();
    if let OpKind::VMov { dst, .. } = &mut result_wrong_destination.blocks[0].ops[2].kind {
        *dst = vector(2, VecWidth::V128);
    }

    let mut result_wrong_source = base.clone();
    if let OpKind::VMov { src, .. } = &mut result_wrong_source.blocks[0].ops[2].kind {
        *src = loaded;
    }

    let mut result_wrong_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut result_wrong_width.blocks[0].ops[2].kind {
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
        ("missing instruction metadata", missing_metadata),
        ("truncated instruction metadata", truncated_metadata),
        (
            "metadata destination mismatch",
            mismatched_destination_metadata,
        ),
        ("metadata VEX.vvvv mismatch", mismatched_source_metadata),
        ("metadata VEX.L mismatch", mismatched_width_metadata),
        ("metadata VEX.W mismatch", mismatched_w_metadata),
        ("unexpected load hint", load_hint),
        (
            "architectural load destination",
            architectural_load_destination,
        ),
        ("invalid load width", invalid_load_width),
        ("virtual address component", virtual_address),
        ("loaded temporary used twice", loaded_used_twice),
        ("FMA guest PC mismatch", fma_wrong_pc),
        ("missing FMA encoding hint", missing_fma_hint),
        ("wrong VEX map", wrong_map),
        ("wrong mandatory prefix", wrong_prefix),
        ("opcode/kind mismatch", wrong_opcode),
        ("hint width mismatch", wrong_hint_width),
        ("hint W mismatch", wrong_hint_w),
        ("architectural raw result", architectural_raw),
        (
            "wrong destructive destination source",
            wrong_destination_source,
        ),
        ("wrong VEX.vvvv source", wrong_vvvv_source),
        ("FMA bypasses memory temporary", bypassed_load),
        ("masked VEX FMA", masked),
        ("W/element mismatch", wrong_element),
        ("invalid lane count", wrong_lanes),
        ("non-dynamic rounding", explicit_round),
        ("wrong FMA kind", wrong_kind),
        ("wrong FMA order", wrong_order),
        ("raw temporary used twice", raw_used_twice),
        ("result guest PC mismatch", result_wrong_pc),
        ("unexpected result hint", result_hint),
        ("wrong result destination", result_wrong_destination),
        ("wrong result source", result_wrong_source),
        ("wrong result width", result_wrong_width),
        ("same-PC trailing operation", same_pc_tail),
        ("missing architectural result", missing_result),
    ];
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
pub(super) struct VectorMemoryContext {
    pub(super) value: [u64; 8],
    pub(super) ok: u64,
    pub(super) calls: u64,
    pub(super) last_addr: u64,
    pub(super) last_index: u32,
    pub(super) last_size: u32,
    pub(super) last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
pub(super) extern "C" fn vector_load_helper(
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

pub(super) fn role_vector(w: bool, data_case: usize, role: usize) -> [u64; 8] {
    const F32: [[[u32; 4]; 3]; 4] = [
        [
            [0x3F80_0000, 0xC000_0000, 0x4040_0000, 0xC080_0000],
            [0x4000_0000, 0x4040_0000, 0xC080_0000, 0xC0A0_0000],
            [0x4120_0000, 0x4130_0000, 0xC140_0000, 0xC150_0000],
        ],
        [
            [0x3F80_0001, 0x3F7F_FFFF, 0x3F80_0000, 0xBF80_0000],
            [0xBF80_0000, 0x3F80_0000, 0x3380_0000, 0xB380_0000],
            [0x3F7F_FFFF, 0x3F80_0001, 0x3F80_0000, 0x3F80_0000],
        ],
        [
            [0x7FC0_0011, 0x7F80_0001, 0, 0x7F80_0000],
            [0x7FC0_0022, 0x3F80_0000, 0xFF80_0000, 0],
            [0x7FC0_0033, 0x4000_0000, 0x7F80_0000, 0x7F80_0000],
        ],
        [
            [0x0080_0000, 0x7F7F_FFFF, 1, 0x8000_0000],
            [0, 0, 1, 0],
            [0x3F00_0000, 0x4000_0000, 0x3F80_0000, 0x8000_0000],
        ],
    ];
    const F64: [[[u64; 4]; 3]; 4] = [
        [
            [
                0x3FF0_0000_0000_0000,
                0xC000_0000_0000_0000,
                0x4008_0000_0000_0000,
                0xC010_0000_0000_0000,
            ],
            [
                0x4000_0000_0000_0000,
                0x4008_0000_0000_0000,
                0xC010_0000_0000_0000,
                0xC014_0000_0000_0000,
            ],
            [
                0x4024_0000_0000_0000,
                0x4026_0000_0000_0000,
                0xC028_0000_0000_0000,
                0xC02A_0000_0000_0000,
            ],
        ],
        [
            [
                0x3FF0_0000_0000_0001,
                0x3FEF_FFFF_FFFF_FFFF,
                0x3FF0_0000_0000_0000,
                0xBFF0_0000_0000_0000,
            ],
            [
                0xBFF0_0000_0000_0000,
                0x3FF0_0000_0000_0000,
                0x3CA0_0000_0000_0000,
                0xBCA0_0000_0000_0000,
            ],
            [
                0x3FEF_FFFF_FFFF_FFFF,
                0x3FF0_0000_0000_0001,
                0x3FF0_0000_0000_0000,
                0x3FF0_0000_0000_0000,
            ],
        ],
        [
            [
                0x7FF8_0000_0000_0011,
                0x7FF0_0000_0000_0001,
                0,
                0x7FF0_0000_0000_0000,
            ],
            [
                0x7FF8_0000_0000_0022,
                0x3FF0_0000_0000_0000,
                0xFFF0_0000_0000_0000,
                0,
            ],
            [
                0x7FF8_0000_0000_0033,
                0x4000_0000_0000_0000,
                0x7FF0_0000_0000_0000,
                0x7FF0_0000_0000_0000,
            ],
        ],
        [
            [0x0010_0000_0000_0000, 0x7FEF_FFFF_FFFF_FFFF, 1, 1 << 63],
            [0, 0, 1, 0],
            [
                0x3FE0_0000_0000_0000,
                0x4000_0000_0000_0000,
                0x3FF0_0000_0000_0000,
                1 << 63,
            ],
        ],
    ];

    let mut bytes = [0xA5; 64];
    if w {
        for lane in 0..8 {
            bytes[lane * 8..lane * 8 + 8]
                .copy_from_slice(&F64[data_case % F64.len()][role][lane & 3].to_le_bytes());
        }
    } else {
        for lane in 0..16 {
            bytes[lane * 4..lane * 4 + 4]
                .copy_from_slice(&F32[data_case % F32.len()][role][lane & 3].to_le_bytes());
        }
    }
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: FmaMemoryCase, ordinal: usize, data_case: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
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
    registers.zmm[usize::from(case.destination())] = role_vector(case.w, data_case, 0);
    if case.source2() != case.destination() {
        registers.zmm[usize::from(case.source2())] = role_vector(case.w, data_case, 1);
    }
    let base = case.base().expect("native cases use base+disp8 addressing");
    registers.gpr[usize::from(base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x80;
    registers
}

fn source_bytes(source: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(source) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    width: VecWidth,
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
    let bytes = source_bytes(source);
    memory.load(address as usize, &bytes[..width.bytes() as usize]);
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
    let words = (width.bytes() / 8) as usize;
    expected.vector_scratch =
        std::array::from_fn(|word| if word < words { source[word] } else { 0 });
    expected
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<FmaMemoryCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for w in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256] {
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

#[cfg(target_arch = "x86_64")]
#[test]
fn native_packed_fma3_memory_matches_o0_o2_interpretation_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("fma") {
        eprintln!("skipping native packed VEX FMA3 memory differential: host lacks AVX/FMA");
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 18 * 2 * 2 * 3);
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            // Rosetta 2 reports spurious OE/PE for a fused
            // `max_finite * 2 - max_finite` cancellation even though the Intel
            // operation is infinite-precision before its single final
            // rounding. Retain every opcode/order/width/alias and MXCSR.RC
            // combination there, but use the bounded exact finite data set.
            // Native x86-64 hosts exercise all four data sets, including
            // NaNs, infinities, subnormals, overflow, and underflow boundaries.
            #[cfg(target_os = "macos")]
            let data_case = if running_under_rosetta() { 0 } else { ordinal };
            #[cfg(not(target_os = "macos"))]
            let data_case = ordinal;
            let source = role_vector(case.w, data_case, 2);
            let base = case.base().expect("native base+disp8 form");

            let mut context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, data_case);
            let address = registers.gpr[usize::from(base)].wrapping_add(DISP as u64);
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
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, data_case);
            let address = registers.gpr[usize::from(base)].wrapping_add(DISP as u64);
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
