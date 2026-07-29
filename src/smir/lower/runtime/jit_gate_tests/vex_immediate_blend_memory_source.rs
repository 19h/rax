//! Exact helper-backed VEX immediate-blend memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexImmediateBlendMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_immediate_blend_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0xB1E4;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    Dword,
    PackedSingle,
    PackedDouble,
    Word,
}

impl Blend {
    const ALL: [Self; 4] = [
        Self::Dword,
        Self::PackedSingle,
        Self::PackedDouble,
        Self::Word,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::Dword => 0x02,
            Self::PackedSingle => 0x0C,
            Self::PackedDouble => 0x0D,
            Self::Word => 0x0E,
        }
    }

    fn element(self) -> VecElementType {
        match self {
            Self::Dword | Self::PackedSingle => VecElementType::I32,
            Self::PackedDouble => VecElementType::I64,
            Self::Word => VecElementType::I16,
        }
    }

    fn repeat_128(self) -> bool {
        self == Self::Word
    }

    fn legal_w(self) -> &'static [bool] {
        match self {
            Self::Dword => &[false],
            Self::PackedSingle | Self::PackedDouble | Self::Word => &[false, true],
        }
    }

    fn needs_avx2(self, width: VecWidth) -> bool {
        self == Self::Dword || self == Self::Word && width == VecWidth::V256
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlendMemoryCase {
    blend: Blend,
    width: VecWidth,
    w: bool,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl BlendMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.blend.legal_w().contains(&self.w));
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
            self.blend.opcode(),
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
            self.blend.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
            self.immediate,
        ]
    }
}

fn scanner_cases() -> Vec<BlendMemoryCase> {
    let mut cases = Vec::with_capacity(3_584);
    let mut ordinal = 0usize;
    for blend in Blend::ALL {
        for &w in blend.legal_w() {
            for width in [VecWidth::V128, VecWidth::V256] {
                for destination in 0..16 {
                    for source1 in 0..16 {
                        cases.push(BlendMemoryCase {
                            blend,
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
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<BlendMemoryCase> {
    let shapes = [
        (0, 1, 3, 0x00),
        (9, 10, 11, 0xFF),
        (15, 15, 14, 0xA5),
        (0, 0, 0, 0x81),
    ];
    let mut cases = Vec::with_capacity(56);
    for blend in Blend::ALL {
        for &w in blend.legal_w() {
            for width in [VecWidth::V128, VecWidth::V256] {
                for (destination, source1, base, immediate) in shapes {
                    cases.push(BlendMemoryCase {
                        blend,
                        width,
                        w,
                        destination,
                        source1,
                        base,
                        immediate,
                    });
                }
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
        _ => unreachable!("VEX immediate blends have only 128-/256-bit forms"),
    })
}

fn expected_address(case: BlendMemoryCase) -> Address {
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
) -> Option<X86JitVexImmediateBlendMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_immediate_blend_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_graph(function: &SmirFunction, case: BlendMemoryCase) {
    let block = &function.blocks[0];
    let lanes = case.width.lanes(case.blend.element()) as usize;
    assert_eq!(block.ops.len(), 4 + lanes * 2, "{case:?}");
    let loaded = match &block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            assert_eq!(
                block.ops[0].x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
                "{case:?}"
            );
            *loaded
        }
        other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    assert!(block.ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert!(block.ops.iter().skip(1).all(|op| op.x86_hint.is_none()));
    let OpKind::VMov { dst, src, width } = block.ops.last().unwrap().kind else {
        panic!("{case:?}: expected final VMov")
    };
    assert_eq!(dst, vector(case.destination, case.width), "{case:?}");
    assert!(matches!(src, VReg::Virtual(_)), "{case:?}");
    assert_ne!(src, loaded, "{case:?}");
    assert_eq!(width, case.width, "{case:?}");

    let sequence = classified_sequence(function, true).expect("classified immediate blend");
    assert_eq!(sequence.consumed, block.ops.len(), "{case:?}");
    assert_eq!(sequence.memory_size, case.width.bytes(), "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.element, case.blend.element(), "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(sequence.encoding.immediate, case.immediate, "{case:?}");
    assert_eq!(sequence.encoding.opcode, case.blend.opcode(), "{case:?}");
    assert_eq!(sequence.encoding.w, case.w, "{case:?}");
    assert_eq!(
        sequence.encoding.repeat_128,
        case.blend.repeat_128(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.needs_avx2,
        case.blend.needs_avx2(case.width),
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

fn lift_case(case: BlendMemoryCase) -> SmirFunction {
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
    case: BlendMemoryCase,
) -> (Vec<u8>, usize, X86JitVexImmediateBlendMemorySequence) {
    let sequence =
        classified_sequence(function, true).expect("classified VEX immediate-blend sequence");
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
        case.blend.needs_avx2(case.width),
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX immediate blend failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX immediate blend"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_10_752_kind_destination_source_w_width_and_optimization_cells_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 3_584);
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
    assert_eq!(lowered, 10_752);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    for (case, memory, register) in [
        (
            BlendMemoryCase {
                blend: Blend::Dword,
                width: VecWidth::V128,
                w: false,
                destination: 1,
                source1: 2,
                base: 7,
                immediate: 0xA5,
            },
            &[0xC4, 0xE3, 0x69, 0x02, 0x4F, 0x20, 0xA5][..],
            &[0xC4, 0xE3, 0x69, 0x02, 0xC8, 0xA5][..],
        ),
        (
            BlendMemoryCase {
                blend: Blend::PackedSingle,
                width: VecWidth::V256,
                w: false,
                destination: 9,
                source1: 10,
                base: 11,
                immediate: 0x81,
            },
            &[0xC4, 0x43, 0x2D, 0x0C, 0x4B, 0x20, 0x81],
            &[0xC4, 0x63, 0x2D, 0x0C, 0xC8, 0x81],
        ),
        (
            BlendMemoryCase {
                blend: Blend::PackedDouble,
                width: VecWidth::V128,
                w: false,
                destination: 15,
                source1: 15,
                base: 14,
                immediate: 0x5A,
            },
            &[0xC4, 0x43, 0x01, 0x0D, 0x7E, 0x20, 0x5A],
            &[0xC4, 0x63, 0x01, 0x0D, 0xF8, 0x5A],
        ),
        (
            BlendMemoryCase {
                blend: Blend::Word,
                width: VecWidth::V256,
                w: false,
                destination: 0,
                source1: 0,
                base: 7,
                immediate: 0xFF,
            },
            &[0xC4, 0xE3, 0x7D, 0x0E, 0x47, 0x20, 0xFF],
            &[0xC4, 0xE3, 0x7D, 0x0E, 0xC1, 0xFF],
        ),
    ] {
        assert_eq!(case.bytes(), memory, "{case:?}");
        assert_eq!(case.emitted_bytes(), register, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_shapes_admit_at_every_level() {
    let encodings: &[(&[u8], BlendMemoryCase)] = &[
        (
            &[0xC4, 0xE3, 0x69, 0x0D, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x03],
            BlendMemoryCase {
                blend: Blend::PackedDouble,
                width: VecWidth::V128,
                w: false,
                destination: 1,
                source1: 2,
                base: 0,
                immediate: 0x03,
            },
        ),
        (
            &[
                0x64, 0xC4, 0xE3, 0x75, 0x0E, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0xA5,
            ],
            BlendMemoryCase {
                blend: Blend::Word,
                width: VecWidth::V256,
                w: false,
                destination: 0,
                source1: 1,
                base: 0,
                immediate: 0xA5,
            },
        ),
        (
            &[
                0x64, 0x67, 0xC4, 0x03, 0xAD, 0x0C, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x81,
            ],
            BlendMemoryCase {
                blend: Blend::PackedSingle,
                width: VecWidth::V256,
                w: true,
                destination: 14,
                source1: 10,
                base: 14,
                immediate: 0x81,
            },
        ),
    ];

    let mut lowered = 0usize;
    for (bytes, case) in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function, *case);
            assert_eq!(sequence.memory_size, case.width.bytes());
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_eq!(sequence.encoding.element, case.blend.element());
            assert_eq!(sequence.encoding.width, case.width);
            assert_eq!(sequence.encoding.immediate, case.immediate);
            assert_eq!(sequence.encoding.opcode, case.blend.opcode());
            assert_eq!(sequence.encoding.w, case.w);
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed immediate-blend graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed immediate-blend graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed immediate-blend graph"
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
    let case = BlendMemoryCase {
        blend: Blend::Word,
        width: VecWidth::V256,
        w: true,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0xA5,
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

    for (name, byte_index, xor) in [
        ("encoded map", 1usize, 0x01u8),
        ("encoded prefix", 2, 0x02),
        ("encoded opcode", 3, 0x01),
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
    bytes.remove(5);
    replace_instruction_bytes(&mut register_source, &bytes);
    malformed.push(("register-source provenance", register_source));

    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing unaligned load hint", missing_hint));

    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(("aligned load hint", wrong_hint));

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
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_destination = base.clone();
    if let OpKind::VMov { dst, .. } = &mut wrong_destination.blocks[0].ops[final_index].kind {
        *dst = vector(8, VecWidth::V256);
    }
    malformed.push(("final destination", wrong_destination));

    let mut wrong_source = base.clone();
    if let OpKind::VMov { src, .. } = &mut wrong_source.blocks[0].ops[final_index].kind {
        *src = loaded;
    }
    malformed.push(("final source", wrong_source));

    let mut wrong_move_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut wrong_move_width.blocks[0].ops[final_index].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("final width", wrong_move_width));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn every_extract_insert_and_zero_vector_invariant_is_fail_closed() {
    let case = BlendMemoryCase {
        blend: Blend::Word,
        width: VecWidth::V256,
        w: false,
        destination: 15,
        source1: 10,
        base: 11,
        immediate: 0xA5,
    };
    let base = lift_case(case);
    let lanes = case.width.lanes(case.blend.element()) as usize;
    let loaded = loaded_virtual(&base);
    let source1 = vector(case.source1, case.width);
    let zero_index = 1 + lanes;
    let (zero, output) = match (
        &base.blocks[0].ops[zero_index].kind,
        &base.blocks[0].ops[zero_index + 1].kind,
    ) {
        (
            OpKind::Mov { dst: zero, .. },
            OpKind::VBroadcast {
                dst: output,
                scalar,
                ..
            },
        ) if scalar == zero => (*zero, *output),
        _ => unreachable!("validated immediate-blend zero-vector construction"),
    };

    let mut wrong_zero = base.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut wrong_zero.blocks[0].ops[zero_index].kind
    {
        *value = 1;
    }
    assert_rejected("nonzero output initializer", &wrong_zero);

    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[zero_index].kind {
        *width = OpWidth::W32;
    }
    assert_rejected("output initializer width", &wrong_zero_width);

    let mut wrong_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } =
        &mut wrong_broadcast_scalar.blocks[0].ops[zero_index + 1].kind
    {
        *scalar = loaded;
    }
    assert_rejected("output broadcast scalar", &wrong_broadcast_scalar);

    let mut wrong_broadcast_output = base.clone();
    if let OpKind::VBroadcast { dst, .. } =
        &mut wrong_broadcast_output.blocks[0].ops[zero_index + 1].kind
    {
        *dst = loaded;
    }
    assert_rejected("output broadcast destination", &wrong_broadcast_output);

    let mut wrong_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } =
        &mut wrong_broadcast_element.blocks[0].ops[zero_index + 1].kind
    {
        *elem = VecElementType::I32;
    }
    assert_rejected("output broadcast element", &wrong_broadcast_element);

    let mut wrong_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } =
        &mut wrong_broadcast_lanes.blocks[0].ops[zero_index + 1].kind
    {
        *lanes -= 1;
    }
    assert_rejected("output broadcast lanes", &wrong_broadcast_lanes);

    for lane in 0..lanes {
        let extract_index = 1 + lane;
        let insert_index = zero_index + 2 + lane;
        let expected_memory = (case.immediate >> (lane % 8)) & 1 != 0;

        let mut wrong_extract_source = base.clone();
        if let OpKind::VExtractLane { vec, .. } =
            &mut wrong_extract_source.blocks[0].ops[extract_index].kind
        {
            *vec = if expected_memory { source1 } else { loaded };
        }
        assert_rejected("extract source", &wrong_extract_source);

        let mut wrong_extract_lane = base.clone();
        if let OpKind::VExtractLane {
            lane: extracted, ..
        } = &mut wrong_extract_lane.blocks[0].ops[extract_index].kind
        {
            *extracted = extracted.wrapping_add(1);
        }
        assert_rejected("extract lane", &wrong_extract_lane);

        let mut wrong_extract_element = base.clone();
        if let OpKind::VExtractLane { elem, .. } =
            &mut wrong_extract_element.blocks[0].ops[extract_index].kind
        {
            *elem = VecElementType::I32;
        }
        assert_rejected("extract element", &wrong_extract_element);

        let mut wrong_extract_sign = base.clone();
        if let OpKind::VExtractLane { sign, .. } =
            &mut wrong_extract_sign.blocks[0].ops[extract_index].kind
        {
            *sign = SignExtend::Sign;
        }
        assert_rejected("extract extension", &wrong_extract_sign);

        let mut duplicate_extract = base.clone();
        if let OpKind::VExtractLane { dst, .. } =
            &mut duplicate_extract.blocks[0].ops[extract_index].kind
        {
            *dst = zero;
        }
        assert_rejected("nonunique extracted scalar", &duplicate_extract);

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
        assert_rejected("insert scalar", &wrong_insert_scalar);

        let mut wrong_insert_lane = base.clone();
        if let OpKind::VInsertLane { lane: inserted, .. } =
            &mut wrong_insert_lane.blocks[0].ops[insert_index].kind
        {
            *inserted = inserted.wrapping_add(1);
        }
        assert_rejected("insert lane", &wrong_insert_lane);

        let mut wrong_insert_element = base.clone();
        if let OpKind::VInsertLane { elem, .. } =
            &mut wrong_insert_element.blocks[0].ops[insert_index].kind
        {
            *elem = VecElementType::I32;
        }
        assert_rejected("insert element", &wrong_insert_element);
    }

    let final_index = base.blocks[0].ops.len() - 1;
    let mut escaped_output = base.clone();
    escaped_output.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: output,
            width: VecWidth::V256,
        },
    ));
    assert_rejected("output vector escapes sequence", &escaped_output);

    let mut duplicate_output = base;
    if let OpKind::VMov { src, .. } = &mut duplicate_output.blocks[0].ops[final_index].kind {
        *src = zero;
    }
    assert_rejected("final move bypasses output vector", &duplicate_output);
}
