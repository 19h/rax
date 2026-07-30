//! Exact register replay for packed AVX-512-FP16 embedded rounding and SAE.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, Avx10FP16Op, BlockId, FpRoundMode, FunctionId, OpId, SourceArch, VReg,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, TrapKind, X86InstructionBytes,
    x86_evex_native_replay_spans, x86_evex_packed_fp16_arithmetic_replay_spans,
    x86_native_replay_spans,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86NativeReplayFeatureRequirements, is_native_clobber_safe,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0xF160;
const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedCase {
    opcode: u8,
    destination: u8,
    source1: u8,
    source2: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
}

impl PackedCase {
    fn bytes(self) -> [u8; 6] {
        encoding(
            self.opcode,
            self.destination,
            self.source1,
            self.source2,
            self.ll,
            self.mask,
            self.zeroing,
            true,
        )
    }

    const fn operation(self) -> Avx10FP16Op {
        match self.opcode {
            0x58 => Avx10FP16Op::Add,
            0x59 => Avx10FP16Op::Mul,
            0x5C => Avx10FP16Op::Sub,
            0x5D => Avx10FP16Op::Min,
            0x5E => Avx10FP16Op::Div,
            0x5F => Avx10FP16Op::Max,
            _ => unreachable!(),
        }
    }

    const fn round(self) -> FpRoundMode {
        match self.ll {
            0 => FpRoundMode::RoundNearest,
            1 => FpRoundMode::RoundDown,
            2 => FpRoundMode::RoundUp,
            3 => FpRoundMode::RoundTowardZero,
            _ => unreachable!(),
        }
    }
}

fn encoding(
    opcode: u8,
    destination: u8,
    source1: u8,
    source2: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    embedded_control: bool,
) -> [u8; 6] {
    assert!(OPCODES.contains(&opcode));
    assert!(destination < 32 && source1 < 32 && source2 < 32);
    assert!(ll < 4 && (embedded_control || ll < 3));
    assert!(mask < 8 && (!zeroing || mask != 0));

    [
        0x62,
        (if destination & 0x08 == 0 { 0x80 } else { 0 })
            | (if source2 & 0x10 == 0 { 0x40 } else { 0 })
            | (if source2 & 0x08 == 0 { 0x20 } else { 0 })
            | (if destination & 0x10 == 0 { 0x10 } else { 0 })
            | 0x05,
        (((!source1) & 0x0F) << 3) | 0x04,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | (if embedded_control { 0x10 } else { 0 })
            | (if source1 & 0x10 == 0 { 0x08 } else { 0 })
            | mask,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
    ]
}

fn zmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)))
}

fn opmask(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::K(index)))
}

fn function(bytes: &[u8; 6]) -> SmirFunction {
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
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn optimized(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    optimize_function(&mut function, level);
    function
}

fn assert_embedded_graph(function: &SmirFunction, case: PackedCase) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 1, "{case:?}: {ops:#?}");
    assert_eq!(ops[0].guest_pc, PC, "{case:?}");
    assert_eq!(ops[0].x86_hint, None, "{case:?}");
    match ops[0].kind {
        OpKind::VFP16Arith {
            dst,
            src1,
            src2,
            mask,
            op,
            round,
            width,
            lanes,
            zeroing,
        } => {
            assert_eq!(dst, zmm(case.destination), "{case:?}");
            assert_eq!(src1, zmm(case.source1), "{case:?}");
            assert_eq!(src2, zmm(case.source2), "{case:?}");
            assert_eq!(
                mask,
                (case.mask != 0).then(|| opmask(case.mask)),
                "{case:?}"
            );
            assert_eq!(op, case.operation(), "{case:?}");
            assert_eq!(round, case.round(), "{case:?}");
            assert_eq!(width, VecWidth::V512, "{case:?}");
            assert_eq!(lanes, 32, "{case:?}");
            assert_eq!(zeroing, case.zeroing, "{case:?}");
        }
        ref other => panic!("{case:?}: expected VFP16Arith, got {other:#?}"),
    }
}

#[test]
fn classifier_accepts_all_196608_register_extension_cells() {
    let mut accepted = 0usize;
    for opcode in OPCODES {
        for destination in 0..32 {
            for source1 in 0..32 {
                for source2 in 0..32 {
                    let bytes = encoding(opcode, destination, source1, source2, 0, 0, false, true);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_packed_fp16_embedded_control_needs_vl(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    accepted += 1;
                }
            }
        }
    }
    assert_eq!(accepted, OPCODES.len() * 32 * 32 * 32);
}

#[test]
fn classifier_accepts_all_controls_and_matches_the_216_encoding_census() {
    let mut controls = 0usize;
    for opcode in OPCODES {
        for ll in 0..4 {
            for mask in 0..8 {
                for zeroing in [false, true] {
                    if zeroing && mask == 0 {
                        continue;
                    }
                    let bytes = encoding(opcode, 17, 18, 19, ll, mask, zeroing, true);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_packed_fp16_embedded_control_needs_vl(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    controls += 1;
                }
            }
        }
    }
    assert_eq!(controls, OPCODES.len() * 4 * 15);

    let mut census = 0usize;
    for opcode in OPCODES {
        for source1 in [0, 1, 15] {
            for ll in 0..4 {
                for (mask, zeroing) in [(0, false), (1, false), (1, true)] {
                    let bytes = encoding(opcode, 0, source1, 2, ll, mask, zeroing, true);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_packed_fp16_embedded_control_needs_vl(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    census += 1;
                }
            }
        }
    }
    assert_eq!(census, 216);
}

#[test]
fn encoding_matches_six_independent_llvm_23_anchors() {
    let anchors = [
        (
            PackedCase {
                opcode: 0x58,
                destination: 0,
                source1: 1,
                source2: 2,
                ll: 0,
                mask: 0,
                zeroing: false,
            },
            [0x62, 0xF5, 0x74, 0x18, 0x58, 0xC2],
        ),
        (
            PackedCase {
                opcode: 0x58,
                destination: 17,
                source1: 18,
                source2: 19,
                ll: 1,
                mask: 3,
                zeroing: true,
            },
            [0x62, 0xA5, 0x6C, 0xB3, 0x58, 0xCB],
        ),
        (
            PackedCase {
                opcode: 0x59,
                destination: 24,
                source1: 25,
                source2: 26,
                ll: 2,
                mask: 7,
                zeroing: false,
            },
            [0x62, 0x05, 0x34, 0x57, 0x59, 0xC2],
        ),
        (
            PackedCase {
                opcode: 0x5C,
                destination: 31,
                source1: 30,
                source2: 29,
                ll: 3,
                mask: 0,
                zeroing: false,
            },
            [0x62, 0x05, 0x0C, 0x70, 0x5C, 0xFD],
        ),
        (
            PackedCase {
                opcode: 0x5D,
                destination: 3,
                source1: 4,
                source2: 5,
                ll: 0,
                mask: 2,
                zeroing: true,
            },
            [0x62, 0xF5, 0x5C, 0x9A, 0x5D, 0xDD],
        ),
        (
            PackedCase {
                opcode: 0x5F,
                destination: 20,
                source1: 21,
                source2: 22,
                ll: 0,
                mask: 6,
                zeroing: false,
            },
            [0x62, 0xA5, 0x54, 0x16, 0x5F, 0xE6],
        ),
    ];

    for (case, expected) in anchors {
        assert_eq!(case.bytes(), expected, "{case:?}");
        assert_eq!(
            X86InstructionBytes::new(&expected)
                .unwrap()
                .evex_register_packed_fp16_embedded_control_needs_vl(),
            Some(false),
            "{case:?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_nonreplay_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF5, 0x74, 0x18, 0x58, 0xC2],       // not EVEX
        &[0x62, 0xF6, 0x74, 0x18, 0x58, 0xC2],       // MAP6, not MAP5
        &[0x62, 0xF5, 0x70, 0x18, 0x58, 0xC2],       // missing fixed-one bit
        &[0x62, 0xF5, 0xF4, 0x18, 0x58, 0xC2],       // W1, not W0
        &[0x62, 0xF5, 0x75, 0x18, 0x58, 0xC2],       // 66, not no prefix
        &[0x62, 0xF5, 0x74, 0x18, 0x58, 0x02],       // memory source
        &[0x62, 0xF5, 0x74, 0x08, 0x58, 0xC2],       // EVEX.b=0
        &[0x62, 0xF5, 0x74, 0x98, 0x58, 0xC2],       // {z} with k0
        &[0x62, 0xF5, 0x74, 0x18, 0x51, 0xC2],       // VSQRTPH is unary
        &[0x62, 0xF5, 0x74, 0x18, 0x58],             // missing ModR/M
        &[0x62, 0xF5, 0x74, 0x18, 0x58, 0xC2, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_fp16_embedded_control_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_safe_provenance() {
    let case = PackedCase {
        opcode: 0x58,
        destination: 17,
        source1: 18,
        source2: 19,
        ll: 2,
        mask: 3,
        zeroing: true,
    };
    let bytes = case.bytes();
    let instruction = X86InstructionBytes::new(&bytes).unwrap();
    let mut block = SmirBlock::new(BlockId(7), PC);
    block.push_op(SmirOp::new(OpId(0), PC, OpKind::Nop));
    let provenance = HashMap::from([((block.id, PC), instruction)]);

    for spans in [
        x86_evex_packed_fp16_arithmetic_replay_spans(&block, &provenance),
        x86_evex_native_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("packed FP16 replay span");
        assert_eq!(span.end, 1);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(span.needs_avx512fp16);
        assert!(!span.preserve_mxcsr_de);
    }

    assert!(x86_evex_packed_fp16_arithmetic_replay_spans(&block, &HashMap::new()).is_empty());

    let mut noncontiguous = block.clone();
    noncontiguous.push_op(SmirOp::new(OpId(1), PC + 6, OpKind::Nop));
    noncontiguous.push_op(SmirOp::new(OpId(2), PC, OpKind::Nop));
    assert!(x86_evex_packed_fp16_arithmetic_replay_spans(&noncontiguous, &provenance).is_empty());

    let mut memory_graph = block;
    memory_graph.ops[0] = SmirOp::new(
        OpId(0),
        PC,
        OpKind::VLoad {
            dst: VReg::Virtual(VirtualId(0)),
            addr: Address::Absolute(0),
            width: VecWidth::V512,
        },
    );
    assert!(x86_evex_packed_fp16_arithmetic_replay_spans(&memory_graph, &provenance).is_empty());
}

fn operand_shapes(opcode: u8, ll: u8) -> [PackedCase; 5] {
    [
        PackedCase {
            opcode,
            destination: 0,
            source1: 1,
            source2: 2,
            ll,
            mask: 0,
            zeroing: false,
        },
        PackedCase {
            opcode,
            destination: 17,
            source1: 18,
            source2: 19,
            ll,
            mask: 3,
            zeroing: false,
        },
        PackedCase {
            opcode,
            destination: 24,
            source1: 25,
            source2: 26,
            ll,
            mask: 7,
            zeroing: true,
        },
        PackedCase {
            opcode,
            destination: 31,
            source1: 31,
            source2: 29,
            ll,
            mask: 2,
            zeroing: false,
        },
        PackedCase {
            opcode,
            destination: 20,
            source1: 21,
            source2: 20,
            ll,
            mask: 5,
            zeroing: true,
        },
    ]
}

fn expected_host_features() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512fp16")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[test]
fn replay_admits_and_exactly_emits_all_operations_rounds_shapes_and_opt_levels() {
    let excluded = HashMap::new();
    let mut lowered = 0usize;
    for opcode in OPCODES {
        for ll in 0..4 {
            for case in operand_shapes(opcode, ll) {
                let bytes = case.bytes();
                let original = function(&bytes);
                for level in LEVELS {
                    let optimized = optimized(original.clone(), level);
                    assert_embedded_graph(&optimized, case);

                    let spans = x86_native_replay_spans(
                        &optimized.blocks[0],
                        &optimized.x86_instruction_bytes,
                    );
                    let span = spans
                        .get(&0)
                        .unwrap_or_else(|| panic!("{level:?} {case:?}"));
                    assert_eq!(span.end, 1, "{level:?} {case:?}");
                    assert_eq!(span.instruction.as_slice(), bytes, "{level:?} {case:?}");
                    assert!(!span.needs_avx512vl, "{level:?} {case:?}");
                    assert!(!span.needs_avx512dq, "{level:?} {case:?}");
                    assert!(span.needs_avx512fp16, "{level:?} {case:?}");

                    assert!(
                        is_native_clobber_safe_excluding(&optimized, &excluded, true),
                        "{level:?} {case:?}"
                    );
                    assert!(
                        is_native_clobber_safe_excluding(&optimized, &excluded, false),
                        "{level:?} {case:?}"
                    );
                    assert!(
                        uses_x86_native_vectors_excluding(&optimized, &excluded),
                        "{level:?} {case:?}"
                    );
                    assert!(
                        !x86_native_vector_uses_avx_ymm16_only_excluding(&optimized, &excluded),
                        "{level:?} {case:?}"
                    );
                    assert!(
                        !is_x86_aarch64_native_clobber_safe_excluding(&optimized, &excluded),
                        "{level:?} {case:?}"
                    );
                    assert_eq!(
                        x86_native_vector_features_supported_excluding(&optimized, &excluded),
                        expected_host_features(),
                        "{level:?} {case:?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&optimized)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    assert!(
                        code.windows(bytes.len()).any(|window| window == bytes),
                        "{level:?} {case:?}"
                    );
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, OPCODES.len() * 4 * 5 * LEVELS.len());
}

#[test]
fn replay_feature_aggregation_requires_fp16_and_full_avx512_state_only() {
    let case = PackedCase {
        opcode: 0x5E,
        destination: 31,
        source1: 30,
        source2: 29,
        ll: 3,
        mask: 7,
        zeroing: true,
    };
    let function = function(&case.bytes());
    let excluded = HashMap::new();
    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    assert!(requirements.any);
    assert!(!requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(requirements.needs_avx512fp16);
    assert!(!requirements.needs_avx512cd);
    assert!(!requirements.needs_gfni);
    assert!(!requirements.needs_avx512vp2intersect);
    assert!(!requirements.needs_vpclmulqdq);

    let excluded = HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn missing_or_nonregister_provenance_leaves_embedded_control_fail_closed() {
    let case = PackedCase {
        opcode: 0x58,
        destination: 17,
        source1: 18,
        source2: 19,
        ll: 2,
        mask: 3,
        zeroing: true,
    };
    let function = function(&case.bytes());

    let mut missing = function.clone();
    missing.x86_instruction_bytes.clear();
    for level in LEVELS {
        let missing = optimized(missing.clone(), level);
        assert!(!is_native_clobber_safe(&missing), "{level:?}");
        let mut lowerer = X86_64Lowerer::new();
        assert!(lowerer.lower_function(&missing).is_err(), "{level:?}");
    }

    let mut memory = case.bytes();
    memory[5] &= 0x3F;
    let mut wrong_shape = function;
    wrong_shape
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&memory).unwrap());
    for level in LEVELS {
        let wrong_shape = optimized(wrong_shape.clone(), level);
        assert!(!is_native_clobber_safe(&wrong_shape), "{level:?}");
        let mut lowerer = X86_64Lowerer::new();
        assert!(lowerer.lower_function(&wrong_shape).is_err(), "{level:?}");
    }
}

#[test]
fn dynamic_rounding_forms_remain_on_the_canonical_semantic_lowerer() {
    for opcode in OPCODES {
        for ll in 0..3 {
            let bytes = encoding(opcode, 17, 18, 19, ll, 3, true, false);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.evex_register_packed_fp16_embedded_control_needs_vl(),
                None,
                "{bytes:02X?}"
            );
            let original = function(&bytes);
            for level in [OptLevel::O0, OptLevel::O2] {
                let optimized = optimized(original.clone(), level);
                assert!(
                    x86_native_replay_spans(&optimized.blocks[0], &optimized.x86_instruction_bytes)
                        .is_empty(),
                    "{level:?} {bytes:02X?}"
                );
                assert!(
                    matches!(
                        optimized.blocks[0].ops.as_slice(),
                        [SmirOp {
                            kind: OpKind::VFP16Arith {
                                round: FpRoundMode::Dynamic,
                                ..
                            },
                            ..
                        }]
                    ),
                    "{level:?} {bytes:02X?}: {:#?}",
                    optimized.blocks[0].ops
                );
                assert!(is_native_clobber_safe(&optimized), "{level:?} {bytes:02X?}");
                let mut lowerer = X86_64Lowerer::new();
                lowerer
                    .lower_function(&optimized)
                    .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                lowerer
                    .finalize()
                    .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackedState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const FP16_PATTERNS: [u16; 16] = [
    0x0000, 0x8000, 0x3C00, 0xC000, 0x0001, 0x8001, 0x03FF, 0x83FF, 0x0400, 0x7BFF, 0x7C00, 0xFC00,
    0x7E01, 0xFE01, 0x7C01, 0xFC01,
];

fn set_lane(vector: &mut [u64; 8], lane: usize, value: u16) {
    assert!(lane < 32);
    let shift = (lane % 4) * 16;
    let mask = 0xFFFFu64 << shift;
    vector[lane / 4] = (vector[lane / 4] & !mask) | (u64::from(value) << shift);
}

fn lane(vector: &[u64; 8], lane: usize) -> u16 {
    ((vector[lane / 4] >> ((lane % 4) * 16)) & 0xFFFF) as u16
}

fn patterned_vector(register: usize) -> [u64; 8] {
    let mut vector = [0u64; 8];
    for lane in 0..32 {
        set_lane(
            &mut vector,
            lane,
            FP16_PATTERNS[(lane + register * 5) % FP16_PATTERNS.len()],
        );
    }
    vector
}

fn initial_state() -> PackedState {
    let mut masks = [0u64; 8];
    masks[1] = 0xFFFF_FFFF;
    masks[2] = 0xFFFF_FFFF;
    masks[3] = 0xA55A_3CC3;
    masks[5] = 0x0F0F_9696;
    masks[7] = 0x55AA_F00F;
    PackedState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(patterned_vector),
        masks,
        rflags: 0x2 | 0x8D5,
        // MXCSR.RD plus pre-existing IE/OE/PE status. Embedded control must
        // ignore RC and preserve all status bits.
        mxcsr: 0x3F80 | 0x25,
    }
}

fn executable_function(bytes: &[u8; 6], level: OptLevel, halt: bool) -> SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
    }
    optimize_function(&mut function, level);
    function
}

fn interpret(case: PackedCase, initial: &PackedState, level: OptLevel) -> PackedState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};

    let function = executable_function(&case.bytes(), level, true);
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    PackedState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: PackedCase, initial: &PackedState, level: OptLevel) -> PackedState {
    let function = executable_function(&case.bytes(), level, false);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{case:?}: {error:?}"));
    let bytes = case.bytes();
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map packed AVX-512-FP16 replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: 1,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    PackedState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[test]
fn interpreter_has_exact_packed_fp16_results_rounding_masks_sae_and_state() {
    let basic = [
        (0x58, 0x4300), // 1.5 + 2.0 = 3.5
        (0x59, 0x4200), // 1.5 * 2.0 = 3.0
        (0x5C, 0xB800), // 1.5 - 2.0 = -0.5
        (0x5D, 0x3E00), // min(1.5, 2.0) = 1.5
        (0x5E, 0x3A00), // 1.5 / 2.0 = 0.75
        (0x5F, 0x4000), // max(1.5, 2.0) = 2.0
    ];
    for level in [OptLevel::O0, OptLevel::O2] {
        for (opcode, expected) in basic {
            let case = operand_shapes(opcode, 0)[0];
            let mut initial = initial_state();
            set_lane(&mut initial.vectors[1], 0, 0x3E00);
            set_lane(&mut initial.vectors[2], 0, 0x4000);
            let result = interpret(case, &initial, level);
            assert_eq!(lane(&result.vectors[0], 0), expected, "{level:?} {case:?}");
            assert_eq!(result.mxcsr, initial.mxcsr, "{level:?} {case:?}");
            assert_eq!(result.gprs, initial.gprs, "{level:?} {case:?}");
            assert_eq!(result.rflags, initial.rflags, "{level:?} {case:?}");
            assert_eq!(result.masks, initial.masks, "{level:?} {case:?}");
            assert_eq!(result.vectors[1], initial.vectors[1], "{level:?} {case:?}");
            assert_eq!(result.vectors[2], initial.vectors[2], "{level:?} {case:?}");
        }

        let expected_rounding = [0x3C00, 0x3C00, 0x3C01, 0x3C00];
        for (ll, expected) in expected_rounding.into_iter().enumerate() {
            let case = operand_shapes(0x58, ll as u8)[0];
            let mut initial = initial_state();
            set_lane(&mut initial.vectors[1], 0, 0x3C00);
            set_lane(&mut initial.vectors[2], 0, 0x1000);
            let result = interpret(case, &initial, level);
            assert_eq!(lane(&result.vectors[0], 0), expected, "{level:?} {case:?}");
            assert_eq!(result.mxcsr, initial.mxcsr, "{level:?} {case:?}");
        }

        let merge_case = operand_shapes(0x5E, 0)[1];
        let mut masked = initial_state();
        masked.masks[3] &= !1;
        set_lane(&mut masked.vectors[17], 0, 0x3555);
        set_lane(&mut masked.vectors[18], 0, 0);
        set_lane(&mut masked.vectors[19], 0, 0);
        let merged = interpret(merge_case, &masked, level);
        assert_eq!(lane(&merged.vectors[17], 0), 0x3555, "{level:?}");
        assert_eq!(merged.mxcsr, masked.mxcsr, "{level:?}");

        let mut zero_case = merge_case;
        zero_case.zeroing = true;
        let zeroed = interpret(zero_case, &masked, level);
        assert_eq!(lane(&zeroed.vectors[17], 0), 0, "{level:?}");
        assert_eq!(zeroed.mxcsr, masked.mxcsr, "{level:?}");

        let min_case = operand_shapes(0x5D, 3)[0];
        let mut minimum = initial_state();
        set_lane(&mut minimum.vectors[1], 0, 0x0000);
        set_lane(&mut minimum.vectors[2], 0, 0x8000);
        let minimum = interpret(min_case, &minimum, level);
        assert_eq!(lane(&minimum.vectors[0], 0), 0x8000, "{level:?}");

        let max_case = operand_shapes(0x5F, 1)[0];
        let mut maximum = initial_state();
        set_lane(&mut maximum.vectors[1], 0, 0x3C00);
        set_lane(&mut maximum.vectors[2], 0, 0x7C01);
        let maximum_result = interpret(max_case, &maximum, level);
        assert_eq!(lane(&maximum_result.vectors[0], 0), 0x7C01, "{level:?}");
        assert_eq!(maximum_result.mxcsr, maximum.mxcsr, "{level:?}");
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_replay_matches_interpreter_for_all_operations_controls_masks_and_aliases() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!(
            "skipping packed FP16 embedded-control differential: host lacks AVX-512-FP16 state"
        );
        return;
    }

    let initial = initial_state();
    let mut executed = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        for opcode in OPCODES {
            for ll in 0..4 {
                for case in operand_shapes(opcode, ll) {
                    let interpreted = interpret(case, &initial, level);
                    let native = execute_native(case, &initial, level);
                    assert_eq!(native, interpreted, "{level:?} {case:?}");
                    executed += 1;
                }
            }
        }
    }
    assert_eq!(executed, 2 * OPCODES.len() * 4 * 5);
}
