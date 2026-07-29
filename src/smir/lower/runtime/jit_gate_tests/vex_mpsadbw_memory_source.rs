//! Exact helper-backed VEX `VMPSADBW` memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexMpsadbwMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_mpsadbw_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0x42AD;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MpsadMemoryCase {
    width: VecWidth,
    w: bool,
    destination: u8,
    source1: u8,
    base: u8,
    immediate: u8,
}

impl MpsadMemoryCase {
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
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            0x42,
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
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            0x42,
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
        _ => unreachable!("VEX VMPSADBW has only 128-/256-bit forms"),
    })
}

fn expected_address(case: MpsadMemoryCase) -> Address {
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

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexMpsadbwMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_mpsadbw_memory_sequence(
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
) -> Option<X86JitVexMpsadbwMemorySequence> {
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

fn assert_exact_pair(function: &SmirFunction, case: MpsadMemoryCase) {
    let [load, consumer] = function.blocks[0].ops.as_slice() else {
        panic!("{case:?}: expected exact VLoad + VMpsadbw pair")
    };
    assert!(
        matches!(
            load.x86_hint,
            Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        ),
        "{case:?}: {:?}",
        load.x86_hint
    );
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
    assert_eq!(consumer.x86_hint, None, "{case:?}");
    let OpKind::VMpsadbw {
        dst,
        src1,
        src2,
        mask,
        width,
        imm,
        zeroing,
    } = consumer.kind
    else {
        panic!("{case:?}: expected VMpsadbw consumer")
    };
    assert_eq!(dst, vector(case.destination, case.width), "{case:?}");
    assert_eq!(src1, vector(case.source1, case.width), "{case:?}");
    assert_eq!(src2, temporary, "{case:?}");
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(width, case.width, "{case:?}");
    assert_eq!(imm, case.immediate, "{case:?}");
    assert!(!zeroing, "{case:?}");
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexMpsadbwMemorySequence {
            consumed: 2,
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

fn lift_case(case: MpsadMemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_eq!(
        function.blocks[0].ops[0].x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}: lifter must retain architectural unaligned-load provenance"
    );
    assert_exact_pair(&function, case);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexMpsadbwMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VMPSADBW memory pair");
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
    assert!(!requirements.needs_sse3);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_fma4);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_gfni);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX VMPSADBW lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VMPSADBW"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_1536_scanner_encoding_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    for width in [VecWidth::V128, VecWidth::V256] {
        for w in [false, true] {
            for destination in 0..8 {
                for source1 in 0..16 {
                    let case = MpsadMemoryCase {
                        width,
                        w,
                        destination,
                        source1,
                        base: 2,
                        immediate: destination.wrapping_mul(17) ^ source1.wrapping_mul(29),
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        assert_exact_pair(&function, case);
                        let (code, _, sequence) = lower(&function);
                        assert_eq!(sequence.width, width, "{level:?} {case:?}");
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
    assert_eq!(lowered, 512 * LEVELS.len());
}

#[test]
fn llvm_23_rip_segment_sib_disp32_addr32_and_wig_aliases_admit_at_every_level() {
    let encodings: &[&[u8]] = &[
        // vmpsadbw ymm9,ymm10,[r11+0x20],0xa5
        &[0xC4, 0x43, 0x2D, 0x42, 0x4B, 0x20, 0xA5],
        // The same encoding with architecturally ignored VEX.W=1.
        &[0xC4, 0x43, 0xAD, 0x42, 0x4B, 0x20, 0xA5],
        // vmpsadbw xmm1,xmm2,[rip+0x44332211],0x3c
        &[0xC4, 0xE3, 0x69, 0x42, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x3C],
        // vmpsadbw ymm0,ymm1,fs:[rcx*4+0x44332211],0x5a
        &[
            0x64, 0xC4, 0xE3, 0x75, 0x42, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x5A,
        ],
        // vmpsadbw xmm9,xmm10,gs:[r12+r13*8+0x20],0x03
        &[0x65, 0xC4, 0x03, 0x29, 0x42, 0x4C, 0xEC, 0x20, 0x03],
        // vmpsadbw ymm14,ymm10,fs:addr32 [esi*2+0x44332211],0xff, W1
        &[
            0x64, 0x67, 0xC4, 0x03, 0xAD, 0x42, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0xFF,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(sequence.immediate, bytes[bytes.len() - 1]);
            let (code, _, _) = lower(&function);
            let expected = MpsadMemoryCase {
                width: sequence.width,
                w: sequence.w,
                destination: sequence.destination,
                source1: sequence.source1,
                base: 0,
                immediate: sequence.immediate,
            }
            .emitted_register_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {bytes:02X?}: missing {expected:02X?}"
            );
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

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_every_graph_and_provenance_invariant() {
    let case = MpsadMemoryCase {
        width: VecWidth::V256,
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
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source map", 1usize, 0x01u8),
        ("source mandatory prefix", 2, 0x03),
        ("source opcode", 3, 0x01),
        ("source destination", 4, 0x08),
        ("source first operand", 2, 0x08),
        ("source vector width", 2, 0x04),
        ("source immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut register_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    bytes.remove(5);
    replace_instruction_bytes(&mut register_metadata, &bytes);
    malformed.push(("register-source metadata", register_metadata));

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing load provenance", missing_load_hint));

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
    consumer_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x42,
        width: VecWidth::V256,
        w: true,
    });
    malformed.push(("invented consumer hint", consumer_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_destination = base.clone();
    if let OpKind::VMpsadbw { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind {
        *dst = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer destination", wrong_destination));

    let mut wrong_source1 = base.clone();
    if let OpKind::VMpsadbw { src1, .. } = &mut wrong_source1.blocks[0].ops[1].kind {
        *src1 = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer first source", wrong_source1));

    let mut wrong_source2 = base.clone();
    if let OpKind::VMpsadbw { src2, .. } = &mut wrong_source2.blocks[0].ops[1].kind {
        *src2 = vector(8, VecWidth::V256);
    }
    malformed.push(("consumer second source", wrong_source2));

    let mut wrong_width = base.clone();
    if let OpKind::VMpsadbw { width, .. } = &mut wrong_width.blocks[0].ops[1].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("consumer width", wrong_width));

    let mut wrong_immediate = base.clone();
    if let OpKind::VMpsadbw { imm, .. } = &mut wrong_immediate.blocks[0].ops[1].kind {
        *imm ^= 1;
    }
    malformed.push(("consumer immediate", wrong_immediate));

    let mut masked = base.clone();
    if let OpKind::VMpsadbw { mask, .. } = &mut masked.blocks[0].ops[1].kind {
        *mask = Some(x86(X86Reg::K(1)));
    }
    malformed.push(("consumer opmask", masked));

    let mut zeroing = base.clone();
    if let OpKind::VMpsadbw { zeroing, .. } = &mut zeroing.blocks[0].ops[1].kind {
        *zeroing = true;
    }
    malformed.push(("consumer zeroing", zeroing));

    let mut high_destination = base.clone();
    if let OpKind::VMpsadbw { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V256);
    }
    malformed.push(("EVEX-only destination", high_destination));

    let mut wrong_namespace = base.clone();
    if let OpKind::VMpsadbw { dst, .. } = &mut wrong_namespace.blocks[0].ops[1].kind {
        *dst = x86(X86Reg::Xmm(case.destination));
    }
    malformed.push(("destination namespace", wrong_namespace));

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

#[test]
fn alignment_inference_hint_is_accepted_without_weakening_source_provenance() {
    let case = MpsadMemoryCase {
        width: VecWidth::V128,
        w: false,
        destination: 1,
        source1: 2,
        base: 3,
        immediate: 7,
    };
    let mut function = lift_case(case);
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    assert_exact_pair(&function, case);
    lower(&function);
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn operand_vectors(ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0u8; 64];
    let mut source2 = [0u8; 64];
    for index in 0..64 {
        source1[index] = (index as u8)
            .wrapping_mul(0x25)
            .wrapping_add((ordinal as u8).wrapping_mul(0x11))
            ^ if index & 3 == 0 { 0xFF } else { 0 };
        source2[index] = (index as u8)
            .wrapping_mul(0x39)
            .wrapping_add((ordinal as u8).wrapping_mul(0x07))
            ^ if index & 5 == 0 { 0x80 } else { 0x13 };
    }
    (bytes_to_words(source1), bytes_to_words(source2))
}

/// Independent transcription of Intel SDM Vol. 2 VMPSADBW pseudocode.
fn architectural_destination(
    width: VecWidth,
    immediate: u8,
    source1: [u64; 8],
    source2: [u64; 8],
) -> [u64; 8] {
    let source1 = words_to_bytes(source1);
    let source2 = words_to_bytes(source2);
    let mut destination = [0u8; 64];
    let blocks = width.bytes() as usize / 16;
    for block in 0..blocks {
        let control = (immediate >> ((block & 1) * 3)) & 7;
        let source1_base = block * 16 + usize::from((control >> 2) & 1) * 4;
        let source2_base = block * 16 + usize::from(control & 3) * 4;
        for result in 0..8 {
            let sum = (0..4).fold(0u16, |sum, byte| {
                sum + u16::from(
                    source1[source1_base + result + byte].abs_diff(source2[source2_base + byte]),
                )
            });
            let output = block * 16 + result * 2;
            destination[output..output + 2].copy_from_slice(&sum.to_le_bytes());
        }
    }
    bytes_to_words(destination)
}

#[test]
fn interpreter_matches_intel_equations_for_all_1024_width_immediate_and_opt_cells() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut checked = 0usize;
    for width in [VecWidth::V128, VecWidth::V256] {
        for immediate in u8::MIN..=u8::MAX {
            let case = MpsadMemoryCase {
                width,
                w: immediate & 1 != 0,
                destination: 1,
                source1: 2,
                base: 3,
                immediate,
            };
            let (source1, source2) = operand_vectors(usize::from(immediate));
            let expected = architectural_destination(width, immediate, source1, source2);
            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let mut context = SmirContext::new_x86_64();
                if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                    x86.gpr[usize::from(case.base)] = 0x2000;
                    x86.xmm[usize::from(case.source1)][..8].copy_from_slice(&source1);
                    x86.xmm[usize::from(case.destination)] = [0xDEAD_BEEF_CAFE_BABE; 16];
                }
                let address = 0x2000usize + DISP as usize;
                let mut memory = FlatMemory::new(0x10000);
                let source2_bytes = words_to_bytes(source2);
                memory.load(address, &source2_bytes[..width.bytes() as usize]);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    &function.blocks[0],
                );
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                assert_eq!(
                    &x86.xmm[usize::from(case.destination)][..8],
                    &expected,
                    "{level:?} {case:?}"
                );
                assert_eq!(
                    &x86.xmm[usize::from(case.destination)][8..],
                    &[0; 8],
                    "{level:?} {case:?}: upper architectural vector state"
                );
                assert_eq!(
                    &x86.xmm[usize::from(case.source1)][..8],
                    &source1,
                    "{level:?} {case:?}: source1"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2 * 256 * DIFFERENTIAL_LEVELS.len());
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
fn full_guest_regs(case: MpsadMemoryCase, ordinal: usize, source1: [u64; 8]) -> GuestRegs {
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
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
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
fn expected_success(
    mut registers: GuestRegs,
    case: MpsadMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1)];
    registers.zmm[usize::from(case.destination)] =
        architectural_destination(case.width, case.immediate, source1, source2);
    let words = (case.width.bytes() / 8) as usize;
    registers.vector_scratch =
        std::array::from_fn(|word| if word < words { source2[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: MpsadMemoryCase,
    level: OptLevel,
) {
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
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let bytes = words_to_bytes(source2);
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, value) in expected.zmm.iter().enumerate() {
        assert_eq!(
            &x86.xmm[index][..8],
            value,
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(x86.k, expected.k, "{level:?} {case:?}: masks");
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<MpsadMemoryCase> {
    let immediates = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x07, 0x08, 0x18, 0x20, 0x3F, 0x40, 0x80, 0xFF,
    ];
    let operands = [(0, 1, 3), (15, 0, 11), (9, 9, 11)];
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256] {
        for w in [false, true] {
            for (ordinal, immediate) in immediates.into_iter().enumerate() {
                let (destination, source1, base) = operands[ordinal % operands.len()];
                cases.push(MpsadMemoryCase {
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
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_vmpsadbw_matches_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX VMPSADBW memory differential: host lacks AVX");
        return;
    }

    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases = semantic_cases()
        .into_iter()
        .filter(|case| avx2 || case.width == VecWidth::V128)
        .collect::<Vec<_>>();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let (source1, source2) = operand_vectors(ordinal);

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
            let mut expected = expected_success(registers, case, source2);

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
            assert_interpreter_matches(
                &function, &initial, &expected, source2, address, case, level,
            );
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

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX VMPSADBW memory cases"
    );
}
