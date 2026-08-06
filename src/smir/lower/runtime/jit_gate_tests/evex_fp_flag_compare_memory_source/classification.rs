use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    BlockId, MemWidth, OpId, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};

fn expected_hint(case: Case) -> Option<X86OpHint> {
    let width = match case.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => unreachable!(),
    };
    match case.format {
        Format::F16 => None,
        Format::F32 => Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: case.opcode(),
            width,
            w: false,
        }),
        Format::F64 => Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode(),
            width,
            w: true,
        }),
    }
}

fn assert_exact_graph(function: &SmirFunction, case: Case) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 3, "{case:?}: {ops:?}");
    let loaded = match &ops[0].kind {
        OpKind::Load {
            dst: scalar @ VReg::Virtual(_),
            width,
            sign: SignExtend::Zero,
            ..
        } => {
            assert_eq!(*width, case.format.memory_width(), "{case:?}");
            assert_eq!(ops[0].x86_hint, None, "{case:?}");
            *scalar
        }
        other => panic!("{case:?}: expected scalar load, got {other:?}"),
    };
    let source2 = match ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: 1,
        } => {
            assert_eq!(scalar, loaded, "{case:?}");
            assert_eq!(elem, case.format.elem(), "{case:?}");
            assert_eq!(ops[1].x86_hint, None, "{case:?}");
            vector
        }
        ref other => panic!("{case:?}: expected scalar broadcast, got {other:?}"),
    };
    assert!(
        matches!(
            ops[2].kind,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
                suppress_exceptions: false,
            } if src1 == x86(X86Reg::Xmm(case.source1))
                && src2 == source2
                && elem == case.format.elem()
                && signaling == case.signaling
        ),
        "{case:?}: {:?}",
        ops[2].kind
    );
    assert_eq!(ops[2].x86_hint, expected_hint(case), "{case:?}");
}

#[test]
fn rewrites_match_six_independent_llvm_23_memory_anchors() {
    let anchors: &[(&[u8], &[u8], Case)] = &[
        (
            &[0x62, 0xC1, 0x7C, 0x08, 0x2F, 0x4B, 0x7F],
            &[0x62, 0xE1, 0x7C, 0x08, 0x2F, 0x0C, 0x24],
            Case {
                format: Format::F32,
                signaling: true,
                source1: 17,
                ll: 0,
            },
        ),
        (
            &[0x62, 0x61, 0x7C, 0x08, 0x2E, 0x7C, 0x24, 0x7F],
            &[0x62, 0x61, 0x7C, 0x08, 0x2E, 0x3C, 0x24],
            Case {
                format: Format::F32,
                signaling: false,
                source1: 31,
                ll: 0,
            },
        ),
        (
            &[0x62, 0x41, 0xFD, 0x08, 0x2F, 0x4D, 0x80],
            &[0x62, 0x61, 0xFD, 0x08, 0x2F, 0x0C, 0x24],
            Case {
                format: Format::F64,
                signaling: true,
                source1: 25,
                ll: 0,
            },
        ),
        (
            &[0x62, 0xE1, 0xFD, 0x08, 0x2E, 0x46, 0x80],
            &[0x62, 0xE1, 0xFD, 0x08, 0x2E, 0x04, 0x24],
            Case {
                format: Format::F64,
                signaling: false,
                source1: 16,
                ll: 0,
            },
        ),
        (
            &[0x62, 0xC5, 0x7C, 0x08, 0x2F, 0x53, 0x7F],
            &[0x62, 0xE5, 0x7C, 0x08, 0x2F, 0x14, 0x24],
            Case {
                format: Format::F16,
                signaling: true,
                source1: 18,
                ll: 0,
            },
        ),
        (
            &[0x62, 0x45, 0x7C, 0x08, 0x2E, 0x75, 0x80],
            &[0x62, 0x65, 0x7C, 0x08, 0x2E, 0x34, 0x24],
            Case {
                format: Format::F16,
                signaling: false,
                source1: 30,
                ll: 0,
            },
        ),
    ];

    for (memory, stack, case) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_fp_flag_compare_memory_encoding()
            .unwrap_or_else(|| panic!("LLVM anchor rejected: {memory:02X?}"));
        assert_eq!(encoding.source1, case.source1, "{memory:02X?}");
        assert_eq!(encoding.elem, case.format.elem(), "{memory:02X?}");
        assert_eq!(encoding.signaling, case.signaling, "{memory:02X?}");
        assert_eq!(encoding.ll, case.ll, "{memory:02X?}");
        assert_eq!(encoding.memory_width, case.format.memory_width());
        assert_eq!(encoding.needs_avx512fp16, case.format == Format::F16);
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *stack,
            "{memory:02X?}"
        );

        for level in LEVELS {
            let function = optimize(function_from_bytes(memory, case), level);
            let (code, _) = lower(&function, *case);
            assert!(
                code.windows(stack.len()).any(|window| window == *stack),
                "{level:?} {case:?}: {code:02X?}"
            );
        }
    }
}

#[test]
fn classifier_exhausts_4_194_304_selector_control_and_length_cells() {
    let mut accepted = 0usize;
    for map in 0u8..=7 {
        for opcode in u8::MIN..=u8::MAX {
            for pp in 0u8..=3 {
                for w in [false, true] {
                    for ll in 0u8..=3 {
                        for b in [false, true] {
                            for z in [false, true] {
                                for aaa in 0u8..=7 {
                                    for trailing in [false, true] {
                                        let bytes = [
                                            0x62,
                                            0xE0 | map,
                                            0x7C | pp | (u8::from(w) << 7),
                                            (u8::from(z) << 7)
                                                | (ll << 5)
                                                | (u8::from(b) << 4)
                                                | 0x08
                                                | aaa,
                                            opcode,
                                            0x0A,
                                            0xA5,
                                        ];
                                        let len = if trailing { 7 } else { 6 };
                                        let actual = X86InstructionBytes::new(&bytes[..len])
                                            .unwrap()
                                            .evex_fp_flag_compare_memory_encoding()
                                            .map(|encoding| {
                                                (
                                                    encoding.elem,
                                                    encoding.signaling,
                                                    encoding.source1,
                                                    encoding.ll,
                                                    encoding.memory_width,
                                                    encoding.needs_avx512fp16,
                                                )
                                            });
                                        let format = match (map, pp, w) {
                                            (1, 0, false) => Some(Format::F32),
                                            (1, 1, true) => Some(Format::F64),
                                            (5, 0, false) => Some(Format::F16),
                                            _ => None,
                                        };
                                        let expected = format
                                            .filter(|_| {
                                                matches!(opcode, 0x2E | 0x2F)
                                                    && ll < 3
                                                    && !b
                                                    && !z
                                                    && aaa == 0
                                                    && !trailing
                                            })
                                            .map(|format| {
                                                (
                                                    format.elem(),
                                                    opcode == 0x2F,
                                                    17,
                                                    ll,
                                                    format.memory_width(),
                                                    format == Format::F16,
                                                )
                                            });
                                        assert_eq!(actual, expected, "{:02X?}", &bytes[..len]);
                                        accepted += usize::from(actual.is_some());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18);
}

#[test]
fn classifier_exhausts_vvvv_v_prime_and_rejects_non_owned_shapes() {
    let canonical = Case {
        format: Format::F32,
        signaling: true,
        source1: 17,
        ll: 2,
    }
    .bytes();

    for vvvv in 0u8..32 {
        let mut bytes = canonical;
        bytes[2] = (bytes[2] & 0x87) | (((!vvvv) & 0x0F) << 3);
        bytes[3] = (bytes[3] & !0x08) | (u8::from(vvvv & 0x10 == 0) << 3);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_fp_flag_compare_memory_encoding()
                .is_some(),
            vvvv == 0,
            "vvvv={vvvv} {bytes:02X?}"
        );
    }

    let rejects = |bytes: &[u8]| {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .and_then(|instruction| instruction.evex_fp_flag_compare_memory_encoding()),
            None,
            "{bytes:02X?}"
        );
    };
    let mut register = canonical.to_vec();
    register[5] |= 0xC0;
    rejects(&register);
    let mut sae = canonical.to_vec();
    sae[3] |= 0x10;
    rejects(&sae);
    let mut zeroing = canonical.to_vec();
    zeroing[3] |= 0x80;
    rejects(&zeroing);
    let mut opmask = canonical.to_vec();
    opmask[3] |= 1;
    rejects(&opmask);
    let mut ll3 = canonical.to_vec();
    ll3[3] = (ll3[3] & !0x60) | 0x60;
    rejects(&ll3);
    let mut wrong_prefix = canonical.to_vec();
    wrong_prefix[2] ^= 1;
    rejects(&wrong_prefix);
    let mut wrong_w = canonical.to_vec();
    wrong_w[2] |= 0x80;
    rejects(&wrong_w);
    let mut wrong_map = canonical.to_vec();
    wrong_map[1] = (wrong_map[1] & !7) | 7;
    rejects(&wrong_map);
    let mut wrong_opcode = canonical.to_vec();
    wrong_opcode[4] ^= 2;
    rejects(&wrong_opcode);
    let mut trailing = canonical.to_vec();
    trailing.push(0);
    rejects(&trailing);
    rejects(&canonical[..5]);
    let mut legacy_mandatory = canonical.to_vec();
    legacy_mandatory.insert(0, 0x66);
    rejects(&legacy_mandatory);
}

#[test]
fn all_1_728_encoding_optimization_cells_admit_and_lower_once() {
    let cases = all_cases();
    assert_eq!(cases.len(), 576);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let sequence = sequence(&function).unwrap_or_else(|| panic!("{level:?} {case:?}"));
            assert_eq!(sequence.consumed, 3, "{level:?} {case:?}");
            assert_eq!(sequence.address_offset, 0, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.source1, case.source1,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.elem,
                case.format.elem(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.signaling, case.signaling,
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.ll, case.ll, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.memory_width,
                case.format.memory_width(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{level:?} {case:?}"
            );
            let (code, _) = lower(&function, case);
            let expected = case.stack_instruction();
            assert_eq!(
                code.windows(expected.len())
                    .filter(|window| *window == expected)
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 576 * LEVELS.len());
}

#[test]
fn segment_addr32_rip_sib_and_apx_address_frontiers_admit_and_lower() {
    let case = Case {
        format: Format::F64,
        signaling: true,
        source1: 25,
        ll: 2,
    };
    let canonical = case.bytes();

    let mut rip = canonical.to_vec();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x4433_2211u32.to_le_bytes());

    let mut addr32 = canonical.to_vec();
    addr32.insert(0, 0x67);

    let mut fs_sib = canonical.to_vec();
    fs_sib[5] = (fs_sib[5] & 0x38) | 4;
    fs_sib.insert(6, 0x4A);
    fs_sib.insert(0, 0x64);

    let mut gs_addr32_sib = canonical.to_vec();
    gs_addr32_sib[5] = (gs_addr32_sib[5] & 0x38) | 0x44;
    gs_addr32_sib.insert(6, 0x8B);
    gs_addr32_sib.insert(7, 2);
    gs_addr32_sib.insert(0, 0x67);
    gs_addr32_sib.insert(0, 0x65);

    let mut apx_base = canonical.to_vec();
    apx_base[1] |= 0x08;

    let mut apx_index = canonical.to_vec();
    apx_index[5] = (apx_index[5] & 0x38) | 4;
    apx_index.insert(6, 0x0A);
    apx_index[2] &= !0x04;

    for (name, bytes) in [
        ("RIP+disp32", rip),
        ("addr32", addr32),
        ("FS SIB", fs_sib),
        ("GS addr32 SIB", gs_addr32_sib),
        ("APX R18 base", apx_base),
        ("APX R17 index", apx_index),
    ] {
        for level in LEVELS {
            let function = optimize(function_from_bytes(&bytes, name), level);
            assert!(
                sequence(&function).is_some(),
                "{level:?} {name}: {bytes:02X?}"
            );
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.stack_instruction().len())
                    .any(|window| window == case.stack_instruction()),
                "{level:?} {name}: {code:02X?}"
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function).is_none(), "{name}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: malformed sequence reached native admission"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_graph_ssa_and_frontier_mutations() {
    let case = Case {
        format: Format::F64,
        signaling: true,
        source1: 25,
        ll: 2,
    };
    for level in LEVELS {
        let base = optimize(lift_case(case), level);
        assert_exact_graph(&base, case);
        let loaded = match base.blocks[0].ops[0].kind {
            OpKind::Load { dst, .. } => dst,
            _ => unreachable!(),
        };
        let broadcast = match base.blocks[0].ops[1].kind {
            OpKind::VBroadcast { dst, .. } => dst,
            _ => unreachable!(),
        };
        let mut malformed = Vec::new();

        let mut missing = base.clone();
        missing.x86_instruction_bytes.clear();
        malformed.push(("missing provenance", missing));

        for (name, index, xor) in [
            ("encoded map", 1, 0x03),
            ("encoded prefix", 2, 0x01),
            ("encoded LL", 3, 0x20),
            ("encoded opcode", 4, 0x01),
            ("encoded source", 5, 0x08),
        ] {
            let mut function = base.clone();
            let mut bytes = case.bytes();
            bytes[index] ^= xor;
            function
                .x86_instruction_bytes
                .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
            malformed.push((name, function));
        }

        let mut load_hint = base.clone();
        load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
        malformed.push(("load hint", load_hint));
        let mut load_width = base.clone();
        if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
            *width = MemWidth::B4;
        }
        malformed.push(("load width", load_width));
        let mut load_sign = base.clone();
        if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[0].kind {
            *sign = SignExtend::Sign;
        }
        malformed.push(("load sign", load_sign));

        let mut broadcast_scalar = base.clone();
        if let OpKind::VBroadcast { scalar, .. } = &mut broadcast_scalar.blocks[0].ops[1].kind {
            *scalar = broadcast;
        }
        malformed.push(("broadcast scalar", broadcast_scalar));
        let mut broadcast_elem = base.clone();
        if let OpKind::VBroadcast { elem, .. } = &mut broadcast_elem.blocks[0].ops[1].kind {
            *elem = VecElementType::F32;
        }
        malformed.push(("broadcast element", broadcast_elem));
        let mut broadcast_lanes = base.clone();
        if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[1].kind {
            *lanes = 2;
        }
        malformed.push(("broadcast lanes", broadcast_lanes));
        let mut broadcast_hint = base.clone();
        broadcast_hint.blocks[0].ops[1].x86_hint =
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
        malformed.push(("broadcast hint", broadcast_hint));

        let mut source1 = base.clone();
        if let OpKind::X86FpCompare { src1, .. } = &mut source1.blocks[0].ops[2].kind {
            *src1 = x86(X86Reg::Xmm(24));
        }
        malformed.push(("compare source1", source1));
        let mut source2 = base.clone();
        if let OpKind::X86FpCompare { src2, .. } = &mut source2.blocks[0].ops[2].kind {
            *src2 = loaded;
        }
        malformed.push(("compare source2", source2));
        let mut compare_elem = base.clone();
        if let OpKind::X86FpCompare { elem, .. } = &mut compare_elem.blocks[0].ops[2].kind {
            *elem = VecElementType::F32;
        }
        malformed.push(("compare element", compare_elem));
        let mut compare_kind = base.clone();
        if let OpKind::X86FpCompare { signaling, .. } = &mut compare_kind.blocks[0].ops[2].kind {
            *signaling = false;
        }
        malformed.push(("compare signaling", compare_kind));
        let mut suppress = base.clone();
        if let OpKind::X86FpCompare {
            suppress_exceptions,
            ..
        } = &mut suppress.blocks[0].ops[2].kind
        {
            *suppress_exceptions = true;
        }
        malformed.push(("exception suppression", suppress));
        let mut compare_hint = base.clone();
        compare_hint.blocks[0].ops[2].x86_hint = None;
        malformed.push(("compare hint", compare_hint));

        let mut loaded_use = base.clone();
        loaded_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFF0),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFF0)),
                src: SrcOperand::Reg(loaded),
                width: OpWidth::W64,
            },
        ));
        malformed.push(("loaded external use", loaded_use));
        let mut broadcast_use = base.clone();
        broadcast_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFF1),
            PC + 1,
            OpKind::X86FpCompare {
                src1: x86(X86Reg::Xmm(25)),
                src2: broadcast,
                elem: VecElementType::F64,
                signaling: true,
                suppress_exceptions: false,
            },
        ));
        malformed.push(("broadcast external use", broadcast_use));
        let mut wrong_pc = base.clone();
        wrong_pc.blocks[0].ops[2].guest_pc += 1;
        malformed.push(("split guest PC", wrong_pc));
        let mut same_pc_tail = base.clone();
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFF2),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFF2)),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        malformed.push(("same-PC tail", same_pc_tail));

        for (name, function) in malformed {
            assert_rejected(name, &function);
        }

        let mut spurious_apx = base.clone();
        spurious_apx.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFFF3), PC, OpKind::X86RequireApx));
        assert_rejected("spurious APX guard", &spurious_apx);

        let mut apx_bytes = case.bytes();
        apx_bytes[1] |= 0x08;
        let mut missing_apx = optimize(function_from_bytes(&apx_bytes, "APX"), level);
        let guard = missing_apx.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86RequireApx))
            .expect("APX address guard");
        missing_apx.blocks[0].ops.remove(guard);
        assert_rejected("missing APX guard", &missing_apx);
    }
}

#[test]
fn matcher_rejects_disabled_memory_and_avx_only_vector_bridge() {
    let case = Case {
        format: Format::F64,
        signaling: false,
        source1: 30,
        ll: 2,
    };
    let function = lift_case(case);
    let (definitions, uses) = virtual_counts(&function);
    assert!((0..function.blocks[0].ops.len()).all(|index| {
        x86_jit_evex_fp_flag_compare_memory_sequence(
            &function.blocks[0],
            index,
            false,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    }));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}
