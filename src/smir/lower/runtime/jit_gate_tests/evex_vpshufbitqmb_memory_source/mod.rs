//! Exact helper-backed EVEX `VPSHUFBITQMB` memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SourceArch,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexVpshufbitqmbMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexVpshufbitqmbMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_vpshufbitqmb_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x8F00;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VpshufbitqmbMemoryCase {
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) source1: u8,
    /// Raw EVEX.aaa value: zero means no writemask, 1..=7 select K1..K7.
    pub(super) mask: u8,
}

impl VpshufbitqmbMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.source1)
            .expect("one source leaves a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.mask == 0 {
            register_encoding(self, self.scratch())
        } else {
            stack_encoding(self)
        }
    }
}

fn evex_fields(case: VpshufbitqmbMemoryCase) -> (u8, u8, u8) {
    assert!(case.destination < 8 && case.source1 < 32 && case.mask < 8);
    (
        0xF2,
        (((!case.source1) & 0x0F) << 3) | 0x05,
        (case.ll() << 5) | (u8::from(case.source1 < 16) << 3) | case.mask,
    )
}

fn memory_encoding(case: VpshufbitqmbMemoryCase, sib: bool) -> Vec<u8> {
    let (p0, p1, p2) = evex_fields(case);
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0x8F,
        (case.destination << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        bytes.push(0x48); // [RAX + RCX*2]
    }
    bytes
}

fn register_encoding(case: VpshufbitqmbMemoryCase, source2: u8) -> Vec<u8> {
    assert!(source2 < 16);
    let (_, p1, p2) = evex_fields(case);
    vec![
        0x62,
        0xD2 | if source2 & 8 == 0 { 0x20 } else { 0 },
        p1,
        p2,
        0x8F,
        0xC0 | (case.destination << 3) | (source2 & 7),
    ]
}

fn stack_encoding(case: VpshufbitqmbMemoryCase) -> Vec<u8> {
    let (_, p1, p2) = evex_fields(case);
    vec![
        0x62,
        0xF2,
        p1,
        p2,
        0x8F,
        (case.destination << 3) | 0x04,
        0x24,
    ]
}

fn replay_instruction(encoding: crate::smir::ir::X86EvexVpshufbitqmbMemoryEncoding) -> Vec<u8> {
    match encoding.replay {
        X86EvexVpshufbitqmbMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexVpshufbitqmbMemoryReplay::MaskedVector { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
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
        X86InstructionBytes::new(bytes).expect("VPSHUFBITQMB instruction provenance"),
    );
    function
}

fn lift_case(case: VpshufbitqmbMemoryCase) -> SmirFunction {
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexVpshufbitqmbMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_vpshufbitqmb_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: VpshufbitqmbMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx512bitalg, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
    assert!(!requirements.needs_gfni, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512bitalg")
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
        .unwrap_or_else(|error| panic!("{case:?}: VPSHUFBITQMB lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize VPSHUFBITQMB replay"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<VpshufbitqmbMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for source1 in [0, 1, 15] {
            for mask in [0, 1] {
                cases.push(VpshufbitqmbMemoryCase {
                    width,
                    destination: 0,
                    source1,
                    mask,
                });
            }
        }
    }
    cases
}

fn expected_op_count(case: VpshufbitqmbMemoryCase, level: OptLevel) -> usize {
    if case.mask == 0 {
        2
    } else {
        let lanes = case.width.bytes() as usize;
        5 * lanes + 4 - usize::from(level == OptLevel::O2)
    }
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
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer admitted malformed graph"
    );
}

#[test]
fn rewrites_match_four_independent_llvm_23_anchors() {
    // Generated with LLVM 23 llvm-mc `-triple=x86_64
    // -x86-asm-syntax=intel -show-encoding`.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF2, 0x75, 0x08, 0x8F, 0x13],
            &[0x62, 0xF2, 0x75, 0x08, 0x8F, 0xD0],
        ),
        (
            &[0x62, 0xD2, 0x75, 0x21, 0x8F, 0x5C, 0x4A, 0x02],
            &[0x62, 0xF2, 0x75, 0x21, 0x8F, 0x1C, 0x24],
        ),
        (
            &[0x62, 0xD2, 0x05, 0x40, 0x8F, 0x7D, 0xFF],
            &[0x62, 0xF2, 0x05, 0x40, 0x8F, 0xF8],
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x47, 0x8F, 0x08],
            &[0x62, 0xF2, 0x7D, 0x47, 0x8F, 0x0C, 0x24],
        ),
    ];
    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_vpshufbitqmb_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(replay_instruction(encoding), *expected, "{memory:02X?}");
    }
}

#[test]
fn classifier_exhausts_all_6144_width_destination_source_and_mask_cells() {
    let mut classified = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for destination in 0..8 {
            for source1 in 0..32 {
                for mask in 0..8 {
                    let case = VpshufbitqmbMemoryCase {
                        width,
                        destination,
                        source1,
                        mask,
                    };
                    let bytes = case.bytes();
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_vpshufbitqmb_memory_encoding()
                        .unwrap_or_else(|| panic!("{case:?}: {bytes:02X?}"));
                    assert_eq!(encoding.width, width, "{case:?}");
                    assert_eq!(encoding.destination, destination, "{case:?}");
                    assert_eq!(encoding.source1, source1, "{case:?}");
                    assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                    assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                    match encoding.replay {
                        X86EvexVpshufbitqmbMemoryReplay::Vector {
                            scratch,
                            register_instruction,
                        } => {
                            assert_eq!(mask, 0, "{case:?}");
                            assert_eq!(scratch, case.scratch(), "{case:?}");
                            assert_ne!(scratch, source1, "{case:?}");
                            assert_eq!(
                                register_instruction.as_slice(),
                                case.expected_replay(),
                                "{case:?}"
                            );
                        }
                        X86EvexVpshufbitqmbMemoryReplay::MaskedVector { stack_instruction } => {
                            assert_ne!(mask, 0, "{case:?}");
                            assert_eq!(
                                stack_instruction.as_slice(),
                                case.expected_replay(),
                                "{case:?}"
                            );
                        }
                    }
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 3 * 8 * 32 * 8);
}

#[test]
fn all_18_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 18);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask != 0).then_some(case.mask)
            );
            assert_eq!(exact.memory_size, case.width.bytes());
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
    assert_eq!(lowerings, 18 * LEVELS.len());
}

#[test]
fn memory_graphs_preserve_exact_full_or_byte_suppressed_access_shapes() {
    let mut checks = 0usize;
    for case in scanner_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let reads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| op.kind.reads_memory())
                .collect::<Vec<_>>();
            if case.mask == 0 {
                assert_eq!(reads.len(), 1, "{level:?} {case:?}");
                assert!(matches!(
                    reads[0].kind,
                    OpKind::VLoad { width, .. } if width == case.width
                ));
            } else {
                assert_eq!(
                    reads.len(),
                    case.width.bytes() as usize,
                    "{level:?} {case:?}"
                );
                for (lane, read) in reads.into_iter().enumerate() {
                    assert!(
                        matches!(
                            &read.kind,
                            OpKind::PredLoad {
                                addr: Address::BaseOffset {
                                    offset,
                                    disp_size: DispSize::Auto,
                                    ..
                                },
                                width: MemWidth::B1,
                                signed: crate::smir::ir::types::SignExtend::Zero,
                                ..
                            } if *offset == lane as i64
                        ),
                        "{level:?} {case:?}: lane {lane} {read:?}"
                    );
                }
            }
            checks += 1;
        }
    }
    assert_eq!(checks, 18 * LEVELS.len());
}

#[test]
fn avx_ymm16_only_bridge_rejects_both_replay_forms() {
    for mask in [0, 1] {
        let case = VpshufbitqmbMemoryCase {
            width: VecWidth::V512,
            destination: 7,
            source1: 31,
            mask,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_preserve_vector_mem_helpers(true);
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer.set_jit_fault_deopt_guards(true);
        let error = lowerer
            .lower_function(&function)
            .expect_err("AVX-only bridge must reject VPSHUFBITQMB");
        assert!(format!("{error:?}").contains("VPSHUFBITQMB"), "{error:?}");
    }
}

#[test]
fn classifier_rejects_reserved_structural_and_length_frontiers() {
    let case = VpshufbitqmbMemoryCase {
        width: VecWidth::V256,
        destination: 3,
        source1: 27,
        mask: 5,
    };
    let valid = case.bytes();
    let mut cases = Vec::new();
    for (index, mask) in [
        (1, 0x01), // wrong map
        (1, 0x10), // reserved K destination extension
        (1, 0x80), // reserved K destination extension
        (2, 0x80), // W=1
        (2, 0x03), // wrong mandatory prefix
        (3, 0x80), // reserved z
        (3, 0x10), // reserved b
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
    for forbidden_prefix in [0x40, 0x66, 0xF2, 0xF3] {
        let mut bytes = valid.clone();
        bytes.insert(0, forbidden_prefix);
        cases.push(bytes);
    }
    let mut truncated = valid;
    truncated.pop();
    cases.push(truncated);

    for bytes in cases {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_vpshufbitqmb_memory_encoding(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn segment_addr32_sib_rip_displacements_and_apx_addresses_admit() {
    for mask in [0, 7] {
        let case = VpshufbitqmbMemoryCase {
            width: VecWidth::V512,
            destination: 7,
            source1: 31,
            mask,
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
        let mut apx_index = memory_encoding(case, true);
        apx_index[2] &= !0x04;
        forms.push(apx_index);

        let mut admissions = 0usize;
        for bytes in forms {
            for level in LEVELS {
                let function = optimize(lift_bytes(&bytes), level);
                let exact = sequence(&function, true).unwrap_or_else(|| {
                    panic!("{level:?} {bytes:02X?}: {:#?}", function.blocks[0].ops)
                });
                assert_eq!(exact.encoding.width, case.width);
                assert_eq!(exact.encoding.writemask, (mask != 0).then_some(mask));
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
fn unmasked_sequence_fails_closed_for_graph_provenance_frontier_and_ssa_mutations() {
    let case = VpshufbitqmbMemoryCase {
        width: VecWidth::V128,
        destination: 3,
        source1: 17,
        mask: 0,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());
    let loaded = match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let tail = function.blocks[0].ops.len() - 1;
    let mut mutations = Vec::<(&str, SmirFunction)>::new();

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_provenance = function.clone();
    let mut bytes = case.bytes();
    bytes[5] ^= 0x08;
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

    let mut wrong_load_width = function.clone();
    let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = VecWidth::V256;
    mutations.push(("load width", wrong_load_width));

    let mut wrong_destination = function.clone();
    let OpKind::VShuffleBitQM { dst, .. } = &mut wrong_destination.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::K(4)));
    mutations.push(("K destination", wrong_destination));

    let mut wrong_source = function.clone();
    let OpKind::VShuffleBitQM { src, .. } = &mut wrong_source.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)));
    mutations.push(("first source", wrong_source));

    let mut wrong_indices = function.clone();
    let OpKind::VShuffleBitQM { indices, .. } = &mut wrong_indices.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *indices = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    mutations.push(("memory source", wrong_indices));

    let mut wrong_mask = function.clone();
    let OpKind::VShuffleBitQM { mask, .. } = &mut wrong_mask.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    mutations.push(("unexpected writemask", wrong_mask));

    let mut wrong_width = function.clone();
    let OpKind::VShuffleBitQM { width, .. } = &mut wrong_width.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *width = VecWidth::V256;
    mutations.push(("semantic width", wrong_width));

    let mut hinted_tail = function.clone();
    hinted_tail.blocks[0].ops[tail].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    mutations.push(("semantic hint", hinted_tail));

    let mut wrong_pc = function.clone();
    wrong_pc.blocks[0].ops[tail].guest_pc = PC + 1;
    mutations.push(("semantic PC", wrong_pc));

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

#[test]
fn masked_sequence_fails_closed_for_lane_graph_and_alias_mutations() {
    let case = VpshufbitqmbMemoryCase {
        width: VecWidth::V128,
        destination: 1,
        source1: 31,
        mask: 1,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let tail = function.blocks[0].ops.len() - 1;
    let pred_load = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let insert = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .unwrap();
    let mut mutations = Vec::<(&str, SmirFunction)>::new();

    let mut nonzero_seed = function.clone();
    let OpKind::Mov { src, .. } = &mut nonzero_seed.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *src = SrcOperand::Imm(1);
    mutations.push(("nonzero vector seed", nonzero_seed));

    let mut wrong_pred_width = function.clone();
    let OpKind::PredLoad { width, .. } = &mut wrong_pred_width.blocks[0].ops[pred_load].kind else {
        unreachable!()
    };
    *width = MemWidth::B2;
    mutations.push(("predicated load width", wrong_pred_width));

    let mut wrong_pred_condition = function.clone();
    let OpKind::PredLoad { cond, .. } = &mut wrong_pred_condition.blocks[0].ops[pred_load].kind
    else {
        unreachable!()
    };
    *cond = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    mutations.push(("predicated load condition", wrong_pred_condition));

    let mut wrong_lane_address = function.clone();
    let OpKind::PredLoad { addr, .. } = &mut wrong_lane_address.blocks[0].ops[pred_load].kind
    else {
        unreachable!()
    };
    let Address::BaseOffset { offset, .. } = addr else {
        unreachable!()
    };
    *offset = 1;
    mutations.push(("predicated load address", wrong_lane_address));

    let mut wrong_insert_lane = function.clone();
    let OpKind::VInsertLane { lane, .. } = &mut wrong_insert_lane.blocks[0].ops[insert].kind else {
        unreachable!()
    };
    *lane = 1;
    mutations.push(("insert lane", wrong_insert_lane));

    let mut wrong_semantic_mask = function.clone();
    let OpKind::VShuffleBitQM { mask, .. } = &mut wrong_semantic_mask.blocks[0].ops[tail].kind
    else {
        unreachable!()
    };
    *mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(2))));
    mutations.push(("semantic writemask", wrong_semantic_mask));

    let mut wrong_destination = function.clone();
    let OpKind::VShuffleBitQM { dst, .. } = &mut wrong_destination.blocks[0].ops[tail].kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    mutations.push(("destination alias", wrong_destination));

    let mut hinted_pred_load = function.clone();
    hinted_pred_load.blocks[0].ops[pred_load].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    mutations.push(("predicated load hint", hinted_pred_load));

    for (name, mutation) in mutations {
        assert_rejected(name, &mutation);
    }
}
