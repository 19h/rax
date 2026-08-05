use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{Address, DispSize, OpId, OpWidth, SignExtend, SrcOperand, VirtualId};

#[test]
fn scalar_unary_classifier_exhaustively_rewrites_2_211_840_control_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in UnaryOperation::ALL {
        for format in ScalarFormat::ALL {
            for ll in 0..=2u8 {
                for destination in 0..32u8 {
                    for merge in 0..32u8 {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let canonical = memory_encoding(
                                    operation,
                                    format,
                                    destination,
                                    merge,
                                    ll,
                                    mask,
                                    zeroing,
                                    3,
                                    0xD7,
                                );
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical.clone();
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_scalar_fp_unary_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.kind, operation.kind(), "{bytes:02X?}");
                                        assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.merge, merge, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.immediate,
                                            operation.has_immediate().then_some(0xD7),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.memory_width,
                                            format.memory_width(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512dq,
                                            operation == UnaryOperation::Reduce
                                                && format != ScalarFormat::F16,
                                            "{bytes:02X?}"
                                        );
                                        assert!(!encoding.needs_avx512er, "{bytes:02X?}");
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
                                                merge,
                                                ll,
                                                mask,
                                                zeroing,
                                                0xD7,
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
    assert_eq!(accepted, 4 * 3 * 3 * 32 * 32 * 15 * 2 * 2);
}

fn selector(map: u8, opcode: u8, pp: u8, w: bool) -> Option<bool> {
    Some(match (map, opcode, pp, w) {
        (6, 0x43, 1, false) | (2, 0x43, 1, false) | (2, 0x43, 1, true) => false,
        (3, 0x27, 0, false)
        | (3, 0x27, 1, false)
        | (3, 0x27, 1, true)
        | (3, 0x0A, 0, false)
        | (3, 0x0A, 1, false)
        | (3, 0x0B, 1, true)
        | (3, 0x57, 0, false)
        | (3, 0x57, 1, false)
        | (3, 0x57, 1, true) => true,
        (2, 0x4D | 0x4F | 0xCB | 0xCD, 1, false | true) | (6, 0x4D | 0x4F, 1, false) => false,
        _ => return None,
    })
}

#[test]
fn scalar_unary_classifier_owns_exactly_twenty_two_map_opcode_pp_w_length_selectors() {
    let template = memory_encoding(
        UnaryOperation::GetExponent,
        ScalarFormat::F32,
        0,
        1,
        0,
        0,
        false,
        3,
        0,
    );
    let mut accepted = 0usize;
    for map in 0..=7u8 {
        for opcode in 0..=u8::MAX {
            for pp in 0..=3u8 {
                for w in [false, true] {
                    for with_immediate in [false, true] {
                        let mut bytes = template.clone();
                        bytes[1] = (bytes[1] & !7) | map;
                        bytes[2] = (bytes[2] & !(0x80 | 3)) | (u8::from(w) << 7) | pp;
                        bytes[4] = opcode;
                        if with_immediate {
                            bytes.push(0xD7);
                        }
                        let expected = selector(map, opcode, pp, w) == Some(with_immediate);
                        let actual = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_scalar_fp_unary_memory_encoding()
                            .is_some();
                        assert_eq!(actual, expected, "{bytes:02X?}");
                        accepted += usize::from(actual);
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 22);
}

#[test]
fn all_2_304_immediate_values_are_preserved_exactly() {
    let mut checks = 0usize;
    for operation in [
        UnaryOperation::GetMantissa,
        UnaryOperation::RoundScale,
        UnaryOperation::Reduce,
    ] {
        for format in ScalarFormat::ALL {
            for immediate in 0..=u8::MAX {
                let bytes = memory_encoding(operation, format, 17, 30, 2, 7, true, 3, immediate);
                let encoding = X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_scalar_fp_unary_memory_encoding()
                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(encoding.immediate, Some(immediate), "{bytes:02X?}");
                assert_eq!(
                    encoding.stack_instruction.as_slice().last(),
                    Some(&immediate),
                    "{bytes:02X?}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 3 * 3 * 256);
}

#[test]
fn scalar_unary_stack_encodings_match_twelve_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git with Intel syntax. Together these cover
    // every family/format, low/high registers, and none/merge/zero masks.
    for (actual, llvm) in [
        (
            stack_encoding(
                UnaryOperation::GetExponent,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
                0,
            ),
            &[0x62, 0xE2, 0xF5, 0x08, 0x43, 0x04, 0x24][..],
        ),
        (
            stack_encoding(
                UnaryOperation::GetExponent,
                ScalarFormat::F16,
                0,
                1,
                0,
                1,
                false,
                0,
            ),
            &[0x62, 0xF6, 0x75, 0x09, 0x43, 0x04, 0x24],
        ),
        (
            stack_encoding(
                UnaryOperation::GetExponent,
                ScalarFormat::F32,
                31,
                30,
                0,
                7,
                true,
                0,
            ),
            &[0x62, 0x62, 0x0D, 0x87, 0x43, 0x3C, 0x24],
        ),
        (
            stack_encoding(
                UnaryOperation::GetMantissa,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
                0xD7,
            ),
            &[0x62, 0xE3, 0xF5, 0x08, 0x27, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::GetMantissa,
                ScalarFormat::F16,
                0,
                1,
                0,
                1,
                false,
                0xD7,
            ),
            &[0x62, 0xF3, 0x74, 0x09, 0x27, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::GetMantissa,
                ScalarFormat::F32,
                31,
                30,
                0,
                7,
                true,
                0xD7,
            ),
            &[0x62, 0x63, 0x0D, 0x87, 0x27, 0x3C, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::RoundScale,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
                0xD7,
            ),
            &[0x62, 0xE3, 0xF5, 0x08, 0x0B, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::RoundScale,
                ScalarFormat::F16,
                0,
                1,
                0,
                1,
                false,
                0xD7,
            ),
            &[0x62, 0xF3, 0x74, 0x09, 0x0A, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::RoundScale,
                ScalarFormat::F32,
                31,
                30,
                0,
                7,
                true,
                0xD7,
            ),
            &[0x62, 0x63, 0x0D, 0x87, 0x0A, 0x3C, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::Reduce,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
                0xD7,
            ),
            &[0x62, 0xE3, 0xF5, 0x08, 0x57, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::Reduce,
                ScalarFormat::F16,
                0,
                1,
                0,
                1,
                false,
                0xD7,
            ),
            &[0x62, 0xF3, 0x74, 0x09, 0x57, 0x04, 0x24, 0xD7],
        ),
        (
            stack_encoding(
                UnaryOperation::Reduce,
                ScalarFormat::F32,
                31,
                30,
                0,
                7,
                true,
                0xD7,
            ),
            &[0x62, 0x63, 0x0D, 0x87, 0x57, 0x3C, 0x24, 0xD7],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn scalar_unary_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = memory_encoding(
        UnaryOperation::GetMantissa,
        ScalarFormat::F64,
        0,
        1,
        0,
        1,
        false,
        3,
        0xD7,
    );
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut embedded_sae = valid.clone();
    embedded_sae[3] |= 0x10;
    malformed.push(embedded_sae);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);
    for (index, mask) in [(1, 0x01), (2, 0x01), (2, 0x82), (4, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut unexpected_immediate = memory_encoding(
        UnaryOperation::GetExponent,
        ScalarFormat::F32,
        0,
        1,
        0,
        0,
        false,
        3,
        0,
    );
    unexpected_immediate.push(0);
    malformed.push(unexpected_immediate);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fp_unary_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_324_scalar_unary_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 324);
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
            assert_eq!(exact.encoding.kind, case.operation.kind());
            assert_eq!(exact.encoding.elem, case.format.elem());
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.merge, case.merge);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(
                exact.encoding.immediate,
                case.operation.has_immediate().then_some(case.immediate)
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
                    .any(|window| { window == [0xBA, case.format.memory_size() as u8, 0, 0, 0] }),
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
    assert_eq!(lowerings, 324 * LEVELS.len());
}

#[test]
fn scalar_unary_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VGETMANTSS xmm17{k3},xmm18,[r16+r17*2+64],D7H. Tuple1 Scalar
    // compresses disp8=10H by the 4-byte source size.
    let bytes = [0x62, 0xEB, 0x69, 0x03, 0x27, 0x4C, 0x48, 0x10, 0xD7];
    let base = function_from_bytes(&bytes, "APX scalar unary");
    let case = ScalarUnaryMemoryCase {
        operation: UnaryOperation::GetMantissa,
        format: ScalarFormat::F32,
        destination: 17,
        merge: 18,
        ll: 0,
        control: MaskControl::Merge,
        immediate: 0xD7,
    };
    let expected = [0x62, 0xE3, 0x6D, 0x03, 0x27, 0x0C, 0x24, 0xD7];
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseIndexScale {
                    base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                    index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                    scale: 2,
                    disp: 64,
                    disp_size: DispSize::Disp8,
                },
                ..
            }
        )));
        let exact = sequence(&function).expect("APX scalar unary sequence");
        assert_eq!(exact.encoding.stack_instruction.as_slice(), expected);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected)
        );
    }
}

#[test]
fn scalar_unary_rip_addr32_segment_and_sib_addresses_remain_helper_owned() {
    let case = ScalarUnaryMemoryCase {
        operation: UnaryOperation::RoundScale,
        format: ScalarFormat::F64,
        destination: 31,
        merge: 30,
        ll: 1,
        control: MaskControl::Zero,
        immediate: 0xD7,
    };
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let mut rip = case.bytes();
    let imm = rip.pop().unwrap();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    rip.push(imm);
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    let mut gs_addr32_sib = case.bytes();
    let imm = gs_addr32_sib.pop().unwrap();
    gs_addr32_sib[5] = (gs_addr32_sib[5] & 0x38) | 0x44;
    gs_addr32_sib.push(0x8B);
    gs_addr32_sib.push(2);
    gs_addr32_sib.push(imm);
    gs_addr32_sib.insert(0, 0x67);
    gs_addr32_sib.insert(0, 0x65);

    let address_cases = [
        (
            "RIP+disp32",
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 11),
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
    for (label, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = optimize(function_from_bytes(&bytes, label), level);
            let exact = sequence(&function).unwrap_or_else(|| panic!("{level:?} {label}"));
            let address = match &function.blocks[0].ops[exact.load_offset].kind {
                OpKind::PredLoad { addr, .. } => addr,
                other => panic!("{level:?} {label}: {other:?}"),
            };
            assert_eq!(address, &expected_address, "{level:?} {label}");
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{level:?} {label}"
            );
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.stack_instruction().len())
                    .any(|window| { window == case.stack_instruction() })
            );
        }
    }
}

fn rejected(function: &SmirFunction, label: &str) {
    assert!(
        sequence(function).is_none(),
        "{label}: unexpectedly matched"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{label}: unexpectedly admitted"
    );
}

#[test]
fn scalar_unary_sequence_rejects_graph_provenance_frontier_and_apx_mutations() {
    let case = ScalarUnaryMemoryCase {
        operation: UnaryOperation::Reduce,
        format: ScalarFormat::F32,
        destination: 17,
        merge: 30,
        ll: 2,
        control: MaskControl::Zero,
        immediate: 0xD7,
    };
    let base = lift_case(case);
    let mut mutations: Vec<(&str, SmirFunction)> = Vec::new();

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_provenance = base.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(
            &ScalarUnaryMemoryCase {
                operation: UnaryOperation::RoundScale,
                ..case
            }
            .bytes(),
        )
        .unwrap(),
    );
    mutations.push(("wrong provenance", wrong_provenance));

    let mut seed_value = base.clone();
    if let OpKind::Mov { src, .. } = &mut seed_value.blocks[0].ops[0].kind {
        *src = SrcOperand::Imm(1);
    }
    mutations.push(("seed value", seed_value));

    let mut seed_hint = base.clone();
    let semantic_hint = seed_hint.blocks[0].ops.last().unwrap().x86_hint;
    seed_hint.blocks[0].ops[0].x86_hint = semantic_hint;
    mutations.push(("seed hint", seed_hint));

    let mut mask_shift = base.clone();
    if let OpKind::Shr { amount, .. } = &mut mask_shift.blocks[0].ops[1].kind {
        *amount = SrcOperand::Imm(1);
    }
    mutations.push(("mask shift", mask_shift));

    let mut load_width = base.clone();
    if let OpKind::PredLoad { width, .. } = &mut load_width.blocks[0].ops[3].kind {
        *width = MemWidth::B8;
    }
    mutations.push(("load width", load_width));

    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[4].kind {
        *lanes = 2;
    }
    mutations.push(("broadcast lanes", broadcast_lanes));

    let mut semantic_immediate = base.clone();
    if let OpKind::X86Reduce { imm, .. } = &mut semantic_immediate.blocks[0].ops[5].kind {
        *imm ^= 1;
    }
    mutations.push(("semantic immediate", semantic_immediate));

    let mut semantic_destination = base.clone();
    if let OpKind::X86Reduce { dst, .. } = &mut semantic_destination.blocks[0].ops[5].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(16)));
    }
    mutations.push(("semantic destination", semantic_destination));

    let mut trailing = base.clone();
    trailing.blocks[0].ops.push(SmirOp::new(
        OpId(99),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(99)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC trailing operation", trailing));

    let mut false_apx_guard = base.clone();
    false_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(99), PC, OpKind::X86RequireApx));
    mutations.push(("false APX guard", false_apx_guard));

    for (label, function) in mutations {
        rejected(&function, label);
    }

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_evex_scalar_fp_unary_memory_sequence(
            &base.blocks[0],
            0,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );
}

#[test]
fn scalar_unary_o2_direct_mask_predicate_is_exact_and_fail_closed() {
    let case = ScalarUnaryMemoryCase {
        operation: UnaryOperation::GetExponent,
        format: ScalarFormat::F16,
        destination: 31,
        merge: 30,
        ll: 1,
        control: MaskControl::Merge,
        immediate: 0,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert!(matches!(base.blocks[0].ops[1].kind, OpKind::And { .. }));
    let mut wrong_mask = base.clone();
    if let OpKind::And { src1, .. } = &mut wrong_mask.blocks[0].ops[1].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    }
    rejected(&wrong_mask, "O2 wrong mask source");

    let mut extra_use = base.clone();
    let loaded = match extra_use.blocks[0].ops[0].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    extra_use.blocks[0].ops.insert(
        4,
        SmirOp::new(
            OpId(98),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(98)),
                src: SrcOperand::Reg(loaded),
                width: OpWidth::W64,
            },
        ),
    );
    rejected(&extra_use, "loaded value extra use");
}
