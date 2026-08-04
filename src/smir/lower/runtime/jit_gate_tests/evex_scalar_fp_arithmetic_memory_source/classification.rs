use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, Avx10FP16Op, DispSize, FpRoundMode, OpId, OpWidth, SignExtend, SrcOperand, VecWidth,
    VirtualId, X86FpBinaryOp,
};

#[test]
fn scalar_fp_memory_classifier_exhaustively_rewrites_3_870_720_control_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in ArithmeticOperation::ALL {
        for format in ScalarFormat::ALL {
            for ll in 0..=2u8 {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let canonical = memory_encoding(
                                    operation,
                                    format,
                                    destination,
                                    source1,
                                    ll,
                                    mask,
                                    zeroing,
                                    3,
                                );
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical;
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_scalar_fp_arithmetic_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.opcode,
                                            operation.opcode(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.memory_width,
                                            format.memory_width(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512fp16,
                                            format == ScalarFormat::F16,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.stack_instruction.as_slice(),
                                            stack_encoding(
                                                operation,
                                                format,
                                                destination,
                                                source1,
                                                ll,
                                                mask,
                                                zeroing,
                                            ),
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
        }
    }
    assert_eq!(accepted, 7 * 3 * 3 * 32 * 32 * 15 * 2 * 2);

    for format in ScalarFormat::ALL {
        for opcode in 0..=u8::MAX {
            let mut bytes = memory_encoding(ArithmeticOperation::Add, format, 0, 1, 0, 0, false, 3);
            bytes[4] = opcode;
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_scalar_fp_arithmetic_memory_encoding()
                    .is_some(),
                matches!(opcode, 0x51 | 0x58 | 0x59 | 0x5C | 0x5D | 0x5E | 0x5F),
                "{format:?} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn scalar_fp_stack_encodings_match_six_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git with Intel syntax. These anchors are
    // independent of the classifier and cover every scalar element size,
    // unary/binary operations, low/high registers, merge, and zero masking.
    for (actual, llvm) in [
        (
            stack_encoding(
                ArithmeticOperation::Add,
                ScalarFormat::F32,
                16,
                1,
                0,
                0,
                false,
            ),
            [0x62, 0xE1, 0x76, 0x08, 0x58, 0x04, 0x24],
        ),
        (
            stack_encoding(
                ArithmeticOperation::Min,
                ScalarFormat::F32,
                0,
                1,
                0,
                1,
                false,
            ),
            [0x62, 0xF1, 0x76, 0x09, 0x5D, 0x04, 0x24],
        ),
        (
            stack_encoding(
                ArithmeticOperation::Sqrt,
                ScalarFormat::F32,
                31,
                30,
                0,
                7,
                true,
            ),
            [0x62, 0x61, 0x0E, 0x87, 0x51, 0x3C, 0x24],
        ),
        (
            stack_encoding(
                ArithmeticOperation::Sub,
                ScalarFormat::F16,
                16,
                1,
                0,
                0,
                false,
            ),
            [0x62, 0xE5, 0x76, 0x08, 0x5C, 0x04, 0x24],
        ),
        (
            stack_encoding(
                ArithmeticOperation::Div,
                ScalarFormat::F16,
                17,
                17,
                0,
                3,
                false,
            ),
            [0x62, 0xE5, 0x76, 0x03, 0x5E, 0x0C, 0x24],
        ),
        (
            stack_encoding(
                ArithmeticOperation::Max,
                ScalarFormat::F64,
                31,
                30,
                0,
                7,
                true,
            ),
            [0x62, 0x61, 0x8F, 0x87, 0x5F, 0x3C, 0x24],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn scalar_fp_memory_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = memory_encoding(
        ArithmeticOperation::Add,
        ScalarFormat::F32,
        0,
        1,
        0,
        1,
        false,
        3,
    )
    .to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut embedded_rounding = valid.clone();
    embedded_rounding[3] |= 0x10;
    malformed.push(embedded_rounding);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);
    for (index, mask) in [
        (1, 0x01), // map 1 -> map 0
        (2, 0x01), // F3 -> unowned pp
        (2, 0x80), // F32 W0 -> W1
        (4, 0x02), // unowned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut fp16_wrong_map = memory_encoding(
        ArithmeticOperation::Add,
        ScalarFormat::F16,
        0,
        1,
        0,
        0,
        false,
        3,
    );
    fp16_wrong_map[1] = (fp16_wrong_map[1] & !7) | 6;
    malformed.push(fp16_wrong_map.to_vec());
    let mut f64_w0 = memory_encoding(
        ArithmeticOperation::Sqrt,
        ScalarFormat::F64,
        0,
        1,
        0,
        0,
        false,
        3,
    );
    f64_w0[2] &= !0x80;
    malformed.push(f64_w0.to_vec());

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fp_arithmetic_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_567_scalar_fp_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 7 * 3 * 3 * 3 * 3);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.encoding.elem,
                case.format.elem(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                exact.encoding.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.source1, case.source1, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.memory_width,
                case.format.memory_width(),
                "{level:?} {case:?}"
            );
            assert!(matches!(
                function.blocks[0].ops[exact.load_offset].kind,
                OpKind::Load { width, sign: SignExtend::Zero, .. }
                    | OpKind::PredLoad { width, signed: SignExtend::Zero, .. }
                    if width == case.format.memory_width()
            ));

            let (code, _) = lower(&function, case);
            let expected = case.stack_instruction();
            assert_eq!(
                code.windows(expected.len())
                    .filter(|window| *window == expected)
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, case.format.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing exact scalar helper width"
            );
            if case.mask() != 0 {
                let kmovq = [0xC4, 0xE1, 0xFB, 0x93, 0xC0 | case.mask()];
                assert!(
                    code.windows(kmovq.len()).any(|window| window == kmovq),
                    "{level:?} {case:?}: missing live K bit-0 guard"
                );
            }
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 567 * LEVELS.len());
}

#[test]
fn scalar_fp_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VADDSS xmm16{k3},xmm17,[r16+r17*2+4]. Tuple1 Scalar compresses disp8=1
    // by the 4-byte source size. B4/X4 are address-only and must disappear
    // from the native `[rsp]` replay.
    let bytes = [0x62, 0xE9, 0x72, 0x03, 0x58, 0x44, 0x48, 0x01];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .expect("APX-extended scalar FP memory source");
    assert_eq!(result.bytes_consumed, bytes.len());

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut base = SmirFunction::new(FunctionId(0), block.id, PC);
    base.add_block(block);
    base.x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let case = ScalarFpMemoryCase {
        operation: ArithmeticOperation::Add,
        format: ScalarFormat::F32,
        source1: 17,
        ll: 0,
        control: MaskControl::Merge,
    };
    let expected = [0x62, 0xE1, 0x76, 0x03, 0x58, 0x04, 0x24];
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp: 4,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                }
            )),
            "{level:?}: {:#?}",
            function.blocks[0].ops
        );
        let exact = sequence(&function).expect("APX scalar arithmetic sequence");
        assert_eq!(exact.encoding.stack_instruction.as_slice(), expected);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected)
        );
    }
}

#[test]
fn scalar_fp_rip_addr32_segment_and_sib_addresses_remain_helper_owned() {
    let case = ScalarFpMemoryCase {
        operation: ArithmeticOperation::Sub,
        format: ScalarFormat::F64,
        source1: 30,
        ll: 1,
        control: MaskControl::Zero,
    };
    let x86 = |register| VReg::Arch(ArchReg::X86(register));

    let mut rip = case.bytes().to_vec();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = case.bytes().to_vec();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes().to_vec();
    fs.insert(0, 0x64);
    let mut gs_addr32_sib = case.bytes().to_vec();
    gs_addr32_sib[5] = (gs_addr32_sib[5] & 0x38) | 0x44;
    gs_addr32_sib.push(0x8B); // [rbx + rcx*4 + disp8]
    gs_addr32_sib.push(2); // Tuple1 Scalar: 2 * 8-byte source = 16 bytes.
    gs_addr32_sib.insert(0, 0x67);
    gs_addr32_sib.insert(0, 0x65);

    let address_cases = [
        (
            "RIP+disp32",
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS base",
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB",
            gs_addr32_sib,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 16,
            })),
        ),
    ];
    for (name, bytes, expected_address) in address_cases {
        let base = function_from_bytes(&bytes, name);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::Lea { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{name} {level:?}"
            );
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.stack_instruction().len())
                    .any(|window| window == case.stack_instruction()),
                "{name} {level:?}"
            );
        }
    }
}

#[test]
fn masked_scalar_fp_lowering_has_one_precise_live_k_bit_zero_guard() {
    for format in ScalarFormat::ALL {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ScalarFpMemoryCase {
                operation: ArithmeticOperation::Div,
                format,
                source1: 17,
                ll: 2,
                control,
            };
            let function = lift_case(case);
            let (code, _) = lower(&function, case);
            let guard = [
                0x9C,
                0x50,
                0xC4,
                0xE1,
                0xFB,
                0x93,
                0xC0 | case.mask(),
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
            let matches: Vec<_> = code
                .windows(guard.len())
                .enumerate()
                .filter_map(|(index, window)| (window == guard).then_some(index))
                .collect();
            assert_eq!(matches.len(), 1, "{case:?}: {code:02X?}");
            let guard_at = matches[0];
            let displacement_at = guard_at + guard.len();
            assert_eq!(
                &code[displacement_at + 4..displacement_at + 6],
                &[0x58, 0x9D]
            );
            let inactive = (displacement_at + 4) as i64
                + i64::from(i32::from_le_bytes(
                    code[displacement_at..displacement_at + 4]
                        .try_into()
                        .unwrap(),
                ));
            let inactive = usize::try_from(inactive).expect("forward inactive target");
            assert_eq!(&code[inactive..inactive + 2], &[0x58, 0x9D]);
            assert_eq!(code[inactive - 5], 0xE9);
            let execute = inactive as i64
                + i64::from(i32::from_le_bytes(
                    code[inactive - 4..inactive].try_into().unwrap(),
                ));
            assert_eq!(usize::try_from(execute).unwrap(), inactive + 2);
            let expected = case.stack_instruction();
            assert_eq!(&code[inactive + 2..inactive + 2 + expected.len()], expected);
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function).is_none(),
        "{name}: exact sequence matcher admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn scalar_fp_memory_sequence_fails_closed_for_provenance_graph_and_ssa_mutations() {
    for case in [
        ScalarFpMemoryCase {
            operation: ArithmeticOperation::Add,
            format: ScalarFormat::F32,
            source1: 1,
            ll: 2,
            control: MaskControl::Merge,
        },
        ScalarFpMemoryCase {
            operation: ArithmeticOperation::Sqrt,
            format: ScalarFormat::F64,
            source1: 30,
            ll: 1,
            control: MaskControl::Zero,
        },
        ScalarFpMemoryCase {
            operation: ArithmeticOperation::Div,
            format: ScalarFormat::F16,
            source1: 17,
            ll: 0,
            control: MaskControl::Merge,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function).is_some(), "{case:?}");
        let (definitions, uses) = virtual_counts(&function);
        assert!(
            x86_jit_evex_scalar_fp_arithmetic_memory_sequence(
                &function.blocks[0],
                0,
                false,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .is_none(),
            "{case:?}: memory-disabled matcher"
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[4] = if case.operation == ArithmeticOperation::Add {
            ArithmeticOperation::Mul.opcode()
        } else {
            ArithmeticOperation::Add.opcode()
        };
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut wrong_width = function.clone();
        let load = wrong_width.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
            .unwrap();
        match &mut load.kind {
            OpKind::Load { width, .. } | OpKind::PredLoad { width, .. } => {
                *width = MemWidth::B16;
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong scalar load width", &wrong_width);

        let mut wrong_round = function.clone();
        let semantic = wrong_round.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpBinary { .. } | OpKind::X86Sqrt { .. } | OpKind::VFP16Arith { .. }
                )
            })
            .unwrap();
        match &mut semantic.kind {
            OpKind::X86FpBinary { round, .. }
            | OpKind::X86Sqrt { round, .. }
            | OpKind::VFP16Arith { round, .. } => *round = FpRoundMode::RoundNearest,
            _ => unreachable!(),
        }
        assert_rejected("wrong rounding", &wrong_round);

        let mut wrong_semantic = function.clone();
        let semantic = wrong_semantic.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpBinary { .. } | OpKind::X86Sqrt { .. } | OpKind::VFP16Arith { .. }
                )
            })
            .unwrap();
        match &mut semantic.kind {
            OpKind::X86FpBinary { op, .. } => {
                *op = if *op == X86FpBinaryOp::Add {
                    X86FpBinaryOp::Mul
                } else {
                    X86FpBinaryOp::Add
                };
            }
            OpKind::X86Sqrt {
                suppress_exceptions,
                ..
            } => *suppress_exceptions = true,
            OpKind::VFP16Arith { op, .. } => {
                *op = if *op == Avx10FP16Op::Add {
                    Avx10FP16Op::Mul
                } else {
                    Avx10FP16Op::Add
                };
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong semantic operation", &wrong_semantic);

        let mut wrong_hint = function.clone();
        let semantic = wrong_hint.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86FpBinary { .. } | OpKind::X86Sqrt { .. } | OpKind::VFP16Arith { .. }
                )
            })
            .unwrap();
        semantic.x86_hint = if case.format == ScalarFormat::F16 {
            Some(X86OpHint::MovImmModRm)
        } else {
            None
        };
        assert_rejected("wrong semantic hint", &wrong_hint);

        let mut wrong_condition = function.clone();
        let condition = wrong_condition.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::And { .. }))
            .unwrap();
        let OpKind::And { src2, .. } = &mut condition.kind else {
            unreachable!()
        };
        *src2 = SrcOperand::Imm(2);
        assert_rejected("wrong mask bit", &wrong_condition);

        let mut wrong_upper_source = function.clone();
        let extract = wrong_upper_source.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::VExtractLane { lane, .. } if lane > 0
                )
            })
            .unwrap();
        let OpKind::VExtractLane { vec, .. } = &mut extract.kind else {
            unreachable!()
        };
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(case.source1 ^ 1)));
        assert_rejected("wrong upper source", &wrong_upper_source);

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
}

#[test]
fn scalar_fp_lowerer_rejects_the_avx_only_vector_bridge() {
    let case = ScalarFpMemoryCase {
        operation: ArithmeticOperation::Max,
        format: ScalarFormat::F64,
        source1: 30,
        ll: 2,
        control: MaskControl::Zero,
    };
    let function = lift_case(case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}
