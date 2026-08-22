//! Exact helper-backed EVEX VFPCLASS* memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::SourceArch;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexFpClassMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexFpClassMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_fp_class_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xB700;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
    Scalar { ll: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FpClassMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    form: SourceForm,
    mask: u8,
    immediate: u8,
}

impl FpClassMemoryCase {
    fn scalar(self) -> bool {
        matches!(self.form, SourceForm::Scalar { .. })
    }

    fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    fn ll(self) -> u8 {
        match self.form {
            SourceForm::Scalar { ll } => ll,
            SourceForm::Vector | SourceForm::Broadcast => match self.width {
                VecWidth::V128 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
                _ => unreachable!("VFPCLASS vector width"),
            },
        }
    }

    fn memory_size(self) -> u32 {
        if self.scalar() || self.broadcast() {
            self.elem.bytes()
        } else {
            self.width.bytes()
        }
    }

    fn needs_avx512vl(self) -> bool {
        !self.scalar() && self.width != VecWidth::V512
    }

    fn p1(self) -> u8 {
        match self.elem {
            VecElementType::F16 => 0x7C,
            VecElementType::F32 => 0x7D,
            VecElementType::F64 => 0xFD,
            _ => unreachable!("VFPCLASS binary16/binary32/binary64 element"),
        }
    }

    fn p2(self) -> u8 {
        (self.ll() << 5) | (u8::from(self.broadcast()) << 4) | 0x08 | self.mask
    }

    fn opcode(self) -> u8 {
        if self.scalar() { 0x67 } else { 0x66 }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 8 && self.mask < 8);
        assert!(!self.broadcast() || !self.scalar());
        assert!(self.scalar() || self.ll() < 3);
        vec![
            0x62,
            0xF3,
            self.p1(),
            self.p2(),
            self.opcode(),
            (self.destination << 3) | 0x02,
            self.immediate,
        ]
    }

    fn expected_replay(self) -> Vec<u8> {
        let replay_p2 = if self.scalar() {
            self.p2() & !0x60
        } else {
            self.p2()
        };
        if !self.scalar() && !self.broadcast() && self.mask == 0 {
            vec![
                0x62,
                0xF3,
                self.p1(),
                replay_p2,
                self.opcode(),
                0xC0 | (self.destination << 3),
                self.immediate,
            ]
        } else {
            vec![
                0x62,
                0xF3,
                self.p1(),
                replay_p2,
                self.opcode(),
                (self.destination << 3) | 0x04,
                0x24,
                self.immediate,
            ]
        }
    }
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
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
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("VFPCLASS instruction metadata"),
    );
    function
}

fn lift_case(case: FpClassMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
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

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexFpClassMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexFpClassMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_fp_class_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: FpClassMemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(requirements.needs_avx512vl, case.needs_avx512vl());
    assert_eq!(
        requirements.needs_avx512dq,
        case.elem != VecElementType::F16
    );
    assert_eq!(
        requirements.needs_avx512fp16,
        case.elem == VecElementType::F16
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.elem == VecElementType::F16 || std::is_x86_feature_detected!("avx512dq"))
            && (case.elem != VecElementType::F16 || std::is_x86_feature_detected!("avx512fp16"))
            && (!case.needs_avx512vl() || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(
        !x86_native_vector_features_supported_excluding(function, &excluded),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: VFPCLASS memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VFPCLASS memory"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<FpClassMemoryCase> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for mask in [0, 1] {
                    cases.push(FpClassMemoryCase {
                        elem,
                        width,
                        destination: 1,
                        form,
                        mask,
                        immediate: 0,
                    });
                }
            }
        }
        for ll in 0..=2 {
            for mask in [0, 1] {
                cases.push(FpClassMemoryCase {
                    elem,
                    width: VecWidth::V128,
                    destination: 1,
                    form: SourceForm::Scalar { ll },
                    mask,
                    immediate: 0,
                });
            }
        }
    }
    assert_eq!(cases.len(), 54);
    cases
}

fn replay_instruction(case: FpClassMemoryCase) -> Vec<u8> {
    let encoding = X86InstructionBytes::new(&case.bytes())
        .unwrap()
        .evex_fp_class_memory_encoding()
        .unwrap_or_else(|| panic!("{case:?}"));
    replay_from_encoding(encoding)
}

fn replay_from_encoding(encoding: crate::smir::ir::X86EvexFpClassMemoryEncoding) -> Vec<u8> {
    match encoding.replay {
        X86EvexFpClassMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexFpClassMemoryReplay::Broadcast { stack_instruction }
        | X86EvexFpClassMemoryReplay::MaskedVector { stack_instruction }
        | X86EvexFpClassMemoryReplay::Scalar { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
        }
    }
}

#[test]
fn byte_anchors_cover_all_six_mnemonics_and_replay_strategies() {
    let anchors: [(FpClassMemoryCase, &[u8]); 6] = [
        (
            FpClassMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V512,
                destination: 1,
                form: SourceForm::Vector,
                mask: 0,
                immediate: 0xFF,
            },
            &[0x62, 0xF3, 0x7C, 0x48, 0x66, 0xC8, 0xFF],
        ),
        (
            FpClassMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V256,
                destination: 1,
                form: SourceForm::Broadcast,
                mask: 1,
                immediate: 0xA5,
            },
            &[0x62, 0xF3, 0x7D, 0x39, 0x66, 0x0C, 0x24, 0xA5],
        ),
        (
            FpClassMemoryCase {
                elem: VecElementType::F64,
                width: VecWidth::V128,
                destination: 1,
                form: SourceForm::Vector,
                mask: 1,
                immediate: 0x01,
            },
            &[0x62, 0xF3, 0xFD, 0x09, 0x66, 0x0C, 0x24, 0x01],
        ),
        (
            FpClassMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V128,
                destination: 1,
                form: SourceForm::Scalar { ll: 3 },
                mask: 1,
                immediate: 0xFF,
            },
            &[0x62, 0xF3, 0x7C, 0x09, 0x67, 0x0C, 0x24, 0xFF],
        ),
        (
            FpClassMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V128,
                destination: 1,
                form: SourceForm::Scalar { ll: 0 },
                mask: 0,
                immediate: 0,
            },
            &[0x62, 0xF3, 0x7D, 0x08, 0x67, 0x0C, 0x24, 0x00],
        ),
        (
            FpClassMemoryCase {
                elem: VecElementType::F64,
                width: VecWidth::V128,
                destination: 1,
                form: SourceForm::Scalar { ll: 2 },
                mask: 1,
                immediate: 1,
            },
            &[0x62, 0xF3, 0xFD, 0x09, 0x67, 0x0C, 0x24, 0x01],
        ),
    ];
    for (case, expected) in anchors {
        assert_eq!(replay_instruction(case), expected, "{case:?}");
    }
}

#[test]
fn all_54_scanner_memory_cells_match_and_lower_at_o0_o1_o2() {
    let mut lowered = 0usize;
    for case in scanner_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(exact.encoding.elem, case.elem, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.encoding.writemask,
                (case.mask != 0).then_some(case.mask),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.immediate, case.immediate);
            assert_eq!(exact.encoding.scalar, case.scalar());
            assert_eq!(exact.memory_size, case.memory_size());
            assert_eq!(
                exact.consumed + sequence_index(&function),
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let address = &function.blocks[0].ops[sequence_index(&function) + exact.address_offset];
            assert!(
                matches!(
                    address.kind,
                    OpKind::Load { .. }
                        | OpKind::PredLoad { .. }
                        | OpKind::VLoad { .. }
                        | OpKind::Lea { .. }
                ),
                "{level:?} {case:?}: {:?}",
                address.kind
            );

            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 54 * LEVELS.len());
}

#[test]
fn every_immediate_matches_full_and_live_graph_profiles() {
    let mut admitted = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for immediate in u8::MIN..=u8::MAX {
            for case in [
                FpClassMemoryCase {
                    elem,
                    width: VecWidth::V128,
                    destination: 7,
                    form: SourceForm::Broadcast,
                    mask: 1,
                    immediate,
                },
                FpClassMemoryCase {
                    elem,
                    width: VecWidth::V128,
                    destination: 7,
                    form: SourceForm::Scalar { ll: 3 },
                    mask: 0,
                    immediate,
                },
            ] {
                let base = lift_case(case);
                for level in LEVELS {
                    let function = optimize(base.clone(), level);
                    let exact = sequence(&function, true).unwrap_or_else(|| {
                        panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops)
                    });
                    assert_eq!(exact.encoding.immediate, immediate);
                    assert_eq!(
                        exact.consumed,
                        function.blocks[0].ops.len(),
                        "{level:?} {case:?}"
                    );
                    admitted += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 3 * 256 * 2 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact VFPCLASS sequence admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native clobber gate admitted malformed graph"
    );
}

#[test]
fn exact_sequence_fails_closed_for_provenance_graph_frontier_and_ssa_mutations() {
    let case = FpClassMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V256,
        destination: 7,
        form: SourceForm::Broadcast,
        mask: 1,
        immediate: 0xA5,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    let mut wrong_provenance = function.clone();
    let mut wrong_bytes = case.bytes();
    *wrong_bytes.last_mut().unwrap() ^= 1;
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );
    assert_rejected("wrong immediate provenance", &wrong_provenance);

    let mut wrong_boolean = function.clone();
    let boolean = wrong_boolean.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VAndNot { .. }))
        .expect("live VFPCLASS VANDN");
    let replacement = match boolean.kind {
        OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        } => OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        },
        _ => unreachable!(),
    };
    boolean.kind = replacement;
    assert_rejected("wrong Boolean operation", &wrong_boolean);

    let mut wrong_daz_compare = function.clone();
    let compare = wrong_daz_compare.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::X86VectorFpCompare { .. }))
        .expect("binary64 DAZ-aware zero comparison");
    let OpKind::X86VectorFpCompare {
        suppress_exceptions,
        ..
    } = &mut compare.kind
    else {
        unreachable!()
    };
    *suppress_exceptions = false;
    assert_rejected("wrong DAZ comparison exception mode", &wrong_daz_compare);

    let mut wrong_movemask_shift = function.clone();
    let shift = wrong_movemask_shift.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::Shr { .. }))
        .expect("decomposed movemask shift");
    let OpKind::Shr { amount, .. } = &mut shift.kind else {
        unreachable!()
    };
    let SrcOperand::Imm(amount) = amount else {
        unreachable!()
    };
    *amount += 1;
    assert_rejected("wrong movemask sign shift", &wrong_movemask_shift);

    let mut wrong_commit = function.clone();
    let commit = wrong_commit.blocks[0].ops.last_mut().expect("K commit");
    let OpKind::And { dst, .. } = &mut commit.kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::K(6)));
    assert_rejected("wrong K destination commit", &wrong_commit);

    let mut hinted = function.clone();
    let exact = sequence(&hinted, true).unwrap();
    hinted.blocks[0].ops[exact.address_offset].x86_hint = Some(X86OpHint::MovImmModRm);
    assert_rejected("hinted helper address", &hinted);

    let mut wrong_pc = function.clone();
    wrong_pc.blocks[0].ops[1].guest_pc = PC + 1;
    assert_rejected("split guest-PC graph", &wrong_pc);

    let memory_source = function.blocks[0]
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::X86VectorFpCompare { src1, .. } => Some(src1),
            _ => None,
        })
        .expect("memory-source virtual register");
    let mut extra_use = function.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFE),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: SrcOperand::Reg(memory_source),
            width: OpWidth::W64,
        },
    ));
    assert_rejected("extra memory-source SSA use", &extra_use);

    let mut same_pc_tail = function.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFF)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    assert_rejected("same-PC tail", &same_pc_tail);
}

#[test]
fn fault_only_live_graphs_fail_closed_for_load_address_predicate_and_ssa_mutations() {
    for form in [
        SourceForm::Vector,
        SourceForm::Broadcast,
        SourceForm::Scalar { ll: 3 },
    ] {
        for mask in [0, 1] {
            if form == SourceForm::Vector && mask == 0 {
                // The common exact VLoad matcher owns this shape; exercise
                // the fault-only scalar and predicated-load variants here.
                continue;
            }
            let case = FpClassMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V128,
                destination: 7,
                form,
                mask,
                immediate: 0,
            };
            let function = optimize(lift_case(case), OptLevel::O2);
            sequence(&function, true)
                .unwrap_or_else(|| panic!("fault-only graph not admitted: {case:?}"));

            let load_index = function.blocks[0]
                .ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
                .expect("fault-only load");
            let loaded = match function.blocks[0].ops[load_index].kind {
                OpKind::Load { dst, .. } | OpKind::PredLoad { dst, .. } => dst,
                _ => unreachable!(),
            };

            let mut wrong_width = function.clone();
            match &mut wrong_width.blocks[0].ops[load_index].kind {
                OpKind::Load { width, .. } | OpKind::PredLoad { width, .. } => {
                    *width = crate::smir::ir::types::MemWidth::B4;
                }
                _ => unreachable!(),
            }
            assert_rejected("fault-only wrong load width", &wrong_width);

            let mut extra_use = function.clone();
            extra_use.blocks[0].ops.push(SmirOp::new(
                OpId(0xFFFC),
                PC + 1,
                OpKind::Mov {
                    dst: VReg::Virtual(VirtualId(0xFFFC)),
                    src: SrcOperand::Reg(loaded),
                    width: OpWidth::W64,
                },
            ));
            assert_rejected("fault-only loaded-value escape", &extra_use);

            if mask != 0 {
                let mut wrong_predicate = function.clone();
                let predload = wrong_predicate.blocks[0]
                    .ops
                    .iter_mut()
                    .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    .expect("masked fault-only PredLoad");
                let OpKind::PredLoad { cond, .. } = &mut predload.kind else {
                    unreachable!()
                };
                *cond = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
                assert_rejected("fault-only wrong predicate", &wrong_predicate);
            }

            if matches!(form, SourceForm::Vector | SourceForm::Broadcast) && mask != 0 {
                let mut wrong_address = function.clone();
                let predload = wrong_address.blocks[0]
                    .ops
                    .iter_mut()
                    .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    .expect("masked packed fault-only PredLoad");
                let OpKind::PredLoad { addr, .. } = &mut predload.kind else {
                    unreachable!()
                };
                let Address::BaseOffset { offset, .. } = addr else {
                    unreachable!()
                };
                *offset += i64::from(case.elem.bytes());
                assert_rejected("fault-only wrong lane address", &wrong_address);
            }
        }
    }
}

#[test]
fn prefixes_rip_relative_and_apx_r16_r17_addresses_remain_helper_owned() {
    let case = FpClassMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        destination: 1,
        form: SourceForm::Vector,
        mask: 1,
        immediate: 0xA5,
    };
    for prefixes in [&[0x64][..], &[0x65][..], &[0x67][..], &[0x64, 0x67][..]] {
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&case.bytes());
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
            lower(&function, case);
        }
    }

    let mut rip = case.bytes();
    let immediate = rip.pop().unwrap();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    rip.push(immediate);
    for level in LEVELS {
        let function = optimize(lift_bytes(&rip), level);
        assert!(function.blocks[0].ops.iter().any(|op| match &op.kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::VLoad { addr, .. }
            | OpKind::Lea { addr, .. } => matches!(addr, Address::PcRel { .. }),
            _ => false,
        }));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} RIP-relative"));
        lower(&function, case);
    }

    let apx = [0x62, 0xFB, 0x79, 0x09, 0x66, 0x4C, 0x48, 0x01, 0xA5];
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx), level);
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseIndexScale {
                    base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                    index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                    scale: 2,
                    disp: 16,
                    disp_size: DispSize::Disp8,
                },
                ..
            }
        )));
        let exact = sequence(&function, true).expect("APX-address VFPCLASS sequence");
        assert_eq!(exact.address_offset, 2, "{level:?}");
        assert_eq!(exact.encoding.destination, 1);
        assert_eq!(replay_from_encoding(exact.encoding), case.expected_replay());
        let (code, _) = lower(&function, case);
        let replay = case.expected_replay();
        assert!(code.windows(replay.len()).any(|window| window == replay));

        let mut missing_guard = function.clone();
        missing_guard.blocks[0].ops.remove(0);
        assert!(sequence_at(&missing_guard, 0, true).is_none());
    }
}

#[test]
fn lowering_shapes_guard_exact_lanes_and_reject_avx_only_bridge() {
    let masked_vector = FpClassMemoryCase {
        elem: VecElementType::F16,
        width: VecWidth::V512,
        destination: 1,
        form: SourceForm::Vector,
        mask: 1,
        immediate: 0xFF,
    };
    let function = optimize(lift_case(masked_vector), OptLevel::O2);
    let (code, _) = lower(&function, masked_vector);
    let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
    assert_eq!(
        code.windows(allocate_frame.len())
            .filter(|window| *window == allocate_frame)
            .count(),
        1
    );
    let load_k1 = [0xC4, 0xE1, 0xFB, 0x93, 0xC1];
    assert_eq!(
        code.windows(load_k1.len())
            .filter(|window| *window == load_k1)
            .count(),
        32,
        "one helper guard per binary16 lane"
    );

    for form in [SourceForm::Broadcast, SourceForm::Scalar { ll: 3 }] {
        let case = FpClassMemoryCase {
            form,
            ..masked_vector
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        assert_eq!(
            code.windows(load_k1.len())
                .filter(|window| *window == load_k1)
                .count(),
            1,
            "aggregate/scalar memory needs one K1 guard: {case:?}"
        );
    }

    let mut avx_only = X86_64Lowerer::new();
    avx_only.set_mem_helpers(true);
    avx_only.set_preserve_vector_mem_helpers(true);
    avx_only.set_avx_ymm16_vector_state(true);
    let error = avx_only
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject AVX-512 VFPCLASS replay");
    assert!(format!("{error:?}").contains("AVX-only vector bridge"));
}
