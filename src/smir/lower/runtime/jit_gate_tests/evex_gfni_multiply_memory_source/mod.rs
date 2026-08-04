//! Exact helper-backed EVEX `VGF2P8MULB` memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SrcOperand, VReg,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexGfniMultiplyMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexGfniMultiplyMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_gfni_multiply_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x72C0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Merge => 1,
            Self::Zero => 2,
        }
    }

    fn zeroing(self) -> bool {
        self == Self::Zero
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GfniMultiplyCase {
    width: VecWidth,
    destination: u8,
    source1: u8,
    control: MaskControl,
}

impl GfniMultiplyCase {
    fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!("EVEX VGF2P8MULB width"),
        }
    }

    fn mask(self) -> u8 {
        self.control.mask()
    }

    fn zeroing(self) -> bool {
        self.control.zeroing()
    }
}

fn memory_encoding(case: GfniMultiplyCase) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    let p0 = (u8::from(case.destination & 8 == 0) << 7)
        | 0x60
        | (u8::from(case.destination & 16 == 0) << 4)
        | 2;
    let p1 = (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.source1 & 16 == 0) << 3)
        | case.mask();
    vec![0x62, p0, p1, p2, 0xCF, ((case.destination & 7) << 3) | 0x02]
}

fn expected_register_encoding(bytes: &[u8], destination: u8, scratch: u8) -> Vec<u8> {
    vec![
        0x62,
        (bytes[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
        bytes[2] | 0x04,
        bytes[3],
        0xCF,
        0xC0 | ((destination & 7) << 3) | (scratch & 7),
    ]
}

fn expected_stack_encoding(bytes: &[u8], destination: u8) -> Vec<u8> {
    vec![
        0x62,
        (bytes[1] & 0x97) | 0x60,
        bytes[2] | 0x04,
        bytes[3],
        0xCF,
        ((destination & 7) << 3) | 0x04,
        0x24,
    ]
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("x86 instruction provenance"),
    );
    function
}

fn lift_case(case: GfniMultiplyCase) -> SmirFunction {
    lift_bytes(&memory_encoding(case))
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
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
) -> Option<X86JitEvexGfniMultiplyMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_gfni_multiply_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn replay_bytes(sequence: X86JitEvexGfniMultiplyMemorySequence) -> X86InstructionBytes {
    match sequence.encoding.replay {
        X86EvexGfniMultiplyMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction,
        X86EvexGfniMultiplyMemoryReplay::MaskedVector { stack_instruction } => stack_instruction,
    }
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .expect("lower EVEX VGF2P8MULB memory source");
    (
        lowerer
            .finalize()
            .expect("finalize EVEX VGF2P8MULB memory source"),
        result.entry_offset,
    )
}

fn scanner_cases() -> Vec<GfniMultiplyCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for source1 in [0, 1, 15] {
            for control in MaskControl::ALL {
                cases.push(GfniMultiplyCase {
                    width,
                    destination: 0,
                    source1,
                    control,
                });
            }
        }
    }
    cases
}

#[test]
fn classifier_exhausts_262_144_extension_operand_mask_and_reserved_cells() {
    let mut tested = 0usize;
    let mut accepted = 0usize;
    for extension_bits in 0u8..32 {
        let p0 = (extension_bits << 3) | 2;
        for encoded_vvvv in 0u8..16 {
            for ordinary_u in [false, true] {
                let p1 = (encoded_vvvv << 3) | (u8::from(ordinary_u) << 2) | 1;
                for encoded_v_high in [false, true] {
                    for ll in 0u8..=3 {
                        for embedded_control in [false, true] {
                            for zeroing in [false, true] {
                                for mask in 0u8..8 {
                                    let p2 = (u8::from(zeroing) << 7)
                                        | (ll << 5)
                                        | (u8::from(embedded_control) << 4)
                                        | (u8::from(encoded_v_high) << 3)
                                        | mask;
                                    let bytes = [0x62, p0, p1, p2, 0xCF, 0x1A];
                                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                                    let actual = instruction.evex_gfni_multiply_memory_encoding();
                                    let expected =
                                        ll < 3 && !embedded_control && (!zeroing || mask != 0);
                                    assert_eq!(actual.is_some(), expected, "{bytes:02X?}");
                                    if let Some(encoding) = actual {
                                        let width =
                                            [VecWidth::V128, VecWidth::V256, VecWidth::V512]
                                                [usize::from(ll)];
                                        let destination = (u8::from(p0 & 0x80 == 0) << 3)
                                            | (u8::from(p0 & 0x10 == 0) << 4)
                                            | 3;
                                        let source1 = ((!encoded_vvvv) & 0x0F)
                                            | (u8::from(!encoded_v_high) << 4);
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                        assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                                        assert_eq!(encoding.zeroing, zeroing);
                                        assert_eq!(encoding.needs_avx512vl, ll != 2);
                                        match encoding.replay {
                                            X86EvexGfniMultiplyMemoryReplay::Vector {
                                                scratch,
                                                register_instruction,
                                            } => {
                                                assert_eq!(mask, 0);
                                                assert_ne!(scratch, destination);
                                                assert_ne!(scratch, source1);
                                                assert_eq!(
                                                    register_instruction
                                                        .evex_register_gfni_needs_vl(),
                                                    Some(ll != 2)
                                                );
                                            }
                                            X86EvexGfniMultiplyMemoryReplay::MaskedVector {
                                                stack_instruction,
                                            } => {
                                                assert_ne!(mask, 0);
                                                assert_eq!(
                                                    stack_instruction.evex_register_gfni_needs_vl(),
                                                    None,
                                                    "stack replay must remain a memory form"
                                                );
                                            }
                                        }
                                        accepted += 1;
                                    }
                                    tested += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(tested, 262_144);
    assert_eq!(accepted, 92_160);
}

#[test]
fn classifier_rejects_non_owned_reserved_incomplete_and_trailing_forms() {
    let valid = memory_encoding(GfniMultiplyCase {
        width: VecWidth::V512,
        destination: 17,
        source1: 30,
        control: MaskControl::Zero,
    });
    let mut invalid = Vec::new();
    for map in [0u8, 1, 3, 4, 7] {
        let mut bytes = valid.clone();
        bytes[1] = (bytes[1] & !7) | map;
        invalid.push(bytes);
    }
    for pp in [0u8, 2, 3] {
        let mut bytes = valid.clone();
        bytes[2] = (bytes[2] & !3) | pp;
        invalid.push(bytes);
    }
    for opcode in [0xCE, 0xCD, 0xD0] {
        let mut bytes = valid.clone();
        bytes[4] = opcode;
        invalid.push(bytes);
    }
    let mut wrong_w = valid.clone();
    wrong_w[2] |= 0x80;
    invalid.push(wrong_w);
    let mut broadcast = valid.clone();
    broadcast[3] |= 0x10;
    invalid.push(broadcast);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    invalid.push(reserved_ll);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    invalid.push(register);
    let mut zero_without_mask = valid.clone();
    zero_without_mask[3] = 0xC8;
    invalid.push(zero_without_mask);
    let mut trailing = valid.clone();
    trailing.push(0xA5);
    invalid.push(trailing);
    invalid.push(valid[..5].to_vec());
    invalid.push(vec![0xC4, 0xE2, 0x6D, 0xCF, 0x02]);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_gfni_multiply_memory_encoding(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_23_anchors_produce_exact_register_and_stack_rewrites() {
    let anchors = [
        (
            &[0x62, 0xE2, 0x6D, 0x08, 0xCF, 0x48, 0x02][..],
            GfniMultiplyCase {
                width: VecWidth::V128,
                destination: 17,
                source1: 2,
                control: MaskControl::None,
            },
            None,
        ),
        (
            &[0x62, 0x82, 0x0D, 0x2B, 0xCF, 0x6C, 0xAC, 0x02][..],
            GfniMultiplyCase {
                width: VecWidth::V256,
                destination: 21,
                source1: 14,
                control: MaskControl::Merge,
            },
            Some(3),
        ),
        (
            &[0x62, 0xC2, 0x0D, 0xC7, 0xCF, 0x4F, 0xFE][..],
            GfniMultiplyCase {
                width: VecWidth::V512,
                destination: 17,
                source1: 30,
                control: MaskControl::Zero,
            },
            Some(7),
        ),
        (
            &[0x62, 0x72, 0x6D, 0x48, 0xCF, 0x88, 0x20, 0x00, 0x00, 0x00][..],
            GfniMultiplyCase {
                width: VecWidth::V512,
                destination: 9,
                source1: 2,
                control: MaskControl::None,
            },
            None,
        ),
    ];
    for (bytes, case, writemask) in anchors {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_gfni_multiply_memory_encoding()
            .unwrap_or_else(|| panic!("LLVM anchor rejected: {bytes:02X?}"));
        assert_eq!(encoding.width, case.width);
        assert_eq!(encoding.destination, case.destination);
        assert_eq!(encoding.source1, case.source1);
        assert_eq!(encoding.writemask, writemask);
        assert_eq!(encoding.zeroing, case.zeroing());
        match encoding.replay {
            X86EvexGfniMultiplyMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                assert_eq!(
                    register_instruction.as_slice(),
                    expected_register_encoding(bytes, case.destination, scratch)
                );
            }
            X86EvexGfniMultiplyMemoryReplay::MaskedVector { stack_instruction } => {
                assert_eq!(
                    stack_instruction.as_slice(),
                    expected_stack_encoding(bytes, case.destination)
                );
            }
        }
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?}: LLVM anchor graph rejected {bytes:02X?}"));
            let (code, _) = lower(&function);
            let replay = replay_bytes(exact);
            assert!(
                code.windows(replay.as_slice().len())
                    .any(|window| window == replay.as_slice()),
                "{level:?} {bytes:02X?}: missing {:02X?}",
                replay.as_slice()
            );
        }
    }
}

#[test]
fn all_27_scanner_cells_admit_and_lower_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 27);
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: exact graph rejected"));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.memory_size, case.width.bytes());
            assert_eq!(
                exact.address_offset,
                if case.control == MaskControl::None {
                    0
                } else {
                    2
                }
            );
            assert_eq!(
                exact.consumed + sequence_index(&function),
                function.blocks[0].ops.len()
            );

            let excluded = HashMap::new();
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
            assert!(!is_native_clobber_safe_excluding(
                &function, &excluded, false
            ));
            assert!(!is_x86_aarch64_native_clobber_safe_excluding(
                &function, &excluded
            ));
            assert!(uses_x86_native_vectors_excluding(&function, &excluded));
            assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));
            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any);
            assert!(requirements.needs_avx);
            assert!(requirements.needs_avx512bw);
            assert_eq!(requirements.needs_avx512vl, case.width != VecWidth::V512);
            assert!(requirements.needs_gfni);
            assert!(!requirements.needs_avx512dq);
            assert!(!requirements.all_spans_support_avx_ymm16);
            admitted += 1;

            let (code, _) = lower(&function);
            let replay = replay_bytes(exact);
            assert_eq!(
                code.windows(replay.as_slice().len())
                    .filter(|window| *window == replay.as_slice())
                    .count(),
                1,
                "{level:?} {case:?}: replay count"
            );
            lowered += 1;
        }
    }
    assert_eq!(admitted, 27 * LEVELS.len());
    assert_eq!(lowered, admitted);
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed sequence"
    );
}

#[test]
fn exact_sequence_fails_closed_for_provenance_graph_ssa_and_frontier_mutations() {
    let case = GfniMultiplyCase {
        width: VecWidth::V512,
        destination: 17,
        source1: 30,
        control: MaskControl::Merge,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    let index = sequence_index(&base);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_provenance = base.clone();
    let wrong_bytes = memory_encoding(GfniMultiplyCase {
        control: MaskControl::None,
        ..case
    });
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );

    let mut source_hint = base.clone();
    source_hint.blocks[0].ops[index].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut virtual_address = base.clone();
    match &mut virtual_address.blocks[0].ops[index + 2].kind {
        OpKind::Lea { addr, .. } => {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
        _ => unreachable!("masked source owns a LEA"),
    }

    let mut wrong_load_width = base.clone();
    let load = wrong_load_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .expect("masked byte load");
    match &mut load.kind {
        OpKind::PredLoad { width, .. } => *width = MemWidth::B2,
        _ => unreachable!(),
    }

    let mut wrong_lane_address = base.clone();
    let load = wrong_lane_address.blocks[0]
        .ops
        .iter_mut()
        .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .nth(1)
        .expect("second masked byte load");
    match &mut load.kind {
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset, .. },
            ..
        } => *offset += 1,
        _ => unreachable!(),
    }

    let mut wrong_constant = base.clone();
    let constant = wrong_constant.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::Mov {
                    src: SrcOperand::Imm(0x1B),
                    ..
                }
            )
        })
        .expect("GFNI reduction constant");
    match &mut constant.kind {
        OpKind::Mov {
            src: SrcOperand::Imm(value),
            ..
        } => *value = 0x1D,
        _ => unreachable!(),
    }

    let mut wrong_shift = base.clone();
    let shift = wrong_shift.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VShift {
                    amount: SrcOperand::Imm(7),
                    ..
                }
            )
        })
        .expect("GFNI carry shift");
    match &mut shift.kind {
        OpKind::VShift {
            amount: SrcOperand::Imm(amount),
            ..
        } => *amount = 6,
        _ => unreachable!(),
    }

    let mut wrong_source = base.clone();
    let source = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            op.kind
                .source_vregs()
                .contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(case.source1))))
        })
        .expect("architectural GFNI source");
    match &mut source.kind {
        OpKind::VAnd { src1, .. } => *src1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(29))),
        _ => unreachable!("first architectural source use is VAnd"),
    }

    let mut child_hint = base.clone();
    let core = child_hint.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VSub { .. }))
        .expect("GFNI core operation");
    core.x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));

    let mut child_pc = base.clone();
    let core = child_pc.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VXor { .. }))
        .expect("GFNI core XOR");
    core.guest_pc += 1;

    let mut wrong_mask_lane = base.clone();
    let extract = wrong_mask_lane.blocks[0]
        .ops
        .iter_mut()
        .rev()
        .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .expect("destination mask reconstruction");
    match &mut extract.kind {
        OpKind::VExtractLane { lane, .. } => *lane = lane.wrapping_sub(1),
        _ => unreachable!(),
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F00), PC, OpKind::Nop));

    let external = base.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .find(|register| matches!(register, VReg::Virtual(_)))
        .expect("sequence virtual");
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F01),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Reg(external),
            width: OpWidth::W64,
        },
    ));

    let mut spurious_apx_guard = base.clone();
    spurious_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7F02), PC, OpKind::X86RequireApx));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("provenance mask differs", wrong_provenance),
        ("source hint differs", source_hint),
        ("address contains virtual register", virtual_address),
        ("lane load width differs", wrong_load_width),
        ("lane address differs", wrong_lane_address),
        ("reduction polynomial differs", wrong_constant),
        ("carry shift differs", wrong_shift),
        ("architectural source differs", wrong_source),
        ("semantic child has a hint", child_hint),
        ("semantic child PC differs", child_pc),
        ("mask result lane differs", wrong_mask_lane),
        ("same-PC operation follows sequence", same_pc_tail),
        ("temporary has an external use", external_use),
        ("low address has APX guard", spurious_apx_guard),
    ] {
        assert_rejected(name, &function);
    }
    assert!(sequence(&base, false).is_none());
}

#[test]
fn rip_addr32_segments_and_apx_b4_x4_remain_helper_address_controls() {
    let case = GfniMultiplyCase {
        width: VecWidth::V256,
        destination: 17,
        source1: 18,
        control: MaskControl::None,
    };
    let vector = memory_encoding(case);
    let masked = memory_encoding(GfniMultiplyCase {
        control: MaskControl::Merge,
        ..case
    });

    let mut rip = vector.clone();
    rip[5] = (rip[5] & 0x38) | 0x05;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = vector.clone();
    addr32.insert(0, 0x67);
    let mut fs = masked.clone();
    fs.insert(0, 0x64);
    let mut gs_addr32 = masked.clone();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.extend_from_slice(&[0x8B, 0x02]);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let cases = [
        (
            "RIP+disp32",
            rip.clone(),
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + rip.len() as u64),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS masked",
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB masked",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 64,
            })),
        ),
    ];
    for (name, bytes, expected_address) in cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } | OpKind::Lea { addr, .. } => {
                        addr == &expected_address
                    }
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{name} {level:?}"));
            lower(&function);
        }
    }

    let mut apx = masked;
    apx[5] = (apx[5] & 0x38) | 0x04;
    apx.insert(6, 0x48); // [RAX + RCX*2]
    apx[1] |= 0x08; // EVEX.B4 extends base to R16
    apx[2] &= !0x04; // EVEX.X4/!U extends index to R17
    let expected_address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(function.blocks[0].ops.iter().any(|op| {
            matches!(&op.kind, OpKind::Lea { addr, .. } if addr == &expected_address)
        }));
        sequence(&function, true).unwrap_or_else(|| panic!("APX {level:?}"));
        lower(&function);

        let mut missing_guard = function.clone();
        missing_guard.blocks[0].ops.remove(0);
        assert_rejected("APX address without dynamic guard", &missing_guard);
    }
}

#[test]
fn masked_byte_replay_rejects_avx_only_bridge_and_emits_one_exact_stack_operation() {
    let case = GfniMultiplyCase {
        width: VecWidth::V512,
        destination: 17,
        source1: 30,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let exact = sequence(&function, true).expect("masked exact sequence");
    let X86EvexGfniMultiplyMemoryReplay::MaskedVector { stack_instruction } = exact.encoding.replay
    else {
        unreachable!()
    };
    let (code, _) = lower(&function);
    assert_eq!(
        code.windows(stack_instruction.as_slice().len())
            .filter(|window| *window == stack_instruction.as_slice())
            .count(),
        1
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX GFNI");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
