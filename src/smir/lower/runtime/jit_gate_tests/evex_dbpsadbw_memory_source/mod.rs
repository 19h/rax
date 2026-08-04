//! Exact helper-backed EVEX `VDBPSADBW` memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, OpId, OpWidth, SourceArch, SrcOperand, VReg, VecElementType,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexDbpsadbwMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_dbpsadbw_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0x7D20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (3, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DbpsadbwMemoryCase {
    width: VecWidth,
    destination: u8,
    source1: u8,
    control: MaskControl,
    immediate: u8,
}

impl DbpsadbwMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
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

fn memory_encoding(case: DbpsadbwMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x63
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0x42,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        bytes.push(0x48); // [RAX + RCX*2]
    }
    bytes.push(case.immediate);
    bytes
}

fn register_encoding(case: DbpsadbwMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x43
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x42,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
        case.immediate,
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
        X86InstructionBytes::new(bytes).expect("EVEX VDBPSADBW provenance"),
    );
    function
}

fn lift_case(case: DbpsadbwMemoryCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexDbpsadbwMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_dbpsadbw_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: DbpsadbwMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: VDBPSADBW lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize EVEX VDBPSADBW"),
        result.entry_offset,
    )
}

fn case_immediate(width: VecWidth, destination: u8, source1: u8, control: MaskControl) -> u8 {
    (width.bytes() as u8)
        .wrapping_mul(17)
        .wrapping_add(destination.wrapping_mul(29))
        .wrapping_add(source1.wrapping_mul(43))
        .wrapping_add(control.fields().0.wrapping_mul(71))
}

fn scanner_cases() -> Vec<DbpsadbwMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for source1 in [0, 1, 15] {
            for control in MaskControl::ALL {
                cases.push(DbpsadbwMemoryCase {
                    width,
                    destination: 0,
                    source1,
                    control,
                    immediate: case_immediate(width, 0, source1, control),
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
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF3, 0x6D, 0x0A, 0x42, 0x0A, 0x4E],
            &[0x62, 0xF3, 0x6D, 0x0A, 0x42, 0xC8, 0x4E],
        ),
        (
            &[0x62, 0x83, 0x6D, 0xA3, 0x42, 0x4C, 0x5A, 0x02, 0xA5],
            &[0x62, 0xE3, 0x6D, 0xA3, 0x42, 0xC8, 0xA5],
        ),
        (
            &[0x62, 0xF3, 0x55, 0x48, 0x42, 0x24, 0x24, 0x1B],
            &[0x62, 0xF3, 0x55, 0x48, 0x42, 0xE0, 0x1B],
        ),
        (
            &[0x62, 0x43, 0x2D, 0xC1, 0x42, 0x49, 0xFE, 0x5A],
            &[0x62, 0x63, 0x2D, 0xC1, 0x42, 0xC8, 0x5A],
        ),
    ];
    for (memory, expected) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_dbpsadbw_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), *expected);
    }
}

#[test]
fn classifier_exhausts_184320_operand_mask_width_and_apx_cells() {
    let mut accepted = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for destination in 0u8..32 {
            for source1 in 0u8..32 {
                for mask in 0u8..8 {
                    for zeroing in [false, true] {
                        if zeroing && mask == 0 {
                            continue;
                        }
                        let case = DbpsadbwMemoryCase {
                            width,
                            destination,
                            source1,
                            control: MaskControl::None,
                            immediate: case_immediate(
                                width,
                                destination,
                                source1,
                                MaskControl::None,
                            ),
                        };
                        let mut canonical = memory_encoding(case, true);
                        canonical[3] = (canonical[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                        for base_high in [false, true] {
                            for index_high in [false, true] {
                                let mut bytes = canonical.clone();
                                bytes[1] |= u8::from(base_high) << 3;
                                if index_high {
                                    bytes[2] &= !0x04;
                                }
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_dbpsadbw_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                                assert_eq!(encoding.zeroing, zeroing);
                                assert_eq!(encoding.immediate, case.immediate);
                                assert_eq!(encoding.needs_avx512vl, width != VecWidth::V512);
                                assert_ne!(encoding.scratch, destination);
                                assert_ne!(encoding.scratch, source1);
                                let mut expected = case.expected_replay();
                                expected[3] =
                                    (expected[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                                assert_eq!(encoding.register_instruction.as_slice(), expected);
                                accepted += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 184_320);
}

#[test]
fn classifier_preserves_all_2304_imm8_width_and_mask_cells() {
    let mut accepted = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for control in MaskControl::ALL {
            for immediate in u8::MIN..=u8::MAX {
                let case = DbpsadbwMemoryCase {
                    width,
                    destination: 25,
                    source1: 26,
                    control,
                    immediate,
                };
                let encoding = X86InstructionBytes::new(&case.bytes())
                    .unwrap()
                    .evex_dbpsadbw_memory_encoding()
                    .unwrap_or_else(|| panic!("{case:?}"));
                assert_eq!(encoding.immediate, immediate);
                assert_eq!(
                    encoding.register_instruction.as_slice().last(),
                    Some(&immediate)
                );
                accepted += 1;
            }
        }
    }
    assert_eq!(accepted, 3 * 3 * 256);
}

#[test]
fn classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = DbpsadbwMemoryCase {
        width: VecWidth::V256,
        destination: 1,
        source1: 2,
        control: MaskControl::Merge,
        immediate: 0x4E,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [(1, 0x01), (2, 0x01), (4, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut w1 = valid.clone();
    w1[2] |= 0x80;
    malformed.push(w1);
    let mut broadcast = valid.clone();
    broadcast[3] |= 0x10;
    malformed.push(broadcast);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);
    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_dbpsadbw_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67] {
        let mut bytes = vec![prefix];
        bytes.extend_from_slice(&valid);
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_dbpsadbw_memory_encoding()
                .is_some(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_27_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 27);
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
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.encoding.immediate, case.immediate);
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
    assert_eq!(lowerings, 27 * LEVELS.len());
}

#[test]
fn e4nf_graphs_retain_one_unconditional_full_tuple_access() {
    for case in scanner_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::VLoad { .. }))
                    .count(),
                1,
                "{level:?} {case:?}"
            );
            assert!(
                !function.blocks[0]
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
            );
        }
    }
}

#[test]
fn sequence_fails_closed_for_provenance_shuffle_sad_mask_and_ssa_mutations() {
    let case = DbpsadbwMemoryCase {
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        control: MaskControl::Zero,
        immediate: 0xA5,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());
    let mut mutations = Vec::<(&str, SmirFunction)>::new();

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_immediate = function.clone();
    let mut bytes = case.bytes();
    *bytes.last_mut().unwrap() ^= 1;
    wrong_immediate
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("immediate provenance", wrong_immediate));

    let mut wrong_load_hint = function.clone();
    wrong_load_hint.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .unwrap()
        .x86_hint = None;
    mutations.push(("load hint", wrong_load_hint));

    let mut wrong_shuffle = function.clone();
    let extract = wrong_shuffle.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .unwrap();
    if let OpKind::VExtractLane { lane, .. } = &mut extract.kind {
        *lane = lane.wrapping_add(1);
    }
    mutations.push(("dword shuffle selector", wrong_shuffle));

    let mut wrong_sad = function.clone();
    let sad = wrong_sad.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
        .unwrap();
    if let OpKind::VMpsadbw { imm, .. } = &mut sad.kind {
        *imm ^= 1;
    }
    mutations.push(("SAD projection", wrong_sad));

    let mut wrong_projection = function.clone();
    let extract = wrong_projection.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .unwrap();
    if let OpKind::VExtractLane { lane, .. } = &mut extract.kind {
        *lane = lane.wrapping_add(1);
    }
    mutations.push(("word projection", wrong_projection));

    let mut escaped_internal = function.clone();
    let internal = escaped_internal.blocks[0]
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::VInsertLane {
                dst: VReg::Virtual(_),
                ..
            } => op.kind.dests().first().copied(),
            _ => None,
        })
        .unwrap();
    escaped_internal.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF1),
        PC + 1,
        OpKind::VMov {
            dst: VReg::Virtual(VirtualId(0xFFF1)),
            src: internal,
            width: case.width,
        },
    ));
    mutations.push(("escaped internal vector", escaped_internal));

    let mut same_pc_tail = function.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF2)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC tail", same_pc_tail));

    for (name, mutation) in mutations {
        assert_rejected(name, &mutation);
    }
}

#[test]
fn segment_addr32_sib_displacements_rip_and_apx_addresses_admit_and_lower() {
    let case = DbpsadbwMemoryCase {
        width: VecWidth::V512,
        destination: 25,
        source1: 26,
        control: MaskControl::Zero,
        immediate: 0x5A,
    };
    let direct = case.bytes();
    let sib = memory_encoding(case, true);
    let mut disp8 = direct.clone();
    disp8[5] = (disp8[5] & 0x38) | 0x42;
    disp8.insert(6, 0xFE);
    let mut disp32 = direct.clone();
    disp32[5] = (disp32[5] & 0x38) | 0x82;
    disp32.splice(6..6, 0x1122_3344u32.to_le_bytes());
    let mut rip = direct.clone();
    rip[5] = (rip[5] & 0x38) | 0x05;
    rip.splice(6..6, 0x20u32.to_le_bytes());
    let mut fs_addr32_sib = sib.clone();
    fs_addr32_sib.insert(0, 0x67);
    fs_addr32_sib.insert(0, 0x64);

    for (name, bytes) in [
        ("direct", direct),
        ("SIB", sib.clone()),
        ("disp8", disp8),
        ("disp32", disp32),
        ("RIP-relative", rip),
        ("FS addr32 SIB", fs_addr32_sib),
    ] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            sequence(&function, true).unwrap_or_else(|| {
                panic!(
                    "{name} {level:?} {bytes:02X?}: {:#?}",
                    function.blocks[0].ops
                )
            });
            lower(&function, case);
        }
    }

    let mut apx = sib;
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let base = lift_bytes(&apx);
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {apx:02X?}"));
        lower(&function, case);
    }
    let mut missing_guard = optimize(base, OptLevel::O2);
    missing_guard.blocks[0].ops.remove(0);
    assert_rejected("APX address without its dynamic guard", &missing_guard);
}

#[test]
fn aliases_unmasked_commit_and_avx_only_bridge_fail_closed() {
    for (destination, source1) in [(1, 1), (9, 10), (17, 17), (25, 26), (31, 31)] {
        let case = DbpsadbwMemoryCase {
            width: VecWidth::V512,
            destination,
            source1,
            control: MaskControl::None,
            immediate: 0x4E,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let exact = sequence(&function, true).unwrap_or_else(|| panic!("{case:?}"));
        assert_ne!(exact.encoding.scratch, destination);
        assert_ne!(exact.encoding.scratch, source1);
        lower(&function, case);
    }

    let case = DbpsadbwMemoryCase {
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        control: MaskControl::None,
        immediate: 0x4E,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut wrong_commit = function.clone();
    let commit = wrong_commit.blocks[0]
        .ops
        .iter_mut()
        .rfind(|op| matches!(op.kind, OpKind::VMov { .. }))
        .unwrap();
    if let OpKind::VMov { dst, .. } = &mut commit.kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Zmm(19)));
    }
    assert_rejected("unmasked destination commit", &wrong_commit);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only bridge must reject EVEX VDBPSADBW");
    assert!(format!("{error:?}").contains("AVX-only vector bridge"));
}
