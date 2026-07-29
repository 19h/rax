//! Exact helper-backed VEX variable-blend memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SrcOperand, VReg, VecCmpCond,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexVariableBlendMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_variable_blend_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0xB14E;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    PackedSingle,
    PackedDouble,
    Byte,
}

impl Blend {
    const ALL: [Self; 3] = [Self::PackedSingle, Self::PackedDouble, Self::Byte];

    fn opcode(self) -> u8 {
        match self {
            Self::PackedSingle => 0x4A,
            Self::PackedDouble => 0x4B,
            Self::Byte => 0x4C,
        }
    }

    fn element(self) -> VecElementType {
        match self {
            Self::PackedSingle => VecElementType::I32,
            Self::PackedDouble => VecElementType::I64,
            Self::Byte => VecElementType::I8,
        }
    }

    fn needs_avx2(self, width: VecWidth) -> bool {
        self == Self::Byte && width == VecWidth::V256
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlendMemoryCase {
    blend: Blend,
    width: VecWidth,
    destination: u8,
    source1: u8,
    mask: u8,
    base: u8,
    ignored_low: u8,
}

impl BlendMemoryCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| {
                *index != self.destination && *index != self.source1 && *index != self.mask
            })
            .expect("three VEX operands leave at least thirteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.mask < 16 && self.base < 16);
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (((!self.source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.blend.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
            (self.mask << 4) | self.ignored_low,
        ]
    }

    fn emitted_bytes(self) -> [u8; 6] {
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.scratch() < 8 { 0x20 } else { 0 })
                | 3,
            (((!self.source1) & 0x0F) << 3) | (u8::from(self.width == VecWidth::V256) << 2) | 1,
            self.blend.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.scratch() & 7),
            (self.mask << 4) | self.ignored_low,
        ]
    }
}

fn scanner_cases() -> Vec<BlendMemoryCase> {
    let mut cases = Vec::with_capacity(768);
    for blend in Blend::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for destination in 0..16u8 {
                for shape in 0..8u8 {
                    let source1 = match shape {
                        0 => destination,
                        1 => destination.wrapping_add(1) & 15,
                        2 => 15,
                        3 => 0,
                        _ => destination.wrapping_add(shape.wrapping_mul(3)) & 15,
                    };
                    let mask = match shape {
                        0 => destination,
                        1 => source1,
                        2 => destination,
                        3 => 15,
                        _ => destination.wrapping_add(shape.wrapping_mul(5)) & 15,
                    };
                    cases.push(BlendMemoryCase {
                        blend,
                        width,
                        destination,
                        source1,
                        mask,
                        base: if shape & 1 == 0 { 3 } else { 11 },
                        ignored_low: (destination ^ source1 ^ mask ^ shape) & 15,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<BlendMemoryCase> {
    let shapes = [
        (1, 2, 3, 7, 0x0),
        (9, 10, 12, 11, 0xF),
        (15, 15, 15, 14, 0x5),
        (0, 0, 1, 7, 0xA),
        (5, 6, 5, 3, 0x3),
        (7, 8, 8, 11, 0xC),
    ];
    let mut cases = Vec::with_capacity(36);
    for blend in Blend::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for (destination, source1, mask, base, ignored_low) in shapes {
                cases.push(BlendMemoryCase {
                    blend,
                    width,
                    destination,
                    source1,
                    mask,
                    base,
                    ignored_low,
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
        _ => unreachable!("VEX variable blends have only 128-/256-bit forms"),
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

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexVariableBlendMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_variable_blend_memory_sequence(
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
    assert_eq!(block.ops.len(), 5, "{case:?}");
    assert!(block.ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert!(block.ops.iter().skip(1).all(|op| op.x86_hint.is_none()));

    let loaded = match block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            ref addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(width, case.width, "{case:?}");
            assert_eq!(
                block.ops[0].x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
                "{case:?}"
            );
            loaded
        }
        ref other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    let zero = match block.ops[1].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => zero,
        ref other => panic!("{case:?}: expected zero scalar, got {other:?}"),
    };
    let zero_vector = match block.ops[2].kind {
        OpKind::VBroadcast {
            dst: zero_vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes,
        } => {
            assert_eq!(scalar, zero, "{case:?}");
            assert_eq!(elem, case.blend.element(), "{case:?}");
            assert_eq!(
                lanes,
                case.width.lanes(case.blend.element()) as u8,
                "{case:?}"
            );
            zero_vector
        }
        ref other => panic!("{case:?}: expected zero broadcast, got {other:?}"),
    };
    let selection_mask = match block.ops[3].kind {
        OpKind::VCmp {
            dst: selection_mask @ VReg::Virtual(_),
            src1,
            src2,
            cond,
            elem,
            lanes,
        } => {
            assert_eq!(src1, vector(case.mask, case.width), "{case:?}");
            assert_eq!(src2, zero_vector, "{case:?}");
            assert_eq!(cond, VecCmpCond::Lt, "{case:?}");
            assert_eq!(elem, case.blend.element(), "{case:?}");
            assert_eq!(
                lanes,
                case.width.lanes(case.blend.element()) as u8,
                "{case:?}"
            );
            selection_mask
        }
        ref other => panic!("{case:?}: expected signed mask comparison, got {other:?}"),
    };
    assert!(matches!(
        block.ops[4].kind,
        OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            width,
        } if dst == vector(case.destination, case.width)
            && mask == selection_mask
            && src_true == loaded
            && src_false == vector(case.source1, case.width)
            && width == case.width
    ));

    let sequence = classified_sequence(function, true).expect("classified variable blend");
    assert_eq!(sequence.consumed, 5, "{case:?}");
    assert_eq!(sequence.encoding.width, case.width, "{case:?}");
    assert_eq!(sequence.encoding.elem, case.blend.element(), "{case:?}");
    assert_eq!(sequence.encoding.destination, case.destination, "{case:?}");
    assert_eq!(sequence.encoding.source1, case.source1, "{case:?}");
    assert_eq!(sequence.encoding.mask, case.mask, "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(sequence.encoding.opcode, case.blend.opcode(), "{case:?}");
    assert_eq!(
        sequence.encoding.memory_size,
        case.width.bytes(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.needs_avx2,
        case.blend.needs_avx2(case.width),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.emitted_bytes(),
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
) -> (Vec<u8>, usize, X86JitVexVariableBlendMemorySequence) {
    let sequence =
        classified_sequence(function, true).expect("classified VEX variable-blend sequence");
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
        .unwrap_or_else(|error| panic!("helper-backed VEX variable blend failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX variable blend"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_2_304_scanner_domain_kind_width_operand_mask_and_optimization_cells_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 768);
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
    assert_eq!(lowered, 2_304);
}

#[test]
fn llvm_23_memory_and_register_encodings_match_the_generators() {
    for (case, memory, register) in [
        (
            BlendMemoryCase {
                blend: Blend::PackedSingle,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                mask: 3,
                base: 7,
                ignored_low: 0xF,
            },
            &[0xC4, 0xE3, 0x69, 0x4A, 0x4F, 0x20, 0x3F][..],
            &[0xC4, 0xE3, 0x69, 0x4A, 0xC8, 0x3F][..],
        ),
        (
            BlendMemoryCase {
                blend: Blend::PackedDouble,
                width: VecWidth::V256,
                destination: 9,
                source1: 10,
                mask: 12,
                base: 11,
                ignored_low: 0x5,
            },
            &[0xC4, 0x43, 0x2D, 0x4B, 0x4B, 0x20, 0xC5],
            &[0xC4, 0x63, 0x2D, 0x4B, 0xC8, 0xC5],
        ),
        (
            BlendMemoryCase {
                blend: Blend::Byte,
                width: VecWidth::V128,
                destination: 15,
                source1: 15,
                mask: 13,
                base: 14,
                ignored_low: 0,
            },
            &[0xC4, 0x43, 0x01, 0x4C, 0x7E, 0x20, 0xD0],
            &[0xC4, 0x63, 0x01, 0x4C, 0xF8, 0xD0],
        ),
        (
            BlendMemoryCase {
                blend: Blend::Byte,
                width: VecWidth::V256,
                destination: 0,
                source1: 0,
                mask: 0,
                base: 7,
                ignored_low: 0xA,
            },
            &[0xC4, 0xE3, 0x7D, 0x4C, 0x47, 0x20, 0x0A],
            &[0xC4, 0xE3, 0x7D, 0x4C, 0xC1, 0x0A],
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
            &[0xC4, 0xE3, 0x69, 0x4B, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x3F],
            BlendMemoryCase {
                blend: Blend::PackedDouble,
                width: VecWidth::V128,
                destination: 1,
                source1: 2,
                mask: 3,
                base: 0,
                ignored_low: 0xF,
            },
        ),
        (
            &[
                0x64, 0xC4, 0xE3, 0x75, 0x4C, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0xA5,
            ],
            BlendMemoryCase {
                blend: Blend::Byte,
                width: VecWidth::V256,
                destination: 0,
                source1: 1,
                mask: 10,
                base: 0,
                ignored_low: 0x5,
            },
        ),
        (
            &[
                0x64, 0x67, 0xC4, 0x03, 0x2D, 0x4A, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xC1,
            ],
            BlendMemoryCase {
                blend: Blend::PackedSingle,
                width: VecWidth::V256,
                destination: 14,
                source1: 10,
                mask: 12,
                base: 14,
                ignored_low: 1,
            },
        ),
    ];

    let mut lowered = 0usize;
    for (bytes, case) in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function, *case);
            assert_eq!(sequence.encoding.width, case.width);
            assert_eq!(sequence.encoding.elem, case.blend.element());
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_eq!(sequence.encoding.mask, case.mask);
            assert_eq!(sequence.encoding.memory_size, case.width.bytes());
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed variable-blend graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed variable-blend graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed variable-blend graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated encoding fits metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_graph_and_provenance_invariants() {
    let case = BlendMemoryCase {
        blend: Blend::Byte,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        mask: 12,
        base: 11,
        ignored_low: 0x5,
    };
    let base = lift_case(case);
    let [loaded, zero, zero_vector, selection_mask] =
        std::array::from_fn(
            |index| match base.blocks[0].ops[index].kind.dests().as_slice() {
                [register @ VReg::Virtual(_)] => *register,
                other => panic!("unexpected virtual destination at {index}: {other:?}"),
            },
        );
    let mut malformed = Vec::new();

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded destination", 4, 0x08),
        ("encoded first source", 2, 0x08),
        ("encoded width", 2, 0x04),
        ("encoded mask", 6, 0x10),
        ("encoded map", 1, 0x01),
        ("encoded prefix", 2, 0x02),
        ("encoded W", 2, 0x80),
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
    let mut wrong_load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_load_width));
    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut wrong_zero_value = base.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut wrong_zero_value.blocks[0].ops[1].kind
    {
        *value = 1;
    }
    malformed.push(("nonzero comparison scalar", wrong_zero_value));
    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[1].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("zero scalar width", wrong_zero_width));
    let mut duplicate_zero = base.clone();
    if let OpKind::Mov { dst, .. } = &mut duplicate_zero.blocks[0].ops[1].kind {
        *dst = loaded;
    }
    malformed.push(("nonunique zero scalar", duplicate_zero));

    for (name, mutate) in [
        ("broadcast scalar", (0u8, loaded, VecElementType::I8, 32u8)),
        ("broadcast destination", (1, loaded, VecElementType::I8, 32)),
        ("broadcast element", (2, zero, VecElementType::I32, 32)),
        ("broadcast lanes", (3, zero, VecElementType::I8, 31)),
    ] {
        let mut function = base.clone();
        if let OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[2].kind
        {
            match mutate.0 {
                0 => *scalar = mutate.1,
                1 => *dst = mutate.1,
                2 => *elem = mutate.2,
                3 => *lanes = mutate.3,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let compare_mutations = [
        ("compare destination", 0u8),
        ("compare mask source", 1),
        ("compare zero source", 2),
        ("compare condition", 3),
        ("compare element", 4),
        ("compare lanes", 5),
    ];
    for (name, field) in compare_mutations {
        let mut function = base.clone();
        if let OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } = &mut function.blocks[0].ops[3].kind
        {
            match field {
                0 => *dst = zero_vector,
                1 => *src1 = vector(11, VecWidth::V256),
                2 => *src2 = loaded,
                3 => *cond = VecCmpCond::Eq,
                4 => *elem = VecElementType::I32,
                5 => *lanes -= 1,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let select_mutations = [
        ("select destination", 0u8),
        ("select mask", 1),
        ("select memory source", 2),
        ("select first source", 3),
        ("select width", 4),
    ];
    for (name, field) in select_mutations {
        let mut function = base.clone();
        if let OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            width,
        } = &mut function.blocks[0].ops[4].kind
        {
            match field {
                0 => *dst = vector(8, VecWidth::V256),
                1 => *mask = zero_vector,
                2 => *src_true = zero_vector,
                3 => *src_false = vector(11, VecWidth::V256),
                4 => *width = VecWidth::V128,
                _ => unreachable!(),
            }
        }
        malformed.push((name, function));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));
    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));
    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFC),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: selection_mask,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("selection mask escapes sequence", external_use));
    let mut duplicate_definition = base;
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFB),
        PC + 1,
        OpKind::Mov {
            dst: zero,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("zero scalar defined twice", duplicate_definition));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn ignored_is4_low_nibble_is_semantically_unbound_but_replayed_exactly() {
    let base = BlendMemoryCase {
        blend: Blend::PackedDouble,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        mask: 12,
        base: 11,
        ignored_low: 0,
    };
    for ignored_low in 0..16 {
        let case = BlendMemoryCase {
            ignored_low,
            ..base
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _, sequence) = lower(&function, case);
        assert_eq!(sequence.encoding.mask, base.mask);
        assert_eq!(
            sequence.encoding.register_instruction.as_slice()[5],
            (base.mask << 4) | ignored_low
        );
        assert!(
            code.windows(6).any(|window| window == case.emitted_bytes()),
            "{case:?}"
        );
    }
}
