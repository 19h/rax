//! Exact helper-backed VEX VPSHUFD/VPSHUFHW/VPSHUFLW memory-source coverage.

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
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexLaneShuffleMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_lane_shuffle_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0x70_50;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShuffleKind {
    Dword,
    HighWord,
    LowWord,
}

impl ShuffleKind {
    const ALL: [Self; 3] = [Self::Dword, Self::HighWord, Self::LowWord];

    const fn pp(self) -> u8 {
        match self {
            Self::Dword => 1,
            Self::HighWord => 2,
            Self::LowWord => 3,
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::Dword => VecElementType::I32,
            Self::HighWord | Self::LowWord => VecElementType::I16,
        }
    }

    const fn high_words(self) -> Option<bool> {
        match self {
            Self::Dword => None,
            Self::HighWord => Some(true),
            Self::LowWord => Some(false),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    C5,
    C4W0,
    C4W1,
}

impl EncodingForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];

    const fn w(self) -> bool {
        matches!(self, Self::C4W1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LaneShuffleMemoryCase {
    kind: ShuffleKind,
    width: VecWidth,
    form: EncodingForm,
    destination: u8,
    base: u8,
    immediate: u8,
}

impl LaneShuffleMemoryCase {
    const fn memory_size(self) -> u32 {
        if matches!(self.width, VecWidth::V128) {
            16
        } else {
            32
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination)
            .expect("one destination leaves at least fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0x40 | ((self.destination & 7) << 3) | (self.base & 7);
        match self.form {
            EncodingForm::C5 => {
                assert!(self.base < 8);
                vec![
                    0xC5,
                    (if self.destination < 8 { 0x80 } else { 0 })
                        | 0x78
                        | (l << 2)
                        | self.kind.pp(),
                    0x70,
                    modrm,
                    DISP as u8,
                    self.immediate,
                ]
            }
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | 0x78 | (l << 2) | self.kind.pp(),
                0x70,
                modrm,
                DISP as u8,
                self.immediate,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let l = u8::from(self.width == VecWidth::V256);
        let scratch = self.scratch();
        let modrm = 0xC0 | ((self.destination & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if self.destination < 8 { 0x80 } else { 0 }) | 0x78 | (l << 2) | self.kind.pp(),
                0x70,
                modrm,
                self.immediate,
            ]
        } else {
            vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | 0x78 | (l << 2) | self.kind.pp(),
                0x70,
                modrm,
                self.immediate,
            ]
        }
    }
}

fn semantic_cases() -> Vec<LaneShuffleMemoryCase> {
    const IMMEDIATES: [u8; 6] = [0x00, 0x1B, 0x4E, 0x93, 0xA5, 0xFF];
    let mut cases = Vec::new();
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in EncodingForm::ALL {
                let (destination, base) = match form {
                    EncodingForm::C5 => (9, 3),
                    EncodingForm::C4W0 => (15, 11),
                    EncodingForm::C4W1 => (0, 11),
                };
                for immediate in IMMEDIATES {
                    cases.push(LaneShuffleMemoryCase {
                        kind,
                        width,
                        form,
                        destination,
                        base,
                        immediate,
                    });
                }
            }
        }
    }
    cases
}

fn scanner_cases() -> Vec<LaneShuffleMemoryCase> {
    let mut cases = Vec::new();
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in EncodingForm::ALL {
                for destination in 0..8 {
                    cases.push(LaneShuffleMemoryCase {
                        kind,
                        width,
                        form,
                        destination,
                        base: 2,
                        immediate: 0,
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

fn destination_reg(case: LaneShuffleMemoryCase) -> VReg {
    x86(match case.width {
        VecWidth::V128 => X86Reg::Xmm(case.destination),
        VecWidth::V256 => X86Reg::Ymm(case.destination),
        _ => unreachable!(),
    })
}

fn expected_address(case: LaneShuffleMemoryCase) -> Address {
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
) -> Option<X86JitVexLaneShuffleMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_lane_shuffle_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
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

fn lift_case(case: LaneShuffleMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn selector(case: LaneShuffleMemoryCase, lane: u8) -> u8 {
    let elem = case.kind.elem();
    let block_lanes = if elem == VecElementType::I32 { 4 } else { 8 };
    let within = lane % block_lanes;
    let lane_block = lane - within;
    let shuffled = match case.kind.high_words() {
        None => true,
        Some(true) => within >= 4,
        Some(false) => within < 4,
    };
    if shuffled {
        let output = within % 4;
        lane_block
            + if case.kind.high_words() == Some(true) {
                4
            } else {
                0
            }
            + ((case.immediate >> (output * 2)) & 3)
    } else {
        lane
    }
}

fn assert_exact_sequence(function: &SmirFunction, case: LaneShuffleMemoryCase) {
    let ops = &function.blocks[0].ops;
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as u8;
    let consumed = 4 + usize::from(lanes) * 2;
    assert_eq!(ops.len(), consumed, "{case:?}");
    assert_eq!(ops[0].guest_pc, PC, "{case:?}");
    assert_eq!(
        ops[0].x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}"
    );
    let loaded = match &ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected VLoad, got {other:?}"),
    };
    let zero = match ops[1].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => zero,
        ref other => panic!("{case:?}: expected zero Mov, got {other:?}"),
    };
    let indices = match ops[2].kind {
        OpKind::VBroadcast {
            dst: indices @ VReg::Virtual(_),
            scalar,
            elem: broadcast_elem,
            lanes: broadcast_lanes,
        } => {
            assert_eq!(scalar, zero, "{case:?}");
            assert_eq!(broadcast_elem, elem, "{case:?}");
            assert_eq!(broadcast_lanes, lanes, "{case:?}");
            indices
        }
        ref other => panic!("{case:?}: expected index broadcast, got {other:?}"),
    };
    for lane in 0..lanes {
        let mov_index = 3 + usize::from(lane) * 2;
        let scalar = match ops[mov_index].kind {
            OpKind::Mov {
                dst: scalar @ VReg::Virtual(_),
                src: SrcOperand::Imm(value),
                width: OpWidth::W64,
            } => {
                assert_eq!(
                    value,
                    i64::from(selector(case, lane)),
                    "{case:?} lane {lane}"
                );
                scalar
            }
            ref other => panic!("{case:?}: expected selector Mov, got {other:?}"),
        };
        assert!(
            matches!(
                ops[mov_index + 1].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted,
                    lane: inserted_lane,
                    elem: inserted_elem,
                } if dst == indices
                    && vec == indices
                    && inserted == scalar
                    && inserted_lane == lane
                    && inserted_elem == elem
            ),
            "{case:?} lane {lane}: {:?}",
            ops[mov_index + 1].kind
        );
    }
    assert!(
        matches!(
            ops[consumed - 1].kind,
            OpKind::VShuffle {
                dst,
                src1,
                src2: None,
                indices: shuffled_indices,
                elem: shuffled_elem,
                lanes: shuffled_lanes,
            } if dst == destination_reg(case)
                && src1 == loaded
                && shuffled_indices == indices
                && shuffled_elem == elem
                && shuffled_lanes == lanes
        ),
        "{case:?}: {:?}",
        ops[consumed - 1].kind
    );
    assert!(ops.iter().skip(1).all(|op| op.x86_hint.is_none()));
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexLaneShuffleMemorySequence {
            consumed,
            memory_size: case.memory_size(),
            destination: case.destination,
            width: case.width,
            elem,
            high_words: case.kind.high_words(),
            immediate: case.immediate,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexLaneShuffleMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VEX lane shuffle");
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
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
            && (sequence.width == VecWidth::V128 || std::is_x86_feature_detected!("avx2"))
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("helper-backed VEX lane-shuffle lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX lane shuffle"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_432_scanner_encoding_and_optimization_cells_admit_and_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 144);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let (code, _, _) = lower(&function);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector scratch index"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xB9, case.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing memory byte size"
            );
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 432);
}

#[test]
fn llvm_23_memory_encodings_match_the_generators() {
    for (case, expected) in [
        (
            LaneShuffleMemoryCase {
                kind: ShuffleKind::Dword,
                width: VecWidth::V256,
                form: EncodingForm::C5,
                destination: 2,
                base: 7,
                immediate: 0x4E,
            },
            &[0xC5, 0xFD, 0x70, 0x57, 0x20, 0x4E][..],
        ),
        (
            LaneShuffleMemoryCase {
                kind: ShuffleKind::HighWord,
                width: VecWidth::V256,
                form: EncodingForm::C5,
                destination: 3,
                base: 7,
                immediate: 0xB1,
            },
            &[0xC5, 0xFE, 0x70, 0x5F, 0x20, 0xB1][..],
        ),
        (
            LaneShuffleMemoryCase {
                kind: ShuffleKind::LowWord,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                destination: 2,
                base: 7,
                immediate: 0x93,
            },
            &[0xC5, 0xFB, 0x70, 0x57, 0x20, 0x93][..],
        ),
    ] {
        assert_eq!(case.bytes(), expected, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_high_register_and_addr32_shapes_admit() {
    let encodings: &[&[u8]] = &[
        // vpshufd xmm1,[rip+0x44332211],0x1b
        &[0xC5, 0xF9, 0x70, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x1B],
        // vpshufhw ymm3,fs:[rcx*4+0x44332211],0x4e
        &[
            0x64, 0xC5, 0xFE, 0x70, 0x1C, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x4E,
        ],
        // vpshuflw ymm14,fs:addr32 [r14d+r15d*2+0x44332211],0xa5
        &[
            0x64, 0x67, 0xC4, 0x01, 0xFF, 0x70, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xA5,
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
        "{name}: classifier admitted malformed lane-shuffle graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed lane-shuffle graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed lane-shuffle graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_lowerer_fail_closed_for_graph_hint_ssa_and_provenance_invariants() {
    let case = LaneShuffleMemoryCase {
        kind: ShuffleKind::HighWord,
        width: VecWidth::V256,
        form: EncodingForm::C4W0,
        destination: 15,
        base: 11,
        immediate: 0xA5,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    assert_exact_sequence(&base, case);
    let lanes = case.width.lanes(case.kind.elem()) as usize;
    let final_index = 3 + lanes * 2;
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let zero = match base.blocks[0].ops[1].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let indices = match base.blocks[0].ops[2].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let first_selector = match base.blocks[0].ops[3].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", load_width));

    let mut extra_loaded_use = base.clone();
    extra_loaded_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF0),
        PC + 1,
        OpKind::VShuffle {
            dst: VReg::Virtual(VirtualId(0xFFF0)),
            src1: loaded,
            src2: None,
            indices,
            elem: case.kind.elem(),
            lanes: case.width.lanes(case.kind.elem()) as u8,
        },
    ));
    malformed.push(("loaded value external use", extra_loaded_use));

    let mut zero_value = base.clone();
    if let OpKind::Mov { src, .. } = &mut zero_value.blocks[0].ops[1].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("zero value", zero_value));

    let mut zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut zero_width.blocks[0].ops[1].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("zero width", zero_width));

    let mut broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut broadcast_scalar.blocks[0].ops[2].kind {
        *scalar = first_selector;
    }
    malformed.push(("broadcast scalar", broadcast_scalar));

    let mut broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut broadcast_element.blocks[0].ops[2].kind {
        *elem = VecElementType::I32;
    }
    malformed.push(("broadcast element", broadcast_element));

    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[2].kind {
        *lanes -= 1;
    }
    malformed.push(("broadcast lanes", broadcast_lanes));

    let mut selector_value = base.clone();
    if let OpKind::Mov { src, .. } = &mut selector_value.blocks[0].ops[3].kind {
        *src = SrcOperand::Imm(31);
    }
    malformed.push(("selector value", selector_value));

    let mut selector_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut selector_width.blocks[0].ops[3].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("selector width", selector_width));

    let mut insert_vector = base.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut insert_vector.blocks[0].ops[4].kind {
        *vec = loaded;
    }
    malformed.push(("insert vector", insert_vector));

    let mut insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut insert_scalar.blocks[0].ops[4].kind {
        *scalar = zero;
    }
    malformed.push(("insert scalar", insert_scalar));

    let mut insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut insert_lane.blocks[0].ops[4].kind {
        *lane = 1;
    }
    malformed.push(("insert lane", insert_lane));

    let mut insert_element = base.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut insert_element.blocks[0].ops[4].kind {
        *elem = VecElementType::I32;
    }
    malformed.push(("insert element", insert_element));

    let mut final_destination = base.clone();
    if let OpKind::VShuffle { dst, .. } = &mut final_destination.blocks[0].ops[final_index].kind {
        *dst = x86(X86Reg::Ymm(14));
    }
    malformed.push(("final destination", final_destination));

    let mut final_source = base.clone();
    if let OpKind::VShuffle { src1, .. } = &mut final_source.blocks[0].ops[final_index].kind {
        *src1 = indices;
    }
    malformed.push(("final source", final_source));

    let mut second_source = base.clone();
    if let OpKind::VShuffle { src2, .. } = &mut second_source.blocks[0].ops[final_index].kind {
        *src2 = Some(loaded);
    }
    malformed.push(("unexpected second source", second_source));

    let mut final_indices = base.clone();
    if let OpKind::VShuffle {
        indices: shuffled, ..
    } = &mut final_indices.blocks[0].ops[final_index].kind
    {
        *shuffled = loaded;
    }
    malformed.push(("final indices", final_indices));

    let mut final_element = base.clone();
    if let OpKind::VShuffle { elem, .. } = &mut final_element.blocks[0].ops[final_index].kind {
        *elem = VecElementType::I32;
    }
    malformed.push(("final element", final_element));

    let mut final_lanes = base.clone();
    if let OpKind::VShuffle { lanes, .. } = &mut final_lanes.blocks[0].ops[final_index].kind {
        *lanes -= 1;
    }
    malformed.push(("final lanes", final_lanes));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF1),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF1)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("same-PC tail", same_pc_tail));

    let mut op_hint = base.clone();
    op_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("non-load hint", op_hint));

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x01),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("encoded vvvv", 2, 0x08),
        ("encoded immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
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

fn lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I16 => u64::from(u16::from_le_bytes(
            bytes[lane * 2..lane * 2 + 2].try_into().unwrap(),
        )),
        VecElementType::I32 => u64::from(u32::from_le_bytes(
            bytes[lane * 4..lane * 4 + 4].try_into().unwrap(),
        )),
        _ => unreachable!("VEX lane-shuffle element"),
    }
}

fn set_lane(bytes: &mut [u8; 64], lane: usize, elem: VecElementType, value: u64) {
    match elem {
        VecElementType::I16 => {
            bytes[lane * 2..lane * 2 + 2].copy_from_slice(&(value as u16).to_le_bytes());
        }
        VecElementType::I32 => {
            bytes[lane * 4..lane * 4 + 4].copy_from_slice(&(value as u32).to_le_bytes());
        }
        _ => unreachable!("VEX lane-shuffle element"),
    }
}

fn independent_shuffle(case: LaneShuffleMemoryCase, source: [u64; 8]) -> [u64; 8] {
    let source = words_to_bytes(source);
    let mut result = [0; 64];
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as usize;
    for output in 0..lanes {
        let selected = selector(case, output as u8);
        set_lane(
            &mut result,
            output,
            elem,
            lane(&source, usize::from(selected), elem),
        );
    }
    bytes_to_words(result)
}

#[test]
fn independent_oracle_covers_every_immediate_and_lane_mapping() {
    let source_bytes = std::array::from_fn(|index| index as u8);
    let source = bytes_to_words(source_bytes);
    let mut checked = 0usize;
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for immediate in u8::MIN..=u8::MAX {
                let case = LaneShuffleMemoryCase {
                    kind,
                    width,
                    form: EncodingForm::C5,
                    destination: 1,
                    base: 3,
                    immediate,
                };
                let result = words_to_bytes(independent_shuffle(case, source));
                let elem = kind.elem();
                for output in 0..width.lanes(elem) as usize {
                    assert_eq!(
                        lane(&result, output, elem),
                        lane(
                            &source_bytes,
                            usize::from(selector(case, output as u8)),
                            elem
                        ),
                        "{case:?} output {output}"
                    );
                }
                assert!(
                    result[case.memory_size() as usize..]
                        .iter()
                        .all(|byte| *byte == 0),
                    "{case:?}: VEX upper bits"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 2 * 256);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct LaneShuffleMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn lane_shuffle_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut LaneShuffleMemoryContext) };
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
    let source = words_to_bytes(context.value);
    let mut scratch = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    scratch[..size as usize].copy_from_slice(&source[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

#[cfg(target_arch = "x86_64")]
fn patterned_vector(shift: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x0123_4567_89AB_CDEFu64.rotate_left(((word * 9 + shift) % 64) as u32)
            ^ (shift as u64).wrapping_mul(0x0101_0101_0101_0101)
    })
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: LaneShuffleMemoryCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80 | (((ordinal as u32 >> 2) & 3) << 13),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(index * 5 + ordinal);
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: LaneShuffleMemoryCase,
    source: [u64; 8],
) -> GuestRegs {
    registers.zmm[usize::from(case.destination)] = independent_shuffle(case, source);
    let source_bytes = words_to_bytes(source);
    let mut scratch = [0; 64];
    scratch[..case.memory_size() as usize]
        .copy_from_slice(&source_bytes[..case.memory_size() as usize]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: LaneShuffleMemoryCase,
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
    let bytes = words_to_bytes(source);
    memory.load(address as usize, &bytes[..case.memory_size() as usize]);
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
#[test]
fn native_lane_shuffles_match_independent_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX lane-shuffle memory differential: host lacks AVX");
        return;
    }
    let has_avx2 = std::is_x86_feature_detected!("avx2");
    let cases = semantic_cases();
    assert_eq!(cases.len(), 108);
    let expected_executions = cases
        .iter()
        .filter(|case| case.width == VecWidth::V128 || has_avx2)
        .count()
        * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        if case.width == VecWidth::V256 && !has_avx2 {
            continue;
        }
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = patterned_vector(ordinal.wrapping_mul(7).wrapping_add(3));

            let mut context = LaneShuffleMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut LaneShuffleMemoryContext) as u64;
            registers.vec_load_fn = lane_shuffle_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, source);

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
            assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function, &initial, &expected, source, address, case, level,
            );
            successes += 1;

            let mut context = LaneShuffleMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut LaneShuffleMemoryContext) as u64;
            registers.vec_load_fn = lane_shuffle_load_helper as usize as u64;
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
                case.memory_size(),
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
        "executed {successes} successful and {faults} faulting native VEX packed lane-shuffle memory cases"
    );
}
