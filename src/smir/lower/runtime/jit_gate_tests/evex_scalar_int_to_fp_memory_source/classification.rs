use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, DispSize, FpRoundMode, OpId, SignExtend, SrcOperand, VirtualId,
};

#[test]
fn scalar_int_to_fp_classifier_exhaustively_rewrites_147_456_evex_and_apx_cells() {
    let mut accepted = 0usize;
    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for ll in 0..=2u8 {
                    for destination in 0..32u8 {
                        for merge in 0..32u8 {
                            let case = ScalarIntMemoryCase {
                                format,
                                signed,
                                w,
                                ll,
                                destination,
                                merge,
                                base: 2,
                            };
                            for b4 in [false, true] {
                                for encoded_x4 in [false, true] {
                                    let mut bytes = case.bytes();
                                    bytes[1] = (bytes[1] & !0x08) | (u8::from(b4) << 3);
                                    bytes[2] = (bytes[2] & !0x04) | (u8::from(!encoded_x4) << 2);
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_scalar_int_to_fp_memory_encoding()
                                        .unwrap_or_else(|| panic!("{case:?} {bytes:02X?}"));
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.merge, merge, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, format.element(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.int_width,
                                        case.int_width(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(encoding.signed, signed, "{bytes:02X?}");
                                    assert_eq!(encoding.map, format.fields().0, "{bytes:02X?}");
                                    assert_eq!(encoding.pp, format.fields().1, "{bytes:02X?}");
                                    assert_eq!(encoding.w, w, "{bytes:02X?}");
                                    assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                    assert_eq!(encoding.opcode, case.opcode(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.memory_width,
                                        case.memory_width(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512fp16,
                                        format.needs_fp16(),
                                        "{bytes:02X?}"
                                    );
                                    let expected = [
                                        0x62,
                                        (bytes[1] & 0x97) | 0x60,
                                        bytes[2] | 0x04,
                                        bytes[3],
                                        bytes[4],
                                        0xC0 | (bytes[5] & 0x38),
                                    ];
                                    assert_eq!(
                                        encoding.register_instruction.as_slice(),
                                        expected,
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
    assert_eq!(accepted, 3 * 2 * 2 * 3 * 32 * 32 * 2 * 2);
}

#[test]
fn scalar_int_to_fp_classifier_owns_exactly_twelve_map_opcode_pp_w_selectors() {
    let template = ScalarIntMemoryCase {
        format: DestinationFormat::F32,
        signed: true,
        w: false,
        ll: 0,
        destination: 0,
        merge: 1,
        base: 2,
    }
    .bytes();
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
                        (map, opcode, pp),
                        (1, 0x2A | 0x7B, 2 | 3) | (5, 0x2A | 0x7B, 2)
                    );
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_scalar_int_to_fp_memory_encoding()
                        .is_some();
                    assert_eq!(actual, expected, "{bytes:02X?}");
                    accepted += usize::from(actual);
                }
            }
        }
    }
    assert_eq!(accepted, 12);
}

#[test]
fn scalar_int_to_fp_memory_and_rax_rewrites_match_twelve_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git with Intel syntax for
    // `xmm17,xmm30,[r10+127*tuple]`; the paired register forms use RAX/EAX.
    for (format, signed, w, memory, register) in [
        (
            DestinationFormat::F32,
            true,
            false,
            [0x62, 0xC1, 0x0E, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE1, 0x0E, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F32,
            true,
            true,
            [0x62, 0xC1, 0x8E, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE1, 0x8E, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F64,
            true,
            false,
            [0x62, 0xC1, 0x0F, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE1, 0x0F, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F64,
            true,
            true,
            [0x62, 0xC1, 0x8F, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE1, 0x8F, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F32,
            false,
            false,
            [0x62, 0xC1, 0x0E, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE1, 0x0E, 0x00, 0x7B, 0xC8],
        ),
        (
            DestinationFormat::F32,
            false,
            true,
            [0x62, 0xC1, 0x8E, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE1, 0x8E, 0x00, 0x7B, 0xC8],
        ),
        (
            DestinationFormat::F64,
            false,
            false,
            [0x62, 0xC1, 0x0F, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE1, 0x0F, 0x00, 0x7B, 0xC8],
        ),
        (
            DestinationFormat::F64,
            false,
            true,
            [0x62, 0xC1, 0x8F, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE1, 0x8F, 0x00, 0x7B, 0xC8],
        ),
        (
            DestinationFormat::F16,
            true,
            false,
            [0x62, 0xC5, 0x0E, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE5, 0x0E, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F16,
            true,
            true,
            [0x62, 0xC5, 0x8E, 0x00, 0x2A, 0x4A, 0x7F],
            [0x62, 0xE5, 0x8E, 0x00, 0x2A, 0xC8],
        ),
        (
            DestinationFormat::F16,
            false,
            false,
            [0x62, 0xC5, 0x0E, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE5, 0x0E, 0x00, 0x7B, 0xC8],
        ),
        (
            DestinationFormat::F16,
            false,
            true,
            [0x62, 0xC5, 0x8E, 0x00, 0x7B, 0x4A, 0x7F],
            [0x62, 0xE5, 0x8E, 0x00, 0x7B, 0xC8],
        ),
    ] {
        let encoding = X86InstructionBytes::new(&memory)
            .unwrap()
            .evex_scalar_int_to_fp_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.destination, 17, "{memory:02X?}");
        assert_eq!(encoding.merge, 30, "{memory:02X?}");
        assert_eq!(encoding.elem, format.element(), "{memory:02X?}");
        assert_eq!(encoding.signed, signed, "{memory:02X?}");
        assert_eq!(encoding.w, w, "{memory:02X?}");
        assert_eq!(encoding.register_instruction.as_slice(), register);
    }
}

#[test]
fn scalar_int_to_fp_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = ScalarIntMemoryCase {
        format: DestinationFormat::F64,
        signed: true,
        w: true,
        ll: 1,
        destination: 17,
        merge: 30,
        base: 2,
    }
    .bytes()
    .to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for bit in [0x01, 0x02, 0x04, 0x80, 0x10] {
        let mut bytes = valid.clone();
        bytes[3] |= bit;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    for prefix in [0x66, 0xF0, 0x40] {
        let mut bytes = valid.clone();
        bytes.insert(0, prefix);
        malformed.push(bytes);
    }
    for (index, xor) in [(1, 0x02), (2, 0x02), (4, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= xor;
        malformed.push(bytes);
    }

    for (mutation, bytes) in malformed.into_iter().enumerate() {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_int_to_fp_memory_encoding()
                .is_none(),
            "mutation={mutation} {bytes:02X?}"
        );
    }
}

#[test]
fn all_108_scanner_cells_optimize_admit_and_lower_exactly_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 108);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.consumed, 2, "{level:?} {case:?}");
            assert_eq!(function.blocks[0].ops.len(), 2, "{level:?} {case:?}");
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.merge, case.merge);
            assert_eq!(exact.encoding.elem, case.format.element());
            assert_eq!(exact.encoding.int_width, case.int_width());
            assert_eq!(exact.encoding.signed, case.signed);
            assert_eq!(exact.encoding.ll, case.ll);
            assert!(matches!(
                function.blocks[0].ops[0].kind,
                OpKind::Load {
                    width,
                    sign,
                    ..
                } if width == case.memory_width()
                    && sign == if case.signed { SignExtend::Sign } else { SignExtend::Zero }
            ));

            let (code, _) = lower(&function, case);
            let expected = case.register_instruction();
            assert_eq!(
                code.windows(expected.len())
                    .filter(|window| *window == expected)
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            assert!(
                code.windows(5)
                    .any(|window| { window == [0xB9, case.memory_size() as u8, 0, 0, 0] }),
                "{level:?} {case:?}: exact helper width"
            );
            let mut scratch_load = Vec::new();
            if case.w {
                scratch_load.push(0x48);
            }
            scratch_load.extend_from_slice(&[0x8B, 0x80]);
            scratch_load.extend_from_slice(&(X86_GUEST_VECTOR_SCRATCH_OFFSET as u32).to_le_bytes());
            assert!(
                code.windows(scratch_load.len())
                    .any(|window| window == scratch_load),
                "{level:?} {case:?}: scratch-to-RAX import"
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 108 * LEVELS.len());
}

#[test]
fn feature_aggregation_requires_fp16_only_for_binary16_and_obeys_exclusion() {
    for format in DestinationFormat::ALL {
        let case = ScalarIntMemoryCase {
            format,
            signed: false,
            w: true,
            ll: 2,
            destination: 31,
            merge: 30,
            base: 2,
        };
        let function = lift_case(case);
        let actual = x86_native_replay_feature_requirements(&function, &HashMap::new());
        assert!(actual.any, "{case:?}");
        assert!(actual.needs_avx, "{case:?}");
        assert!(actual.needs_avx512bw, "{case:?}");
        assert_eq!(actual.needs_avx512fp16, format.needs_fp16(), "{case:?}");
        assert!(!actual.needs_avx512vl, "{case:?}");
        assert!(!actual.needs_avx512dq, "{case:?}");
        assert!(!actual.needs_avx512er, "{case:?}");
        assert!(!actual.needs_avx5124fmaps, "{case:?}");
        assert!(!actual.has_k16_opmask_span, "{case:?}");
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &HashMap::from([(BlockId(0), PC)])),
            X86NativeReplayFeatureRequirements::default(),
            "{case:?}"
        );
    }
}

#[test]
fn segment_addr32_rip_sib_and_apx_addresses_remain_helper_owned() {
    let case = ScalarIntMemoryCase {
        format: DestinationFormat::F64,
        signed: false,
        w: true,
        ll: 1,
        destination: 17,
        merge: 30,
        base: 3,
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
                    OpKind::Load { addr, .. } | OpKind::Lea { addr, .. } => {
                        addr == &expected_address
                    }
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.encoding.register_instruction.as_slice(),
                case.register_instruction()
            );
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.register_instruction().len())
                    .any(|window| window == case.register_instruction()),
                "{name} {level:?}"
            );
        }
    }

    let apx_case = ScalarIntMemoryCase {
        format: DestinationFormat::F32,
        signed: true,
        w: false,
        ll: 0,
        destination: 16,
        merge: 17,
        base: 0,
    };
    // VCVTSI2SS xmm16,xmm17,[r16+r17*2+4]. B4/X4 are address-only.
    let apx = [0x62, 0xE9, 0x72, 0x00, 0x2A, 0x44, 0x48, 0x01];
    let base = function_from_bytes(&apx, "APX scalar integer-to-FP");
    let expected_address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 4,
        disp_size: DispSize::Disp8,
    };
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(
            function.blocks[0].ops.iter().any(
                |op| matches!(&op.kind, OpKind::Load { addr, .. } if addr == &expected_address)
            )
        );
        let exact = sequence(&function).expect("APX guarded sequence");
        assert_eq!(
            exact.encoding.register_instruction.as_slice(),
            apx_case.register_instruction()
        );
        let (code, _) = lower(&function, apx_case);
        assert!(
            code.windows(apx_case.register_instruction().len())
                .any(|window| window == apx_case.register_instruction())
        );
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
fn scalar_int_to_fp_sequence_fails_closed_for_every_provenance_graph_and_ssa_mutation() {
    for case in [
        ScalarIntMemoryCase {
            format: DestinationFormat::F32,
            signed: true,
            w: false,
            ll: 2,
            destination: 17,
            merge: 30,
            base: 2,
        },
        ScalarIntMemoryCase {
            format: DestinationFormat::F64,
            signed: false,
            w: true,
            ll: 1,
            destination: 31,
            merge: 17,
            base: 2,
        },
        ScalarIntMemoryCase {
            format: DestinationFormat::F16,
            signed: true,
            w: true,
            ll: 0,
            destination: 0,
            merge: 1,
            base: 2,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        let exact = sequence(&function).unwrap_or_else(|| panic!("{case:?}"));
        let load_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::Load { .. }))
            .unwrap();
        let (definitions, uses) = virtual_counts(&function);
        assert!(
            x86_jit_evex_scalar_int_to_fp_memory_sequence(
                &function.blocks[0],
                load_index,
                false,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .is_none()
        );
        assert_eq!(exact.consumed, 2);

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut wrong_bytes = case.bytes();
        wrong_bytes[4] ^= 0x51;
        wrong_provenance.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&wrong_bytes).unwrap(),
        );
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut load_hint = function.clone();
        load_hint.blocks[0].ops[load_index].x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("invented load hint", &load_hint);

        let mut wrong_width = function.clone();
        let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[load_index].kind else {
            unreachable!()
        };
        *width = if case.w { MemWidth::B4 } else { MemWidth::B8 };
        assert_rejected("load width", &wrong_width);

        let mut wrong_sign = function.clone();
        let OpKind::Load { sign, .. } = &mut wrong_sign.blocks[0].ops[load_index].kind else {
            unreachable!()
        };
        *sign = if case.signed {
            SignExtend::Zero
        } else {
            SignExtend::Sign
        };
        assert_rejected("load extension", &wrong_sign);

        let mut architectural_load = function.clone();
        let OpKind::Load { dst, .. } = &mut architectural_load.blocks[0].ops[load_index].kind
        else {
            unreachable!()
        };
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        assert_rejected("architectural load destination", &architectural_load);

        let mut virtual_address = function.clone();
        let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[load_index].kind else {
            unreachable!()
        };
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        assert_rejected("virtual address component", &virtual_address);

        let conversion_index = load_index + 1;
        let mut wrong_hint = function.clone();
        wrong_hint.blocks[0].ops[conversion_index].x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("conversion hint", &wrong_hint);

        macro_rules! mutate_conversion {
            ($name:literal, $field:ident, $value:expr) => {{
                let mut malformed = function.clone();
                let OpKind::X86IntToFp { $field, .. } =
                    &mut malformed.blocks[0].ops[conversion_index].kind
                else {
                    unreachable!()
                };
                *$field = $value;
                assert_rejected($name, &malformed);
            }};
        }
        mutate_conversion!(
            "destination",
            dst,
            VReg::Arch(ArchReg::X86(X86Reg::Xmm((case.destination + 1) % 32)))
        );
        mutate_conversion!(
            "merge source",
            merge,
            VReg::Arch(ArchReg::X86(X86Reg::Xmm((case.merge + 1) % 32)))
        );
        mutate_conversion!("source value", src, VReg::Virtual(VirtualId(0xFFFD)));
        mutate_conversion!(
            "destination format",
            elem,
            if case.format == DestinationFormat::F16 {
                VecElementType::F32
            } else {
                VecElementType::F16
            }
        );
        mutate_conversion!(
            "integer width",
            int_width,
            if case.w { OpWidth::W32 } else { OpWidth::W64 }
        );
        mutate_conversion!("signedness", signed, !case.signed);
        mutate_conversion!("rounding", round, FpRoundMode::RoundNearest);
        mutate_conversion!("exception suppression", suppress_exceptions, true);
        mutate_conversion!("upper-lane policy", zero_upper, false);

        let loaded = match function.blocks[0].ops[load_index].kind {
            OpKind::Load { dst, .. } => dst,
            _ => unreachable!(),
        };
        let mut extra_use = function.clone();
        extra_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFC),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFC)),
                src: SrcOperand::Reg(loaded),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("loaded value escapes", &extra_use);

        let mut duplicate_definition = function.clone();
        duplicate_definition.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFB),
            PC + 1,
            OpKind::Mov {
                dst: loaded,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("loaded value redefined", &duplicate_definition);

        let mut wrong_pc = function.clone();
        wrong_pc.blocks[0].ops[conversion_index].guest_pc += 1;
        assert_rejected("split guest PC", &wrong_pc);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0]
            .ops
            .push(SmirOp::new(OpId(0xFFFA), PC, OpKind::Nop));
        assert_rejected("same-PC tail", &same_pc_tail);

        let mut preceding_same_pc = function.clone();
        preceding_same_pc.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFFF9), PC, OpKind::Nop));
        assert_rejected("non-APX same-PC prefix", &preceding_same_pc);

        let mut unexpected_apx_guard = function.clone();
        unexpected_apx_guard.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFFF8), PC, OpKind::X86RequireApx));
        assert_rejected("unnecessary APX guard", &unexpected_apx_guard);
    }

    let apx = [0x62, 0xE9, 0x72, 0x00, 0x2A, 0x44, 0x48, 0x01];
    let mut missing_apx_guard = function_from_bytes(&apx, "APX guard mutation");
    assert!(matches!(
        missing_apx_guard.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    missing_apx_guard.blocks[0].ops.remove(0);
    assert_rejected("missing APX guard", &missing_apx_guard);
}

#[test]
fn scalar_int_to_fp_lowerer_rejects_the_avx_only_vector_bridge() {
    let case = ScalarIntMemoryCase {
        format: DestinationFormat::F32,
        signed: true,
        w: true,
        ll: 2,
        destination: 17,
        merge: 30,
        base: 2,
    };
    let function = lift_case(case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}
