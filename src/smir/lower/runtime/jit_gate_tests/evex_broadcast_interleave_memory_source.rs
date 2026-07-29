//! Exact helper-backed EVEX VPUNPCK*DQ/QDQ broadcast-memory coverage.

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
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_broadcast_interleave_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xE62C;
const DISP8: u8 = 1;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InterleaveKind {
    elem: VecElementType,
    high: bool,
    opcode: u8,
}

const KINDS: [InterleaveKind; 4] = [
    InterleaveKind {
        elem: VecElementType::I32,
        high: false,
        opcode: 0x62,
    },
    InterleaveKind {
        elem: VecElementType::I64,
        high: false,
        opcode: 0x6C,
    },
    InterleaveKind {
        elem: VecElementType::I32,
        high: true,
        opcode: 0x6A,
    },
    InterleaveKind {
        elem: VecElementType::I64,
        high: true,
        opcode: 0x6D,
    },
];

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
struct InterleaveCase {
    kind: InterleaveKind,
    width: VecWidth,
    form: RegisterForm,
    mask: u8,
    zeroing: bool,
}

impl InterleaveCase {
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

    const fn memory_width(self) -> MemWidth {
        match self.kind.elem {
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
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

    fn bytes(self) -> [u8; 7] {
        let destination = self.destination();
        let source1 = self.source1();
        let base = self.base();
        let qword = self.kind.elem == VecElementType::I64;
        [
            0x62,
            (if destination & 8 == 0 { 0x80 } else { 0 })
                | 0x40
                | (if base & 8 == 0 { 0x20 } else { 0 })
                | (if destination & 16 == 0 { 0x10 } else { 0 })
                | 0x01,
            (u8::from(qword) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
            (u8::from(self.zeroing) << 7)
                | (self.ll() << 5)
                | 0x10
                | (if source1 & 16 == 0 { 0x08 } else { 0 })
                | self.mask,
            self.kind.opcode,
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
            self.kind.opcode,
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

fn lift_case(case: InterleaveCase) -> SmirFunction {
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
        X86InstructionBytes::new(&bytes).expect("EVEX broadcast interleave provenance"),
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
) -> crate::smir::lower::runtime::X86JitEvexBroadcastInterleaveMemorySequence {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_broadcast_interleave_memory_sequence(
        &function.blocks[0],
        0,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .expect("exact EVEX broadcast interleave sequence")
}

fn lower(function: &SmirFunction, case: InterleaveCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_avx512dq, "{case:?}");
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
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed interleave: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX broadcast interleave"),
        result.entry_offset,
    )
}

#[test]
fn evex_broadcast_interleave_byte_classifier_exhaustively_rewrites_184_320_shapes() {
    let mut accepted = 0usize;
    for kind in KINDS {
        for (ll, width) in [
            (0, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    for mask in 0..=7u8 {
                        for zeroing in [false, true] {
                            if mask == 0 && zeroing {
                                continue;
                            }
                            let qword = kind.elem == VecElementType::I64;
                            let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                                | 0x60
                                | (if destination & 16 == 0 { 0x10 } else { 0 })
                                | 0x01;
                            let p1 = (u8::from(qword) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
                            let p2 = (u8::from(zeroing) << 7)
                                | (ll << 5)
                                | 0x10
                                | (if source1 & 16 == 0 { 0x08 } else { 0 })
                                | mask;
                            let bytes = [
                                0x62,
                                p0,
                                p1,
                                p2,
                                kind.opcode,
                                0x40 | ((destination & 7) << 3) | 3,
                                DISP8,
                            ];
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_broadcast_interleave_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                            assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                            assert_eq!(encoding.high, kind.high, "{bytes:02X?}");
                            assert_eq!(encoding.opcode, kind.opcode, "{bytes:02X?}");
                            assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                            assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                            assert_eq!(
                                encoding.writemask,
                                (mask != 0).then_some(mask),
                                "{bytes:02X?}"
                            );
                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                            assert_eq!(
                                encoding.memory_width,
                                if qword { MemWidth::B8 } else { MemWidth::B4 },
                                "{bytes:02X?}"
                            );
                            assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                            assert_eq!(
                                encoding.stack_instruction.as_slice(),
                                [
                                    0x62,
                                    (p0 & 0x97) | 0x60,
                                    p1 | 0x04,
                                    p2,
                                    kind.opcode,
                                    (bytes[5] & 0x38) | 0x04,
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
    }
    assert_eq!(accepted, KINDS.len() * 3 * 32 * 32 * 15);
}

#[test]
fn evex_broadcast_interleave_rewrite_matches_independent_llvm_23_encodings() {
    for (bytes, expected) in [
        (
            &[0x62, 0xF1, 0x6D, 0x58, 0x62, 0x0F][..],
            &[0x62, 0xF1, 0x6D, 0x58, 0x62, 0x0C, 0x24][..],
        ),
        (
            &[0x62, 0x51, 0xAD, 0xBB, 0x6C, 0x4B, 0x08][..],
            &[0x62, 0x71, 0xAD, 0xBB, 0x6C, 0x0C, 0x24][..],
        ),
        (
            &[0x62, 0x51, 0x05, 0x19, 0x6A, 0x7E, 0xFC][..],
            &[0x62, 0x71, 0x05, 0x19, 0x6A, 0x3C, 0x24][..],
        ),
        (
            &[0x62, 0x61, 0xFD, 0xD7, 0x6D, 0x3C, 0x24][..],
            &[0x62, 0x61, 0xFD, 0xD7, 0x6D, 0x3C, 0x24][..],
        ),
    ] {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_broadcast_interleave_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(encoding.stack_instruction.as_slice(), expected);
    }
}

#[test]
fn evex_broadcast_interleave_classifier_rejects_reserved_and_nonbroadcast_shapes() {
    let valid = InterleaveCase {
        kind: KINDS[0],
        width: VecWidth::V128,
        form: RegisterForm::Low,
        mask: 1,
        zeroing: false,
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
        (2, 0x01), // mandatory 66H
        (2, 0x80), // DQ requires W0
        (3, 0x10), // nonbroadcast full-vector memory form
        (4, 0x01), // adjacent opcode
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
    let mut byte_form = valid.clone();
    byte_form[4] = 0x60;
    malformed.push(byte_form);
    let mut forbidden_legacy_prefix = valid.clone();
    forbidden_legacy_prefix.insert(0, 0x66);
    malformed.push(forbidden_legacy_prefix);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_broadcast_interleave_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_broadcast_interleave_memory_encoding()
        .expect("FS/address-size prefixes belong only to helper address evaluation");
    assert_eq!(
        encoding.stack_instruction.as_slice(),
        X86InstructionBytes::new(&valid)
            .unwrap()
            .evex_broadcast_interleave_memory_encoding()
            .unwrap()
            .stack_instruction
            .as_slice()
    );
}

#[test]
fn all_540_evex_broadcast_interleave_shapes_optimize_admit_and_lower_exactly() {
    let mut cases = 0usize;
    let mut lowerings = 0usize;
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in RegisterForm::ALL {
                for mask in 0..=7 {
                    for zeroing in [false, true] {
                        if mask == 0 && zeroing {
                            continue;
                        }
                        let case = InterleaveCase {
                            kind,
                            width,
                            form,
                            mask,
                            zeroing,
                        };
                        cases += 1;
                        for level in LEVELS {
                            let function = optimize(lift_case(case), level);
                            let sequence = sequence(&function);
                            assert_eq!(
                                sequence.memory_offset,
                                if mask == 0 { 0 } else { 5 },
                                "{level:?} {case:?}"
                            );
                            assert_eq!(sequence.encoding.destination, case.destination());
                            assert_eq!(sequence.encoding.source1, case.source1());
                            assert_eq!(sequence.encoding.elem, kind.elem);
                            assert_eq!(sequence.encoding.high, kind.high);
                            assert_eq!(sequence.encoding.opcode, kind.opcode);
                            assert_eq!(sequence.encoding.writemask, (mask != 0).then_some(mask));
                            assert_eq!(sequence.encoding.zeroing, zeroing);
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
    assert_eq!(cases, KINDS.len() * 3 * 3 * 15);
    assert_eq!(lowerings, cases * LEVELS.len());
}

#[test]
fn masked_evex_broadcast_interleave_lowering_has_exact_live_k_guard() {
    let case = InterleaveCase {
        kind: KINDS[0],
        width: VecWidth::V512,
        form: RegisterForm::High,
        mask: 3,
        zeroing: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let (code, _) = lower(&function, case);
    let lane_mask = 0xFFFFu32.to_le_bytes();
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
    let guard_at = code
        .windows(guard.len())
        .position(|window| window == guard)
        .expect("complete live-K applicable-lane guard");
    let jz_disp = guard_at + guard.len();
    assert_eq!(&code[jz_disp + 4..jz_disp + 6], &[0x58, 0x9D]);
    let inactive = (jz_disp + 4) as i64
        + i64::from(i32::from_le_bytes(
            code[jz_disp..jz_disp + 4].try_into().unwrap(),
        ));
    let inactive = usize::try_from(inactive).expect("forward inactive target");
    assert_eq!(&code[inactive..inactive + 2], &[0x58, 0x9D]);
    let replay = case.stack_instruction();
    assert_eq!(&code[inactive + 2..inactive + 2 + replay.len()], &replay);

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

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_broadcast_interleave_memory_sequence(
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
fn evex_broadcast_interleave_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = InterleaveCase {
        kind: KINDS[0],
        width: VecWidth::V128,
        form: RegisterForm::Low,
        mask: 3,
        zeroing: true,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_broadcast_interleave_memory_sequence(
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

    let mut predicate_mask = base.clone();
    if let OpKind::And { src2, .. } = &mut predicate_mask.blocks[0].ops[0].kind {
        *src2 = crate::smir::ir::types::SrcOperand::Imm(3);
    }
    mutations.push(("aggregate predicate mask", predicate_mask));

    let load_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let broadcast_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
        .unwrap();
    let interleave_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInterleave { .. }))
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
    mutations.push(("virtual load address", load_address));

    let mut broadcast_elem = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut broadcast_elem.blocks[0].ops[broadcast_index].kind
    {
        *elem = VecElementType::I64;
    }
    mutations.push(("broadcast element", broadcast_elem));

    let mut interleave_half = base.clone();
    if let OpKind::VInterleave { high, .. } =
        &mut interleave_half.blocks[0].ops[interleave_index].kind
    {
        *high = true;
    }
    mutations.push(("interleave half", interleave_half));

    let mut interleave_block = base.clone();
    if let OpKind::VInterleave { block_lanes, .. } =
        &mut interleave_block.blocks[0].ops[interleave_index].kind
    {
        *block_lanes = 2;
    }
    mutations.push(("interleave block", interleave_block));

    let mut interleave_source = base.clone();
    if let OpKind::VInterleave { src1, .. } =
        &mut interleave_source.blocks[0].ops[interleave_index].kind
    {
        *src1 = vector(2, case.width);
    }
    mutations.push(("interleave source", interleave_source));

    let mut interleave_hint = base.clone();
    interleave_hint.blocks[0].ops[interleave_index].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0x6A,
        width: VecWidth::V128,
        w: false,
    });
    mutations.push(("interleave hint", interleave_hint));

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

fn initial_registers(case: InterleaveCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x20)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| {
            0x8000_0000_0000_0000u64 | (0xA55Au64.rotate_left((index * 5) as u32))
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

fn memory_address(case: InterleaveCase, registers: &GuestRegs) -> u64 {
    registers.gpr[usize::from(case.base())]
        + u64::from(DISP8) * u64::from(case.memory_width().bytes())
}

fn scalar_bits(case: InterleaveCase, alternate: bool) -> u64 {
    match case.kind.elem {
        VecElementType::I32 => u64::from(if alternate {
            0xA5C3_6996u32
        } else {
            0x1357_9BDFu32
        }),
        VecElementType::I64 => {
            if alternate {
                0xA5C3_6996_F00D_5AA5
            } else {
                0x1357_9BDF_2468_ACE0
            }
        }
        _ => unreachable!(),
    }
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    scalar: u64,
    address: u64,
    case: InterleaveCase,
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
        &scalar.to_le_bytes()[..case.memory_width().bytes() as usize],
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

fn manual_destination(initial: &GuestRegs, scalar: u64, case: InterleaveCase) -> [u64; 8] {
    let mut destination_bytes = [0u8; 64];
    let mut source1_bytes = [0u8; 64];
    for word in 0..8 {
        destination_bytes[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.destination())][word].to_le_bytes());
        source1_bytes[word * 8..word * 8 + 8]
            .copy_from_slice(&initial.zmm[usize::from(case.source1())][word].to_le_bytes());
    }
    let elem_bytes = case.memory_width().bytes() as usize;
    let lanes = case.width.bytes() as usize / elem_bytes;
    let block_lanes = 16 / elem_bytes;
    let half_lanes = block_lanes / 2;
    let mut raw = [0u8; 64];
    let scalar_bytes = scalar.to_le_bytes();
    for block in 0..(lanes / block_lanes) {
        for output_lane in 0..block_lanes {
            let pair = output_lane / 2;
            let input_lane = block * block_lanes
                + if case.kind.high {
                    half_lanes + pair
                } else {
                    pair
                };
            let output_lane = block * block_lanes + output_lane;
            let output_start = output_lane * elem_bytes;
            if output_lane & 1 == 0 {
                let input_start = input_lane * elem_bytes;
                raw[output_start..output_start + elem_bytes]
                    .copy_from_slice(&source1_bytes[input_start..input_start + elem_bytes]);
            } else {
                raw[output_start..output_start + elem_bytes]
                    .copy_from_slice(&scalar_bytes[..elem_bytes]);
            }
        }
    }

    let applicable_mask = if case.mask == 0 {
        u64::MAX
    } else {
        initial.k[usize::from(case.mask)]
    };
    for lane in 0..lanes {
        let start = lane * elem_bytes;
        if applicable_mask & (1 << lane) != 0 {
            destination_bytes[start..start + elem_bytes]
                .copy_from_slice(&raw[start..start + elem_bytes]);
        } else if case.zeroing {
            destination_bytes[start..start + elem_bytes].fill(0);
        }
    }
    destination_bytes[case.width.bytes() as usize..].fill(0);
    std::array::from_fn(|word| {
        u64::from_le_bytes(
            destination_bytes[word * 8..word * 8 + 8]
                .try_into()
                .unwrap(),
        )
    })
}

#[test]
fn interpreter_o0_o1_o2_matches_block_local_model_for_masks_aliases_and_upper_zeroing() {
    let masks = [(0, false), (1, false), (3, true), (7, false)];
    let mut executions = 0usize;
    for (ordinal, (kind, width, form, (mask, zeroing))) in KINDS
        .into_iter()
        .flat_map(|kind| {
            [VecWidth::V128, VecWidth::V256, VecWidth::V512]
                .into_iter()
                .map(move |width| (kind, width))
        })
        .flat_map(|(kind, width)| {
            RegisterForm::ALL
                .into_iter()
                .map(move |form| (kind, width, form))
        })
        .flat_map(|(kind, width, form)| {
            masks.into_iter().map(move |mask| (kind, width, form, mask))
        })
        .enumerate()
    {
        let case = InterleaveCase {
            kind,
            width,
            form,
            mask,
            zeroing,
        };
        let mut initial = initial_registers(case, ordinal);
        if mask != 0 {
            initial.k[usize::from(mask)] = match ordinal % 3 {
                0 => 0,
                1 => 0xAAAA_AAAA_AAAA_AAAA,
                _ => u64::MAX,
            };
        }
        let address = memory_address(case, &initial);
        let scalar = scalar_bits(case, ordinal & 1 != 0);
        let manual = manual_destination(&initial, scalar, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, scalar, address, case);
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
    assert_eq!(executions, KINDS.len() * 3 * 3 * masks.len() * LEVELS.len());
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
#[test]
fn native_evex_broadcast_interleave_matches_interpretation_faults_and_mask_suppression() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX broadcast interleave: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressed = 0usize;
    for (ordinal, (kind, width, mask, zeroing)) in [
        (KINDS[0], VecWidth::V128, 1, false),
        (KINDS[1], VecWidth::V256, 3, true),
        (KINDS[2], VecWidth::V512, 0, false),
        (KINDS[3], VecWidth::V512, 7, false),
    ]
    .into_iter()
    .filter(|(_, width, _, _)| *width == VecWidth::V512 || has_vl)
    .enumerate()
    {
        let case = InterleaveCase {
            kind,
            width,
            form: if ordinal & 1 == 0 {
                RegisterForm::Low
            } else {
                RegisterForm::DestinationSourceAlias
            },
            mask,
            zeroing,
        };
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let scalar = scalar_bits(case, ordinal & 1 != 0);
            let mut context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = initial_registers(case, ordinal);
            if mask != 0 {
                registers.k[usize::from(mask)] = 0x5555_5555_5555_5555;
            }
            let address = memory_address(case, &registers);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, scalar, address, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_size,
                u64::from(case.memory_width().bytes()),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut fault_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = initial_registers(case, ordinal ^ 0x55);
            if mask != 0 {
                fault_registers.k[usize::from(mask)] = 1;
            }
            let fault_address = memory_address(case, &fault_registers);
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
            assert_eq!(fault_context.last_addr, fault_address, "{level:?} {case:?}");
            faults += 1;

            if mask != 0 {
                let mut suppressed_context = ScalarMemoryContext {
                    value: scalar ^ 0xFFFF,
                    ok: 0,
                    ..ScalarMemoryContext::default()
                };
                let mut suppressed_registers = initial_registers(case, ordinal ^ 0xAA);
                suppressed_registers.k[usize::from(mask)] = 1u64 << 63;
                let suppressed_address = memory_address(case, &suppressed_registers);
                suppressed_registers.ctx =
                    (&mut suppressed_context as *mut ScalarMemoryContext) as u64;
                suppressed_registers.load_fn = scalar_load_helper as usize as u64;
                let mut suppressed_expected = interpreter_success(
                    &function,
                    &suppressed_registers,
                    0,
                    suppressed_address,
                    case,
                );

                exec.run(entry, &mut suppressed_registers);
                suppressed_expected.host_mxcsr = suppressed_registers.host_mxcsr;
                assert_eq!(
                    suppressed_registers, suppressed_expected,
                    "{level:?} {case:?}: suppressed access"
                );
                assert_eq!(suppressed_context.calls, 0, "{level:?} {case:?}");
                suppressed += 1;
            }
        }
    }
    assert!(successes >= 2);
    assert_eq!(faults, successes);
    assert!(suppressed >= 2);
}
