use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{DispSize, OpId, VirtualId};

fn selector_for(map: u8, pp: u8, w: bool, opcode: u8) -> Option<IntegerSelector> {
    IntegerSelector::ALL.into_iter().find(|selector| {
        (
            selector.map(),
            selector.pp(),
            selector.w(),
            selector.opcode(),
        ) == (map, pp, w, opcode)
    })
}

#[test]
fn integer_scalar_move_stack_rewrites_match_eight_independent_llvm_23_anchors() {
    // Generated independently by LLVM 23.0.0git with AVX-512F/BW/FP16.
    let anchors: &[(IntegerSelector, u8, &[u8], &[u8])] = &[
        (
            IntegerSelector::DLoad,
            17,
            &[0x62, 0xC1, 0x7D, 0x08, 0x6E, 0x4B, 0x7F],
            &[0x62, 0xE1, 0x7D, 0x08, 0x6E, 0x0C, 0x24],
        ),
        (
            IntegerSelector::DStore,
            31,
            &[0x62, 0x61, 0x7D, 0x08, 0x7E, 0x7C, 0x24, 0x7F],
            &[0x62, 0x61, 0x7D, 0x08, 0x7E, 0x3C, 0x24],
        ),
        (
            IntegerSelector::QLoad7e,
            25,
            &[0x62, 0x41, 0xFE, 0x08, 0x7E, 0x4D, 0x80],
            &[0x62, 0x61, 0xFE, 0x08, 0x7E, 0x0C, 0x24],
        ),
        (
            IntegerSelector::QStoreD6,
            16,
            &[0x62, 0xE1, 0xFD, 0x08, 0xD6, 0x46, 0x80],
            &[0x62, 0xE1, 0xFD, 0x08, 0xD6, 0x04, 0x24],
        ),
        (
            IntegerSelector::QLoad7e,
            18,
            &[0x62, 0xC1, 0xFE, 0x08, 0x7E, 0x53, 0x7F],
            &[0x62, 0xE1, 0xFE, 0x08, 0x7E, 0x14, 0x24],
        ),
        (
            IntegerSelector::QStoreD6,
            29,
            &[0x62, 0x41, 0xFD, 0x08, 0xD6, 0x6C, 0x24, 0x7F],
            &[0x62, 0x61, 0xFD, 0x08, 0xD6, 0x2C, 0x24],
        ),
        (
            IntegerSelector::W0Load,
            18,
            &[0x62, 0xC5, 0x7D, 0x08, 0x6E, 0x53, 0x7F],
            &[0x62, 0xE5, 0x7D, 0x08, 0x6E, 0x14, 0x24],
        ),
        (
            IntegerSelector::W0Store,
            30,
            &[0x62, 0x45, 0x7D, 0x08, 0x7E, 0x75, 0x80],
            &[0x62, 0x65, 0x7D, 0x08, 0x7E, 0x34, 0x24],
        ),
    ];

    for &(selector, vector, source, stack) in anchors {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_scalar_move_memory_encoding()
            .unwrap_or_else(|| panic!("{source:02X?}"));
        assert_eq!(encoding.kind, selector.kind(), "{source:02X?}");
        assert_eq!(encoding.elem, selector.elem(), "{source:02X?}");
        assert_eq!(encoding.vector, vector, "{source:02X?}");
        assert_eq!(encoding.memory_width, selector.memory_width());
        assert_eq!(encoding.stack_instruction.as_slice(), stack);

        let case = IntegerCase {
            selector,
            vector,
            base: 2,
        };
        for level in LEVELS {
            let function = optimize(lift_bytes(source), level);
            assert_exact_graph(&function, case);
            let (code, _) = lower_case(&function, case);
            assert!(code.windows(stack.len()).any(|window| window == stack));
        }
    }
}

#[test]
fn integer_scalar_move_classifier_exhaustively_partitions_selectors_and_controls() {
    let mut accepted_selectors = 0usize;
    for map in 0u8..=7 {
        for pp in 0u8..=3 {
            for w in [false, true] {
                for opcode in u8::MIN..=u8::MAX {
                    let bytes = [
                        0x62,
                        0xF0 | map,
                        (u8::from(w) << 7) | 0x7C | pp,
                        0x08,
                        opcode,
                        0x02,
                    ];
                    let classified = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_scalar_move_memory_encoding()
                        .filter(|encoding| {
                            matches!(
                                encoding.elem,
                                VecElementType::I16 | VecElementType::I32 | VecElementType::I64
                            )
                        });
                    let expected = selector_for(map, pp, w, opcode);
                    assert_eq!(
                        classified.map(|encoding| (
                            encoding.kind,
                            encoding.elem,
                            encoding.memory_width,
                            encoding.needs_avx512fp16,
                        )),
                        expected.map(|selector| (
                            selector.kind(),
                            selector.elem(),
                            selector.memory_width(),
                            selector.needs_avx512fp16(),
                        )),
                        "{bytes:02X?}"
                    );
                    accepted_selectors += usize::from(expected.is_some());
                }
            }
        }
    }
    assert_eq!(accepted_selectors, IntegerSelector::ALL.len());

    let mut accepted_controls = 0usize;
    for selector in IntegerSelector::ALL {
        for vector in [0u8, 7, 8, 15, 16, 23, 24, 31] {
            let canonical = IntegerCase {
                selector,
                vector,
                base: 11,
            }
            .bytes();
            for apx_base_high in [false, true] {
                for apx_index_high in [false, true] {
                    for encoded_vvvv in [0x00u8, 0x38, 0x70, 0x78] {
                        for v_prime in [false, true] {
                            for ll in 0u8..=3 {
                                for b in [false, true] {
                                    for mask in [0u8, 1, 7] {
                                        for zeroing in [false, true] {
                                            let mut bytes = canonical.clone();
                                            bytes[1] =
                                                (bytes[1] & !0x08) | (u8::from(apx_base_high) << 3);
                                            bytes[2] = (bytes[2] & !0x7C)
                                                | encoded_vvvv
                                                | if apx_index_high { 0 } else { 0x04 };
                                            bytes[3] = (u8::from(zeroing) << 7)
                                                | (ll << 5)
                                                | (u8::from(b) << 4)
                                                | (u8::from(v_prime) << 3)
                                                | mask;
                                            let classified = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_scalar_move_memory_encoding();
                                            let valid = encoded_vvvv == 0x78
                                                && v_prime
                                                && ll == 0
                                                && !b
                                                && mask == 0
                                                && !zeroing;
                                            assert_eq!(classified.is_some(), valid, "{bytes:02X?}");
                                            if let Some(encoding) = classified {
                                                assert_eq!(
                                                    encoding,
                                                    IntegerCase {
                                                        selector,
                                                        vector,
                                                        base: 11,
                                                    }
                                                    .expected_encoding(),
                                                    "{bytes:02X?}"
                                                );
                                                accepted_controls += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted_controls, 10 * 8 * 4);

    for selector in IntegerSelector::ALL {
        let case = IntegerCase {
            selector,
            vector: 17,
            base: 2,
        };
        let mut register = case.bytes();
        register[5] |= 0xC0;
        let mut trailing = case.bytes();
        trailing.push(0xA5);
        for bytes in [register, trailing] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_scalar_move_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn all_320_selector_vector_cells_admit_and_lower_at_o0_o1_o2() {
    let bases = [0u8, 2, 4, 5, 7, 8, 12, 13];
    let mut cells = 0usize;
    let mut lowerings = 0usize;
    for selector in IntegerSelector::ALL {
        for vector in 0u8..32 {
            let case = IntegerCase {
                selector,
                vector,
                base: bases[usize::from(vector) & 7],
            };
            cells += 1;
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                lower_case(&function, case);
                lowerings += 1;
            }
        }
    }
    assert_eq!(cells, 10 * 32);
    assert_eq!(lowerings, 10 * 32 * LEVELS.len());
}

pub(super) fn apx_sib_bytes(selector: IntegerSelector) -> Vec<u8> {
    vec![
        0x62,
        0xE8 | selector.map(),
        (u8::from(selector.w()) << 7) | 0x78 | selector.pp(),
        0x08,
        selector.opcode(),
        0x4C,
        0x48,
        0x01,
    ]
}

#[test]
fn integer_scalar_moves_keep_apx_r16_r17_and_all_address_classes_helper_owned() {
    for selector in [
        IntegerSelector::DLoad,
        IntegerSelector::DStore,
        IntegerSelector::QStoreD6,
        IntegerSelector::W1Store,
    ] {
        let bytes = apx_sib_bytes(selector);
        let case = IntegerCase {
            selector,
            vector: 17,
            base: 0,
        };
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(matches!(
                function.blocks[0].ops[0].kind,
                OpKind::X86RequireApx
            ));
            let expected_disp = selector.memory_width().bytes() as i32;
            assert!(function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                } | OpKind::Store {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                } if disp == expected_disp
            )));
            assert_exact_graph(&function, case);
            lower_case(&function, case);
        }
    }

    let case = IntegerCase {
        selector: IntegerSelector::QLoad7e,
        vector: 25,
        base: 2,
    };
    let mut rip = case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    let mut gs_sib = IntegerCase {
        selector: IntegerSelector::W1Store,
        vector: 25,
        base: 2,
    }
    .bytes();
    gs_sib[5] = (gs_sib[5] & 0x38) | 0x44;
    gs_sib.push(0x8B);
    gs_sib.push(2);
    gs_sib.insert(0, 0x65);

    for (name, bytes, expected) in [
        ("RIP", rip, case),
        ("addr32", addr32, case),
        ("FS", fs, case),
        (
            "GS SIB",
            gs_sib,
            IntegerCase {
                selector: IntegerSelector::W1Store,
                vector: 25,
                base: 2,
            },
        ),
    ] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert_exact_graph(&function, expected);
            lower_case(&function, expected);
        }
        assert!(!name.is_empty());
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        exact_sequence(function, true),
        None,
        "{name}: malformed sequence admitted"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: malformed sequence passed the clobber gate"
    );
}

#[test]
fn integer_load_and_store_graph_provenance_hint_and_ssa_mutations_fail_closed() {
    let load_case = IntegerCase {
        selector: IntegerSelector::QLoad6e,
        vector: 17,
        base: 2,
    };
    let load = lift_case(load_case);
    let (loaded, zero) = match (&load.blocks[0].ops[0].kind, &load.blocks[0].ops[1].kind) {
        (OpKind::Load { dst: loaded, .. }, OpKind::Mov { dst: zero, .. }) => (*loaded, *zero),
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut mutation = load.clone();
    let OpKind::Load { width, .. } = &mut mutation.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    malformed.push(("load width", mutation));
    let mut mutation = load.clone();
    let OpKind::Load { sign, .. } = &mut mutation.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *sign = SignExtend::Sign;
    malformed.push(("load extension", mutation));
    let mut mutation = load.clone();
    let OpKind::Load { addr, .. } = &mut mutation.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *addr = Address::Direct(VReg::Virtual(VirtualId(0x7F00)));
    malformed.push(("load address", mutation));
    let mut mutation = load.clone();
    let OpKind::Mov { src, .. } = &mut mutation.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *src = SrcOperand::Imm(1);
    malformed.push(("zero value", mutation));
    let mut mutation = load.clone();
    let OpKind::VBroadcast { lanes, .. } = &mut mutation.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *lanes = 2;
    malformed.push(("broadcast lanes", mutation));
    let mut mutation = load.clone();
    let OpKind::VBroadcast { scalar, .. } = &mut mutation.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *scalar = loaded;
    malformed.push(("broadcast source", mutation));
    let mut mutation = load.clone();
    let OpKind::VInsertLane { vec, .. } = &mut mutation.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *vec = xmm(16);
    malformed.push(("insert vector", mutation));
    let mut mutation = load.clone();
    let OpKind::VInsertLane { scalar, .. } = &mut mutation.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *scalar = zero;
    malformed.push(("insert scalar", mutation));

    for index in 0..4 {
        let mut mutation = load.clone();
        mutation.blocks[0].ops[index].x86_hint = Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0x6E,
            width: VecWidth::V128,
            w: true,
        });
        malformed.push(("invented load hint", mutation));
    }
    for (name, register, id) in [
        ("loaded value escapes", loaded, 0x7F10),
        ("zero value escapes", zero, 0x7F11),
    ] {
        let mut mutation = load.clone();
        mutation.blocks[0].ops.push(SmirOp::new(
            OpId(id),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Reg(register),
                width: OpWidth::W64,
            },
        ));
        malformed.push((name, mutation));
    }
    let mut mutation = load.clone();
    mutation.blocks[0].ops[3].guest_pc += 1;
    malformed.push(("split load guest PC", mutation));
    let mut mutation = load.clone();
    mutation.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F12), PC, OpKind::Nop));
    malformed.push(("same-PC load tail", mutation));
    let mut mutation = load.clone();
    mutation.x86_instruction_bytes.clear();
    malformed.push(("missing load bytes", mutation));

    let store_case = IntegerCase {
        selector: IntegerSelector::W0Store,
        vector: 31,
        base: 2,
    };
    let store = lift_case(store_case);
    let extracted = match store.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut mutation = store.clone();
    let OpKind::VExtractLane { lane, .. } = &mut mutation.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *lane = 1;
    malformed.push(("extract lane", mutation));
    let mut mutation = store.clone();
    let OpKind::VExtractLane { elem, .. } = &mut mutation.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *elem = VecElementType::I32;
    malformed.push(("extract element", mutation));
    let mut mutation = store.clone();
    let OpKind::Store { src, .. } = &mut mutation.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *src = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    malformed.push(("store source", mutation));
    let mut mutation = store.clone();
    let OpKind::Store { width, .. } = &mut mutation.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    malformed.push(("store width", mutation));
    let mut mutation = store.clone();
    mutation.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F20),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Reg(extracted),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value escapes", mutation));
    let mut mutation = store.clone();
    mutation.blocks[0].ops[1].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map5,
        pp: X86SsePrefix::OpSize,
        opcode: 0x7E,
        width: VecWidth::V128,
        w: false,
    });
    malformed.push(("invented store hint", mutation));
    let mut mutation = store;
    mutation.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split store guest PC", mutation));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}
