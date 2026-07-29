//! Exact helper-backed VEX `VPALIGNR` memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexAlignrMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_alignr_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0xA11F;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AlignrMemoryCase {
    width: VecWidth,
    w: bool,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl AlignrMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.w) << 7)
                | (((!self.source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            0x0F,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
            self.immediate,
        ]
    }

    fn emitted_bytes(self) -> [u8; 6] {
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.w) << 7)
                | (((!self.source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            0x0F,
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
            self.immediate,
        ]
    }
}

fn scanner_cases() -> Vec<AlignrMemoryCase> {
    let mut cases = Vec::with_capacity(16 * 16 * 2 * 2);
    let mut ordinal = 0usize;
    for width in [VecWidth::V128, VecWidth::V256] {
        for w in [false, true] {
            for destination in 0..16 {
                for source1 in 0..16 {
                    cases.push(AlignrMemoryCase {
                        width,
                        w,
                        destination,
                        source1,
                        base: if ordinal & 1 == 0 { 3 } else { 11 },
                        immediate: ordinal as u8,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<AlignrMemoryCase> {
    let shapes = [
        (false, 0, 1, 3, 0x00),
        (true, 9, 10, 11, 0x01),
        (false, 15, 15, 14, 0x0F),
        (true, 0, 0, 3, 0x10),
        (false, 9, 10, 11, 0x11),
        (true, 15, 15, 14, 0x1F),
        (false, 0, 1, 3, 0x20),
        (true, 9, 9, 11, 0xFF),
    ];
    let mut cases = Vec::with_capacity(16);
    for width in [VecWidth::V128, VecWidth::V256] {
        for (w, destination, source1, base, immediate) in shapes {
            cases.push(AlignrMemoryCase {
                width,
                w,
                destination,
                source1,
                base,
                immediate,
            });
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
        _ => unreachable!("VEX VPALIGNR has only 128-/256-bit forms"),
    })
}

fn expected_address(case: AlignrMemoryCase) -> Address {
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
) -> Option<X86JitVexAlignrMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_alignr_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_graph(function: &SmirFunction, case: AlignrMemoryCase) {
    let block = &function.blocks[0];
    let lanes = case.width.lanes(VecElementType::I8) as usize;
    assert_eq!(block.ops.len(), 4 + lanes * 2, "{case:?}");
    let loaded = match &block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    assert!(block.ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert!(block.ops.iter().all(|op| op.x86_hint.is_none()), "{case:?}");
    let OpKind::VShuffle {
        dst,
        src1,
        src2,
        elem,
        lanes: shuffled_lanes,
        ..
    } = block.ops.last().unwrap().kind
    else {
        panic!("{case:?}: expected final VShuffle")
    };
    assert_eq!(dst, vector(case.destination, case.width), "{case:?}");
    assert_eq!(src1, loaded, "{case:?}");
    assert_eq!(src2, Some(vector(case.source1, case.width)), "{case:?}");
    assert_eq!(elem, VecElementType::I8, "{case:?}");
    assert_eq!(usize::from(shuffled_lanes), lanes, "{case:?}");
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexAlignrMemorySequence {
            consumed: block.ops.len(),
            memory_size: case.width.bytes(),
            destination: case.destination,
            source1: case.source1,
            width: case.width,
            immediate: case.immediate,
            w: case.w,
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

fn lift_case(case: AlignrMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_graph(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(
    function: &SmirFunction,
    case: AlignrMemoryCase,
) -> (Vec<u8>, usize, X86JitVexAlignrMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VEX VPALIGNR sequence");
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
    assert_eq!(
        requirements.needs_avx2,
        case.width == VecWidth::V256,
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX VPALIGNR lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VPALIGNR"),
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
            assert_exact_graph(&function, case);
            let (code, _, _) = lower(&function, case);
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
    let xmm = AlignrMemoryCase {
        width: VecWidth::V128,
        w: false,
        destination: 2,
        source1: 0,
        base: 7,
        immediate: 0x07,
    };
    assert_eq!(xmm.bytes(), [0xC4, 0xE3, 0x79, 0x0F, 0x57, 0x20, 0x07]);
    assert_eq!(xmm.emitted_bytes(), [0xC4, 0xE3, 0x79, 0x0F, 0xD1, 0x07]);

    let ymm = AlignrMemoryCase {
        width: VecWidth::V256,
        w: false,
        destination: 15,
        source1: 0,
        base: 11,
        immediate: 0x1F,
    };
    assert_eq!(ymm.bytes(), [0xC4, 0x43, 0x7D, 0x0F, 0x7B, 0x20, 0x1F]);
    assert_eq!(ymm.emitted_bytes(), [0xC4, 0x63, 0x7D, 0x0F, 0xF9, 0x1F]);
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[(&[u8], AlignrMemoryCase)] = &[
        (
            // vpalignr xmm1,xmm2,[rip+0x44332211],0x03
            &[0xC4, 0xE3, 0x69, 0x0F, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x03],
            AlignrMemoryCase {
                width: VecWidth::V128,
                w: false,
                destination: 1,
                source1: 2,
                base: 0,
                immediate: 0x03,
            },
        ),
        (
            // vpalignr ymm0,ymm1,fs:[rcx*4+0x44332211],0x11
            &[
                0x64, 0xC4, 0xE3, 0x75, 0x0F, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x11,
            ],
            AlignrMemoryCase {
                width: VecWidth::V256,
                w: false,
                destination: 0,
                source1: 1,
                base: 0,
                immediate: 0x11,
            },
        ),
        (
            // vpalignr ymm14,ymm10,fs:addr32 [r14d+r15d*2+0x44332211],0x1F
            &[
                0x64, 0x67, 0xC4, 0x03, 0x2D, 0x0F, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x1F,
            ],
            AlignrMemoryCase {
                width: VecWidth::V256,
                w: false,
                destination: 14,
                source1: 10,
                base: 14,
                immediate: 0x1F,
            },
        ),
    ];

    let mut lowered = 0usize;
    for (bytes, case) in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function, *case);
            assert_eq!(sequence.memory_size, case.width.bytes());
            assert_eq!(sequence.destination, case.destination);
            assert_eq!(sequence.source1, case.source1);
            assert_eq!(sequence.width, case.width);
            assert_eq!(sequence.immediate, case.immediate);
            assert_eq!(sequence.w, case.w);
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed VPALIGNR graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed VPALIGNR graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed VPALIGNR graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated encoding fits metadata"),
    );
}

fn loaded_virtual(function: &SmirFunction) -> VReg {
    match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    }
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_graph_and_provenance_invariants() {
    let case = AlignrMemoryCase {
        width: VecWidth::V256,
        w: true,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0x11,
    };
    let base = lift_case(case);
    let loaded = loaded_virtual(&base);
    let final_index = base.blocks[0].ops.len() - 1;
    let mut malformed = Vec::new();

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded width", 2, 0x04),
        ("encoded immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    for (name, mutate) in [
        ("encoded map", (1usize, 0x01u8)),
        ("encoded prefix", (2, 0x02)),
        ("encoded opcode", (3, 0x01)),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[mutate.0] ^= mutate.1;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut register_source = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    replace_instruction_bytes(&mut register_source, &bytes);
    malformed.push(("register-source provenance", register_source));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented load hint", load_hint));

    let mut wrong_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_width));

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFF),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value escapes sequence", external_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFE),
        PC + 1,
        OpKind::VLoad {
            dst: loaded,
            addr: expected_address(case),
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value defined twice", duplicate_definition));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[3].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_destination = base.clone();
    if let OpKind::VShuffle { dst, .. } = &mut wrong_destination.blocks[0].ops[final_index].kind {
        *dst = vector(8, VecWidth::V256);
    }
    malformed.push(("final destination", wrong_destination));

    let mut wrong_low = base.clone();
    if let OpKind::VShuffle { src1, .. } = &mut wrong_low.blocks[0].ops[final_index].kind {
        *src1 = vector(8, VecWidth::V256);
    }
    malformed.push(("final low source", wrong_low));

    let mut wrong_high = base.clone();
    if let OpKind::VShuffle { src2, .. } = &mut wrong_high.blocks[0].ops[final_index].kind {
        *src2 = Some(vector(8, VecWidth::V256));
    }
    malformed.push(("final high source", wrong_high));

    let mut wrong_element = base.clone();
    if let OpKind::VShuffle { elem, .. } = &mut wrong_element.blocks[0].ops[final_index].kind {
        *elem = VecElementType::I16;
    }
    malformed.push(("final element", wrong_element));

    let mut wrong_lanes = base.clone();
    if let OpKind::VShuffle { lanes, .. } = &mut wrong_lanes.blocks[0].ops[final_index].kind {
        *lanes -= 1;
    }
    malformed.push(("final lane count", wrong_lanes));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn every_selector_and_insert_invariant_is_fail_closed() {
    let case = AlignrMemoryCase {
        width: VecWidth::V256,
        w: false,
        destination: 15,
        source1: 10,
        base: 11,
        immediate: 0x11,
    };
    let base = lift_case(case);
    let lanes = case.width.lanes(VecElementType::I8) as usize;
    let loaded = loaded_virtual(&base);
    let (zero, indices) = match (&base.blocks[0].ops[1].kind, &base.blocks[0].ops[2].kind) {
        (
            OpKind::Mov { dst: zero, .. },
            OpKind::VBroadcast {
                dst: indices,
                scalar,
                ..
            },
        ) if scalar == zero => (*zero, *indices),
        _ => unreachable!("validated zero-vector index construction"),
    };

    let mut wrong_zero = base.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut wrong_zero.blocks[0].ops[1].kind
    {
        *value = 1;
    }
    assert_rejected("nonzero index initializer", &wrong_zero);

    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[1].kind {
        *width = OpWidth::W32;
    }
    assert_rejected("index initializer width", &wrong_zero_width);

    let mut wrong_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut wrong_broadcast_scalar.blocks[0].ops[2].kind {
        *scalar = loaded;
    }
    assert_rejected("index broadcast scalar", &wrong_broadcast_scalar);

    let mut wrong_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut wrong_broadcast_element.blocks[0].ops[2].kind {
        *elem = VecElementType::I16;
    }
    assert_rejected("index broadcast element", &wrong_broadcast_element);

    let mut wrong_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast_lanes.blocks[0].ops[2].kind {
        *lanes -= 1;
    }
    assert_rejected("index broadcast lanes", &wrong_broadcast_lanes);

    for lane in 0..lanes {
        let mov_index = 3 + lane * 2;
        let insert_index = mov_index + 1;

        let mut wrong_selector = base.clone();
        if let OpKind::Mov {
            src: SrcOperand::Imm(selector),
            ..
        } = &mut wrong_selector.blocks[0].ops[mov_index].kind
        {
            *selector ^= 1;
        }
        assert_rejected("lane selector immediate", &wrong_selector);

        let mut wrong_selector_width = base.clone();
        if let OpKind::Mov { width, .. } = &mut wrong_selector_width.blocks[0].ops[mov_index].kind {
            *width = OpWidth::W32;
        }
        assert_rejected("lane selector width", &wrong_selector_width);

        let mut duplicate_selector = base.clone();
        if let OpKind::Mov { dst, .. } = &mut duplicate_selector.blocks[0].ops[mov_index].kind {
            *dst = zero;
        }
        assert_rejected("nonunique lane selector", &duplicate_selector);

        let mut wrong_insert_destination = base.clone();
        if let OpKind::VInsertLane { dst, .. } =
            &mut wrong_insert_destination.blocks[0].ops[insert_index].kind
        {
            *dst = loaded;
        }
        assert_rejected("insert destination", &wrong_insert_destination);

        let mut wrong_insert_vector = base.clone();
        if let OpKind::VInsertLane { vec, .. } =
            &mut wrong_insert_vector.blocks[0].ops[insert_index].kind
        {
            *vec = loaded;
        }
        assert_rejected("insert input vector", &wrong_insert_vector);

        let mut wrong_insert_scalar = base.clone();
        if let OpKind::VInsertLane { scalar, .. } =
            &mut wrong_insert_scalar.blocks[0].ops[insert_index].kind
        {
            *scalar = loaded;
        }
        assert_rejected("insert selector register", &wrong_insert_scalar);

        let mut wrong_insert_lane = base.clone();
        if let OpKind::VInsertLane { lane, .. } =
            &mut wrong_insert_lane.blocks[0].ops[insert_index].kind
        {
            *lane = lane.wrapping_add(1);
        }
        assert_rejected("insert destination lane", &wrong_insert_lane);

        let mut wrong_insert_element = base.clone();
        if let OpKind::VInsertLane { elem, .. } =
            &mut wrong_insert_element.blocks[0].ops[insert_index].kind
        {
            *elem = VecElementType::I16;
        }
        assert_rejected("insert element", &wrong_insert_element);
    }

    let final_index = base.blocks[0].ops.len() - 1;
    let mut wrong_indices = base.clone();
    if let OpKind::VShuffle { indices, .. } = &mut wrong_indices.blocks[0].ops[final_index].kind {
        *indices = loaded;
    }
    assert_rejected("final shuffle indices", &wrong_indices);

    let mut escaped_indices = base.clone();
    escaped_indices.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: indices,
            width: VecWidth::V256,
        },
    ));
    assert_rejected("index vector escapes sequence", &escaped_indices);
}
