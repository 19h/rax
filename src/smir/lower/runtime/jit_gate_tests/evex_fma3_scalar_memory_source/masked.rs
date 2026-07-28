//! Exact helper-backed writemasked scalar EVEX FMA3 memory-source coverage.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaskedScalarFmaCase {
    scalar: ScalarFmaCase,
    mask: u8,
    zeroing: bool,
}

impl MaskedScalarFmaCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = self.scalar.bytes();
        let evex = usize::from(matches!(self.scalar.form, MemoryForm::FsAddr32Sib)) * 2;
        bytes[evex + 3] |= self.mask | (u8::from(self.zeroing) << 7);
        bytes
    }

    fn stack_instruction(self) -> [u8; 7] {
        let mut instruction = self.scalar.stack_instruction();
        instruction[3] |= self.mask | (u8::from(self.zeroing) << 7);
        instruction
    }
}

fn lift_masked_case(case: MaskedScalarFmaCase) -> SmirFunction {
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
        X86InstructionBytes::new(&bytes).expect("masked EVEX scalar FMA3 provenance"),
    );
    function
}

fn masked_sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::K(_))),
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    ..
                }
            )
        })
        .expect("masked scalar EVEX FMA3 bit-0 condition")
}

fn masked_sequence(
    function: &SmirFunction,
) -> Option<crate::smir::lower::runtime::X86JitEvexScalarFma3MemorySequence> {
    let index = masked_sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_scalar_fma3_memory_sequence(
        &function.blocks[0],
        index,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn all_masked_cases() -> Vec<MaskedScalarFmaCase> {
    let mut cases = Vec::new();
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3 {
                for form in MemoryForm::ALL {
                    for mask in 1..=7 {
                        for zeroing in [false, true] {
                            cases.push(MaskedScalarFmaCase {
                                scalar: ScalarFmaCase {
                                    opcode,
                                    format,
                                    ll,
                                    form,
                                },
                                mask,
                                zeroing,
                            });
                        }
                    }
                }
            }
        }
    }
    cases
}

fn assert_exact_masked_sequence(function: &SmirFunction, case: MaskedScalarFmaCase) {
    let index = masked_sequence_index(function);
    let ops = &function.blocks[0].ops[index..];
    let elem = case.scalar.format.elem();
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    assert_eq!(ops.len(), 2 * xmm_lanes + 9, "{case:?}: {ops:#?}");
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");

    let condition = match &ops[0].kind {
        OpKind::And {
            dst: condition @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: crate::smir::ir::flags::FlagUpdate::None,
        } => {
            assert_eq!(
                *src1,
                VReg::Arch(ArchReg::X86(X86Reg::K(case.mask))),
                "{case:?}"
            );
            *condition
        }
        other => panic!("{case:?}: expected bit-0 And, got {other:?}"),
    };
    let loaded = match &ops[1].kind {
        OpKind::Mov {
            dst: loaded @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *loaded,
        other => panic!("{case:?}: expected zero seed, got {other:?}"),
    };
    match &ops[2].kind {
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } => {
            assert_eq!(*dst, loaded, "{case:?}");
            assert_eq!(*cond, condition, "{case:?}");
            assert_eq!(*width, case.scalar.format.memory_width(), "{case:?}");
            assert!(addr.is_x86_state_backed_shape(), "{case:?}: {addr:?}");
        }
        other => panic!("{case:?}: expected PredLoad, got {other:?}"),
    }
    let source_vector = match &ops[3].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } => {
            assert_eq!(*scalar, loaded, "{case:?}");
            assert_eq!(*broadcast_elem, elem, "{case:?}");
            *vector
        }
        other => panic!("{case:?}: expected scalar VBroadcast, got {other:?}"),
    };

    let (raw, src1, src2, src3, mask, kind, order, round, lanes) = match &ops[4].kind {
        OpKind::X86Fma(X86FmaOp {
            dst,
            src1,
            src2,
            src3,
            mask,
            elem: fma_elem,
            kind,
            order,
            round,
            lanes,
        }) if elem != VecElementType::F16 => {
            assert_eq!(*fma_elem, elem, "{case:?}");
            (
                *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
            )
        }
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        other => panic!("{case:?}: expected masked scalar FMA, got {other:?}"),
    };
    assert!(matches!(raw, VReg::Virtual(_)), "{case:?}");
    assert_eq!(src1, xmm(case.scalar.destination()), "{case:?}");
    assert_eq!(src2, xmm(case.scalar.source1()), "{case:?}");
    assert_eq!(src3, source_vector, "{case:?}");
    assert_eq!(
        mask,
        Some(VReg::Arch(ArchReg::X86(X86Reg::K(case.mask)))),
        "{case:?}"
    );
    assert_eq!(kind, case.scalar.kind(), "{case:?}");
    assert_eq!(order, case.scalar.order(), "{case:?}");
    assert_eq!(round, FpRoundMode::Dynamic, "{case:?}");
    assert_eq!(lanes, 1, "{case:?}");

    let scalar_result = match &ops[5].kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, raw, "{case:?}");
            assert_eq!(*extract_elem, elem, "{case:?}");
            *dst
        }
        other => panic!("{case:?}: expected low result extract, got {other:?}"),
    };
    let fallback = if case.zeroing {
        match &ops[6].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width,
            } => {
                assert_eq!(
                    *width,
                    match elem {
                        VecElementType::F16 => OpWidth::W16,
                        VecElementType::F32 => OpWidth::W32,
                        VecElementType::F64 => OpWidth::W64,
                        _ => unreachable!(),
                    },
                    "{case:?}"
                );
                *dst
            }
            other => panic!("{case:?}: expected zero fallback, got {other:?}"),
        }
    } else {
        match &ops[6].kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: fallback_elem,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*vec, xmm(case.scalar.destination()), "{case:?}");
                assert_eq!(*fallback_elem, elem, "{case:?}");
                *dst
            }
            other => panic!("{case:?}: expected merge fallback, got {other:?}"),
        }
    };
    match &ops[7].kind {
        OpKind::Select {
            cond,
            src_true,
            src_false,
            ..
        } => {
            assert_eq!(*cond, condition, "{case:?}");
            assert_eq!(*src_true, scalar_result, "{case:?}");
            assert_eq!(*src_false, fallback, "{case:?}");
        }
        other => panic!("{case:?}: expected mask Select, got {other:?}"),
    }
}

#[test]
fn masked_scalar_evex_fma3_byte_classifier_exhaustively_rewrites_2_064_384_operands() {
    let mut accepted = 0usize;
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3u8 {
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
                                    .evex_scalar_fma3_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.writemask, Some(mask), "{bytes:02X?}");
                                assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.stack_instruction.as_slice(),
                                    &[
                                        0x62,
                                        (p0 & 0x97) | 0x60,
                                        p1 | 0x04,
                                        p2 & 0x8F,
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
    assert_eq!(accepted, 12 * 3 * 4 * 32 * 32 * 7 * 2);
}

#[test]
fn masked_stack_rewrite_matches_independent_llvm_23_encodings() {
    let cases = [
        (
            MaskedScalarFmaCase {
                scalar: ScalarFmaCase {
                    opcode: 0x99,
                    format: ScalarFormat::F32,
                    ll: 3,
                    form: MemoryForm::Low,
                },
                mask: 1,
                zeroing: true,
            },
            [0x62, 0xF2, 0x75, 0x89, 0x99, 0x04, 0x24],
        ),
        (
            MaskedScalarFmaCase {
                scalar: ScalarFmaCase {
                    opcode: 0xAB,
                    format: ScalarFormat::F64,
                    ll: 2,
                    form: MemoryForm::High,
                },
                mask: 7,
                zeroing: false,
            },
            [0x62, 0x62, 0xB5, 0x07, 0xAB, 0x04, 0x24],
        ),
        (
            MaskedScalarFmaCase {
                scalar: ScalarFmaCase {
                    opcode: 0xBD,
                    format: ScalarFormat::F16,
                    ll: 1,
                    form: MemoryForm::DestinationSourceAlias,
                },
                mask: 4,
                zeroing: true,
            },
            [0x62, 0xE6, 0x75, 0x84, 0xBD, 0x0C, 0x24],
        ),
    ];
    for (case, llvm) in cases {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_scalar_fma3_memory_encoding()
            .unwrap();
        assert_eq!(encoding.stack_instruction.as_slice(), llvm, "{case:?}");
        assert_eq!(case.stack_instruction(), llvm, "{case:?}");
    }
}

#[test]
fn all_14_112_masked_scalar_shapes_lift_optimize_admit_and_lower_exactly() {
    let cases = all_masked_cases();
    assert_eq!(cases.len(), 12 * 3 * 4 * 7 * 7 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_masked_case(case), level);
            assert_exact_masked_sequence(&function, case);
            let sequence = masked_sequence(&function)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: rejected"));
            assert_eq!(
                sequence.consumed,
                2 * VecWidth::V128.lanes(case.scalar.format.elem()) as usize + 9,
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.load_offset, 2, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_width,
                case.scalar.format.memory_width(),
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

            let (code, _) = lower(&function, case.scalar);
            let expected = case.stack_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            let kmovq = [0xC4, 0xE1, 0xFB, 0x93, 0xC0 | case.mask];
            assert!(
                code.windows(kmovq.len()).any(|window| window == kmovq),
                "{level:?} {case:?}: missing live K{} bit-0 guard in {code:02X?}",
                case.mask
            );
            let guard = [
                0x9C,
                0x50,
                0xC4,
                0xE1,
                0xFB,
                0x93,
                0xC0 | case.mask,
                0x48,
                0xF7,
                0xC0,
                1,
                0,
                0,
                0,
                0x0F,
                0x84,
            ];
            let guard_at = code
                .windows(guard.len())
                .position(|window| window == guard)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: incomplete bit-0 guard"));
            let jz_disp = guard_at + guard.len();
            assert_eq!(
                &code[jz_disp + 4..jz_disp + 6],
                &[0x58, 0x9D],
                "{level:?} {case:?}: active path must restore RAX/RFLAGS"
            );
            let inactive = (jz_disp + 4) as i64
                + i64::from(i32::from_le_bytes(
                    code[jz_disp..jz_disp + 4].try_into().unwrap(),
                ));
            let inactive = usize::try_from(inactive).expect("forward inactive target");
            assert_eq!(
                &code[inactive..inactive + 2],
                &[0x58, 0x9D],
                "{level:?} {case:?}: inactive path must restore RAX/RFLAGS"
            );
            assert_eq!(
                code[inactive - 5],
                0xE9,
                "{level:?} {case:?}: helper path must bypass inactive cleanup"
            );
            let execute = inactive as i64
                + i64::from(i32::from_le_bytes(
                    code[inactive - 4..inactive].try_into().unwrap(),
                ));
            assert_eq!(
                usize::try_from(execute).unwrap(),
                inactive + 2,
                "{level:?} {case:?}: both paths must join at native replay"
            );
            assert_eq!(
                &code[inactive + 2..inactive + 2 + expected.len()],
                &expected,
                "{level:?} {case:?}: join target must be exact masked replay"
            );
            assert!(
                code.windows(5)
                    .any(|window| window
                        == [0xBA, case.scalar.format.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing scalar helper size"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 14_112 * LEVELS.len());
}

fn assert_masked_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_evex_scalar_fma3_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: sequence classifier admitted malformed masked scalar EVEX FMA3"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed masked scalar EVEX FMA3"
    );
}

#[test]
fn masked_scalar_sequence_fails_closed_for_mask_fault_and_ssa_mutations() {
    let merge_case = MaskedScalarFmaCase {
        scalar: ScalarFmaCase {
            opcode: 0x99,
            format: ScalarFormat::F32,
            ll: 0,
            form: MemoryForm::Low,
        },
        mask: 1,
        zeroing: false,
    };
    let merge = lift_masked_case(merge_case);
    assert_eq!(merge.blocks[0].ops.len(), 17);
    let (definitions, uses) = virtual_counts(&merge);
    assert!(
        x86_jit_evex_scalar_fma3_memory_sequence(
            &merge.blocks[0],
            0,
            false,
            &merge.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );

    let mut malformed: Vec<(&str, SmirFunction)> = Vec::new();

    let mut missing_metadata = merge.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing instruction provenance", missing_metadata));

    let mut wrong_mask_metadata = merge.clone();
    let mut bytes = merge_case.bytes();
    bytes[3] = (bytes[3] & !7) | 2;
    wrong_mask_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("metadata mask mismatch", wrong_mask_metadata));

    let mut zeroing_metadata = merge.clone();
    let mut bytes = merge_case.bytes();
    bytes[3] |= 0x80;
    zeroing_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    malformed.push(("metadata zeroing mismatch", zeroing_metadata));

    let mut condition_mask = merge.clone();
    if let OpKind::And { src1, .. } = &mut condition_mask.blocks[0].ops[0].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    }
    malformed.push(("wrong condition mask", condition_mask));

    let mut condition_bit = merge.clone();
    if let OpKind::And { src2, .. } = &mut condition_bit.blocks[0].ops[0].kind {
        *src2 = SrcOperand::Imm(2);
    }
    malformed.push(("condition tests more than bit zero", condition_bit));

    let mut condition_width = merge.clone();
    if let OpKind::And { width, .. } = &mut condition_width.blocks[0].ops[0].kind {
        *width = OpWidth::W32;
    }
    malformed.push(("wrong condition width", condition_width));

    let mut seed_nonzero = merge.clone();
    if let OpKind::Mov { src, .. } = &mut seed_nonzero.blocks[0].ops[1].kind {
        *src = SrcOperand::Imm(1);
    }
    malformed.push(("nonzero suppressed-load seed", seed_nonzero));

    let mut pred_wrong_condition = merge.clone();
    if let OpKind::PredLoad { cond, .. } = &mut pred_wrong_condition.blocks[0].ops[2].kind {
        *cond = VReg::Virtual(VirtualId(999));
    }
    malformed.push(("PredLoad bypasses mask condition", pred_wrong_condition));

    let mut pred_virtual_address = merge.clone();
    if let OpKind::PredLoad { addr, .. } = &mut pred_virtual_address.blocks[0].ops[2].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(999)));
    }
    malformed.push(("PredLoad virtual address component", pred_virtual_address));

    let mut pred_signed = merge.clone();
    if let OpKind::PredLoad { signed, .. } = &mut pred_signed.blocks[0].ops[2].kind {
        *signed = SignExtend::Sign;
    }
    malformed.push(("signed PredLoad", pred_signed));

    let mut pred_width = merge.clone();
    if let OpKind::PredLoad { width, .. } = &mut pred_width.blocks[0].ops[2].kind {
        *width = MemWidth::B8;
    }
    malformed.push(("wrong PredLoad width", pred_width));

    let mut fma_mask = merge.clone();
    if let OpKind::X86Fma(op) = &mut fma_mask.blocks[0].ops[4].kind {
        op.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(2))));
    }
    malformed.push(("FMA mask mismatch", fma_mask));

    let mut fma_source = merge.clone();
    if let OpKind::X86Fma(op) = &mut fma_source.blocks[0].ops[4].kind {
        op.src2 = xmm(2);
    }
    malformed.push(("FMA source mismatch", fma_source));

    let mut merge_fallback = merge.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut merge_fallback.blocks[0].ops[6].kind {
        *vec = xmm(1);
    }
    malformed.push(("merge fallback uses wrong destination", merge_fallback));

    let mut select_condition = merge.clone();
    if let OpKind::Select { cond, .. } = &mut select_condition.blocks[0].ops[7].kind {
        *cond = VReg::Virtual(VirtualId(999));
    }
    malformed.push(("Select bypasses mask condition", select_condition));

    let mut select_arms = merge.clone();
    if let OpKind::Select {
        src_true,
        src_false,
        ..
    } = &mut select_arms.blocks[0].ops[7].kind
    {
        std::mem::swap(src_true, src_false);
    }
    malformed.push(("Select arms reversed", select_arms));

    let mut same_pc_tail = merge.clone();
    let tail = same_pc_tail.blocks[0].ops[16].clone();
    same_pc_tail.blocks[0].ops.push(tail);
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_masked_rejected(name, &function);
    }

    let zero_case = MaskedScalarFmaCase {
        zeroing: true,
        ..merge_case
    };
    let zero = lift_masked_case(zero_case);
    assert!(masked_sequence(&zero).is_some());
    let mut nonzero_fallback = zero;
    if let OpKind::Mov { src, .. } = &mut nonzero_fallback.blocks[0].ops[6].kind {
        *src = SrcOperand::Imm(1);
    }
    assert_masked_rejected("nonzero zero-mask fallback", &nonzero_fallback);
}

fn masked_guest_regs(
    case: MaskedScalarFmaCase,
    ordinal: usize,
    data_case: usize,
    active: bool,
) -> GuestRegs {
    let mut registers = full_guest_regs(case.scalar, ordinal, data_case);
    let mask = &mut registers.k[usize::from(case.mask)];
    *mask = (*mask & !1) | u64::from(active);
    registers
}

fn scalar_element_mask(format: ScalarFormat) -> u64 {
    match format {
        ScalarFormat::F16 => u64::from(u16::MAX),
        ScalarFormat::F32 => u64::from(u32::MAX),
        ScalarFormat::F64 => u64::MAX,
    }
}

#[test]
fn interpreter_o0_o1_o2_match_all_14_112_active_merge_zero_and_suppressed_shapes() {
    let cases = all_masked_cases();
    assert_eq!(cases.len(), 14_112);
    let mut executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = source_words(case.scalar.format, 0, 2);
        let active_initial = masked_guest_regs(case, ordinal, 0, true);
        let inactive_initial = masked_guest_regs(case, ordinal, 0, false);
        let address = memory_address(case.scalar, &active_initial);
        assert!(
            address + case.scalar.format.memory_size() as u64 <= 0x10000,
            "{case:?}: address {address:#x}"
        );

        let baseline = optimize(lift_masked_case(case), OptLevel::O0);
        let active_expected = interpreter_success(
            &baseline,
            &active_initial,
            source,
            address,
            case.scalar.format,
        );
        let inactive_expected = interpreter_success(
            &baseline,
            &inactive_initial,
            source,
            address,
            case.scalar.format,
        );

        let destination = usize::from(case.scalar.destination());
        let mut exact_inactive = inactive_initial.zmm[destination];
        if case.zeroing {
            exact_inactive[0] &= !scalar_element_mask(case.scalar.format);
        }
        exact_inactive[2..].fill(0);
        assert_eq!(
            inactive_expected.zmm[destination], exact_inactive,
            "{case:?}: inactive merge/zero or upper-lane semantics"
        );
        assert_eq!(
            inactive_expected.rflags, inactive_initial.rflags,
            "{case:?}"
        );
        assert_eq!(inactive_expected.k, inactive_initial.k, "{case:?}");

        for level in LEVELS {
            let function = optimize(lift_masked_case(case), level);
            let active = interpreter_success(
                &function,
                &active_initial,
                source,
                address,
                case.scalar.format,
            );
            let inactive = interpreter_success(
                &function,
                &inactive_initial,
                source,
                address,
                case.scalar.format,
            );
            assert_eq!(active, active_expected, "{level:?} {case:?}: active");
            assert_eq!(inactive, inactive_expected, "{level:?} {case:?}: inactive");
            executions += 2;
        }
    }
    assert_eq!(executions, 14_112 * LEVELS.len() * 2);
}

#[test]
fn interpreter_inactive_mask_suppresses_an_unmapped_scalar_source() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let case = MaskedScalarFmaCase {
        scalar: ScalarFmaCase {
            opcode: 0x99,
            format: ScalarFormat::F32,
            ll: 3,
            form: MemoryForm::Low,
        },
        mask: 7,
        zeroing: true,
    };
    let function = lift_masked_case(case);
    let execute = |active: bool| {
        let mut initial = masked_guest_regs(case, 0, 0, active);
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
        "active bit 0 must attempt the unmapped scalar read"
    );
}

#[test]
fn active_masks_consume_memory_while_inactive_masks_ignore_its_value() {
    let mut checked = 0usize;
    for (ordinal, opcode) in SCALAR_OPCODES.into_iter().enumerate() {
        for (format_ordinal, format) in ScalarFormat::ALL.into_iter().enumerate() {
            for zeroing in [false, true] {
                let case = MaskedScalarFmaCase {
                    scalar: ScalarFmaCase {
                        opcode,
                        format,
                        ll: ((ordinal + format_ordinal) & 3) as u8,
                        form: MemoryForm::Low,
                    },
                    mask: ((ordinal + format_ordinal) % 7 + 1) as u8,
                    zeroing,
                };
                let function = lift_masked_case(case);
                let initial_active = masked_guest_regs(case, ordinal, 0, true);
                let initial_inactive = masked_guest_regs(case, ordinal, 0, false);
                let address = memory_address(case.scalar, &initial_active);
                let source = source_words(format, 0, 2);
                let alternate = source_words(format, 0, 1);
                let active =
                    interpreter_success(&function, &initial_active, source, address, format);
                let active_alternate =
                    interpreter_success(&function, &initial_active, alternate, address, format);
                assert_ne!(
                    active.zmm[usize::from(case.scalar.destination())],
                    active_alternate.zmm[usize::from(case.scalar.destination())],
                    "{case:?}: active FMA did not consume its scalar memory source"
                );

                let inactive =
                    interpreter_success(&function, &initial_inactive, source, address, format);
                let inactive_alternate =
                    interpreter_success(&function, &initial_inactive, alternate, address, format);
                assert_eq!(
                    inactive, inactive_alternate,
                    "{case:?}: inactive FMA observed a suppressed memory value"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 12 * 3 * 2);
}

#[cfg(target_arch = "x86_64")]
fn masked_native_cases() -> Vec<MaskedScalarFmaCase> {
    let mut cases = Vec::new();
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for ll in 0..=3 {
                for form in MemoryForm::NATIVE {
                    for mask in 1..=7 {
                        for zeroing in [false, true] {
                            cases.push(MaskedScalarFmaCase {
                                scalar: ScalarFmaCase {
                                    opcode,
                                    format,
                                    ll,
                                    form,
                                },
                                mask,
                                zeroing,
                            });
                        }
                    }
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_masked_scalar_memory_matches_interpretation_suppresses_helpers_and_faults_precisely() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native masked scalar EVEX FMA3 differential: host lacks AVX512F/BW");
        return;
    }

    let cases = masked_native_cases();
    assert_eq!(cases.len(), 12 * 3 * 4 * 3 * 7 * 2);
    let mut successes = 0usize;
    let mut suppressed = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        if case.scalar.format == ScalarFormat::F16 && !std::is_x86_feature_detected!("avx512fp16") {
            continue;
        }
        for level in NATIVE_LEVELS {
            let function = optimize(lift_masked_case(case), level);
            let (code, entry) = lower(&function, case.scalar);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_words(case.scalar.format, ordinal, 2);
            let scalar = scalar_bits(case.scalar.format, ordinal, 2);

            let mut success_context = ScalarMemoryContext {
                value: scalar,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut success_registers = masked_guest_regs(case, ordinal, ordinal, true);
            let address = memory_address(case.scalar, &success_registers);
            success_registers.ctx =
                (&mut success_context as *mut ScalarMemoryContext).cast::<()>() as usize as u64;
            success_registers.load_fn = scalar_load_helper as usize as u64;
            let mut success_expected = interpreter_success(
                &function,
                &success_registers,
                source,
                address,
                case.scalar.format,
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
                case.scalar.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(success_context.last_signed, 0, "{level:?} {case:?}");
            successes += 1;

            let mut suppressed_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut suppressed_registers = masked_guest_regs(case, ordinal, ordinal, false);
            let suppressed_address = memory_address(case.scalar, &suppressed_registers);
            suppressed_registers.ctx =
                (&mut suppressed_context as *mut ScalarMemoryContext).cast::<()>() as usize as u64;
            suppressed_registers.load_fn = scalar_load_helper as usize as u64;
            let mut suppressed_expected = interpreter_success(
                &function,
                &suppressed_registers,
                source,
                suppressed_address,
                case.scalar.format,
            );

            exec.run(entry, &mut suppressed_registers);
            suppressed_expected.host_mxcsr = suppressed_registers.host_mxcsr;
            assert_eq!(
                suppressed_registers, suppressed_expected,
                "{level:?} {case:?}: inactive suppression"
            );
            assert_eq!(
                suppressed_context.calls, 0,
                "{level:?} {case:?}: inactive helper call"
            );
            suppressed += 1;

            let mut fault_context = ScalarMemoryContext {
                value: scalar ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault_registers = masked_guest_regs(case, ordinal, ordinal, true);
            let fault_address = memory_address(case.scalar, &fault_registers);
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
                case.scalar.format.memory_size() as u64,
                "{level:?} {case:?}"
            );
            assert_eq!(fault_context.last_signed, 0, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, suppressed);
    assert_eq!(successes, faults);
    assert!(successes >= 12 * 2 * 4 * 3 * 7 * 2 * NATIVE_LEVELS.len());
}
