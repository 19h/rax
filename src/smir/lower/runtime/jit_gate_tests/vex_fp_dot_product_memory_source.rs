//! Exact helper-backed VEX `VDPPS`/`VDPPD` memory-source coverage.

use super::*;
#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexFpDotProductMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_fp_dot_product_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0xD04D;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotKind {
    Ps128,
    Ps256,
    Pd128,
}

impl DotKind {
    const ALL: [Self; 3] = [Self::Ps128, Self::Ps256, Self::Pd128];

    const fn elem(self) -> VecElementType {
        match self {
            Self::Ps128 | Self::Ps256 => VecElementType::F32,
            Self::Pd128 => VecElementType::F64,
        }
    }

    const fn width(self) -> VecWidth {
        match self {
            Self::Ps128 | Self::Pd128 => VecWidth::V128,
            Self::Ps256 => VecWidth::V256,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::Ps128 | Self::Ps256 => 0x40,
            Self::Pd128 => 0x41,
        }
    }

    const fn lanes_per_group(self) -> usize {
        match self {
            Self::Ps128 | Self::Ps256 => 4,
            Self::Pd128 => 2,
        }
    }

    const fn groups(self) -> usize {
        match self {
            Self::Ps256 => 2,
            Self::Ps128 | Self::Pd128 => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DotCase {
    kind: DotKind,
    w: bool,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl DotCase {
    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && *index != self.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.w) << 7)
                | (((!self.source1) & 0x0F) << 3)
                | (u8::from(self.kind.width() == VecWidth::V256) << 2)
                | 1,
            self.kind.opcode(),
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes.push(self.immediate);
        bytes
    }

    fn emitted_register_bytes(self) -> [u8; 6] {
        let scratch = self.scratch();
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.w) << 7)
                | (((!self.source1) & 0x0F) << 3)
                | (u8::from(self.kind.width() == VecWidth::V256) << 2)
                | 1,
            self.kind.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (scratch & 7),
            self.immediate,
        ]
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX dot products have only 128-/256-bit forms"),
    })
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

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexFpDotProductMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_fp_dot_product_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexFpDotProductMemorySequence> {
    classified_at(function, 0, allow_mem)
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

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: DotCase) {
    let block = &function.blocks[0];
    assert_eq!(block.ops.len(), 2, "{case:?}");
    let loaded = match &block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            width,
            ..
        } => {
            assert_eq!(*width, case.kind.width(), "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    assert!(
        matches!(
            block.ops[0].x86_hint,
            Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        ),
        "{case:?}: unexpected load hint {:?}",
        block.ops[0].x86_hint
    );
    assert_eq!(block.ops[1].guest_pc, PC, "{case:?}");
    assert_eq!(block.ops[1].x86_hint, None, "{case:?}");
    let OpKind::X86DotProduct {
        dst,
        src1,
        src2,
        elem,
        width,
        imm,
    } = block.ops[1].kind
    else {
        panic!("{case:?}: expected X86DotProduct consumer")
    };
    assert_eq!(dst, vector(case.destination, case.kind.width()), "{case:?}");
    assert_eq!(src1, vector(case.source1, case.kind.width()), "{case:?}");
    assert_eq!(src2, loaded, "{case:?}");
    assert_eq!(elem, case.kind.elem(), "{case:?}");
    assert_eq!(width, case.kind.width(), "{case:?}");
    assert_eq!(imm, case.immediate, "{case:?}");

    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexFpDotProductMemorySequence {
            consumed: 2,
            memory_size: case.kind.width().bytes(),
            destination: case.destination,
            source1: case.source1,
            width: case.kind.width(),
            elem: case.kind.elem(),
            immediate: case.immediate,
            w: case.w,
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_case(case: DotCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_eq!(
        function.blocks[0].ops[0].x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}: lifter must retain architectural unaligned-load provenance"
    );
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn lower(function: &SmirFunction, case: DotCase) -> (Vec<u8>, usize) {
    assert_exact_lift_and_sequence(function, case);
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
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_sse3, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512cd, "{case:?}");
    assert!(!requirements.needs_gfni, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX dot-product lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX floating-point dot product"),
        result.entry_offset,
    )
}

#[test]
fn all_2304_scanner_encoding_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    for kind in DotKind::ALL {
        for w in [false, true] {
            for destination in 0..8 {
                for source1 in 0..16 {
                    let case = DotCase {
                        kind,
                        w,
                        destination,
                        source1,
                        base: 2,
                        immediate: (destination << 4) | source1,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        assert_exact_lift_and_sequence(&function, case);
                        let (code, _) = lower(&function, case);
                        let expected = case.emitted_register_bytes();
                        assert!(
                            code.windows(expected.len())
                                .any(|window| window == expected),
                            "{level:?} {case:?}: missing {expected:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 768 * LEVELS.len());
}

#[test]
fn llvm_23_rip_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vdppd xmm1, xmm2, [rip + 0x44332211], 0x3c
        &[0xC4, 0xE3, 0x69, 0x41, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x3C],
        // vdpps ymm0, ymm1, fs:[rcx*4 + 0x44332211], 0xa5
        &[
            0x64, 0xC4, 0xE3, 0x75, 0x40, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ],
        // vdpps xmm9, xmm10, gs:[r12 + r13*8 + 0x20], 0x5a
        &[0x65, 0xC4, 0x03, 0x29, 0x40, 0x4C, 0xEC, 0x20, 0x5A],
        // vdppd xmm14, xmm10, addr32 [esi*2 + 0x44332211], 0x03
        &[
            0x67, 0xC4, 0x63, 0x29, 0x41, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0x03,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(sequence.immediate, bytes[bytes.len() - 1]);
            assert!(is_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                true
            ));

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer.set_preserve_vector_mem_helpers(true);
            lowerer.set_avx_ymm16_vector_state(true);
            let result = lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            assert!(result.relocations.is_empty());
            lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed IR"
    );
}

fn loaded_virtual(function: &SmirFunction) -> VReg {
    match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    }
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_every_graph_and_provenance_invariant() {
    let case = DotCase {
        kind: DotKind::Ps256,
        w: true,
        destination: 9,
        source1: 10,
        base: 11,
        immediate: 0xA5,
    };
    let base = lift_case(case);
    let loaded = loaded_virtual(&base);
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC));
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source destination", 4, 0x08),
        ("source first operand", 2, 0x08),
        ("source vector width", 2, 0x04),
        ("source immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        malformed.push((name, function));
    }

    for (name, mutate) in [
        ("wrong map", (1usize, 0x01u8)),
        ("wrong mandatory prefix", (2, 0x02)),
        ("wrong opcode", (3, 0x02)),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[mutate.0] ^= mutate.1;
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        malformed.push((name, function));
    }

    let mut register_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    register_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("register-source metadata", register_metadata));

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing unaligned load provenance", missing_load_hint));

    let mut unrelated_load_hint = base.clone();
    unrelated_load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x10,
    });
    malformed.push(("unrelated load provenance", unrelated_load_hint));

    let mut wrong_load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_load_width));

    let mut architectural_load = base.clone();
    if let OpKind::VLoad { dst, .. } = &mut architectural_load.blocks[0].ops[0].kind {
        *dst = vector(7, VecWidth::V256);
    }
    malformed.push(("architectural load destination", architectural_load));

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
        OpKind::VMov {
            dst: loaded,
            src: vector(4, VecWidth::V256),
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value has another definition", duplicate_definition));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut consumer_hint = base.clone();
    consumer_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented consumer hint", consumer_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_destination = base.clone();
    if let OpKind::X86DotProduct { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind {
        *dst = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer destination", wrong_destination));

    let mut wrong_source1 = base.clone();
    if let OpKind::X86DotProduct { src1, .. } = &mut wrong_source1.blocks[0].ops[1].kind {
        *src1 = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer first source", wrong_source1));

    let mut wrong_source2 = base.clone();
    if let OpKind::X86DotProduct { src2, .. } = &mut wrong_source2.blocks[0].ops[1].kind {
        *src2 = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer second source", wrong_source2));

    let mut wrong_element = base.clone();
    if let OpKind::X86DotProduct { elem, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("consumer element", wrong_element));

    let mut wrong_width = base.clone();
    if let OpKind::X86DotProduct { width, .. } = &mut wrong_width.blocks[0].ops[1].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("consumer width", wrong_width));

    let mut wrong_immediate = base.clone();
    if let OpKind::X86DotProduct { imm, .. } = &mut wrong_immediate.blocks[0].ops[1].kind {
        *imm ^= 1;
    }
    malformed.push(("consumer immediate", wrong_immediate));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7FFC), PC, OpKind::Nop));
    assert_eq!(classified_at(&same_pc_head, 1, true), None);
    assert_rejected("unconsumed same-PC head", &same_pc_head);
}

#[cfg(target_arch = "x86_64")]
fn set_f32_lane(vector: &mut [u64; 8], lane: usize, bits: u32) {
    let word = lane / 2;
    let shift = (lane % 2) * 32;
    vector[word] = (vector[word] & !(u64::from(u32::MAX) << shift)) | (u64::from(bits) << shift);
}

#[cfg(target_arch = "x86_64")]
fn f32_lane(vector: &[u64; 8], lane: usize) -> u32 {
    (vector[lane / 2] >> ((lane % 2) * 32)) as u32
}

#[cfg(target_arch = "x86_64")]
fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn exact_operands(case: DotCase, ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0; 8];
    let mut source2 = [0; 8];
    match case.kind {
        DotKind::Ps128 | DotKind::Ps256 => {
            for lane in 0..8 {
                set_f32_lane(
                    &mut source1,
                    lane,
                    ((1 + ((lane * 3 + ordinal) % 7) as u32) as f32).to_bits(),
                );
                set_f32_lane(
                    &mut source2,
                    lane,
                    ((1 + ((lane * 5 + ordinal * 2) % 7) as u32) as f32).to_bits(),
                );
            }
        }
        DotKind::Pd128 => {
            for lane in 0..2 {
                source1[lane] = ((1 + ((lane * 3 + ordinal) % 7) as u64) as f64).to_bits();
                source2[lane] = ((1 + ((lane * 5 + ordinal * 2) % 7) as u64) as f64).to_bits();
            }
        }
    }
    (source1, source2)
}

#[cfg(target_arch = "x86_64")]
fn architectural_destination(case: DotCase, source1: [u64; 8], source2: [u64; 8]) -> [u64; 8] {
    let mut destination = [0; 8];
    let lanes = case.kind.lanes_per_group();
    for group in 0..case.kind.groups() {
        let mut total = 0u32;
        for lane in 0..lanes {
            if case.immediate & (1 << (lane + 4)) == 0 {
                continue;
            }
            let index = group * lanes + lane;
            let product = match case.kind {
                DotKind::Ps128 | DotKind::Ps256 => {
                    (f32::from_bits(f32_lane(&source1, index)) as u32)
                        * (f32::from_bits(f32_lane(&source2, index)) as u32)
                }
                DotKind::Pd128 => {
                    (f64::from_bits(source1[index]) as u32)
                        * (f64::from_bits(source2[index]) as u32)
                }
            };
            total += product;
        }
        for lane in 0..lanes {
            if case.immediate & (1 << lane) == 0 {
                continue;
            }
            let index = group * lanes + lane;
            match case.kind {
                DotKind::Ps128 | DotKind::Ps256 => {
                    set_f32_lane(&mut destination, index, (total as f32).to_bits());
                }
                DotKind::Pd128 => destination[index] = (total as f64).to_bits(),
            }
        }
    }
    destination
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
        || !matches!(size, 16 | 32)
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
fn full_guest_regs(case: DotCase, ordinal: usize, source1: [u64; 8]) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80
            | ((ordinal as u32) & 0x3F)
            | (((ordinal as u32) & 3) << 13)
            | (u32::from(ordinal & 4 != 0) << 6)
            | (u32::from(ordinal & 8 != 0) << 15),
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
    registers.zmm[usize::from(case.source1)] = source1;
    if case.destination != case.source1 {
        registers.zmm[usize::from(case.destination)] =
            std::array::from_fn(|word| 0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7) as u32));
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreted_expected(
    function: &SmirFunction,
    initial: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: DotCase,
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
    let bytes = words_to_bytes(source2);
    memory.load(
        address as usize,
        &bytes[..case.kind.width().bytes() as usize],
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
    for (index, value) in expected.zmm.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    let words = (case.kind.width().bytes() / 8) as usize;
    expected.vector_scratch =
        std::array::from_fn(|word| if word < words { source2[word] } else { 0 });
    expected
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<DotCase> {
    const OPERANDS: [(u8, u8, u8); 8] = [
        (0, 1, 3),
        (1, 1, 11),
        (15, 0, 4),
        (9, 9, 5),
        (0, 15, 12),
        (15, 15, 13),
        (8, 7, 4),
        (7, 8, 5),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in DotKind::ALL {
        for w in [false, true] {
            for immediate in u8::MIN..=u8::MAX {
                let (destination, source1, base) = OPERANDS[ordinal % OPERANDS.len()];
                cases.push(DotCase {
                    kind,
                    w,
                    destination,
                    source1,
                    base,
                    immediate,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: DotCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(
        context.last_size,
        case.kind.width().bytes(),
        "{label} {case:?}"
    );
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_dot_products_match_intel_interpreter_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory dot-product differential: host lacks AVX");
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 1_536);
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    eprintln!("executing {expected_executions} native VEX memory dot-product success/fault pairs");
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let (source1, source2) = exact_operands(case, ordinal);
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, source1);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = interpreted_expected(&function, &initial, source2, address, case);
            assert_eq!(
                expected.zmm[usize::from(case.destination)],
                architectural_destination(case, source1, source2),
                "{level:?} {case:?}: interpreter versus Intel equations"
            );

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_observation(&context, address, case, "success");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, source1);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_helper_observation(&context, address, case, "fault");
            faults += 1;
        }
    }

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct MxcsrEdge {
    immediate: u8,
    source1: [u32; 4],
    source2: [u32; 4],
    mxcsr: u32,
    expected_status: u32,
    expected_lane0: Option<u32>,
}

#[cfg(target_arch = "x86_64")]
fn mxcsr_edges() -> Vec<MxcsrEdge> {
    let snan = 0x7F80_0001;
    let one = 1.0f32.to_bits();
    let zero = 0.0f32.to_bits();
    let infinity = f32::INFINITY.to_bits();
    let mut edges = vec![
        MxcsrEdge {
            immediate: 0,
            source1: [snan; 4],
            source2: [snan; 4],
            mxcsr: 0x1F80,
            expected_status: 0,
            expected_lane0: None,
        },
        MxcsrEdge {
            immediate: 0x10,
            source1: [zero, snan, snan, snan],
            source2: [infinity, snan, snan, snan],
            mxcsr: 0x1F80,
            expected_status: 1,
            expected_lane0: None,
        },
        MxcsrEdge {
            immediate: 0x10,
            source1: [1, snan, snan, snan],
            source2: [one, snan, snan, snan],
            mxcsr: 0x1F80,
            expected_status: 1 << 1,
            expected_lane0: None,
        },
        MxcsrEdge {
            immediate: 0x11,
            source1: [1, snan, snan, snan],
            source2: [one, snan, snan, snan],
            mxcsr: 0x1F80 | (1 << 6),
            expected_status: 0,
            expected_lane0: Some(zero),
        },
        MxcsrEdge {
            immediate: 0x10,
            source1: [f32::MAX.to_bits(), snan, snan, snan],
            source2: [2.0f32.to_bits(), snan, snan, snan],
            mxcsr: 0x1F80,
            expected_status: (1 << 3) | (1 << 5),
            expected_lane0: None,
        },
        MxcsrEdge {
            immediate: 0x10,
            source1: [f32::MIN_POSITIVE.to_bits(), snan, snan, snan],
            source2: [0.1f32.to_bits(), snan, snan, snan],
            mxcsr: 0x1F80,
            expected_status: (1 << 1) | (1 << 4) | (1 << 5),
            expected_lane0: None,
        },
        MxcsrEdge {
            immediate: 0x11,
            source1: [f32::MIN_POSITIVE.to_bits(), snan, snan, snan],
            source2: [0.1f32.to_bits(), snan, snan, snan],
            mxcsr: 0x1F80 | (1 << 15),
            expected_status: (1 << 4) | (1 << 5),
            expected_lane0: Some(zero),
        },
    ];
    for rc in 0u32..4 {
        edges.push(MxcsrEdge {
            immediate: 0x31,
            source1: [one, 0x3380_0000, snan, snan],
            source2: [one, one, snan, snan],
            mxcsr: 0x1F80 | (rc << 13),
            expected_status: 1 << 5,
            expected_lane0: Some(if rc == 2 { 0x3F80_0001 } else { 0x3F80_0000 }),
        });
    }
    edges
}

#[cfg(target_arch = "x86_64")]
fn f32_words(lanes: [u32; 4]) -> [u64; 8] {
    let mut words = [0; 8];
    for (lane, bits) in lanes.into_iter().enumerate() {
        set_f32_lane(&mut words, lane, bits);
    }
    words
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_dot_product_preserves_stage_order_rounding_daz_ftz_and_mxcsr_status() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory dot-product MXCSR edges: host lacks AVX");
        return;
    }

    for (ordinal, edge) in mxcsr_edges().into_iter().enumerate() {
        let case = DotCase {
            kind: DotKind::Ps128,
            w: ordinal & 1 != 0,
            destination: 1,
            source1: 2,
            base: if ordinal & 2 == 0 { 4 } else { 5 },
            immediate: edge.immediate,
        };
        let source1 = f32_words(edge.source1);
        let source2 = f32_words(edge.source2);
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let mut context = VectorMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, source1);
            registers.mxcsr = edge.mxcsr;
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = interpreted_expected(&function, &initial, source2, address, case);
            assert_eq!(
                expected.mxcsr & 0x3F,
                edge.expected_status,
                "interpreter {level:?} {case:?}"
            );
            if let Some(expected_lane0) = edge.expected_lane0 {
                assert_eq!(
                    f32_lane(&expected.zmm[usize::from(case.destination)], 0),
                    expected_lane0,
                    "interpreter {level:?} {case:?}"
                );
            } else {
                assert_eq!(
                    expected.zmm[usize::from(case.destination)],
                    [0; 8],
                    "interpreter {level:?} {case:?}"
                );
            }

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected,
                "native versus interpreter {level:?} {case:?}"
            );
            assert_helper_observation(&context, address, case, "MXCSR");
        }
    }
}
