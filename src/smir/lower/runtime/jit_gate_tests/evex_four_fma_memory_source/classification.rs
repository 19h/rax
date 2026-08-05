use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{OpId, OpWidth, SrcOperand, VecWidth, VirtualId};

#[test]
fn four_fma_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8], bool, bool, u8, u8, Option<u8>, bool)] = &[
        (
            &[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x0A],
            &[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x0C, 0x24],
            false,
            false,
            1,
            4,
            None,
            false,
        ),
        (
            &[0x62, 0xF2, 0x5F, 0xCA, 0x9A, 0x0A],
            &[0x62, 0xF2, 0x5F, 0xCA, 0x9A, 0x0C, 0x24],
            false,
            false,
            1,
            4,
            Some(2),
            true,
        ),
        (
            &[0x62, 0xE2, 0x5F, 0xC2, 0xAA, 0x0A],
            &[0x62, 0xE2, 0x5F, 0xC2, 0xAA, 0x0C, 0x24],
            false,
            true,
            17,
            20,
            Some(2),
            true,
        ),
        (
            &[0x62, 0xF2, 0x5F, 0x08, 0x9B, 0x0A],
            &[0x62, 0xF2, 0x5F, 0x08, 0x9B, 0x0C, 0x24],
            true,
            false,
            1,
            4,
            None,
            false,
        ),
        (
            &[0x62, 0xF2, 0x5F, 0x89, 0x9B, 0x0A],
            &[0x62, 0xF2, 0x5F, 0x89, 0x9B, 0x0C, 0x24],
            true,
            false,
            1,
            4,
            Some(1),
            true,
        ),
        (
            &[0x62, 0xE2, 0x5F, 0x82, 0xAB, 0x0A],
            &[0x62, 0xE2, 0x5F, 0x82, 0xAB, 0x0C, 0x24],
            true,
            true,
            17,
            20,
            Some(2),
            true,
        ),
    ];

    for (memory, stack, scalar, negate, destination, source, mask, zeroing) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_four_fma_memory_encoding()
            .unwrap_or_else(|| panic!("LLVM anchor rejected: {memory:02X?}"));
        assert_eq!(encoding.scalar, *scalar, "{memory:02X?}");
        assert_eq!(encoding.negate_product, *negate, "{memory:02X?}");
        assert_eq!(encoding.destination, *destination, "{memory:02X?}");
        assert_eq!(encoding.source_index, *source, "{memory:02X?}");
        assert_eq!(encoding.source_base, *source & !3, "{memory:02X?}");
        assert_eq!(encoding.writemask, *mask, "{memory:02X?}");
        assert_eq!(encoding.zeroing, *zeroing, "{memory:02X?}");
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *stack,
            "{memory:02X?}"
        );
    }
}

#[test]
fn four_fma_classifier_exhausts_491_520_control_register_and_apx_cells() {
    let mut accepted = 0usize;
    for form in FourFmaForm::ALL {
        for negate_product in [false, true] {
            let ll_values: &[u8] = if form.scalar() { &[0, 1, 2] } else { &[2] };
            for &ll in ll_values {
                for destination in 0..32u8 {
                    for source_index in 0..32u8 {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let case = FourFmaMemoryCase {
                                    form,
                                    negate_product,
                                    destination,
                                    source_index,
                                    ll,
                                    control: MaskControl::None,
                                };
                                let mut canonical = memory_encoding(case, 2);
                                canonical[3] =
                                    (canonical[3] & !0x87) | (u8::from(zeroing) << 7) | mask;
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical;
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_four_fma_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.scalar, form.scalar(), "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.negate_product, negate_product,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.source_index, source_index,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.source_base,
                                            source_index & !3,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                        assert_eq!(encoding.opcode, case.opcode(), "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.stack_instruction.as_slice(),
                                            &[
                                                0x62,
                                                (bytes[1] & 0x97) | 0x60,
                                                bytes[2] | 0x04,
                                                bytes[3],
                                                bytes[4],
                                                (bytes[5] & 0x38) | 0x04,
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
        }
    }
    assert_eq!(accepted, 4 * 2 * 32 * 32 * 15 * 4);
}

#[test]
fn four_fma_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = FourFmaMemoryCase {
        form: FourFmaForm::Packed,
        negate_product: false,
        destination: 17,
        source_index: 30,
        ll: 2,
        control: MaskControl::Zero,
    };
    let canonical = case.bytes().to_vec();
    let rejects = |bytes: &[u8]| {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .and_then(|instruction| instruction.evex_four_fma_memory_encoding()),
            None,
            "{bytes:02X?}"
        );
    };

    let mut register = canonical.clone();
    register[5] |= 0xC0;
    rejects(&register);
    let mut broadcast = canonical.clone();
    broadcast[3] |= 0x10;
    rejects(&broadcast);
    let mut wrong_w = canonical.clone();
    wrong_w[2] |= 0x80;
    rejects(&wrong_w);
    let mut wrong_pp = canonical.clone();
    wrong_pp[2] ^= 1;
    rejects(&wrong_pp);
    let mut wrong_map = canonical.clone();
    wrong_map[1] ^= 1;
    rejects(&wrong_map);
    let mut packed_vl256 = canonical.clone();
    packed_vl256[3] = (packed_vl256[3] & !0x60) | 0x20;
    rejects(&packed_vl256);
    let mut reserved_ll = canonical.clone();
    reserved_ll[3] |= 0x60;
    rejects(&reserved_ll);
    let mut zero_k0 = canonical.clone();
    zero_k0[3] &= !0x07;
    rejects(&zero_k0);
    for opcode in [0x99, 0x9C, 0xA9, 0xAC] {
        let mut wrong_opcode = canonical.clone();
        wrong_opcode[4] = opcode;
        rejects(&wrong_opcode);
    }
    let mut trailing = canonical.clone();
    trailing.push(0);
    rejects(&trailing);
    rejects(&canonical[..canonical.len() - 1]);
    let mut legacy_mandatory = canonical.clone();
    legacy_mandatory.insert(0, 0x66);
    rejects(&legacy_mandatory);
}

#[test]
fn segment_addr32_sib_rip_displacements_and_apx_addresses_rewrite_exactly() {
    let case = FourFmaMemoryCase {
        form: FourFmaForm::Packed,
        negate_product: true,
        destination: 17,
        source_index: 30,
        ll: 2,
        control: MaskControl::Zero,
    };
    let shapes: &[(&str, &[u8], u8, &[u8])] = &[
        ("base", &[], 0x02, &[]),
        ("disp8", &[], 0x43, &[0x7F]),
        ("disp32", &[], 0x83, &[0x78, 0x56, 0x34, 0x12]),
        ("sib", &[], 0x04, &[0x73]),
        ("sib-no-base", &[], 0x04, &[0x25, 0x78, 0x56, 0x34, 0x12]),
        ("rip-relative", &[], 0x05, &[0x78, 0x56, 0x34, 0x12]),
        ("fs-addr32-sib-disp8", &[0x64, 0x67], 0x44, &[0x73, 1]),
        ("high-base", &[], 0x02, &[]),
        ("apx-r18-base", &[], 0x02, &[]),
        ("apx-r22-index", &[], 0x04, &[0x73]),
    ];

    for &(name, prefixes, addressing, tail) in shapes {
        let mut instruction = case.bytes().to_vec();
        match name {
            "high-base" => instruction[1] &= !0x20,
            "apx-r18-base" => instruction[1] |= 0x08,
            "apx-r22-index" => instruction[2] &= !0x04,
            _ => {}
        }
        instruction[5] = (instruction[5] & 0x38) | addressing;
        instruction.extend_from_slice(tail);
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&instruction);

        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap_or_else(|| panic!("{name}: malformed test bytes {bytes:02X?}"))
            .evex_four_fma_memory_encoding()
            .unwrap_or_else(|| panic!("{name}: classifier rejected {bytes:02X?}"));
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            case.stack_instruction(),
            "{name}: {bytes:02X?}"
        );
        for level in LEVELS {
            let function = optimize(function_from_bytes(&bytes), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {bytes:02X?}"));
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{name} {level:?}"
            );
            let (code, _) = lower(&function, case);
            assert_eq!(
                code.windows(case.stack_instruction().len())
                    .filter(|window| *window == case.stack_instruction())
                    .count(),
                1,
                "{name} {level:?}: {code:02X?}"
            );
        }
    }
}

#[test]
fn all_72_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 72);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence =
                sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {case:?}"));
            assert_eq!(
                sequence.encoding.scalar,
                case.scalar(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.negate_product, case.negate_product,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source_index, case.source_index,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source_base,
                case.source_base(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.zeroing,
                case.zeroing(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.ll, case.ll, "{level:?} {case:?}");
            assert_eq!(
                sequence.address_offset,
                if case.mask() == 0 { 0 } else { 6 },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.consumed,
                if case.mask() == 0 { 2 } else { 8 },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            assert_eq!(
                code.windows(case.stack_instruction().len())
                    .filter(|window| *window == case.stack_instruction())
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            // VEX.128.F3.0F VMOVDQU [rsp+8],xmm0 stages the complete tuple.
            assert_eq!(
                code.windows(6)
                    .filter(|window| *window == [0xC5, 0xFA, 0x7F, 0x44, 0x24, 0x08])
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            if case.mask() != 0 {
                // KMOVW EAX,K1 is AVX512F-only; KMOVQ would incorrectly add
                // an AVX512BW requirement on the intended 4FMAPS hosts.
                assert_eq!(
                    code.windows(5)
                        .filter(|window| *window == [0xC4, 0xE1, 0x78, 0x93, 0xC1])
                        .count(),
                    1,
                    "{level:?} {case:?}: missing KMOVW guard: {code:02X?}"
                );
                assert!(
                    !code
                        .windows(5)
                        .any(|window| window == [0xC4, 0xE1, 0xFB, 0x93, 0xC1]),
                    "{level:?} {case:?}: KMOVQ guard requires AVX512BW: {code:02X?}"
                );
            }
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 72 * LEVELS.len());
}

fn assert_rejected(label: &str, function: &SmirFunction) {
    assert!(sequence(function, true).is_none(), "{label}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{label}"
    );
}

#[test]
fn four_fma_matcher_binds_provenance_graph_frontiers_and_apx_guard() {
    let case = FourFmaMemoryCase {
        form: FourFmaForm::Scalar,
        negate_product: true,
        destination: 17,
        source_index: 30,
        ll: 1,
        control: MaskControl::Zero,
    };
    for level in LEVELS {
        let function = optimize(lift_case(case), level);

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[4] = 0x9B;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong opcode provenance", &wrong_provenance);

        let operation_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86FourFma { .. }))
            .unwrap();
        for mutation in 0..6u8 {
            let mut malformed = function.clone();
            let operation = &mut malformed.blocks[0].ops[operation_index];
            match mutation {
                0 => operation.x86_hint = None,
                1 => {
                    let OpKind::X86FourFma { scalar, .. } = &mut operation.kind else {
                        unreachable!()
                    };
                    *scalar = false;
                }
                2 => {
                    let OpKind::X86FourFma { negate_product, .. } = &mut operation.kind else {
                        unreachable!()
                    };
                    *negate_product = false;
                }
                3 => {
                    let OpKind::X86FourFma { src2, .. } = &mut operation.kind else {
                        unreachable!()
                    };
                    *src2 = vector(2, VecWidth::V128);
                }
                4 => {
                    let OpKind::X86FourFma { dst, .. } = &mut operation.kind else {
                        unreachable!()
                    };
                    *dst = vector(2, VecWidth::V128);
                }
                5 => {
                    let OpKind::X86FourFma { mask_zeroing, .. } = &mut operation.kind else {
                        unreachable!()
                    };
                    *mask_zeroing = false;
                }
                _ => unreachable!(),
            }
            assert_rejected("semantic mutation", &malformed);
        }

        let load_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::PredVLoad { .. }))
            .unwrap();
        let mut wrong_load_width = function.clone();
        let OpKind::PredVLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[load_index].kind
        else {
            unreachable!()
        };
        *width = VecWidth::V256;
        assert_rejected("wrong tuple width", &wrong_load_width);

        let mut split_pc = function.clone();
        split_pc.blocks[0].ops[operation_index].guest_pc += 1;
        assert_rejected("split guest-PC frontier", &split_pc);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("same-PC trailing operation", &same_pc_tail);

        let mut spurious_apx = function.clone();
        spurious_apx.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFFFD), PC, OpKind::X86RequireApx));
        assert_rejected("spurious APX guard", &spurious_apx);

        let mut apx_bytes = case.bytes();
        apx_bytes[1] |= 0x08;
        let mut missing_apx = optimize(function_from_bytes(&apx_bytes), level);
        let guard = missing_apx.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86RequireApx))
            .expect("APX address guard");
        assert!(sequence(&missing_apx, true).is_some(), "{level:?}");
        missing_apx.blocks[0].ops.remove(guard);
        assert_rejected("missing APX guard", &missing_apx);
    }
}

#[test]
fn four_fma_matcher_rejects_disabled_memory_and_avx_only_bridge() {
    let case = FourFmaMemoryCase {
        form: FourFmaForm::Packed,
        negate_product: true,
        destination: 17,
        source_index: 20,
        ll: 2,
        control: MaskControl::Merge,
    };
    let function = lift_case(case);
    assert!(sequence(&function, false).is_none());

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}

#[test]
fn masked_lowering_branches_around_the_helper_and_joins_at_exact_replay() {
    for form in FourFmaForm::ALL {
        let case = FourFmaMemoryCase {
            form,
            negate_product: form.scalar(),
            destination: 17,
            source_index: 20,
            ll: if form.scalar() { 1 } else { 2 },
            control: MaskControl::Zero,
        };
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, _) = lower(&function, case);
            let guard = [0x9C, 0x50, 0xC4, 0xE1, 0x78, 0x93, 0xC1, 0x48, 0xF7, 0xC0];
            let guard_at = code
                .windows(guard.len())
                .position(|window| window == guard)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: incomplete K16 guard"));
            let immediate_at = guard_at + guard.len();
            assert_eq!(
                u32::from_le_bytes(code[immediate_at..immediate_at + 4].try_into().unwrap()),
                if form.scalar() { 1 } else { 0xFFFF },
                "{level:?} {case:?}"
            );
            assert_eq!(
                &code[immediate_at + 4..immediate_at + 6],
                &[0x0F, 0x84],
                "{level:?} {case:?}: expected JE inactive"
            );
            let displacement_at = immediate_at + 6;
            let active_at = displacement_at + 4;
            assert_eq!(&code[active_at..active_at + 2], &[0x58, 0x9D]);
            let inactive_at = usize::try_from(
                active_at as i64
                    + i64::from(i32::from_le_bytes(
                        code[displacement_at..active_at].try_into().unwrap(),
                    )),
            )
            .expect("forward inactive target");
            assert_eq!(
                &code[inactive_at..inactive_at + 7],
                &[0x58, 0x9D, 0x48, 0x8D, 0x64, 0x24, 0xF0],
                "{level:?} {case:?}: inactive cleanup/frame"
            );
            assert_eq!(code[inactive_at - 5], 0xE9, "helper-path join jump");
            let execute_at = usize::try_from(
                inactive_at as i64
                    + i64::from(i32::from_le_bytes(
                        code[inactive_at - 4..inactive_at].try_into().unwrap(),
                    )),
            )
            .expect("forward replay target");
            assert_eq!(execute_at, inactive_at + 7, "{level:?} {case:?}");
            let replay = case.stack_instruction();
            assert_eq!(
                &code[execute_at..execute_at + replay.len()],
                &replay,
                "{level:?} {case:?}: both paths join at exact replay"
            );
            assert_eq!(
                &code[execute_at + replay.len()..execute_at + replay.len() + 5],
                &[0x48, 0x8D, 0x64, 0x24, 0x10],
                "{level:?} {case:?}: replay frame restore"
            );
        }
    }
}

#[test]
fn four_fma_llig_and_whole_tuple_graph_are_exact() {
    for case in all_cases() {
        let function = lift_case(case);
        let ops = &function.blocks[0].ops;
        let start = usize::from(matches!(ops[0].kind, OpKind::X86RequireApx));
        if case.mask() == 0 {
            assert!(matches!(
                ops[start].kind,
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                }
            ));
        } else {
            assert!(matches!(ops[start].kind, OpKind::And { .. }));
            assert!(matches!(ops[start + 4].kind, OpKind::Mov { .. }));
            assert!(matches!(ops[start + 5].kind, OpKind::VBroadcast { .. }));
            assert!(matches!(
                ops[start + 6].kind,
                OpKind::PredVLoad {
                    width: VecWidth::V128,
                    ..
                }
            ));
        }
        let operation = ops
            .iter()
            .find(|op| matches!(op.kind, OpKind::X86FourFma { .. }))
            .unwrap();
        assert_eq!(
            operation.x86_hint,
            Some(X86OpHint::EvexOp {
                map: crate::smir::ir::ops::X86VecMap::Map0F38,
                pp: crate::smir::ir::ops::X86SsePrefix::Repne,
                opcode: case.opcode(),
                width: match case.ll {
                    0 => VecWidth::V128,
                    1 => VecWidth::V256,
                    2 => VecWidth::V512,
                    _ => unreachable!(),
                },
                w: false,
            })
        );
    }
}
