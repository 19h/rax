use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, DispSize, FpRoundMode, OpId, OpWidth, SignExtend, SrcOperand, VirtualId,
};

#[test]
fn scalar_convert_classifier_exhaustively_rewrites_1_105_920_control_and_apx_cells() {
    let mut accepted = 0usize;
    for conversion in Conversion::ALL {
        for ll in 0..=2u8 {
            for destination in 0..32u8 {
                for merge in 0..32u8 {
                    for mask in 0..8u8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            let canonical = memory_encoding(
                                conversion,
                                destination,
                                merge,
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
                                        .evex_scalar_fp_convert_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.from, conversion.from(), "{bytes:02X?}");
                                    assert_eq!(encoding.to, conversion.to(), "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.merge, merge, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.writemask,
                                        (mask != 0).then_some(mask),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.memory_width,
                                        conversion.memory_width(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512fp16,
                                        conversion.needs_fp16(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.stack_instruction.as_slice(),
                                        stack_encoding(
                                            conversion,
                                            destination,
                                            merge,
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
    assert_eq!(accepted, 6 * 3 * 32 * 32 * 15 * 2 * 2);
}

#[test]
fn scalar_convert_classifier_owns_exactly_six_map_opcode_pp_w_selectors() {
    let template = memory_encoding(Conversion::F64ToF32, 0, 1, 0, 0, false, 3);
    let mut accepted = 0usize;
    for map in 0..=7u8 {
        for opcode in 0..=u8::MAX {
            for pp in 0..=3u8 {
                for w in [false, true] {
                    let mut bytes = template;
                    bytes[1] = (bytes[1] & !7) | map;
                    bytes[2] = (bytes[2] & !(0x80 | 3)) | (u8::from(w) << 7) | pp;
                    bytes[4] = opcode;
                    let expected = matches!(
                        (map, opcode, pp, w),
                        (1, 0x5A, 3, true)
                            | (1, 0x5A, 2, false)
                            | (5, 0x5A, 3, true)
                            | (5, 0x5A, 2, false)
                            | (5, 0x1D, 0, false)
                            | (6, 0x13, 0, false)
                    );
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_scalar_fp_convert_memory_encoding()
                        .is_some();
                    assert_eq!(actual, expected, "{bytes:02X?}");
                    accepted += usize::from(actual);
                }
            }
        }
    }
    assert_eq!(accepted, 6);
}

#[test]
fn scalar_convert_stack_encodings_match_six_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git with Intel syntax. The anchors cover each
    // conversion, low/high registers, no mask, merge masking, and zeroing.
    for (actual, llvm) in [
        (
            stack_encoding(Conversion::F64ToF16, 16, 1, 0, 0, false),
            [0x62, 0xE5, 0xF7, 0x08, 0x5A, 0x04, 0x24],
        ),
        (
            stack_encoding(Conversion::F64ToF32, 0, 1, 0, 1, false),
            [0x62, 0xF1, 0xF7, 0x09, 0x5A, 0x04, 0x24],
        ),
        (
            stack_encoding(Conversion::F16ToF64, 31, 30, 0, 7, true),
            [0x62, 0x65, 0x0E, 0x87, 0x5A, 0x3C, 0x24],
        ),
        (
            stack_encoding(Conversion::F16ToF32, 17, 17, 0, 3, false),
            [0x62, 0xE6, 0x74, 0x03, 0x13, 0x0C, 0x24],
        ),
        (
            stack_encoding(Conversion::F32ToF64, 16, 1, 0, 0, false),
            [0x62, 0xE1, 0x76, 0x08, 0x5A, 0x04, 0x24],
        ),
        (
            stack_encoding(Conversion::F32ToF16, 31, 30, 0, 7, true),
            [0x62, 0x65, 0x0C, 0x87, 0x1D, 0x3C, 0x24],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn scalar_convert_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = memory_encoding(Conversion::F64ToF32, 0, 1, 0, 1, false, 3).to_vec();
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
        (2, 0x01), // F2 -> unowned F3 with W1
        (2, 0x80), // VCVTSD2SS W1 -> W0
        (4, 0x01), // 5A -> unowned 5B
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut wrong_fp16_map = memory_encoding(Conversion::F16ToF32, 0, 1, 0, 0, false, 3);
    wrong_fp16_map[1] = (wrong_fp16_map[1] & !7) | 5;
    malformed.push(wrong_fp16_map.to_vec());

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fp_convert_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_162_scalar_convert_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 162);
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
            assert_eq!(exact.encoding.from, case.conversion.from());
            assert_eq!(exact.encoding.to, case.conversion.to());
            assert_eq!(exact.encoding.destination, case.destination());
            assert_eq!(exact.encoding.merge, case.merge);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert!(matches!(
                function.blocks[0].ops[exact.load_offset].kind,
                OpKind::Load { width, sign: SignExtend::Zero, .. }
                    | OpKind::PredLoad { width, signed: SignExtend::Zero, .. }
                    if width == case.conversion.memory_width()
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
                code.windows(5).any(|window| {
                    window == [0xBA, case.conversion.memory_size() as u8, 0, 0, 0]
                }),
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
    assert_eq!(lowerings, 162 * LEVELS.len());
}

#[test]
fn scalar_convert_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VCVTSS2SH xmm16{k3},xmm17,[r16+r17*2+4]. Tuple1 Scalar
    // compresses disp8=1 by the 4-byte source size.
    let bytes = [0x62, 0xED, 0x70, 0x03, 0x1D, 0x44, 0x48, 0x01];
    let base = function_from_bytes(&bytes, "APX scalar conversion");
    let case = ScalarConvertMemoryCase {
        conversion: Conversion::F32ToF16,
        merge: 17,
        ll: 0,
        control: MaskControl::Merge,
    };
    let expected = [0x62, 0xE5, 0x74, 0x03, 0x1D, 0x04, 0x24];
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
        let exact = sequence(&function).expect("APX scalar conversion sequence");
        assert_eq!(exact.encoding.stack_instruction.as_slice(), expected);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected)
        );
    }
}

#[test]
fn scalar_convert_rip_addr32_segment_and_sib_addresses_remain_helper_owned() {
    let case = ScalarConvertMemoryCase {
        conversion: Conversion::F64ToF32,
        merge: 30,
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
    gs_addr32_sib.push(0x8B);
    gs_addr32_sib.push(2); // 2 * 8-byte Tuple1 Scalar = 16 bytes.
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
                case.stack_instruction()
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
fn masked_scalar_convert_lowering_has_one_precise_live_k_bit_zero_guard() {
    for conversion in Conversion::ALL {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ScalarConvertMemoryCase {
                conversion,
                merge: 17,
                ll: 2,
                control,
            };
            let (code, _) = lower(&lift_case(case), case);
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
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function).is_none(),
        "{name}: matcher admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn scalar_convert_sequence_fails_closed_for_provenance_graph_and_ssa_mutations() {
    for case in [
        ScalarConvertMemoryCase {
            conversion: Conversion::F64ToF32,
            merge: 1,
            ll: 2,
            control: MaskControl::Merge,
        },
        ScalarConvertMemoryCase {
            conversion: Conversion::F16ToF64,
            merge: 30,
            ll: 1,
            control: MaskControl::Zero,
        },
        ScalarConvertMemoryCase {
            conversion: Conversion::F32ToF16,
            merge: 17,
            ll: 0,
            control: MaskControl::None,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function).is_some(), "{case:?}");
        let (definitions, uses) = virtual_counts(&function);
        assert!(
            x86_jit_evex_scalar_fp_convert_memory_sequence(
                &function.blocks[0],
                0,
                false,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .is_none()
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let bytes = memory_encoding(Conversion::F16ToF32, 0, 1, 0, 0, false, 3);
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
            OpKind::Load { width, .. } | OpKind::PredLoad { width, .. } => *width = MemWidth::B16,
            _ => unreachable!(),
        }
        assert_rejected("wrong scalar load width", &wrong_width);

        let mut wrong_round = function.clone();
        let convert = wrong_round.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { round, .. } = &mut convert.kind else {
            unreachable!()
        };
        *round = FpRoundMode::RoundNearest;
        assert_rejected("wrong rounding", &wrong_round);

        let mut wrong_conversion = function.clone();
        let convert = wrong_conversion.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { to, .. } = &mut convert.kind else {
            unreachable!()
        };
        *to = if *to == VecElementType::F16 {
            VecElementType::F32
        } else {
            VecElementType::F16
        };
        assert_rejected("wrong conversion", &wrong_conversion);

        let mut wrong_hint = function.clone();
        wrong_hint.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap()
            .x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("wrong hint", &wrong_hint);

        let mut wrong_destination = function.clone();
        let convert = wrong_destination.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { dst, .. } = &mut convert.kind else {
            unreachable!()
        };
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm((case.destination() + 1) % 32)));
        assert_rejected("wrong destination", &wrong_destination);

        let mut wrong_merge = function.clone();
        let convert = wrong_merge.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { merge, .. } = &mut convert.kind else {
            unreachable!()
        };
        *merge = VReg::Arch(ArchReg::X86(X86Reg::Xmm((case.merge + 1) % 32)));
        assert_rejected("wrong merge", &wrong_merge);

        let mut wrong_zeroing = function.clone();
        let convert = wrong_zeroing.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { mask_zeroing, .. } = &mut convert.kind else {
            unreachable!()
        };
        *mask_zeroing = !*mask_zeroing;
        assert_rejected("wrong zeroing policy", &wrong_zeroing);

        let mut wrong_exception_policy = function.clone();
        let convert = wrong_exception_policy.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert {
            suppress_exceptions,
            ..
        } = &mut convert.kind
        else {
            unreachable!()
        };
        *suppress_exceptions = true;
        assert_rejected("wrong exception policy", &wrong_exception_policy);

        let mut wrong_upper_lane_policy = function.clone();
        let convert = wrong_upper_lane_policy.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FpConvert { .. }))
            .unwrap();
        let OpKind::X86FpConvert { zero_upper, .. } = &mut convert.kind else {
            unreachable!()
        };
        *zero_upper = false;
        assert_rejected("wrong upper-lane policy", &wrong_upper_lane_policy);

        if case.control != MaskControl::None {
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
        }

        let mut extra_source_use = function.clone();
        let loaded = extra_source_use.blocks[0]
            .ops
            .iter()
            .find_map(|op| match op.kind {
                OpKind::Load { dst, .. } | OpKind::PredLoad { dst, .. } => Some(dst),
                _ => None,
            })
            .unwrap();
        extra_source_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Reg(loaded),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("extra scalar-source use", &extra_source_use);

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
fn scalar_convert_lowerer_rejects_the_avx_only_vector_bridge() {
    let case = ScalarConvertMemoryCase {
        conversion: Conversion::F32ToF64,
        merge: 30,
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
