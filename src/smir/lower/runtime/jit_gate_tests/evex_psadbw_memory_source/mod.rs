//! Exact helper-backed EVEX `VPSADBW` memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, OpWidth, SourceArch, SrcOperand, VReg, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPsadbwMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_psadbw_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7EA0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PsadbwMemoryCase {
    pub(super) width: VecWidth,
    pub(super) destination: u8,
    pub(super) source1: u8,
    pub(super) w: bool,
}

impl PsadbwMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave a low vector scratch")
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn expected_replay(self) -> Vec<u8> {
        register_encoding(self, self.scratch())
    }
}

fn memory_encoding(case: PsadbwMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    let p0 = 0x61
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (case.ll() << 5) | if case.source1 & 16 == 0 { 0x08 } else { 0 };
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0xF6,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        bytes.push(0x48); // [RAX + RCX*2]
    }
    bytes
}

fn register_encoding(case: PsadbwMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x41
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (case.ll() << 5) | if case.source1 & 16 == 0 { 0x08 } else { 0 };
    vec![
        0x62,
        p0,
        p1,
        p2,
        0xF6,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
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
        X86InstructionBytes::new(bytes).expect("EVEX VPSADBW provenance"),
    );
    function
}

fn lift_case(case: PsadbwMemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexPsadbwMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_psadbw_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: PsadbwMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: VPSADBW lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize EVEX VPSADBW"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<PsadbwMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for source1 in [0, 1, 15] {
            for w in [false, true] {
                cases.push(PsadbwMemoryCase {
                    width,
                    destination: 0,
                    source1,
                    w,
                });
            }
        }
    }
    cases
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
fn rewrites_match_four_independent_llvm_23_anchors() {
    // Generated with LLVM 23 llvm-mc `-triple=x86_64
    // -x86-asm-syntax=intel -show-encoding`; `{evex}` forces VL=128/256.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF1, 0x6D, 0x08, 0xF6, 0x0A],
            &[0x62, 0xF1, 0x6D, 0x08, 0xF6, 0xC8],
        ),
        (
            &[0x62, 0x51, 0x2D, 0x28, 0xF6, 0x4A, 0x02],
            &[0x62, 0x71, 0x2D, 0x28, 0xF6, 0xC8],
        ),
        (
            &[0x62, 0x41, 0x2D, 0x40, 0xF6, 0x49, 0xFE],
            &[0x62, 0x61, 0x2D, 0x40, 0xF6, 0xC8],
        ),
        (
            &[0x62, 0xF1, 0x55, 0x48, 0xF6, 0x6C, 0x44, 0x02],
            &[0x62, 0xF1, 0x55, 0x48, 0xF6, 0xE8],
        ),
    ];
    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_psadbw_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), *expected);
    }
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
            assert_eq!(exact.encoding.w, case.w);
            assert_eq!(exact.consumed, 2);
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
fn e4nf_graphs_retain_one_unconditional_full_tuple_access() {
    for case in scanner_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(function.blocks[0].ops.len(), 2, "{level:?} {case:?}");
            assert!(matches!(
                function.blocks[0].ops[0].kind,
                OpKind::VLoad { width, .. } if width == case.width
            ));
            assert!(matches!(
                function.blocks[0].ops[1].kind,
                OpKind::VSadBytes { width, .. } if width == case.width
            ));
        }
    }
}

#[test]
fn segment_addr32_sib_rip_displacements_and_apx_addresses_admit() {
    let case = PsadbwMemoryCase {
        width: VecWidth::V512,
        destination: 25,
        source1: 26,
        w: true,
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
    apx_base[1] |= 0x08; // B4 extends RDX to R18.
    forms.push(apx_base);

    let mut apx_sib = memory_encoding(case, true);
    apx_sib[1] |= 0x08; // B4 extends RAX to R16.
    apx_sib[2] &= !0x04; // X4 extends RCX to R17.
    forms.push(apx_sib);

    let mut admissions = 0usize;
    for bytes in forms {
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(code.windows(replay.len()).any(|window| window == replay));
            admissions += 1;
        }
    }
    assert_eq!(admissions, 11 * LEVELS.len());
}

#[test]
fn sequence_fails_closed_for_provenance_graph_frontier_and_ssa_mutations() {
    let case = PsadbwMemoryCase {
        width: VecWidth::V512,
        destination: 9,
        source1: 10,
        w: false,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());
    let temporary = match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut mutations = Vec::<(&str, SmirFunction)>::new();

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_destination_provenance = function.clone();
    let mut bytes = case.bytes();
    bytes[5] ^= 0x08;
    wrong_destination_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("destination provenance", wrong_destination_provenance));

    let mut wrong_load_hint = function.clone();
    wrong_load_hint.blocks[0].ops[0].x86_hint = None;
    mutations.push(("load hint", wrong_load_hint));

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

    let mut wrong_operation_width = function.clone();
    let OpKind::VSadBytes { width, .. } = &mut wrong_operation_width.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *width = VecWidth::V256;
    mutations.push(("operation width", wrong_operation_width));

    let mut wrong_destination = function.clone();
    let OpKind::VSadBytes { dst, .. } = &mut wrong_destination.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::Zmm(8)));
    mutations.push(("destination", wrong_destination));

    let mut wrong_source1 = function.clone();
    let OpKind::VSadBytes { src1, .. } = &mut wrong_source1.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *src1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(11)));
    mutations.push(("source1", wrong_source1));

    let mut wrong_temporary = function.clone();
    let OpKind::VSadBytes { src2, .. } = &mut wrong_temporary.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *src2 = VReg::Virtual(VirtualId(0x7FFF));
    mutations.push(("temporary", wrong_temporary));

    let mut hinted_operation = function.clone();
    hinted_operation.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    mutations.push(("operation hint", hinted_operation));

    let mut wrong_pc = function.clone();
    wrong_pc.blocks[0].ops[1].guest_pc = PC + 1;
    mutations.push(("operation PC", wrong_pc));

    let mut following_same_pc = function.clone();
    following_same_pc.blocks[0].ops.push(SmirOp::new(
        OpId(90),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(90)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC tail", following_same_pc));

    let mut extra_use = function.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(91),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(91)),
            src: SrcOperand::Reg(temporary),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("extra temporary use", extra_use));

    let mut duplicate_definition = function.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(92),
        PC + 1,
        OpKind::Mov {
            dst: temporary,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("duplicate temporary definition", duplicate_definition));

    let mut unexpected_apx_guard = function.clone();
    unexpected_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(93), PC, OpKind::X86RequireApx));
    mutations.push(("unexpected APX guard", unexpected_apx_guard));

    let mut preceding_same_pc = function.clone();
    preceding_same_pc.blocks[0].ops.insert(
        0,
        SmirOp::new(
            OpId(94),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(94)),
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
