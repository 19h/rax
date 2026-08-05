//! Exact helper-backed EVEX `VP2INTERSECTD/Q` memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecCmpCond, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexVp2IntersectMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexVp2IntersectMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_vp2intersect_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7EC0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Vp2IntersectMemoryCase {
    pub(super) width: VecWidth,
    pub(super) elem: VecElementType,
    /// Raw ModR/M.reg value; the architectural pair clears bit zero.
    pub(super) destination: u8,
    pub(super) source1: u8,
    pub(super) broadcast: bool,
}

impl Vp2IntersectMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn w(self) -> bool {
        match self.elem {
            VecElementType::I32 => false,
            VecElementType::I64 => true,
            _ => unreachable!(),
        }
    }

    const fn destination_base(self) -> u8 {
        self.destination & !1
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.source1)
            .expect("one source leaves a low vector scratch")
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn register_replay(self) -> Vec<u8> {
        register_encoding(self, self.scratch())
    }

    fn stack_replay(self) -> Vec<u8> {
        vec![
            0x62,
            0xF2,
            (u8::from(self.w()) << 7) | (((!self.source1) & 0x0F) << 3) | 0x07,
            (self.ll() << 5) | (u8::from(self.broadcast) << 4) | (u8::from(self.source1 < 16) << 3),
            0x68,
            (self.destination << 3) | 0x04,
            0x24,
        ]
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast {
            self.stack_replay()
        } else {
            self.register_replay()
        }
    }
}

fn memory_encoding(case: Vp2IntersectMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 8 && case.source1 < 32);
    let p1 = (u8::from(case.w()) << 7) | (((!case.source1) & 0x0F) << 3) | 0x07;
    let p2 =
        (case.ll() << 5) | (u8::from(case.broadcast) << 4) | (u8::from(case.source1 < 16) << 3);
    let mut bytes = vec![
        0x62,
        0xF2,
        p1,
        p2,
        0x68,
        (case.destination << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        bytes.push(0x48); // [RAX + RCX*2]
    }
    bytes
}

fn register_encoding(case: Vp2IntersectMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0xD2 | if scratch & 8 == 0 { 0x20 } else { 0 };
    vec![
        0x62,
        p0,
        (u8::from(case.w()) << 7) | (((!case.source1) & 0x0F) << 3) | 0x07,
        (case.ll() << 5) | (u8::from(case.source1 < 16) << 3),
        0x68,
        0xC0 | (case.destination << 3) | scratch,
    ]
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
        X86InstructionBytes::new(bytes).expect("EVEX VP2INTERSECT provenance"),
    );
    function
}

fn lift_case(case: Vp2IntersectMemoryCase) -> SmirFunction {
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexVp2IntersectMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_vp2intersect_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: Vp2IntersectMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert!(requirements.needs_avx512vp2intersect, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512vp2intersect")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: VP2INTERSECT lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize EVEX VP2INTERSECT"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<Vp2IntersectMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for source1 in [0, 1, 15] {
                for broadcast in [false, true] {
                    cases.push(Vp2IntersectMemoryCase {
                        width,
                        elem,
                        destination: 0,
                        source1,
                        broadcast,
                    });
                }
            }
        }
    }
    cases
}

fn expected_op_count(case: Vp2IntersectMemoryCase, level: OptLevel) -> usize {
    let lanes = case.width.lanes(case.elem) as usize;
    let memory_ops = if case.broadcast { 2 } else { 1 };
    memory_ops + 3 + lanes * (4 * lanes + 7 + usize::from(level == OptLevel::O0)) + 2
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer admitted malformed graph"
    );
}

#[test]
fn rewrites_match_eight_independent_llvm_23_anchors() {
    // Generated with LLVM 23 llvm-mc `-triple=x86_64
    // -x86-asm-syntax=intel -show-encoding`; `{evex}` forces VL=128/256.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0x77, 0x00, 0x68, 0x12],
            &[0x62, 0xF2, 0x77, 0x00, 0x68, 0xD0],
        ),
        (
            &[0x62, 0xD2, 0x77, 0x20, 0x68, 0x52, 0x02],
            &[0x62, 0xF2, 0x77, 0x20, 0x68, 0xD0],
        ),
        (
            &[0x62, 0xD2, 0x77, 0x40, 0x68, 0x51, 0xFE],
            &[0x62, 0xF2, 0x77, 0x40, 0x68, 0xD0],
        ),
        (
            &[0x62, 0xF2, 0x77, 0x50, 0x68, 0x54, 0x44, 0x20],
            &[0x62, 0xF2, 0x77, 0x50, 0x68, 0x14, 0x24],
        ),
        (
            &[0x62, 0xF2, 0xE7, 0x00, 0x68, 0x22],
            &[0x62, 0xF2, 0xE7, 0x00, 0x68, 0xE0],
        ),
        (
            &[0x62, 0xD2, 0xE7, 0x20, 0x68, 0x62, 0x02],
            &[0x62, 0xF2, 0xE7, 0x20, 0x68, 0xE0],
        ),
        (
            &[0x62, 0xD2, 0xE7, 0x40, 0x68, 0x61, 0xFE],
            &[0x62, 0xF2, 0xE7, 0x40, 0x68, 0xE0],
        ),
        (
            &[0x62, 0xF2, 0xE7, 0x50, 0x68, 0x64, 0x44, 0x10],
            &[0x62, 0xF2, 0xE7, 0x50, 0x68, 0x24, 0x24],
        ),
    ];
    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_vp2intersect_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let actual = match encoding.replay {
            X86EvexVp2IntersectMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexVp2IntersectMemoryReplay::Broadcast {
                stack_instruction, ..
            } => stack_instruction,
        };
        assert_eq!(actual.as_slice(), *expected, "{memory:02X?}");
    }
}

#[test]
fn classifier_exhausts_all_3072_operand_width_element_and_tuple_cells() {
    let mut classified = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for destination in 0..8 {
                for source1 in 0..32 {
                    for broadcast in [false, true] {
                        let case = Vp2IntersectMemoryCase {
                            width,
                            elem,
                            destination,
                            source1,
                            broadcast,
                        };
                        let bytes = case.bytes();
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_vp2intersect_memory_encoding()
                            .unwrap_or_else(|| panic!("{case:?}: {bytes:02X?}"));
                        assert_eq!(encoding.width, width, "{case:?}");
                        assert_eq!(encoding.elem, elem, "{case:?}");
                        assert_eq!(encoding.destination_base, destination & !1, "{case:?}");
                        assert_eq!(encoding.source1, source1, "{case:?}");
                        assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                        let actual = match encoding.replay {
                            X86EvexVp2IntersectMemoryReplay::Vector {
                                scratch,
                                register_instruction,
                            } => {
                                assert!(!broadcast, "{case:?}");
                                assert_ne!(scratch, source1, "{case:?}");
                                register_instruction
                            }
                            X86EvexVp2IntersectMemoryReplay::Broadcast {
                                memory_width,
                                stack_instruction,
                            } => {
                                assert!(broadcast, "{case:?}");
                                assert_eq!(memory_width.bytes(), elem.bytes() as u32, "{case:?}");
                                stack_instruction
                            }
                        };
                        assert_eq!(actual.as_slice(), case.expected_replay(), "{case:?}");
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 3 * 2 * 8 * 32 * 2);
}

#[test]
fn all_36_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 36);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.elem);
            assert_eq!(exact.encoding.destination_base, case.destination_base());
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.memory_size,
                if case.broadcast {
                    case.elem.bytes() as u32
                } else {
                    case.width.bytes()
                }
            );
            assert_eq!(
                exact.consumed,
                expected_op_count(case, level),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.consumed, function.blocks[0].ops.len());

            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 36 * LEVELS.len());
}

#[test]
fn every_graph_retains_one_unconditional_exact_tuple_read() {
    let mut checks = 0usize;
    for case in scanner_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let reads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| op.kind.reads_memory())
                .collect::<Vec<_>>();
            assert_eq!(reads.len(), 1, "{level:?} {case:?}");
            match (&reads[0].kind, case.broadcast) {
                (
                    OpKind::Load {
                        width,
                        sign: crate::smir::ir::types::SignExtend::Zero,
                        ..
                    },
                    true,
                ) => assert_eq!(
                    width.bytes(),
                    case.elem.bytes() as u32,
                    "{level:?} {case:?}"
                ),
                (OpKind::VLoad { width, .. }, false) => {
                    assert_eq!(*width, case.width, "{level:?} {case:?}")
                }
                _ => panic!("{level:?} {case:?}: unexpected tuple read {:#?}", reads[0]),
            }
            checks += 1;
        }
    }
    assert_eq!(checks, 36 * LEVELS.len());
}

#[test]
fn avx_ymm16_only_bridge_rejects_both_memory_replay_forms() {
    for broadcast in [false, true] {
        let case = Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I64,
            destination: 7,
            source1: 31,
            broadcast,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_preserve_vector_mem_helpers(true);
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer.set_jit_fault_deopt_guards(true);
        let error = lowerer
            .lower_function(&function)
            .expect_err("AVX-only bridge must reject VP2INTERSECT");
        assert!(format!("{error:?}").contains("VP2INTERSECT"), "{error:?}");
    }
}

#[test]
fn classifier_rejects_all_reserved_structural_and_length_frontiers() {
    let case = Vp2IntersectMemoryCase {
        width: VecWidth::V256,
        elem: VecElementType::I64,
        destination: 3,
        source1: 27,
        broadcast: true,
    };
    let valid = case.bytes();
    let mut cases = Vec::new();
    for (index, mask) in [
        (1, 0x01), // wrong map
        (1, 0x10), // reserved K destination extension
        (1, 0x80), // reserved K destination extension
        (2, 0x03), // wrong mandatory prefix
        (3, 0x01), // reserved aaa
        (3, 0x80), // reserved z
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        cases.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    cases.push(reserved_ll);
    let mut wrong_opcode = valid.clone();
    wrong_opcode[4] ^= 1;
    cases.push(wrong_opcode);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    cases.push(register);
    let mut trailing = valid.clone();
    trailing.push(0);
    cases.push(trailing);
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    cases.push(forbidden_prefix);
    let mut truncated = valid;
    truncated.pop();
    cases.push(truncated);

    for bytes in cases {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_vp2intersect_memory_encoding(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn segment_addr32_sib_rip_displacements_and_apx_addresses_admit() {
    for broadcast in [false, true] {
        let case = Vp2IntersectMemoryCase {
            width: VecWidth::V512,
            elem: VecElementType::I64,
            destination: 7,
            source1: 26,
            broadcast,
        };
        let direct = case.bytes();
        let mut forms = Vec::new();
        for prefix in [0x64, 0x65, 0x67] {
            let mut bytes = vec![prefix];
            bytes.extend_from_slice(&direct);
            forms.push(bytes);
        }
        let mut fs_addr32 = vec![0x64, 0x67];
        fs_addr32.extend_from_slice(&direct);
        forms.push(fs_addr32);
        forms.push(memory_encoding(case, true));

        let mut disp8 = direct.clone();
        disp8[5] |= 0x40;
        disp8.push(0xFE);
        forms.push(disp8);
        let mut disp32 = direct.clone();
        disp32[5] |= 0x80;
        disp32.extend_from_slice(&0x4433_2211u32.to_le_bytes());
        forms.push(disp32);
        let mut rip_relative = direct.clone();
        rip_relative[5] = (rip_relative[5] & 0x38) | 5;
        rip_relative.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        forms.push(rip_relative);
        let mut absolute = direct.clone();
        absolute[5] = (absolute[5] & 0x38) | 4;
        absolute.extend_from_slice(&[0x25, 0x11, 0x22, 0x33, 0x44]);
        forms.push(absolute);
        let mut apx_base = direct.clone();
        apx_base[1] |= 0x08;
        forms.push(apx_base);
        let mut apx_sib = memory_encoding(case, true);
        apx_sib[1] |= 0x08;
        apx_sib[2] &= !0x04;
        forms.push(apx_sib);

        let mut admissions = 0usize;
        for bytes in forms {
            for level in LEVELS {
                let function = optimize(lift_bytes(&bytes), level);
                let exact = sequence(&function, true).unwrap_or_else(|| {
                    panic!("{level:?} {bytes:02X?}: {:#?}", function.blocks[0].ops)
                });
                assert_eq!(exact.encoding.width, case.width);
                let (code, _) = lower(&function, case);
                let replay = case.expected_replay();
                assert!(code.windows(replay.len()).any(|window| window == replay));
                admissions += 1;
            }
        }
        assert_eq!(admissions, 11 * LEVELS.len());
    }
}

#[test]
fn sequence_fails_closed_for_graph_provenance_frontier_and_ssa_mutations() {
    let case = Vp2IntersectMemoryCase {
        width: VecWidth::V128,
        elem: VecElementType::I32,
        destination: 3,
        source1: 17,
        broadcast: false,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());
    let loaded = match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut mutations = Vec::<(&str, SmirFunction)>::new();

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_provenance = function.clone();
    let mut bytes = case.bytes();
    bytes[5] ^= 0x10;
    wrong_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("destination provenance", wrong_provenance));

    let mut hinted_load = function.clone();
    hinted_load.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    mutations.push(("load hint", hinted_load));

    let mut wrong_address = function.clone();
    let OpKind::VLoad { addr, .. } = &mut wrong_address.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *addr = Address::GpRel { offset: 0 };
    mutations.push(("address shape", wrong_address));

    let mut wrong_width = function.clone();
    let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = VecWidth::V256;
    mutations.push(("load width", wrong_width));

    let compare_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VCmp { .. }))
        .unwrap();
    let mut wrong_compare = function.clone();
    let OpKind::VCmp { cond, .. } = &mut wrong_compare.blocks[0].ops[compare_index].kind else {
        unreachable!()
    };
    *cond = VecCmpCond::Ne;
    mutations.push(("comparison condition", wrong_compare));

    let shr_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Shr { .. }))
        .unwrap();
    let mut wrong_shift = function.clone();
    let OpKind::Shr { amount, .. } = &mut wrong_shift.blocks[0].ops[shr_index].kind else {
        unreachable!()
    };
    *amount = SrcOperand::Imm(30);
    mutations.push(("movemask sign bit", wrong_shift));

    let mut wrong_source1 = function.clone();
    let source_extract = wrong_source1.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(_),
                    ..
                }
            )
        })
        .unwrap();
    let OpKind::VExtractLane { vec, .. } = &mut source_extract.kind else {
        unreachable!()
    };
    *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)));
    mutations.push(("source1", wrong_source1));

    let mut wrong_commit = function.clone();
    let commit = wrong_commit.blocks[0].ops.len() - 2;
    let OpKind::Mov { dst, .. } = &mut wrong_commit.blocks[0].ops[commit].kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::K(0)));
    mutations.push(("first K commit", wrong_commit));

    let mut duplicate_fresh = function.clone();
    let OpKind::Mov { dst, .. } = &mut duplicate_fresh.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *dst = loaded;
    mutations.push(("fresh virtual alias", duplicate_fresh));

    let mut hinted_graph = function.clone();
    hinted_graph.blocks[0].ops[compare_index].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    mutations.push(("graph hint", hinted_graph));

    let mut wrong_pc = function.clone();
    wrong_pc.blocks[0].ops[compare_index].guest_pc = PC + 1;
    mutations.push(("graph PC", wrong_pc));

    let mut following_same_pc = function.clone();
    following_same_pc.blocks[0].ops.push(SmirOp::new(
        OpId(0x7000),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0x7000)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC tail", following_same_pc));

    let mut extra_use = function.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7001),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0x7001)),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("extra virtual use", extra_use));

    let mut duplicate_definition = function.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7002),
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("duplicate virtual definition", duplicate_definition));

    let mut unexpected_apx_guard = function.clone();
    unexpected_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7003), PC, OpKind::X86RequireApx));
    mutations.push(("unexpected APX guard", unexpected_apx_guard));

    let mut preceding_same_pc = function.clone();
    preceding_same_pc.blocks[0].ops.insert(
        0,
        SmirOp::new(
            OpId(0x7004),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0x7004)),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ),
    );
    mutations.push(("same-PC predecessor", preceding_same_pc));

    let mut apx_bytes = case.bytes();
    apx_bytes[1] |= 0x08;
    let mut missing_apx_guard = optimize(lift_bytes(&apx_bytes), OptLevel::O2);
    assert!(matches!(
        missing_apx_guard.blocks[0].ops[0].kind,
        OpKind::X86RequireApx
    ));
    missing_apx_guard.blocks[0].ops.remove(0);
    mutations.push(("missing APX guard", missing_apx_guard));

    for (name, mutation) in mutations {
        assert_rejected(name, &mutation);
    }
}
