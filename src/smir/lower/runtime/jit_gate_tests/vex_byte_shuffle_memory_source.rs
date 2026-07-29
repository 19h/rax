//! Exact helper-backed VEX `VPSHUFB` memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexBinaryMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_binary_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0xB6FB;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteShuffleMemoryCase {
    width: VecWidth,
    w: bool,
    destination: u8,
    source1: u8,
    base: u8,
}

impl ByteShuffleMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        let l = u8::from(self.width == VecWidth::V256);
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w) << 7) | (((!self.source1) & 0x0F) << 3) | (l << 2) | 1,
            0x00,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
        ]
    }

    fn emitted_bytes(self) -> [u8; 5] {
        let l = u8::from(self.width == VecWidth::V256);
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 }) | 0x60 | 2,
            (u8::from(self.w) << 7) | (((!self.source1) & 0x0F) << 3) | (l << 2) | 1,
            0x00,
            0xC0 | ((self.destination & 7) << 3) | self.scratch(),
        ]
    }
}

fn scanner_cases() -> Vec<ByteShuffleMemoryCase> {
    let mut cases = Vec::with_capacity(16 * 16 * 2 * 2);
    for width in [VecWidth::V128, VecWidth::V256] {
        for w in [false, true] {
            for destination in 0..16 {
                for source1 in 0..16 {
                    cases.push(ByteShuffleMemoryCase {
                        width,
                        w,
                        destination,
                        source1,
                        base: 2,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<ByteShuffleMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256] {
        for w in [false, true] {
            for (destination, source1, base) in [(0, 1, 3), (9, 10, 11), (15, 15, 14)] {
                cases.push(ByteShuffleMemoryCase {
                    width,
                    w,
                    destination,
                    source1,
                    base,
                });
            }
        }
    }
    cases
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX VPSHUFB has only 128-/256-bit forms"),
    })
}

fn expected_address(case: ByteShuffleMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexBinaryMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_binary_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_pair(function: &SmirFunction, case: ByteShuffleMemoryCase) {
    let [load, consumer] = function.blocks[0].ops.as_slice() else {
        panic!("{case:?}: expected exact VLoad + VByteShuffle pair")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let temporary = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *temporary
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };
    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(
        consumer.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x00,
            width: case.width,
            w: case.w,
        }),
        "{case:?}"
    );
    let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    } = consumer.kind
    else {
        panic!("{case:?}: expected VByteShuffle consumer")
    };
    assert_eq!(dst, vector(case.destination, case.width), "{case:?}");
    assert_eq!(src, vector(case.source1, case.width), "{case:?}");
    assert_eq!(control, temporary, "{case:?}");
    assert_eq!(
        u32::from(lanes),
        case.width.lanes(VecElementType::I8),
        "{case:?}"
    );
    assert_eq!(block_lanes, 16, "{case:?}");
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexBinaryMemorySequence {
            consumed: 2,
            memory_size: case.width.bytes(),
            destination: case.destination,
            source1: case.source1,
            width: case.width,
            map: X86VecMap::Map0F38,
            prefix: X86SsePrefix::OpSize,
            opcode: 0x00,
            w: case.w,
            needs_avx2: case.width == VecWidth::V256,
            needs_fma: false,
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn lift_case(case: ByteShuffleMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_pair(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexBinaryMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VPSHUFB memory pair");
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
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert_eq!(requirements.needs_avx2, sequence.width == VecWidth::V256);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX VPSHUFB lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VPSHUFB"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_3072_destination_source_wig_width_and_optimization_cells_admit_and_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 16 * 16 * 2 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function, case);
            let (code, _, _) = lower(&function);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 3_072);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    let xmm = ByteShuffleMemoryCase {
        width: VecWidth::V128,
        w: false,
        destination: 0,
        source1: 1,
        base: 3,
    };
    assert_eq!(xmm.bytes(), [0xC4, 0xE2, 0x71, 0x00, 0x43, 0x20]);
    assert_eq!(xmm.emitted_bytes(), [0xC4, 0xE2, 0x71, 0x00, 0xC2]);

    let ymm = ByteShuffleMemoryCase {
        width: VecWidth::V256,
        w: false,
        destination: 15,
        source1: 0,
        base: 11,
    };
    assert_eq!(ymm.bytes(), [0xC4, 0x42, 0x7D, 0x00, 0x7B, 0x20]);
    assert_eq!(ymm.emitted_bytes(), [0xC4, 0x62, 0x7D, 0x00, 0xF9]);
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vpshufb xmm1,xmm2,[rip+0x44332211]
        &[0xC4, 0xE2, 0x69, 0x00, 0x0D, 0x11, 0x22, 0x33, 0x44],
        // vpshufb ymm0,ymm1,fs:[rcx*4+0x44332211]
        &[
            0x64, 0xC4, 0xE2, 0x75, 0x00, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44,
        ],
        // vpshufb ymm14,ymm10,fs:addr32 [r14d+r15d*2+0x44332211]
        &[
            0x64, 0x67, 0xC4, 0x02, 0xAD, 0x00, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function);
            assert!(matches!(sequence.width, VecWidth::V128 | VecWidth::V256));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed VPSHUFB pair"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed VPSHUFB pair"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed VPSHUFB pair"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_every_pair_and_provenance_invariant() {
    let case = ByteShuffleMemoryCase {
        width: VecWidth::V128,
        w: false,
        destination: 3,
        source1: 9,
        base: 11,
    };
    let base = lift_case(case);
    let temporary = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: temporary,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("temporary used twice", extra_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VLoad {
            dst: temporary,
            addr: expected_address(case),
            width: VecWidth::V128,
        },
    ));
    malformed.push(("temporary defined twice", duplicate_definition));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("unexpected load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }
    malformed.push(("load/consumer width mismatch", load_width));

    let mut invalid_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut invalid_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", invalid_address));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("different guest PCs", wrong_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_control = base.clone();
    if let OpKind::VByteShuffle { control, .. } = &mut wrong_control.blocks[0].ops[1].kind {
        *control = vector(2, VecWidth::V128);
    }
    malformed.push(("consumer bypasses temporary", wrong_control));

    let mut wrong_lanes = base.clone();
    if let OpKind::VByteShuffle { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes = 32;
    }
    malformed.push(("consumer width", wrong_lanes));

    let mut wrong_block_lanes = base.clone();
    if let OpKind::VByteShuffle { block_lanes, .. } = &mut wrong_block_lanes.blocks[0].ops[1].kind {
        *block_lanes = 8;
    }
    malformed.push(("cross-lane domain", wrong_block_lanes));

    let mut high_destination = base.clone();
    if let OpKind::VByteShuffle { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V128);
    }
    malformed.push(("high EVEX-only destination", high_destination));

    let mut high_source1 = base.clone();
    if let OpKind::VByteShuffle { src, .. } = &mut high_source1.blocks[0].ops[1].kind {
        *src = vector(16, VecWidth::V128);
    }
    malformed.push(("high EVEX-only first source", high_source1));

    let mut wrong_namespace = base.clone();
    if let OpKind::VByteShuffle { dst, .. } = &mut wrong_namespace.blocks[0].ops[1].kind {
        *dst = x86(X86Reg::Ymm(case.destination));
    }
    malformed.push(("destination namespace", wrong_namespace));

    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[1].x86_hint = None;
    malformed.push(("missing consumer hint", missing_hint));

    for (name, hint) in [
        (
            "hint map",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            "hint prefix",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0x00,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            "hint opcode",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x01,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            "hint width",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            "hint W",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: true,
            },
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[1].x86_hint = Some(hint);
        malformed.push((name, function));
    }

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x03),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded width", 2, 0x04),
        ("encoded W", 2, 0x80),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut register_source = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.truncate(5);
    replace_instruction_bytes(&mut register_source, &bytes);
    malformed.push(("register-source provenance", register_source));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}
