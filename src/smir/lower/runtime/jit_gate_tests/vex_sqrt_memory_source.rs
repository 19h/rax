//! Exact helper-backed VEX floating-point square-root memory coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpId, OpWidth,
    SignExtend, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexSqrtMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_sqrt_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x51_20;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SqrtKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl SqrtKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    const fn scalar(self) -> bool {
        matches!(self, Self::ScalarF32 | Self::ScalarF64)
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::PackedF32 | Self::ScalarF32 => VecElementType::F32,
            Self::PackedF64 | Self::ScalarF64 => VecElementType::F64,
        }
    }

    const fn prefix(self) -> X86SsePrefix {
        match self {
            Self::PackedF32 => X86SsePrefix::None,
            Self::PackedF64 => X86SsePrefix::OpSize,
            Self::ScalarF32 => X86SsePrefix::Rep,
            Self::ScalarF64 => X86SsePrefix::Repne,
        }
    }

    const fn pp(self) -> u8 {
        match self.prefix() {
            X86SsePrefix::None => 0,
            X86SsePrefix::OpSize => 1,
            X86SsePrefix::Rep => 2,
            X86SsePrefix::Repne => 3,
        }
    }

    const fn widths(self) -> &'static [VecWidth] {
        if self.scalar() {
            &[VecWidth::V128]
        } else {
            &[VecWidth::V128, VecWidth::V256]
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
struct SqrtMemoryCase {
    kind: SqrtKind,
    width: VecWidth,
    form: EncodingForm,
    destination: u8,
    source1: u8,
    base: u8,
}

impl SqrtMemoryCase {
    const fn source1(self) -> Option<u8> {
        if self.kind.scalar() {
            Some(self.source1)
        } else {
            None
        }
    }

    const fn memory_size(self) -> u32 {
        if self.kind.scalar() {
            self.kind.elem().bytes()
        } else {
            self.width.bytes()
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination && self.source1() != Some(*index))
            .expect("at most two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.source1 < 16 && self.base < 16);
        assert!(!self.kind.scalar() || self.width == VecWidth::V128);
        let encoded_vvvv = self.source1().map_or(0x0F, |index| !index & 0x0F);
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0x40 | ((self.destination & 7) << 3) | (self.base & 7);
        match self.form {
            EncodingForm::C5 => {
                assert!(self.base < 8);
                vec![
                    0xC5,
                    (if self.destination < 8 { 0x80 } else { 0 })
                        | (encoded_vvvv << 3)
                        | (l << 2)
                        | self.kind.pp(),
                    0x51,
                    modrm,
                    DISP as u8,
                ]
            }
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | (encoded_vvvv << 3) | (l << 2) | self.kind.pp(),
                0x51,
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let scratch = self.scratch();
        let encoded_vvvv = self.source1().map_or(0x0F, |index| !index & 0x0F);
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0xC0 | ((self.destination & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if self.destination < 8 { 0x80 } else { 0 })
                    | (encoded_vvvv << 3)
                    | (l << 2)
                    | self.kind.pp(),
                0x51,
                modrm,
            ]
        } else {
            vec![
                0xC4,
                (if self.destination < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | (encoded_vvvv << 3) | (l << 2) | self.kind.pp(),
                0x51,
                modrm,
            ]
        }
    }
}

fn scanner_cases() -> Vec<SqrtMemoryCase> {
    let mut cases = Vec::new();
    for kind in SqrtKind::ALL {
        for &width in kind.widths() {
            for form in EncodingForm::ALL {
                for destination in 0..8 {
                    let source1s: &[u8] = if kind.scalar() {
                        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
                    } else {
                        &[0]
                    };
                    for &source1 in source1s {
                        cases.push(SqrtMemoryCase {
                            kind,
                            width,
                            form,
                            destination,
                            source1,
                            base: 2,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn semantic_cases() -> Vec<SqrtMemoryCase> {
    let mut cases = Vec::new();
    for kind in SqrtKind::ALL {
        for &width in kind.widths() {
            for form in EncodingForm::ALL {
                let base = match form {
                    EncodingForm::C5 => 3,
                    EncodingForm::C4W0 => 11,
                    EncodingForm::C4W1 => 14,
                };
                let operands: &[(u8, u8)] = if kind.scalar() {
                    &[(0, 1), (1, 1), (1, 0), (9, 10), (10, 9), (15, 15)]
                } else {
                    &[(0, 0), (9, 0), (15, 0)]
                };
                for &(destination, source1) in operands {
                    cases.push(SqrtMemoryCase {
                        kind,
                        width,
                        form,
                        destination,
                        source1,
                        base,
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
        _ => unreachable!("VEX square root has only 128-/256-bit vector operands"),
    })
}

fn expected_address(case: SqrtMemoryCase) -> Address {
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
) -> Option<X86JitVexSqrtMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_sqrt_memory_sequence(
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
        X86InstructionBytes::new(bytes).expect("VEX square-root instruction fits metadata"),
    );
    function
}

fn lift_case(case: SqrtMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_sequence(function: &SmirFunction, case: SqrtMemoryCase) {
    let ops = &function.blocks[0].ops;
    assert!(
        ops.iter().all(|op| op.guest_pc == PC),
        "{case:?}: split guest-PC provenance"
    );
    let source = match &ops[0].kind {
        OpKind::VLoad {
            dst: source @ VReg::Virtual(_),
            addr,
            width,
        } if !case.kind.scalar() => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            assert_eq!(ops[0].x86_hint, None, "{case:?}");
            *source
        }
        OpKind::Load {
            dst: source @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } if case.kind.scalar() => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(
                *width,
                if case.kind.elem() == VecElementType::F32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                "{case:?}"
            );
            assert_eq!(ops[0].x86_hint, None, "{case:?}");
            *source
        }
        other => panic!("{case:?}: unexpected memory source {other:?}"),
    };

    if !case.kind.scalar() {
        assert_eq!(ops.len(), 2, "{case:?}");
        let OpKind::X86Sqrt {
            dst,
            src,
            elem,
            lanes,
            round,
            suppress_exceptions,
        } = ops[1].kind
        else {
            panic!("{case:?}: unexpected packed consumer {:?}", ops[1].kind)
        };
        assert_eq!(dst, vector(case.destination, case.width), "{case:?}");
        assert_eq!(src, source, "{case:?}");
        assert_eq!(elem, case.kind.elem(), "{case:?}");
        assert_eq!(u32::from(lanes), case.width.lanes(elem), "{case:?}");
        assert_eq!(round, FpRoundMode::Dynamic, "{case:?}");
        assert!(!suppress_exceptions, "{case:?}");
    } else {
        let elem = case.kind.elem();
        let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
        let expected_ops = 5 + 2 * xmm_lanes;
        assert_eq!(ops.len(), expected_ops, "{case:?}");
        let source_vector = match ops[1].kind {
            OpKind::VBroadcast {
                dst: vector @ VReg::Virtual(_),
                scalar,
                elem: broadcast_elem,
                lanes: 1,
            } => {
                assert_eq!(scalar, source, "{case:?}");
                assert_eq!(broadcast_elem, elem, "{case:?}");
                vector
            }
            ref other => panic!("{case:?}: unexpected scalar source broadcast {other:?}"),
        };
        let sqrt_result = match ops[2].kind {
            OpKind::X86Sqrt {
                dst: result @ VReg::Virtual(_),
                src,
                elem: sqrt_elem,
                lanes: 1,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            } => {
                assert_eq!(src, source_vector, "{case:?}");
                assert_eq!(sqrt_elem, elem, "{case:?}");
                result
            }
            ref other => panic!("{case:?}: unexpected scalar square root {other:?}"),
        };
        let scalar_result = match ops[3].kind {
            OpKind::VExtractLane {
                dst: scalar @ VReg::Virtual(_),
                vec,
                lane: 0,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(vec, sqrt_result, "{case:?}");
                assert_eq!(extract_elem, elem, "{case:?}");
                scalar
            }
            ref other => panic!("{case:?}: unexpected low extraction {other:?}"),
        };
        let source1 = vector(case.source1, VecWidth::V128);
        let mut upper_scalars = Vec::new();
        for lane in 1..xmm_lanes {
            match ops[3 + lane].kind {
                OpKind::VExtractLane {
                    dst: scalar @ VReg::Virtual(_),
                    vec,
                    lane: extract_lane,
                    elem: extract_elem,
                    sign: SignExtend::Zero,
                } => {
                    assert_eq!(vec, source1, "{case:?} lane {lane}");
                    assert_eq!(usize::from(extract_lane), lane, "{case:?}");
                    assert_eq!(extract_elem, elem, "{case:?}");
                    upper_scalars.push(scalar);
                }
                ref other => panic!("{case:?}: unexpected upper extraction {lane}: {other:?}"),
            }
        }
        let zero_index = 3 + xmm_lanes;
        let zero = match ops[zero_index].kind {
            OpKind::Mov {
                dst: zero @ VReg::Virtual(_),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => zero,
            ref other => panic!("{case:?}: unexpected destination-zero value {other:?}"),
        };
        let destination = vector(case.destination, VecWidth::V128);
        assert!(
            matches!(
                ops[zero_index + 1].kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: broadcast_elem,
                    lanes: 1,
                } if dst == destination && scalar == zero && broadcast_elem == elem
            ),
            "{case:?}: {:?}",
            ops[zero_index + 1].kind
        );
        assert!(
            matches!(
                ops[zero_index + 2].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem: insert_elem,
                } if dst == destination
                    && vec == destination
                    && scalar == scalar_result
                    && insert_elem == elem
            ),
            "{case:?}: {:?}",
            ops[zero_index + 2].kind
        );
        for (lane, scalar) in upper_scalars.into_iter().enumerate() {
            let lane = lane + 1;
            assert!(
                matches!(
                    ops[zero_index + 2 + lane].kind,
                    OpKind::VInsertLane {
                        dst,
                        vec,
                        scalar: inserted,
                        lane: insert_lane,
                        elem: insert_elem,
                    } if dst == destination
                        && vec == destination
                        && inserted == scalar
                        && usize::from(insert_lane) == lane
                        && insert_elem == elem
                ),
                "{case:?} lane {lane}: {:?}",
                ops[zero_index + 2 + lane].kind
            );
        }
        assert!(
            ops.iter()
                .enumerate()
                .all(|(index, op)| index == 2 || op.x86_hint.is_none()),
            "{case:?}: only X86Sqrt may carry VEX provenance"
        );
    }

    assert_eq!(
        ops[if case.kind.scalar() { 2 } else { 1 }].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: case.kind.prefix(),
            opcode: 0x51,
            width: case.width,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    let expected = X86JitVexSqrtMemorySequence {
        consumed: ops.len(),
        memory_size: case.memory_size(),
        destination: case.destination,
        source1: case.source1(),
        elem: case.kind.elem(),
        width: case.width,
        w: case.form.w(),
    };
    assert_eq!(
        classified_sequence(function, true),
        Some(expected),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(
    function: &SmirFunction,
    case: SqrtMemoryCase,
) -> (Vec<u8>, usize, X86JitVexSqrtMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VEX square root");
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
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("{case:?}: helper-backed VEX square root failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX square root"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_2592_scanner_encoding_and_optimization_cells_admit_and_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 864);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let (code, _, _) = lower(&function, case);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector scratch index"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xB9, case.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing exact memory byte size"
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
    assert_eq!(lowered, 2_592);
}

#[test]
fn llvm_23_memory_encodings_match_the_generator() {
    for (case, expected) in [
        (
            SqrtMemoryCase {
                kind: SqrtKind::PackedF32,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                destination: 1,
                source1: 0,
                base: 7,
            },
            &[0xC5, 0xF8, 0x51, 0x4F, 0x20][..],
        ),
        (
            SqrtMemoryCase {
                kind: SqrtKind::PackedF32,
                width: VecWidth::V256,
                form: EncodingForm::C5,
                destination: 2,
                source1: 0,
                base: 7,
            },
            &[0xC5, 0xFC, 0x51, 0x57, 0x20][..],
        ),
        (
            SqrtMemoryCase {
                kind: SqrtKind::PackedF64,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                destination: 3,
                source1: 0,
                base: 7,
            },
            &[0xC5, 0xF9, 0x51, 0x5F, 0x20][..],
        ),
        (
            SqrtMemoryCase {
                kind: SqrtKind::PackedF64,
                width: VecWidth::V256,
                form: EncodingForm::C5,
                destination: 4,
                source1: 0,
                base: 7,
            },
            &[0xC5, 0xFD, 0x51, 0x67, 0x20][..],
        ),
        (
            SqrtMemoryCase {
                kind: SqrtKind::ScalarF32,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                destination: 5,
                source1: 2,
                base: 7,
            },
            &[0xC5, 0xEA, 0x51, 0x6F, 0x20][..],
        ),
        (
            SqrtMemoryCase {
                kind: SqrtKind::ScalarF64,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                destination: 6,
                source1: 3,
                base: 7,
            },
            &[0xC5, 0xE3, 0x51, 0x77, 0x20][..],
        ),
    ] {
        assert_eq!(case.bytes(), expected, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_high_register_and_addr32_shapes_admit() {
    let encodings: &[&[u8]] = &[
        // vsqrtpd xmm1,[rip+0x44332211]
        &[0xC5, 0xF9, 0x51, 0x0D, 0x11, 0x22, 0x33, 0x44],
        // vsqrtps ymm3,fs:[rcx*4+0x44332211]
        &[0x64, 0xC5, 0xFC, 0x51, 0x1C, 0x8D, 0x11, 0x22, 0x33, 0x44],
        // vsqrtsd xmm14,xmm13,fs:addr32 [r14d+r15d*2+0x44332211]
        &[
            0x64, 0x67, 0xC4, 0x01, 0x93, 0x51, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ],
    ];
    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let case = match classified_sequence(&function, true) {
                Some(sequence) => SqrtMemoryCase {
                    kind: match (sequence.source1.is_some(), sequence.elem) {
                        (false, VecElementType::F32) => SqrtKind::PackedF32,
                        (false, VecElementType::F64) => SqrtKind::PackedF64,
                        (true, VecElementType::F32) => SqrtKind::ScalarF32,
                        (true, VecElementType::F64) => SqrtKind::ScalarF64,
                        _ => unreachable!(),
                    },
                    width: sequence.width,
                    form: if sequence.w {
                        EncodingForm::C4W1
                    } else {
                        EncodingForm::C5
                    },
                    destination: sequence.destination,
                    source1: sequence.source1.unwrap_or(0),
                    base: 0,
                },
                None => panic!("{level:?} {bytes:02X?}: address shape rejected"),
            };
            lower(&function, case);
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated VEX square-root metadata fits"),
    );
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed VEX square-root graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native clobber gate admitted malformed VEX square-root graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed VEX square-root graph"
    );
}

#[test]
fn packed_graph_and_source_byte_provenance_fail_closed_for_every_invariant() {
    let case = SqrtMemoryCase {
        kind: SqrtKind::PackedF32,
        width: VecWidth::V256,
        form: EncodingForm::C4W1,
        destination: 9,
        source1: 0,
        base: 11,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert_exact_sequence(&base, case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0x51,
        width: VecWidth::V256,
        w: true,
    });
    malformed.push(("load provenance hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", load_width));

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xF100)));
    }
    malformed.push(("virtual address", virtual_address));

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xF101),
        PC + 1,
        OpKind::VMov {
            dst: vector(3, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value used twice", extra_use));

    let mut second_definition = base.clone();
    second_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0xF102),
        PC + 1,
        OpKind::VMov {
            dst: loaded,
            src: vector(3, VecWidth::V256),
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value defined twice", second_definition));

    let mut wrong_source = base.clone();
    if let OpKind::X86Sqrt { src, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src = vector(3, VecWidth::V256);
    }
    malformed.push(("square-root source bypasses load", wrong_source));

    let mut wrong_destination = base.clone();
    if let OpKind::X86Sqrt { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V256);
    }
    malformed.push(("EVEX-only destination", wrong_destination));

    let mut wrong_element = base.clone();
    if let OpKind::X86Sqrt { elem, lanes, .. } = &mut wrong_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
        *lanes = 4;
    }
    malformed.push(("element/prefix mismatch", wrong_element));

    let mut wrong_lanes = base.clone();
    if let OpKind::X86Sqrt { lanes, .. } = &mut wrong_lanes.blocks[0].ops[1].kind {
        *lanes -= 1;
    }
    malformed.push(("nonintegral packed lane geometry", wrong_lanes));

    let mut static_rounding = base.clone();
    if let OpKind::X86Sqrt {
        round,
        suppress_exceptions,
        ..
    } = &mut static_rounding.blocks[0].ops[1].kind
    {
        *round = FpRoundMode::RoundUp;
        *suppress_exceptions = true;
    }
    malformed.push(("embedded rounding and SAE", static_rounding));

    let mut missing_hint = base.clone();
    missing_hint.blocks[0].ops[1].x86_hint = None;
    malformed.push(("missing VEX hint", missing_hint));

    for (name, hint) in [
        (
            "wrong VEX map",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0x51,
                width: VecWidth::V256,
                w: true,
            },
        ),
        (
            "wrong mandatory prefix",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x51,
                width: VecWidth::V256,
                w: true,
            },
        ),
        (
            "wrong opcode",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x52,
                width: VecWidth::V256,
                w: true,
            },
        ),
        (
            "wrong hinted width",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x51,
                width: VecWidth::V128,
                w: true,
            },
        ),
    ] {
        let mut function = base.clone();
        function.blocks[0].ops[1].x86_hint = Some(hint);
        malformed.push((name, function));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xF103),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xF103)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("same-PC tail", same_pc_tail));

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction-byte provenance", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x01),
        ("encoded L", 2, 0x04),
        ("encoded W", 2, 0x80),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("nonreserved packed vvvv", 2, 0x08),
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
    malformed.push(("encoded register source", register_source));

    let mut trailing = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    replace_instruction_bytes(&mut trailing, &bytes);
    malformed.push(("trailing source byte", trailing));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn scalar_graph_and_source_byte_provenance_fail_closed_for_every_invariant() {
    let case = SqrtMemoryCase {
        kind: SqrtKind::ScalarF32,
        width: VecWidth::V128,
        form: EncodingForm::C4W1,
        destination: 9,
        source1: 10,
        base: 11,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert_exact_sequence(&base, case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let source_vector = match base.blocks[0].ops[1].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let sqrt_result = match base.blocks[0].ops[2].kind {
        OpKind::X86Sqrt { dst, .. } => dst,
        _ => unreachable!(),
    };
    let scalar_result = match base.blocks[0].ops[3].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let upper_scalar = match base.blocks[0].ops[4].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let zero = match base.blocks[0].ops[7].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let destination = vector(case.destination, VecWidth::V128);
    let mut malformed = Vec::new();

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x51,
        width: VecWidth::V128,
        w: true,
    });
    malformed.push(("scalar load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("scalar load footprint", load_width));

    let mut load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("scalar load extension", load_sign));

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xF200)));
    }
    malformed.push(("virtual scalar address", virtual_address));

    let mut source_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut source_broadcast_scalar.blocks[0].ops[1].kind {
        *scalar = source_vector;
    }
    malformed.push(("source broadcast scalar", source_broadcast_scalar));

    let mut source_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut source_broadcast_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("source broadcast element", source_broadcast_element));

    let mut source_broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut source_broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(("source broadcast lanes", source_broadcast_lanes));

    let mut sqrt_source = base.clone();
    if let OpKind::X86Sqrt { src, .. } = &mut sqrt_source.blocks[0].ops[2].kind {
        *src = loaded;
    }
    malformed.push(("square-root source", sqrt_source));

    let mut sqrt_element = base.clone();
    if let OpKind::X86Sqrt { elem, .. } = &mut sqrt_element.blocks[0].ops[2].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("square-root element", sqrt_element));

    let mut sqrt_lanes = base.clone();
    if let OpKind::X86Sqrt { lanes, .. } = &mut sqrt_lanes.blocks[0].ops[2].kind {
        *lanes = 2;
    }
    malformed.push(("square-root lanes", sqrt_lanes));

    let mut sqrt_rounding = base.clone();
    if let OpKind::X86Sqrt {
        round,
        suppress_exceptions,
        ..
    } = &mut sqrt_rounding.blocks[0].ops[2].kind
    {
        *round = FpRoundMode::RoundTowardZero;
        *suppress_exceptions = true;
    }
    malformed.push(("square-root rounding/SAE", sqrt_rounding));

    let mut low_extract_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut low_extract_source.blocks[0].ops[3].kind {
        *vec = source_vector;
    }
    malformed.push(("low extraction source", low_extract_source));

    let mut low_extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut low_extract_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }
    malformed.push(("low extraction lane", low_extract_lane));

    let mut low_extract_sign = base.clone();
    if let OpKind::VExtractLane { sign, .. } = &mut low_extract_sign.blocks[0].ops[3].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("low extraction extension", low_extract_sign));

    let mut upper_extract_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut upper_extract_source.blocks[0].ops[4].kind {
        *vec = vector(11, VecWidth::V128);
    }
    malformed.push(("inconsistent scalar merge source", upper_extract_source));

    let mut upper_extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut upper_extract_lane.blocks[0].ops[4].kind {
        *lane = 2;
    }
    malformed.push(("upper extraction lane", upper_extract_lane));

    let mut upper_extract_hint = base.clone();
    upper_extract_hint.blocks[0].ops[4].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    malformed.push(("upper extraction hint", upper_extract_hint));

    let mut zero_value = base.clone();
    if let OpKind::Mov { src, .. } = &mut zero_value.blocks[0].ops[7].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("destination clear value", zero_value));

    let mut zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut zero_width.blocks[0].ops[7].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("destination clear width", zero_width));

    let mut destination_broadcast = base.clone();
    if let OpKind::VBroadcast { dst, .. } = &mut destination_broadcast.blocks[0].ops[8].kind {
        *dst = vector(8, VecWidth::V128);
    }
    malformed.push(("destination broadcast register", destination_broadcast));

    let mut destination_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut destination_scalar.blocks[0].ops[8].kind {
        *scalar = loaded;
    }
    malformed.push(("destination broadcast scalar", destination_scalar));

    let mut destination_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut destination_element.blocks[0].ops[8].kind {
        *elem = VecElementType::F64;
    }
    malformed.push(("destination broadcast element", destination_element));

    let mut low_insert_vector = base.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut low_insert_vector.blocks[0].ops[9].kind {
        *vec = vector(8, VecWidth::V128);
    }
    malformed.push(("low insert vector", low_insert_vector));

    let mut low_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut low_insert_scalar.blocks[0].ops[9].kind {
        *scalar = upper_scalar;
    }
    malformed.push(("low insert scalar", low_insert_scalar));

    let mut upper_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut upper_insert_scalar.blocks[0].ops[10].kind {
        *scalar = scalar_result;
    }
    malformed.push(("upper insert scalar", upper_insert_scalar));

    let mut upper_insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut upper_insert_lane.blocks[0].ops[10].kind {
        *lane = 2;
    }
    malformed.push(("upper insert lane", upper_insert_lane));

    let mut hidden_extra_use = base.clone();
    hidden_extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xF201),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xF201)),
            src: SrcOperand::Reg(sqrt_result),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("hidden scalar used twice", hidden_extra_use));

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[8].guest_pc += 1;
    malformed.push(("split scalar guest PC", split_pc));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xF202),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xF202)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("same-PC scalar tail", same_pc_tail));

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing scalar source bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x01),
        ("unpredictable encoded scalar L=1", 2, 0x04),
        ("encoded W", 2, 0x80),
        ("encoded opcode", 3, 0x01),
        ("encoded destination", 4, 0x08),
        ("encoded scalar source1", 2, 0x08),
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
    malformed.push(("encoded scalar register source", register_source));

    let mut trailing = base.clone();
    let mut bytes = case.bytes();
    bytes.push(0);
    replace_instruction_bytes(&mut trailing, &bytes);
    malformed.push(("trailing scalar source byte", trailing));

    // Keep explicit bindings live so mutations above exercise their intended
    // single-definition/single-use roles rather than dead local scaffolding.
    assert!(matches!(loaded, VReg::Virtual(_)));
    assert!(matches!(source_vector, VReg::Virtual(_)));
    assert!(matches!(sqrt_result, VReg::Virtual(_)));
    assert!(matches!(scalar_result, VReg::Virtual(_)));
    assert!(matches!(upper_scalar, VReg::Virtual(_)));
    assert!(matches!(zero, VReg::Virtual(_)));
    assert!(matches!(destination, VReg::Arch(_)));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn scalar_vex_l1_lifts_but_stays_at_the_generation_dependent_frontier() {
    for kind in [SqrtKind::ScalarF32, SqrtKind::ScalarF64] {
        for form in EncodingForm::ALL {
            let case = SqrtMemoryCase {
                kind,
                width: VecWidth::V128,
                form,
                destination: 5,
                source1: 6,
                base: if matches!(form, EncodingForm::C5) {
                    3
                } else {
                    11
                },
            };
            let mut bytes = case.bytes();
            let p1 = if matches!(form, EncodingForm::C5) {
                1
            } else {
                2
            };
            bytes[p1] |= 0x04;
            for level in LEVELS {
                let function = optimize(lift_bytes(&bytes), level);
                assert!(
                    !function.blocks[0].ops.is_empty(),
                    "{level:?} {kind:?} {form:?}: scalar L=1 did not lift"
                );
                assert_rejected("scalar VEX.L=1", &function);
            }
        }
    }
}
