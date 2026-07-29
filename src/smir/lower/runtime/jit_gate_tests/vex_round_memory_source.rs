//! Exact helper-backed VEX floating-point round memory-source coverage.

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
    Address, ArchReg, BlockId, FpRoundMode, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexRoundMemoryEncoding,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexRoundMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_round_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0xA11C_E000;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoundKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl RoundKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::PackedF32 => 0x08,
            Self::PackedF64 => 0x09,
            Self::ScalarF32 => 0x0A,
            Self::ScalarF64 => 0x0B,
        }
    }

    const fn scalar(self) -> bool {
        matches!(self, Self::ScalarF32 | Self::ScalarF64)
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::PackedF32 | Self::ScalarF32 => VecElementType::F32,
            Self::PackedF64 | Self::ScalarF64 => VecElementType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoundCase {
    kind: RoundKind,
    w: bool,
    l: bool,
    destination: u8,
    merge: u8,
    base: u8,
    immediate: u8,
}

impl RoundCase {
    fn width(self) -> VecWidth {
        if self.kind.scalar() || !self.l {
            VecWidth::V128
        } else {
            VecWidth::V256
        }
    }

    fn memory_size(self) -> u32 {
        if self.kind.scalar() {
            match self.kind.elem() {
                VecElementType::F32 => 4,
                VecElementType::F64 => 8,
                _ => unreachable!(),
            }
        } else {
            self.width().bytes()
        }
    }

    fn architectural_merge(self) -> Option<u8> {
        self.kind.scalar().then_some(self.merge)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| {
                *candidate != self.destination && self.architectural_merge() != Some(*candidate)
            })
            .expect("two VEX round operands leave at least fourteen scratch registers")
    }

    fn mode(self) -> FpRoundMode {
        if self.immediate & 4 != 0 {
            FpRoundMode::Dynamic
        } else {
            match self.immediate & 3 {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.merge < 16 && self.base < 16);
        let encoded_vvvv = if self.kind.scalar() {
            ((!self.merge) & 15) << 3
        } else {
            0x78
        };
        let mut bytes = vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if self.base < 8 { 0x20 } else { 0 })
                | 3,
            (u8::from(self.w) << 7) | encoded_vvvv | (u8::from(self.l) << 2) | 1,
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
                | if self.kind.scalar() {
                    ((!self.merge) & 15) << 3
                } else {
                    0x78
                }
                | (u8::from(self.l) << 2)
                | 1,
            self.kind.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (scratch & 7),
            self.immediate,
        ]
    }

    fn expected_encoding(self) -> X86VexRoundMemoryEncoding {
        X86VexRoundMemoryEncoding {
            width: self.width(),
            elem: self.kind.elem(),
            destination: self.destination,
            merge: self.architectural_merge(),
            scratch: self.scratch(),
            immediate: self.immediate,
            memory_size: self.memory_size(),
            register_instruction: X86InstructionBytes::new(&self.emitted_register_bytes()).unwrap(),
        }
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX round has only 128-/256-bit vector registers"),
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
) -> Option<X86JitVexRoundMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_round_memory_sequence(
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
) -> Option<X86JitVexRoundMemorySequence> {
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

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: RoundCase) {
    let block = &function.blocks[0];
    assert_eq!(block.ops.len(), 2, "{case:?}");
    let loaded = if case.kind.scalar() {
        match &block.ops[0].kind {
            OpKind::Load {
                dst: loaded @ VReg::Virtual(_),
                width,
                sign: SignExtend::Zero,
                ..
            } => {
                assert_eq!(
                    *width,
                    if case.kind.elem() == VecElementType::F32 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    },
                    "{case:?}"
                );
                assert_eq!(block.ops[0].x86_hint, None, "{case:?}");
                *loaded
            }
            other => panic!("{case:?}: expected leading scalar Load, got {other:?}"),
        }
    } else {
        match &block.ops[0].kind {
            OpKind::VLoad {
                dst: loaded @ VReg::Virtual(_),
                width,
                ..
            } => {
                assert_eq!(*width, case.width(), "{case:?}");
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
                *loaded
            }
            other => panic!("{case:?}: expected leading VLoad, got {other:?}"),
        }
    };

    assert_eq!(block.ops[1].guest_pc, PC, "{case:?}");
    assert_eq!(block.ops[1].x86_hint, None, "{case:?}");
    let OpKind::X86Round {
        dst,
        merge,
        src,
        elem,
        width,
        lanes,
        scalar_source,
        zero_upper,
        mode,
        suppress_precision,
    } = block.ops[1].kind
    else {
        panic!("{case:?}: expected X86Round consumer")
    };
    assert_eq!(dst, vector(case.destination, case.width()), "{case:?}");
    assert_eq!(
        merge,
        if case.kind.scalar() {
            vector(case.merge, VecWidth::V128)
        } else {
            vector(case.destination, case.width())
        },
        "{case:?}"
    );
    assert_eq!(src, loaded, "{case:?}");
    assert_eq!(elem, case.kind.elem(), "{case:?}");
    assert_eq!(width, case.width(), "{case:?}");
    assert_eq!(
        lanes,
        if case.kind.scalar() {
            1
        } else {
            case.width().lanes(case.kind.elem()) as u8
        },
        "{case:?}"
    );
    assert_eq!(scalar_source, case.kind.scalar(), "{case:?}");
    assert!(zero_upper, "{case:?}");
    assert_eq!(mode, case.mode(), "{case:?}");
    assert_eq!(suppress_precision, case.immediate & 8 != 0, "{case:?}");

    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexRoundMemorySequence {
            consumed: 2,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_case(case: RoundCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn lower(function: &SmirFunction, case: RoundCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("helper-backed VEX round lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX floating-point round"),
        result.entry_offset,
    )
}

#[test]
fn all_3264_scanner_encoding_and_optimization_cells_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    for kind in RoundKind::ALL {
        for w in [false, true] {
            for l in [false, true] {
                for destination in 0..8 {
                    let merges: &[u8] = if kind.scalar() {
                        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
                    } else {
                        &[0]
                    };
                    for &merge in merges {
                        let case = RoundCase {
                            kind,
                            w,
                            l,
                            destination,
                            merge,
                            base: 2,
                            immediate: 0,
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
    }
    assert_eq!(lowered, 1_088 * LEVELS.len());
}

#[test]
fn rip_segment_sib_disp32_and_addr32_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vroundps xmm1, fs:[rip + 0x44332211], 0xa5
        &[
            0x64, 0xC4, 0xE3, 0x79, 0x08, 0x0D, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ],
        // vroundpd ymm9, gs:[r12 + r13*8 + 0x20], 0x5a
        &[0x65, 0xC4, 0x03, 0xFD, 0x09, 0x4C, 0xEC, 0x20, 0x5A],
        // vroundss xmm14, xmm10, addr32 [esi*2 + 0x44332211], 0x03
        &[
            0x67, 0xC4, 0x63, 0x29, 0x0A, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0x03,
        ],
        // vroundsd xmm0, xmm15, [r13], 0xfc
        &[0xC4, 0xC3, 0x81, 0x0B, 0x45, 0x00, 0xFC],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
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
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            assert!(
                code.windows(6)
                    .any(|window| { window == sequence.encoding.register_instruction.as_slice() })
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
        OpKind::Load { dst, .. } | OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    }
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_every_graph_and_provenance_invariant() {
    let case = RoundCase {
        kind: RoundKind::ScalarF32,
        w: true,
        l: true,
        destination: 9,
        merge: 10,
        base: 11,
        immediate: 0xFD,
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
        ("source merge operand", 2, 0x08),
        ("source immediate mode", case.bytes().len() - 1, 0x04),
        ("source element opcode", 3, 0x01),
        ("source mandatory prefix", 2, 0x02),
        ("source map", 1, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
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

    let mut trailing_metadata = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    trailing_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("trailing source byte", trailing_metadata));

    let mut invented_load_hint = base.clone();
    invented_load_hint.blocks[0].ops[0].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented scalar load hint", invented_load_hint));

    let mut wrong_load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("scalar load width", wrong_load_width));

    let mut signed_load = base.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("scalar load extension", signed_load));

    let mut architectural_load = base.clone();
    if let OpKind::Load { dst, .. } = &mut architectural_load.blocks[0].ops[0].kind {
        *dst = x86(X86Reg::Rax);
    }
    malformed.push(("architectural load destination", architectural_load));

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFF),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value escapes sequence", external_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFE),
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
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
    if let OpKind::X86Round { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind {
        *dst = vector(8, VecWidth::V128);
    }
    malformed.push(("consumer destination", wrong_destination));

    let mut wrong_merge = base.clone();
    if let OpKind::X86Round { merge, .. } = &mut wrong_merge.blocks[0].ops[1].kind {
        *merge = vector(8, VecWidth::V128);
    }
    malformed.push(("consumer merge", wrong_merge));

    let mut wrong_source = base.clone();
    if let OpKind::X86Round { src, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src = x86(X86Reg::Rax);
    }
    malformed.push(("consumer source", wrong_source));

    let mut wrong_element = base.clone();
    if let OpKind::X86Round { elem, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("consumer element", wrong_element));

    let mut wrong_width = base.clone();
    if let OpKind::X86Round { width, .. } = &mut wrong_width.blocks[0].ops[1].kind {
        *width = VecWidth::V256;
    }
    malformed.push(("consumer width", wrong_width));

    let mut wrong_lanes = base.clone();
    if let OpKind::X86Round { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(("consumer lanes", wrong_lanes));

    let mut wrong_scalar_source = base.clone();
    if let OpKind::X86Round { scalar_source, .. } = &mut wrong_scalar_source.blocks[0].ops[1].kind {
        *scalar_source = false;
    }
    malformed.push(("consumer scalar marker", wrong_scalar_source));

    let mut wrong_zero_upper = base.clone();
    if let OpKind::X86Round { zero_upper, .. } = &mut wrong_zero_upper.blocks[0].ops[1].kind {
        *zero_upper = false;
    }
    malformed.push(("consumer upper-zero marker", wrong_zero_upper));

    let mut wrong_mode = base.clone();
    if let OpKind::X86Round { mode, .. } = &mut wrong_mode.blocks[0].ops[1].kind {
        *mode = FpRoundMode::RoundNearest;
    }
    malformed.push(("consumer round mode", wrong_mode));

    let mut wrong_suppression = base.clone();
    if let OpKind::X86Round {
        suppress_precision, ..
    } = &mut wrong_suppression.blocks[0].ops[1].kind
    {
        *suppress_precision = false;
    }
    malformed.push(("consumer precision suppression", wrong_suppression));

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
fn packed_load_contract_and_optimizer_alignment_refinement_are_exact() {
    let case = RoundCase {
        kind: RoundKind::PackedF64,
        w: true,
        l: true,
        destination: 15,
        merge: 0,
        base: 12,
        immediate: 0xA5,
    };
    let base = lift_case(case);

    let mut aligned = base.clone();
    aligned.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    assert_exact_lift_and_sequence(&aligned, case);
    lower(&aligned, case);

    let mut malformed = Vec::new();
    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing packed load alignment", missing_hint));

    let mut unrelated_hint = base.clone();
    unrelated_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x10,
    });
    malformed.push(("unrelated packed load hint", unrelated_hint));

    let mut wrong_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("packed load width", wrong_width));

    let mut wrong_merge = base.clone();
    if let OpKind::X86Round { merge, .. } = &mut wrong_merge.blocks[0].ops[1].kind {
        *merge = vector(14, VecWidth::V256);
    }
    malformed.push(("packed self-merge", wrong_merge));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
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
fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn f32_source_words(seed: usize) -> [u64; 8] {
    let values = [
        0x3FC0_0000u32, // +1.5
        0x4020_0000,    // +2.5
        0xBFC0_0000,    // -1.5
        0xC020_0000,    // -2.5
        0x7FC0_0001,    // QNaN
        0x7F80_0001,    // SNaN
        0x0000_0001,    // minimum positive subnormal
        0x8000_0000,    // -0
        0x7F80_0000,    // +infinity
        0xFF80_0000,    // -infinity
        0x3F00_0000,    // +0.5
        0xBF00_0000,    // -0.5
        0x4B00_0001,    // 2^23 + 1, already integral
        0xCB00_0001,    // -(2^23 + 1), already integral
        0x0000_0000,    // +0
        0x3F80_0000,    // +1
    ];
    let mut words = [0u64; 8];
    for lane in 0..16 {
        set_f32_lane(&mut words, lane, values[(lane + seed) % values.len()]);
    }
    words
}

#[cfg(target_arch = "x86_64")]
fn f64_source_words(seed: usize) -> [u64; 8] {
    let values = [
        0x3FF8_0000_0000_0000u64, // +1.5
        0x4004_0000_0000_0000,    // +2.5
        0xBFF8_0000_0000_0000,    // -1.5
        0xC004_0000_0000_0000,    // -2.5
        0x7FF8_0000_0000_0001,    // QNaN
        0x7FF0_0000_0000_0001,    // SNaN
        0x0000_0000_0000_0001,    // minimum positive subnormal
        0x8000_0000_0000_0000,    // -0
        0x7FF0_0000_0000_0000,    // +infinity
        0xFFF0_0000_0000_0000,    // -infinity
        0x3FE0_0000_0000_0000,    // +0.5
        0xBFE0_0000_0000_0000,    // -0.5
        0x4340_0000_0000_0001,    // 2^53 + 2, already integral
        0xC340_0000_0000_0001,    // -(2^53 + 2), already integral
        0x0000_0000_0000_0000,    // +0
        0x3FF0_0000_0000_0000,    // +1
    ];
    std::array::from_fn(|lane| values[(lane + seed) % values.len()])
}

#[cfg(target_arch = "x86_64")]
fn source_words(case: RoundCase, seed: usize) -> [u64; 8] {
    match case.kind.elem() {
        VecElementType::F32 => f32_source_words(seed),
        VecElementType::F64 => f64_source_words(seed),
        _ => unreachable!(),
    }
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
        || !matches!(size, 4 | 8 | 16 | 32)
    {
        return 0;
    }
    let mut destination_bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    let source_bytes = words_to_bytes(context.value);
    destination_bytes[..size as usize].copy_from_slice(&source_bytes[..size as usize]);
    state.vector_scratch = bytes_to_words(destination_bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: RoundCase, seed: usize, mxcsr: u32) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr,
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
    registers.gpr[usize::from(case.base)] = 0x2000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreted_expected(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: RoundCase,
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
    let source_bytes = words_to_bytes(source);
    memory.load(
        address as usize,
        &source_bytes[..case.memory_size() as usize],
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
    let mut scratch_bytes = [0u8; 64];
    scratch_bytes[..case.memory_size() as usize]
        .copy_from_slice(&source_bytes[..case.memory_size() as usize]);
    expected.vector_scratch = bytes_to_words(scratch_bytes);
    expected
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: RoundCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, case.memory_size(), "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: RoundCase,
    seed: usize,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let forms = [
        (RoundKind::PackedF32, false, false, 1, 0, 3),
        (RoundKind::PackedF32, true, true, 9, 0, 11),
        (RoundKind::PackedF64, true, false, 9, 0, 4),
        (RoundKind::PackedF64, false, true, 15, 0, 12),
        (RoundKind::ScalarF32, false, false, 1, 2, 3),
        (RoundKind::ScalarF32, true, true, 9, 10, 11),
        (RoundKind::ScalarF32, true, false, 9, 9, 4),
        (RoundKind::ScalarF32, false, true, 0, 1, 12),
        (RoundKind::ScalarF64, false, false, 12, 13, 14),
        (RoundKind::ScalarF64, true, true, 15, 15, 5),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in DIFFERENTIAL_LEVELS {
        for &(kind, w, l, destination, merge, base) in &forms {
            for control in 0u8..16 {
                let rc_values = if control & 4 != 0 {
                    &[0u32, 1, 2, 3][..]
                } else {
                    &[0u32][..]
                };
                for &rc in rc_values {
                    let high = [0x00, 0xA0, 0xF0][ordinal % 3];
                    let prior_status = 1 << (ordinal % 6);
                    let daz_ftz = if ordinal & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    cases.push(NativeCase {
                        level,
                        instruction: RoundCase {
                            kind,
                            w,
                            l,
                            destination,
                            merge,
                            base,
                            immediate: high | control,
                        },
                        seed: ordinal,
                        mxcsr: 0x1F80 | prior_status | (rc << 13) | daz_ftz,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_ROUND_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    use crate::smir::lower::runtime::ExecMem;

    assert!(range.start < range.end && range.end <= cases.len());
    let executions = range.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for native_case in &cases[range] {
        let case = native_case.instruction;
        let source = source_words(case, native_case.seed);
        let function = optimize(lift_case(case), native_case.level);
        let (code, entry) = lower(&function, case);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));

        let mut context = VectorMemoryContext {
            value: source,
            ok: 1,
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
        };
        let mut registers = full_guest_regs(case, native_case.seed, native_case.mxcsr);
        let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let initial = registers;
        let mut expected = interpreted_expected(&function, &initial, source, address, case);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_helper_observation(&context, address, case, "success");
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
        let mut registers =
            full_guest_regs(case, native_case.seed ^ 0x55, native_case.mxcsr ^ (1 << 5));
        let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let mut expected = registers;
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_helper_observation(&context, address, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX round memory cases"
    );
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX round memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 800);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = case.instruction.bytes();
    panic!(
        "isolated native VEX round memory failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_round_matches_o0_o2_interpreter_and_faults_without_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX round memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_round_memory_source::\
         native_memory_round_matches_o0_o2_interpreter_and_faults_without_commit",
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn interpreter_memory_round_matches_intel_controls_exceptions_merge_and_zeroing() {
    let mode_expectations = [
        (0u8, 2.0f32.to_bits(), 2.0f64.to_bits()),
        (1, 1.0f32.to_bits(), 1.0f64.to_bits()),
        (2, 2.0f32.to_bits(), 2.0f64.to_bits()),
        (3, 1.0f32.to_bits(), 1.0f64.to_bits()),
    ];
    for kind in [RoundKind::ScalarF32, RoundKind::ScalarF64] {
        for (mode, expected_f32, expected_f64) in mode_expectations {
            for dynamic in [false, true] {
                for suppress_precision in [false, true] {
                    let case = RoundCase {
                        kind,
                        w: mode & 1 != 0,
                        l: mode & 2 != 0,
                        destination: 9,
                        merge: 10,
                        base: 11,
                        immediate: (if dynamic { 4 | 3 } else { mode })
                            | (u8::from(suppress_precision) << 3)
                            | 0xA0,
                    };
                    let function = optimize(lift_case(case), OptLevel::O2);
                    let initial =
                        full_guest_regs(case, usize::from(mode), 0x1F80 | (u32::from(mode) << 13));
                    let mut source = [0u64; 8];
                    if kind == RoundKind::ScalarF32 {
                        set_f32_lane(&mut source, 0, 1.5f32.to_bits());
                    } else {
                        source[0] = 1.5f64.to_bits();
                    }
                    let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                    let expected = interpreted_expected(&function, &initial, source, address, case);
                    let actual_low = expected.zmm[usize::from(case.destination)][0];
                    assert_eq!(
                        if kind == RoundKind::ScalarF32 {
                            actual_low & u64::from(u32::MAX)
                        } else {
                            actual_low
                        },
                        if kind == RoundKind::ScalarF32 {
                            u64::from(expected_f32)
                        } else {
                            expected_f64
                        },
                        "{case:?}"
                    );
                    assert_eq!(
                        expected.mxcsr & (1 << 5),
                        if suppress_precision { 0 } else { 1 << 5 },
                        "{case:?}"
                    );
                    let merge = initial.zmm[usize::from(case.merge)];
                    if kind == RoundKind::ScalarF32 {
                        assert_eq!(
                            actual_low & !u64::from(u32::MAX),
                            merge[0] & !u64::from(u32::MAX),
                            "{case:?}"
                        );
                    }
                    assert_eq!(
                        expected.zmm[usize::from(case.destination)][1],
                        merge[1],
                        "{case:?}"
                    );
                    assert_eq!(
                        expected.zmm[usize::from(case.destination)][2..],
                        [0; 6],
                        "{case:?}"
                    );
                }
            }
        }
    }

    for kind in [RoundKind::ScalarF32, RoundKind::ScalarF64] {
        let case = RoundCase {
            kind,
            w: true,
            l: true,
            destination: 1,
            merge: 2,
            base: 3,
            immediate: 8,
        };
        let function = lift_case(case);
        let initial = full_guest_regs(case, 0, 0x1F80);
        let mut source = [0u64; 8];
        source[0] = if kind == RoundKind::ScalarF32 {
            u64::from(0x7F80_0001u32)
        } else {
            0x7FF0_0000_0000_0001
        };
        let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
        let expected = interpreted_expected(&function, &initial, source, address, case);
        assert_eq!(expected.mxcsr & 1, 1, "{case:?}");
        assert_eq!(expected.mxcsr & (1 << 5), 0, "{case:?}");
        assert_eq!(
            if kind == RoundKind::ScalarF32 {
                u64::from(f32_lane(&expected.zmm[usize::from(case.destination)], 0))
            } else {
                expected.zmm[usize::from(case.destination)][0]
            },
            if kind == RoundKind::ScalarF32 {
                u64::from(0x7FC0_0001u32)
            } else {
                0x7FF8_0000_0000_0001
            },
            "{case:?}"
        );
    }
}
