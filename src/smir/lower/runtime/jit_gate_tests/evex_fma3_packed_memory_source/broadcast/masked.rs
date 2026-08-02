//! Exact helper-backed writemasked packed EVEX FMA3 broadcast coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::types::{OpWidth, SrcOperand};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaskedBroadcastCase {
    broadcast: BroadcastCase,
    mask: u8,
    zeroing: bool,
}

impl MaskedBroadcastCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = self.broadcast.bytes();
        let evex = self.broadcast.evex_start();
        bytes[evex + 3] |= self.mask | (u8::from(self.zeroing) << 7);
        bytes
    }

    fn stack_instruction(self) -> [u8; 7] {
        let mut bytes = self.broadcast.stack_instruction();
        bytes[3] |= self.mask | (u8::from(self.zeroing) << 7);
        bytes
    }

    fn lanes(self) -> u8 {
        self.broadcast.width.lanes(self.broadcast.format.elem()) as u8
    }

    fn lane_mask(self) -> u64 {
        (1u64 << self.lanes()) - 1
    }
}

fn all_masked_cases() -> Vec<MaskedBroadcastCase> {
    let mut cases = Vec::new();
    for broadcast in all_cases() {
        for mask in 1..=7 {
            for zeroing in [false, true] {
                cases.push(MaskedBroadcastCase {
                    broadcast,
                    mask,
                    zeroing,
                });
            }
        }
    }
    cases
}

fn lift_masked_case(case: MaskedBroadcastCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("masked packed EVEX FMA3 provenance"),
    );
    function
}

fn sequence_index(function: &SmirFunction, case: MaskedBroadcastCase) -> usize {
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(case.mask)));
    function.blocks[0]
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    src1,
                    src2: SrcOperand::Imm(lane_mask),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                    ..
                } if src1 == mask && lane_mask == case.lane_mask() as i64
            )
        })
        .expect("masked packed EVEX FMA3 aggregate condition")
}

fn masked_sequence(
    function: &SmirFunction,
    case: MaskedBroadcastCase,
) -> Option<crate::smir::lower::runtime::X86JitEvexPackedFma3MemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_fma3_memory_sequence(
        &function.blocks[0],
        sequence_index(function, case),
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_masked_prefix(function: &SmirFunction, case: MaskedBroadcastCase) {
    let index = sequence_index(function, case);
    let ops = &function.blocks[0].ops[index..];
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(case.mask)));
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        1,
        "{case:?}: one scalar architectural memory operand"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. })),
        "{case:?}: masked broadcast must not contain an eager load"
    );

    let active_mask = match ops[0].kind {
        OpKind::And {
            dst: active_mask @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Imm(lane_mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == mask && lane_mask == case.lane_mask() as i64 => active_mask,
        ref other => panic!("{case:?}: aggregate mask condition {other:?}"),
    };
    let negated = match ops[1].kind {
        OpKind::Neg {
            dst: negated @ VReg::Virtual(_),
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src == active_mask => negated,
        ref other => panic!("{case:?}: aggregate mask negation {other:?}"),
    };
    let combined = match ops[2].kind {
        OpKind::Or {
            dst: combined @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == active_mask && src2 == negated => combined,
        ref other => panic!("{case:?}: aggregate mask combination {other:?}"),
    };
    let condition = match ops[3].kind {
        OpKind::Shr {
            dst: condition @ VReg::Virtual(_),
            src,
            amount: SrcOperand::Imm(63),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src == combined => condition,
        ref other => panic!("{case:?}: aggregate mask normalization {other:?}"),
    };
    let scalar = match ops[4].kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => scalar,
        ref other => panic!("{case:?}: scalar seed {other:?}"),
    };
    let address = match &ops[5].kind {
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } if *dst == scalar
            && *cond == condition
            && *width == case.broadcast.format.memory_width() =>
        {
            addr
        }
        other => panic!("{case:?}: scalar PredLoad {other:?}"),
    };
    assert!(
        crate::smir::lower::runtime::x86_jit_op_uses_mem_helper(&ops[5].kind),
        "{case:?}: PredLoad must preserve live vector state across its MMU helper"
    );
    assert!(address.is_x86_state_backed_shape(), "{case:?}: {address:?}");
    let loaded = match ops[6].kind {
        OpKind::VBroadcast {
            dst: loaded @ VReg::Virtual(_),
            scalar: source,
            elem,
            lanes,
        } if source == scalar && elem == case.broadcast.format.elem() && lanes == case.lanes() => {
            loaded
        }
        ref other => panic!("{case:?}: source broadcast {other:?}"),
    };
    match (&ops[7].kind, case.broadcast.format) {
        (
            OpKind::X86Fma(X86FmaOp {
                src3,
                mask: actual_mask,
                lanes,
                ..
            }),
            BroadcastFormat::F32 | BroadcastFormat::F64,
        ) => {
            assert_eq!(*src3, loaded, "{case:?}");
            assert_eq!(*actual_mask, Some(mask), "{case:?}");
            assert_eq!(*lanes, case.lanes(), "{case:?}");
        }
        (
            OpKind::X86FP16Fma {
                src3,
                mask: actual_mask,
                lanes,
                ..
            },
            BroadcastFormat::F16,
        ) => {
            assert_eq!(*src3, loaded, "{case:?}");
            assert_eq!(*actual_mask, Some(mask), "{case:?}");
            assert_eq!(*lanes, case.lanes(), "{case:?}");
        }
        (other, _) => panic!("{case:?}: FMA op {other:?}"),
    }
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op.kind, OpKind::Select { .. }))
            .count(),
        usize::from(case.lanes()),
        "{case:?}: one merge/zero selection per destination lane"
    );
}

#[test]
fn masked_broadcast_classifier_exhaustively_rewrites_23_224_320_operands() {
    let mut accepted = 0usize;
    for opcode in PACKED_OPCODES {
        for format in BroadcastFormat::ALL {
            for ll in 0..=2u8 {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for mask in 1..=7u8 {
                            for zeroing in [false, true] {
                                let p0 = (if destination & 8 == 0 { 0x80 } else { 0 })
                                    | 0x60
                                    | (if destination & 16 == 0 { 0x10 } else { 0 })
                                    | format.map();
                                let p1 =
                                    (u8::from(format.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
                                let p2 = (u8::from(zeroing) << 7)
                                    | (ll << 5)
                                    | 0x10
                                    | if source1 & 16 == 0 { 0x08 } else { 0 }
                                    | mask;
                                let bytes = [
                                    0x62,
                                    p0,
                                    p1,
                                    p2,
                                    opcode,
                                    0x40 | ((destination & 7) << 3) | 3,
                                    DISP8,
                                ];
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_packed_fma3_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                let X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } =
                                    encoding.replay
                                else {
                                    panic!("{bytes:02X?}: masked broadcast selected vector replay");
                                };
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, Some(mask), "{bytes:02X?}");
                                assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(
                                    stack_instruction.as_slice(),
                                    &[
                                        0x62,
                                        (p0 & 0x97) | 0x60,
                                        p1 | 0x04,
                                        p2,
                                        opcode,
                                        ((destination & 7) << 3) | 0x04,
                                        0x24,
                                    ],
                                    "{bytes:02X?}"
                                );
                                accepted += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18 * 3 * 3 * 32 * 32 * 7 * 2);

    for format in BroadcastFormat::ALL {
        for ll in 0..=2u8 {
            let base_p2 = (ll << 5) | 0x08;
            for p2 in [base_p2 | 1, base_p2 | 0x81] {
                let bytes = [
                    0x62,
                    0xF0 | format.map(),
                    (u8::from(format.w()) << 7) | 0x75,
                    p2,
                    0x98,
                    0x43,
                    DISP8,
                ];
                let encoding = X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_packed_fma3_memory_encoding()
                    .unwrap_or_else(|| panic!("{bytes:02X?}: masked vector rejected"));
                assert!(
                    matches!(
                        encoding.replay,
                        X86EvexPackedFma3MemoryReplay::MaskedVector { .. }
                    ),
                    "{bytes:02X?}: masked vector selected non-vector replay"
                );
            }

            let p2 = base_p2 | 0x90;
            let bytes = [
                0x62,
                0xF0 | format.map(),
                (u8::from(format.w()) << 7) | 0x75,
                p2,
                0x98,
                0x43,
                DISP8,
            ];
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_packed_fma3_memory_encoding()
                    .is_none(),
                "{bytes:02X?}: z-without-aaa"
            );
        }
    }
}

#[test]
fn masked_broadcast_stack_rewrites_match_independent_llvm_23_encodings() {
    // LLVM 23.0.0, Intel syntax.
    let cases: [(&[u8], &[u8]); 3] = [
        (
            &[0x62, 0xF2, 0x6D, 0x9B, 0xB8, 0x0B],
            &[0x62, 0xF2, 0x6D, 0x9B, 0xB8, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x62, 0xB5, 0x37, 0xB8, 0x03],
            &[0x62, 0x62, 0xB5, 0x37, 0xB8, 0x04, 0x24],
        ),
        (
            &[0x62, 0x66, 0x0D, 0xD6, 0x96, 0x3B],
            &[0x62, 0x66, 0x0D, 0xD6, 0x96, 0x3C, 0x24],
        ),
    ];
    for (source, expected) in cases {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_packed_fma3_memory_encoding()
            .unwrap_or_else(|| panic!("{source:02X?}"));
        let X86EvexPackedFma3MemoryReplay::Broadcast { stack_instruction } = encoding.replay else {
            panic!("{source:02X?}: masked broadcast selected vector replay");
        };
        assert_eq!(stack_instruction.as_slice(), expected, "{source:02X?}");
    }
}

#[test]
fn all_15_876_masked_broadcast_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_masked_cases();
    assert_eq!(cases.len(), 18 * 3 * 3 * 7 * 7 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_masked_case(case), level);
            assert_exact_masked_prefix(&function, case);
            let sequence = masked_sequence(&function, case).unwrap_or_else(|| {
                let index = sequence_index(&function, case);
                panic!(
                    "{level:?} {case:?}: sequence rejected: {:#?}",
                    &function.blocks[0].ops[index..]
                )
            });
            let unoptimized_consumed = if case.zeroing {
                10 + 5 * usize::from(case.lanes())
            } else {
                11 + 6 * usize::from(case.lanes())
            };
            let expected_consumed =
                unoptimized_consumed - usize::from(matches!(level, OptLevel::O2));
            assert_eq!(sequence.consumed, expected_consumed, "{level:?} {case:?}");
            assert_eq!(sequence.memory_offset, 5, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_size,
                case.broadcast.format.memory_width().bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.writemask,
                Some(case.mask),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.zeroing, case.zeroing,
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case.broadcast);
            let expected = case.stack_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            let lane_mask = u32::try_from(case.lane_mask()).unwrap().to_le_bytes();
            let guard = [
                0x9C,
                0x50,
                0xC4,
                0xE1,
                0xFB,
                0x93,
                0xC0 | case.mask,
                0xF7,
                0xC0,
                lane_mask[0],
                lane_mask[1],
                lane_mask[2],
                lane_mask[3],
                0x0F,
                0x84,
            ];
            let guard_at = code
                .windows(guard.len())
                .position(|window| window == guard)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: incomplete live-K guard"));
            let jz_disp = guard_at + guard.len();
            assert_eq!(
                &code[jz_disp + 4..jz_disp + 6],
                &[0x58, 0x9D],
                "{level:?} {case:?}: active cleanup"
            );
            let inactive = (jz_disp + 4) as i64
                + i64::from(i32::from_le_bytes(
                    code[jz_disp..jz_disp + 4].try_into().unwrap(),
                ));
            let inactive = usize::try_from(inactive).expect("forward inactive target");
            assert_eq!(
                &code[inactive..inactive + 2],
                &[0x58, 0x9D],
                "{level:?} {case:?}: inactive cleanup"
            );
            let execute = inactive as i64
                + i64::from(i32::from_le_bytes(
                    code[inactive - 4..inactive].try_into().unwrap(),
                ));
            assert_eq!(
                usize::try_from(execute).unwrap(),
                inactive + 2,
                "{level:?} {case:?}: both paths join at native replay"
            );
            assert_eq!(
                &code[inactive + 2..inactive + 2 + expected.len()],
                &expected,
                "{level:?} {case:?}: exact join target"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 15_876 * LEVELS.len());
}

fn assert_masked_rejected(name: &str, case: MaskedBroadcastCase, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_packed_fma3_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: sequence classifier admitted malformed masked broadcast"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed masked broadcast"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name} {case:?}: lowerer accepted malformed masked broadcast"
    );
}

#[test]
fn masked_broadcast_sequence_fails_closed_for_fault_mask_tail_and_ssa_mutations() {
    let case = MaskedBroadcastCase {
        broadcast: BroadcastCase {
            opcode: 0x98,
            format: BroadcastFormat::F32,
            width: VecWidth::V256,
            form: MemoryForm::Low,
        },
        mask: 3,
        zeroing: false,
    };
    let base = lift_masked_case(case);
    assert_eq!(sequence_index(&base, case), 0);
    assert!(masked_sequence(&base, case).is_some());
    let raw = match base.blocks[0].ops[7].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing metadata", missing_metadata));

    let mut metadata_mask = base.clone();
    let mut bytes = case.bytes();
    bytes[3] ^= 1;
    metadata_mask
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("metadata mask", metadata_mask));

    let mut lane_mask = base.clone();
    if let OpKind::And { src2, .. } = &mut lane_mask.blocks[0].ops[0].kind {
        *src2 = SrcOperand::Imm(case.lane_mask() as i64 ^ 1);
    }
    malformed.push(("aggregate lane mask", lane_mask));

    let mut predicate_negation = base.clone();
    if let OpKind::Neg { src, .. } = &mut predicate_negation.blocks[0].ops[1].kind {
        *src = VReg::Virtual(VirtualId(0xFFFD));
    }
    malformed.push(("aggregate mask negation", predicate_negation));

    let mut predicate_or = base.clone();
    if let OpKind::Or { src2, .. } = &mut predicate_or.blocks[0].ops[2].kind {
        *src2 = SrcOperand::Imm(0);
    }
    malformed.push(("aggregate mask combination", predicate_or));

    let mut predicate_shift = base.clone();
    if let OpKind::Shr { amount, .. } = &mut predicate_shift.blocks[0].ops[3].kind {
        *amount = SrcOperand::Imm(62);
    }
    malformed.push(("aggregate mask normalization", predicate_shift));

    let mut seed = base.clone();
    if let OpKind::Mov { src, .. } = &mut seed.blocks[0].ops[4].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("nonzero scalar seed", seed));

    let mut pred_condition = base.clone();
    if let OpKind::PredLoad { cond, .. } = &mut pred_condition.blocks[0].ops[5].kind {
        *cond = VReg::Imm(1);
    }
    malformed.push(("PredLoad condition", pred_condition));

    let mut pred_address = base.clone();
    if let OpKind::PredLoad { addr, .. } = &mut pred_address.blocks[0].ops[5].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("PredLoad address", pred_address));

    let mut pred_width = base.clone();
    if let OpKind::PredLoad { width, .. } = &mut pred_width.blocks[0].ops[5].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("PredLoad width", pred_width));

    let mut pred_sign = base.clone();
    if let OpKind::PredLoad { signed, .. } = &mut pred_sign.blocks[0].ops[5].kind {
        *signed = SignExtend::Sign;
    }
    malformed.push(("PredLoad sign", pred_sign));

    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[6].kind {
        *lanes -= 1;
    }
    malformed.push(("source broadcast lanes", broadcast_lanes));

    let mut fma_mask = base.clone();
    if let OpKind::X86Fma(fma) = &mut fma_mask.blocks[0].ops[7].kind {
        fma.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(4))));
    }
    malformed.push(("FMA mask", fma_mask));

    let mut old_source = base.clone();
    if let OpKind::VMov { src, .. } = &mut old_source.blocks[0].ops[8].kind {
        *src = vector(2, case.broadcast.width);
    }
    malformed.push(("merge source", old_source));

    let mut result_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut result_zero.blocks[0].ops[9].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("result zero", result_zero));

    let mut mask_shift = base.clone();
    if let OpKind::Shr { amount, .. } = &mut mask_shift.blocks[0].ops[11].kind {
        *amount = SrcOperand::Imm(1);
    }
    malformed.push(("lane-zero mask shift", mask_shift));

    let mut active_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut active_lane.blocks[0].ops[13].kind {
        *lane = 1;
    }
    malformed.push(("active result lane", active_lane));

    let mut inactive_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut inactive_lane.blocks[0].ops[14].kind {
        *lane = 1;
    }
    malformed.push(("inactive merge lane", inactive_lane));

    let mut select_condition = base.clone();
    if let OpKind::Select { cond, .. } = &mut select_condition.blocks[0].ops[15].kind {
        *cond = VReg::Imm(1);
    }
    malformed.push(("lane select condition", select_condition));

    let mut insert_lane = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut insert_lane.blocks[0].ops[16].kind {
        *lane = 1;
    }
    malformed.push(("result insert lane", insert_lane));

    let mut raw_reused = base.clone();
    raw_reused.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFE),
        PC + 1,
        OpKind::VMov {
            dst: vector(2, case.broadcast.width),
            src: raw,
            width: case.broadcast.width,
        },
    ));
    malformed.push(("raw result reused", raw_reused));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("same-PC tail", same_pc_tail));

    let mut missing_tail = base.clone();
    missing_tail.blocks[0].ops.pop();
    malformed.push(("missing result tail", missing_tail));

    for (name, function) in malformed {
        assert_masked_rejected(name, case, &function);
    }

    let zero_case = MaskedBroadcastCase {
        zeroing: true,
        ..case
    };
    let zero = lift_masked_case(zero_case);
    assert!(masked_sequence(&zero, zero_case).is_some());
    let mut zero_fallback = zero;
    let zero_register = match zero_fallback.blocks[0].ops[8].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let select = zero_fallback.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::Select { .. }))
        .unwrap();
    if let OpKind::Select { src_false, .. } = &mut select.kind {
        *src_false = VReg::Imm(1);
    }
    assert_ne!(zero_register, VReg::Imm(1));
    assert_masked_rejected("zeroing fallback", zero_case, &zero_fallback);
}

fn masked_guest_regs(
    case: MaskedBroadcastCase,
    ordinal: usize,
    applicable_active: bool,
) -> GuestRegs {
    let mut registers = full_guest_regs(case.broadcast, ordinal);
    let applicable = if applicable_active {
        1 | ((ordinal as u64).wrapping_mul(0x9E37_79B9) & case.lane_mask())
    } else {
        0
    };
    registers.k[usize::from(case.mask)] = !case.lane_mask() | applicable;
    registers
}

#[test]
fn interpreter_o0_o1_o2_matches_all_15_876_active_merge_zero_and_suppressed_shapes() {
    let cases = all_masked_cases();
    assert_eq!(cases.len(), 15_876);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let scalar = case.broadcast.format.scalar_bits(ordinal & 1 != 0);
        let active_initial = masked_guest_regs(case, ordinal, true);
        let suppressed_initial = masked_guest_regs(case, ordinal, false);
        let address = memory_address(case.broadcast, &active_initial);
        assert!(
            address + case.broadcast.format.memory_size() as u64 <= 0x10000,
            "{case:?}: address {address:#x}"
        );
        let baseline = optimize(lift_masked_case(case), OptLevel::O0);
        let active_expected = interpreter_success(
            &baseline,
            &active_initial,
            scalar,
            address,
            case.broadcast.format,
        );
        let suppressed_expected = interpreter_success(
            &baseline,
            &suppressed_initial,
            scalar,
            address,
            case.broadcast.format,
        );

        let destination = usize::from(case.broadcast.destination());
        let mut exact_suppressed = suppressed_initial.zmm[destination];
        let active_words = case.broadcast.width.bytes() as usize / 8;
        if case.zeroing {
            exact_suppressed[..active_words].fill(0);
        }
        exact_suppressed[active_words..].fill(0);
        assert_eq!(
            suppressed_expected.zmm[destination], exact_suppressed,
            "{case:?}: irrelevant high K bits affected merge/zero semantics"
        );
        assert_eq!(
            suppressed_expected.rflags, suppressed_initial.rflags,
            "{case:?}"
        );
        assert_eq!(suppressed_expected.k, suppressed_initial.k, "{case:?}");

        for level in LEVELS {
            let function = optimize(lift_masked_case(case), level);
            let active = interpreter_success(
                &function,
                &active_initial,
                scalar,
                address,
                case.broadcast.format,
            );
            let suppressed = interpreter_success(
                &function,
                &suppressed_initial,
                scalar,
                address,
                case.broadcast.format,
            );
            assert_eq!(active, active_expected, "{level:?} {case:?}: active");
            assert_eq!(
                suppressed, suppressed_expected,
                "{level:?} {case:?}: suppressed"
            );
            executions += 2;
        }
    }
    assert_eq!(executions, 15_876 * LEVELS.len() * 2);
}

#[test]
fn interpreter_ignores_high_mask_bits_and_suppresses_unmapped_broadcast_sources() {
    let cases = [
        MaskedBroadcastCase {
            broadcast: BroadcastCase {
                opcode: 0x98,
                format: BroadcastFormat::F32,
                width: VecWidth::V128,
                form: MemoryForm::Low,
            },
            mask: 1,
            zeroing: false,
        },
        MaskedBroadcastCase {
            broadcast: BroadcastCase {
                opcode: 0xBE,
                format: BroadcastFormat::F16,
                width: VecWidth::V512,
                form: MemoryForm::Low,
            },
            mask: 7,
            zeroing: true,
        },
    ];
    for case in cases {
        let function = lift_masked_case(case);
        let execute = |applicable_active: bool| {
            let mut initial = masked_guest_regs(case, 0, applicable_active);
            initial.gpr[3] = 0x1_0000;
            let mut context = SmirContext::new_x86_64();
            if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
                x86.gpr = initial.gpr;
                for (index, value) in initial.zmm.iter().enumerate() {
                    x86.xmm[index][..8].copy_from_slice(value);
                }
                x86.k = initial.k;
                x86.rflags = initial.rflags;
                x86.mxcsr = initial.mxcsr;
                x86.fs_base = initial.fs_base;
                x86.gs_base = initial.gs_base;
                x86.apx_enabled = true;
            }
            context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
            context.flags.lazy = None;
            let mut memory = FlatMemory::new(1);
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0])
        };

        assert!(matches!(
            execute(false),
            BlockResult::Exit(ExitReason::Return { .. })
        ));
        assert!(
            !matches!(execute(true), BlockResult::Exit(ExitReason::Return { .. })),
            "{case:?}: an applicable active lane must attempt the scalar read"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn masked_native_cases() -> Vec<MaskedBroadcastCase> {
    let mut cases = Vec::new();
    for broadcast in native_cases() {
        for mask in 1..=7 {
            for zeroing in [false, true] {
                cases.push(MaskedBroadcastCase {
                    broadcast,
                    mask,
                    zeroing,
                });
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_masked_broadcast_matches_interpretation_suppresses_helpers_and_faults_precisely() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native masked packed FMA3 broadcast: host lacks AVX512F/BW");
        return;
    }

    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let cases: Vec<_> = masked_native_cases()
        .into_iter()
        .filter(|case| case.broadcast.width == VecWidth::V512 || has_vl)
        .filter(|case| case.broadcast.format != BroadcastFormat::F16 || has_fp16)
        .collect();
    assert!(!cases.is_empty());
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut suppressed = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_masked_case(case), level);
            let (code, entry) = lower(&function, case.broadcast);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let scalar = case.broadcast.format.scalar_bits(ordinal & 1 != 0);

            let mut success_context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut success_registers = masked_guest_regs(case, ordinal, true);
            let address = memory_address(case.broadcast, &success_registers);
            success_registers.ctx =
                (&mut success_context as *mut ScalarMemoryContext).cast::<()>() as usize as u64;
            success_registers.load_fn = scalar_load_helper as usize as u64;
            let mut success_expected = interpreter_success(
                &function,
                &success_registers,
                scalar,
                address,
                case.broadcast.format,
            );

            exec.run(entry, &mut success_registers);
            success_expected.host_mxcsr = success_registers.host_mxcsr;
            assert_eq!(
                success_registers, success_expected,
                "{level:?} {case:?}: active success"
            );
            assert_eq!(success_context.calls, 1, "{level:?} {case:?}");
            assert_eq!(success_context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                success_context.last_size,
                case.broadcast.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(success_context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut suppressed_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut suppressed_registers = masked_guest_regs(case, ordinal, false);
            let suppressed_address = memory_address(case.broadcast, &suppressed_registers);
            suppressed_registers.ctx =
                (&mut suppressed_context as *mut ScalarMemoryContext).cast::<()>() as usize as u64;
            suppressed_registers.load_fn = scalar_load_helper as usize as u64;
            let mut suppressed_expected = interpreter_success(
                &function,
                &suppressed_registers,
                scalar,
                suppressed_address,
                case.broadcast.format,
            );

            exec.run(entry, &mut suppressed_registers);
            suppressed_expected.host_mxcsr = suppressed_registers.host_mxcsr;
            assert_eq!(
                suppressed_registers, suppressed_expected,
                "{level:?} {case:?}: high-only K bits must suppress the helper"
            );
            assert_eq!(
                suppressed_context.calls, 0,
                "{level:?} {case:?}: suppressed helper call"
            );
            suppressed += 1;

            let mut fault_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = masked_guest_regs(case, ordinal, true);
            let fault_address = memory_address(case.broadcast, &fault_registers);
            fault_registers.ctx =
                (&mut fault_context as *mut ScalarMemoryContext).cast::<()>() as usize as u64;
            fault_registers.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault_registers;
            fault_expected.exit_pc = PC;

            exec.run(entry, &mut fault_registers);
            fault_expected.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, fault_expected,
                "{level:?} {case:?}: active fault committed state"
            );
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}");
            assert_eq!(fault_context.last_addr, fault_address, "{level:?} {case:?}");
            assert_eq!(
                fault_context.last_size,
                case.broadcast.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(successes, suppressed);
    assert_eq!(successes, faults);
}
