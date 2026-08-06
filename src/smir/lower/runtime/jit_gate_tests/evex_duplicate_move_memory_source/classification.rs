use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, OpId, OpWidth, SrcOperand, VReg, VecWidth, VirtualId, X86Reg,
};

#[test]
fn classifier_exhausts_41_472_operand_control_and_apx_cells() {
    let bases = [0u8, 1, 2, 3, 6, 7, 8, 9, 10, 11, 14, 15];
    let mut accepted = 0usize;
    for kind in DuplicateKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in MaskControl::ALL {
                for destination in 0..32u8 {
                    for base in bases {
                        let case = DuplicateMemoryCase {
                            kind,
                            width,
                            destination,
                            base,
                            control,
                        };
                        let canonical = case.bytes();
                        for base4 in [false, true] {
                            for index4 in [false, true] {
                                let mut bytes = canonical;
                                bytes[1] |= u8::from(base4) << 3;
                                if index4 {
                                    bytes[2] &= !0x04;
                                }
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_duplicate_move_memory_encoding()
                                    .unwrap_or_else(|| panic!("{case:?} {bytes:02X?}"));
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, kind.elem(), "{bytes:02X?}");
                                assert_eq!(encoding.high, kind.high(), "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.writemask,
                                    (case.mask() != 0).then_some(case.mask()),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(encoding.zeroing, case.zeroing(), "{bytes:02X?}");
                                assert_eq!(
                                    encoding.memory_size,
                                    case.memory_size(),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(encoding.scratch, case.scratch(), "{bytes:02X?}");
                                assert_eq!(
                                    encoding.register_instruction.as_slice(),
                                    case.register_instruction(),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoding.needs_avx512vl,
                                    width != VecWidth::V512,
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
    assert_eq!(accepted, 3 * 3 * 3 * 32 * bases.len() * 4);
}

#[test]
fn classifier_exhausts_map1_opcode_pp_w_and_vector_length_space() {
    let expected: Vec<_> = DuplicateKind::ALL
        .into_iter()
        .flat_map(|kind| (0..=2u8).map(move |ll| (kind.opcode(), kind.pp(), kind.w(), ll, kind)))
        .collect();
    assert_eq!(expected.len(), 9);

    let mut classified = 0usize;
    for opcode in u8::MIN..=u8::MAX {
        for pp in 0..=3u8 {
            for w in [false, true] {
                for ll in 0..=3u8 {
                    let bytes = [
                        0x62,
                        0xF1,
                        0x7C | pp | (u8::from(w) << 7),
                        (ll << 5) | 0x09,
                        opcode,
                        0x0A,
                    ];
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_duplicate_move_memory_encoding();
                    let matching: Vec<_> = expected
                        .iter()
                        .filter(|(op, prefix, fixed_w, expected_ll, _)| {
                            (*op, *prefix, *fixed_w, *expected_ll) == (opcode, pp, w, ll)
                        })
                        .collect();
                    assert!(matching.len() <= 1, "ambiguous selector {bytes:02X?}");
                    match matching.first() {
                        Some((_, _, _, _, kind)) => {
                            let encoding = actual.unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.elem, kind.elem(), "{bytes:02X?}");
                            assert_eq!(encoding.high, kind.high(), "{bytes:02X?}");
                            classified += 1;
                        }
                        None => assert_eq!(actual, None, "{bytes:02X?}"),
                    }
                }
            }
        }
    }
    assert_eq!(classified, 9);
}

#[test]
fn register_rewrites_match_independently_assembled_llvm_23_anchors() {
    // Each source/replay pair was independently assembled with llvm-mc
    // 23.0.0git. The replay replaces only the helper-owned memory operand.
    for (source, replay) in [
        (
            &[0x62, 0xF1, 0x7E, 0x8B, 0x12, 0x08][..],
            &[0x62, 0xF1, 0x7E, 0x8B, 0x12, 0xC8][..],
        ),
        (
            &[0x62, 0x51, 0x7E, 0x2A, 0x16, 0x48, 0x01],
            &[0x62, 0x71, 0x7E, 0x2A, 0x16, 0xC8],
        ),
        (
            &[0x62, 0xC1, 0xFF, 0xCF, 0x12, 0x49, 0x01],
            &[0x62, 0xE1, 0xFF, 0xCF, 0x12, 0xC8],
        ),
        (
            &[0x62, 0x61, 0xFF, 0x08, 0x12, 0x0D, 0x10, 0, 0, 0],
            &[0x62, 0x61, 0xFF, 0x08, 0x12, 0xC8],
        ),
    ] {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_duplicate_move_memory_encoding()
            .unwrap_or_else(|| panic!("{source:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), replay);
    }
}

#[test]
fn classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = DuplicateMemoryCase {
        kind: DuplicateKind::HighF32,
        width: VecWidth::V256,
        destination: 17,
        base: 2,
        control: MaskControl::Merge,
    };
    let valid = case.bytes().to_vec();
    let mut malformed = vec![valid[..5].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut embedded_control = valid.clone();
    embedded_control[3] |= 0x10;
    malformed.push(embedded_control);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    let mut reserved_v = valid.clone();
    reserved_v[2] &= !0x08;
    malformed.push(reserved_v);
    let mut reserved_v_prime = valid.clone();
    reserved_v_prime[3] &= !0x08;
    malformed.push(reserved_v_prime);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    for (index, xor) in [(1, 0x03), (2, 0x01), (2, 0x80), (4, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= xor;
        malformed.push(bytes);
    }
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    malformed.push(forbidden_prefix);

    for bytes in malformed {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .and_then(|instruction| instruction.evex_duplicate_move_memory_encoding()),
            None,
            "{bytes:02X?}"
        );
    }
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67] {
        let mut bytes = valid.clone();
        bytes.insert(0, prefix);
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_duplicate_move_memory_encoding()
                .is_some(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_27_census_cells_admit_and_lower_at_o0_o1_o2() {
    let mut lowerings = 0usize;
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{case:?}");
            assert_eq!(exact.encoding.elem, case.kind.elem(), "{case:?}");
            assert_eq!(exact.encoding.high, case.kind.high(), "{case:?}");
            assert_eq!(exact.encoding.destination, case.destination, "{case:?}");
            assert_eq!(exact.encoding.memory_size, case.memory_size(), "{case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{case:?}");
            assert_eq!(
                exact.encoding.register_instruction.as_slice(),
                case.register_instruction(),
                "{case:?}"
            );
            assert_eq!(
                exact.consumed + sequence_index(&function),
                function.blocks[0].ops.len(),
                "{case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{case:?}");

            let (code, _) = lower(&function, case);
            let replay = case.register_instruction();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 27 * LEVELS.len());
}

#[test]
fn apx_r16_r17_sib_rip_addr32_and_segments_remain_helper_owned() {
    let case = DuplicateMemoryCase {
        kind: DuplicateKind::LowF32,
        width: VecWidth::V512,
        destination: 17,
        base: 2,
        control: MaskControl::Zero,
    };
    let mut apx = case.bytes().to_vec();
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    apx[5] = (apx[5] & 0x38) | 0x44;
    apx.extend_from_slice(&[0x48, 0x01]);

    let mut rip = case.bytes().to_vec();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x1020i32.to_le_bytes());
    let mut addr32 = case.bytes().to_vec();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes().to_vec();
    fs.insert(0, 0x64);
    let mut gs_sib = case.bytes().to_vec();
    gs_sib[5] = (gs_sib[5] & 0x38) | 0x44;
    gs_sib.extend_from_slice(&[0x8B, 1]);
    gs_sib.insert(0, 0x65);

    for (name, bytes) in [
        ("APX R16+R17*2+64", apx),
        ("RIP", rip),
        ("addr32", addr32),
        ("FS", fs),
        ("GS SIB", gs_sib),
    ] {
        let base = function_from_bytes(&bytes, name);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.encoding.register_instruction.as_slice(),
                case.register_instruction(),
                "{name} {level:?}"
            );
            lower(&function, case);
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer admitted malformed graph"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_graph_hint_frontier_and_ssa_mutations() {
    let case = DuplicateMemoryCase {
        kind: DuplicateKind::EvenF64,
        width: VecWidth::V512,
        destination: 17,
        base: 2,
        control: MaskControl::Zero,
    };
    let exact = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&exact, true).is_some());

    let mut mutations = Vec::new();
    let mut missing_provenance = exact.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing byte provenance", missing_provenance));

    let mut changed_provenance = exact.clone();
    changed_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&[0x90]).unwrap());
    mutations.push(("changed byte provenance", changed_provenance));

    let start = sequence_index(&exact);
    let mut load_width = exact.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[start].kind {
        *width = VecWidth::V256;
    } else {
        panic!("expected VLoad");
    }
    mutations.push(("load width", load_width));

    let mut load_hint = exact.clone();
    load_hint.blocks[0].ops[start].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    mutations.push(("load hint", load_hint));

    let selector_index = exact.blocks[0].ops[start..]
        .iter()
        .enumerate()
        .find_map(|(offset, op)| {
            (offset >= 3
                && matches!(
                    op.kind,
                    OpKind::Mov {
                        src: SrcOperand::Imm(_),
                        ..
                    }
                ))
            .then_some(start + offset)
        })
        .unwrap();
    let mut selector = exact.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut selector.blocks[0].ops[selector_index].kind
    {
        *value ^= 1;
    }
    mutations.push(("selector", selector));

    let insert_index = exact.blocks[0].ops[start..]
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .map(|offset| start + offset)
        .unwrap();
    let mut insert = exact.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut insert.blocks[0].ops[insert_index].kind {
        *lane ^= 1;
    }
    mutations.push(("selector insert lane", insert));

    let shuffle_index = exact.blocks[0].ops[start..]
        .iter()
        .position(|op| matches!(op.kind, OpKind::VShuffle { .. }))
        .map(|offset| start + offset)
        .unwrap();
    let mut shuffle = exact.clone();
    if let OpKind::VShuffle { src1, .. } = &mut shuffle.blocks[0].ops[shuffle_index].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
    }
    mutations.push(("shuffle source", shuffle));

    let mut guest_pc = exact.clone();
    guest_pc.blocks[0].ops[shuffle_index].guest_pc = PC + 1;
    mutations.push(("split guest PC", guest_pc));

    let loaded = exact.blocks[0].ops[start].kind.dests()[0];
    let mut extra_use = exact.clone();
    let extra_use_id = OpId(extra_use.blocks[0].ops.len() as u16);
    extra_use.blocks[0].ops.push(SmirOp::new(
        extra_use_id,
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0x7FF0)),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("extra source use", extra_use));

    let mut redefined = exact.clone();
    let redefined_id = OpId(redefined.blocks[0].ops.len() as u16);
    redefined.blocks[0].ops.push(SmirOp::new(
        redefined_id,
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("source redefinition", redefined));

    let mut trailing = exact.clone();
    let trailing_id = OpId(trailing.blocks[0].ops.len() as u16);
    trailing.blocks[0].ops.push(SmirOp::new(
        trailing_id,
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0x7FF1)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC trailing operation", trailing));

    for (name, function) in mutations {
        assert_rejected(name, &function);
    }
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let case = DuplicateMemoryCase {
        kind: DuplicateKind::LowF32,
        width: VecWidth::V128,
        destination: 1,
        base: 2,
        control: MaskControl::Merge,
    };
    let function = lift_case(case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer.lower_function(&function).unwrap_err();
    assert!(format!("{error:?}").contains("AVX-only vector bridge"));
}

#[test]
fn compressed_disp8_uses_exact_e4nf_and_e5nf_tuple_scaling() {
    let mut checked = 0usize;
    for kind in DuplicateKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            let case = DuplicateMemoryCase {
                kind,
                width,
                destination: 17,
                base: 2,
                control: MaskControl::Zero,
            };
            let mut bytes = case.bytes().to_vec();
            bytes[5] |= 0x40;
            bytes.push(1);
            let function = function_from_bytes(&bytes, case);
            let index = sequence_index(&function);
            let address = match &function.blocks[0].ops[index].kind {
                OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => addr,
                other => panic!("{case:?}: {other:?}"),
            };
            assert!(
                matches!(address, Address::BaseOffset { offset, .. } if *offset == i64::from(case.memory_size())),
                "{case:?}: {address:?}"
            );
            for level in LEVELS {
                let optimized = optimize(function.clone(), level);
                let exact = sequence(&optimized, true).unwrap_or_else(|| panic!("{case:?}"));
                assert_eq!(exact.encoding.memory_size, case.memory_size());
                lower(&optimized, case);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 3 * LEVELS.len());
}
