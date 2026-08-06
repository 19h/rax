use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, MemWidth, OpId, OpWidth, SrcOperand, VirtualId, X86Reg,
};

#[test]
fn packed_move_classifier_exhausts_80_640_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for spec in SPECS {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for direction in Direction::ALL {
                for vector in 0..32u8 {
                    for mask in 1..8u8 {
                        for control in [MaskControl::Merge, MaskControl::Zero] {
                            if !control.valid_for(direction) {
                                continue;
                            }
                            let case = PackedMoveMemoryCase {
                                spec,
                                direction,
                                width,
                                vector,
                                base: 3,
                                mask,
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
                                        .evex_packed_move_memory_encoding()
                                        .unwrap_or_else(|| panic!("{case:?} {bytes:02X?}"));
                                    assert_eq!(encoding.kind, direction.kind(), "{bytes:02X?}");
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, spec.elem, "{bytes:02X?}");
                                    assert_eq!(encoding.vector, vector, "{bytes:02X?}");
                                    assert_eq!(encoding.writemask, mask, "{bytes:02X?}");
                                    assert_eq!(encoding.zeroing, control.zeroing(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.alignment,
                                        spec.aligned.then_some(width.bytes() as u8),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512vl,
                                        width != VecWidth::V512,
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512bw, spec.needs_avx512bw,
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.stack_instruction.as_slice(),
                                        case.stack_instruction(),
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
    assert_eq!(accepted, 10 * 3 * 32 * 7 * 3 * 4);
}

#[test]
fn packed_move_classifier_exhausts_map1_opcode_pp_w_and_vector_length_space() {
    let mut expected = Vec::new();
    for spec in SPECS {
        for direction in Direction::ALL {
            for ll in 0..=2u8 {
                let shape = (
                    spec.opcode(direction),
                    spec.pp,
                    spec.w,
                    ll,
                    direction.kind(),
                    spec.elem,
                    spec.aligned,
                );
                assert!(!expected.contains(&shape));
                expected.push(shape);
            }
        }
    }
    assert_eq!(expected.len(), 60);

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
                        .evex_packed_move_memory_encoding();
                    let matching: Vec<_> = expected
                        .iter()
                        .filter(
                            |(expected_opcode, expected_pp, expected_w, expected_ll, ..)| {
                                (*expected_opcode, *expected_pp, *expected_w, *expected_ll)
                                    == (opcode, pp, w, ll)
                            },
                        )
                        .collect();
                    assert!(matching.len() <= 1, "ambiguous selector {bytes:02X?}");
                    match matching.first() {
                        Some((_, _, _, _, kind, elem, aligned)) => {
                            let actual = actual.unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(actual.kind, *kind, "{bytes:02X?}");
                            assert_eq!(actual.elem, *elem, "{bytes:02X?}");
                            assert_eq!(actual.alignment.is_some(), *aligned, "{bytes:02X?}");
                            classified += 1;
                        }
                        None => assert_eq!(actual, None, "{bytes:02X?}"),
                    }
                }
            }
        }
    }
    assert_eq!(classified, 60);
}

#[test]
fn stack_rewrites_match_independently_assembled_llvm_23_anchors() {
    // Source and expected stack images were assembled independently with
    // llvm-mc 23.0.0git. Aligned guest forms intentionally map to their
    // unaligned stack analogues after the separate architectural guard.
    for (source, expected_stack) in [
        (
            &[0x62, 0xE1, 0x7C, 0xCB, 0x10, 0x0A][..],
            &[0x62, 0xE1, 0x7C, 0xCB, 0x10, 0x0C, 0x24][..],
        ),
        (
            &[0x62, 0x71, 0x7C, 0x09, 0x28, 0x0A],
            &[0x62, 0x71, 0x7C, 0x09, 0x10, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x61, 0x7D, 0xAD, 0x6F, 0x0A],
            &[0x62, 0x61, 0x7E, 0xAD, 0x6F, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x71, 0x7F, 0x89, 0x6F, 0x0A],
            &[0x62, 0x71, 0x7F, 0x89, 0x6F, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x61, 0xFD, 0x2D, 0x11, 0x0A],
            &[0x62, 0x61, 0xFD, 0x28, 0x11, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xE1, 0xFD, 0x4B, 0x7F, 0x0A],
            &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x0C, 0x24],
        ),
        (
            &[0x62, 0x61, 0xFF, 0x2D, 0x7F, 0x0A],
            &[0x62, 0x61, 0xFF, 0x28, 0x7F, 0x0C, 0x24],
        ),
    ] {
        let encoding = X86InstructionBytes::new(source)
            .unwrap()
            .evex_packed_move_memory_encoding()
            .unwrap_or_else(|| panic!("{source:02X?}"));
        assert_eq!(encoding.stack_instruction.as_slice(), expected_stack);
    }
}

#[test]
fn classifier_rejects_unmasked_reserved_non_owned_and_trailing_shapes() {
    let case = PackedMoveMemoryCase {
        spec: SPECS[0],
        direction: Direction::Load,
        width: VecWidth::V256,
        vector: 17,
        base: 2,
        mask: 3,
        control: MaskControl::Merge,
    };
    let valid = case.bytes().to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut unmasked = valid.clone();
    unmasked[3] &= !7;
    malformed.push(unmasked);
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
    let mut store_zero = valid.clone();
    store_zero[3] |= 0x80;
    store_zero[4] = case.spec.store_opcode;
    malformed.push(store_zero);
    for (index, xor) in [(1, 0x03), (2, 0x01), (2, 0x80), (4, 0x02)] {
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
                .and_then(|instruction| instruction.evex_packed_move_memory_encoding()),
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
                .evex_packed_move_memory_encoding()
                .is_some(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_90_census_cells_admit_and_lower_at_o0_o1_o2() {
    let mut lowerings = 0usize;
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.kind, case.direction.kind(), "{case:?}");
            assert_eq!(exact.encoding.width, case.width, "{case:?}");
            assert_eq!(exact.encoding.elem, case.spec.elem, "{case:?}");
            assert_eq!(exact.encoding.vector, case.vector, "{case:?}");
            assert_eq!(exact.encoding.writemask, case.mask, "{case:?}");
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{case:?}");
            assert_eq!(
                exact.encoding.alignment,
                case.spec.aligned.then_some(case.width.bytes() as u8),
                "{case:?}"
            );
            assert_eq!(exact.memory_size, case.width.bytes(), "{case:?}");
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{case:?}"
            );
            assert_eq!(
                exact.consumed + sequence_index(&function),
                function.blocks[0].ops.len(),
                "{case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{case:?}");
            assert!(matches!(
                function.blocks[0].ops[sequence_index(&function) + exact.address_offset].kind,
                OpKind::Lea { .. }
            ));

            let (code, _) = lower(&function, case);
            let replay = case.stack_instruction();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {case:?}: missing {replay:02X?} in {} bytes",
                code.len()
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0x48, 0x8D, 0x64, 0x24, 0xB0]),
                "{level:?} {case:?}: missing 80-byte frame"
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 90 * LEVELS.len());
}

#[test]
fn apx_r16_r17_sib_rip_addr32_and_segments_remain_helper_owned() {
    let case = PackedMoveMemoryCase {
        spec: SPECS[2],
        direction: Direction::Load,
        width: VecWidth::V512,
        vector: 17,
        base: 2,
        mask: 3,
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
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{name} {level:?}"
            );
            assert!(
                exact
                    .encoding
                    .stack_instruction
                    .as_slice()
                    .windows(2)
                    .any(|window| window[0] & 7 == 4 && window[1] == 0x24),
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
    assert!(
        !x86_native_replay_feature_requirements(function, &HashMap::new()).any,
        "{name}: feature gate admitted malformed graph"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_alignment_graph_hint_and_ssa_mutations() {
    let aligned_load = PackedMoveMemoryCase {
        spec: SPECS[4],
        direction: Direction::Load,
        width: VecWidth::V256,
        vector: 17,
        base: 2,
        mask: 3,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(aligned_load), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    let mut wrong_direction = function.clone();
    let mut bytes = aligned_load.bytes();
    bytes[4] = aligned_load.spec.store_opcode;
    bytes[3] &= !0x80;
    wrong_direction
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert_rejected("wrong direction provenance", &wrong_direction);

    let mut wrong_alignment = function.clone();
    let alignment = wrong_alignment.blocks[0]
        .ops
        .iter_mut()
        .find_map(|op| match &mut op.kind {
            OpKind::X86CheckAlignment { alignment, .. } => Some(alignment),
            _ => None,
        })
        .unwrap();
    *alignment = 16;
    assert_rejected("wrong alignment", &wrong_alignment);

    let mut wrong_guard_address = function.clone();
    let guard = wrong_guard_address.blocks[0]
        .ops
        .iter_mut()
        .find_map(|op| match &mut op.kind {
            OpKind::X86CheckAlignment { addr, .. } => Some(addr),
            _ => None,
        })
        .unwrap();
    *guard = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)));
    assert_rejected("wrong guard address", &wrong_guard_address);

    let mut wrong_width = function.clone();
    let width = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find_map(|op| match &mut op.kind {
            OpKind::PredLoad { width, .. } => Some(width),
            _ => None,
        })
        .unwrap();
    *width = MemWidth::B8;
    assert_rejected("wrong lane width", &wrong_width);

    let mut wrong_predicate = function.clone();
    let predicate = wrong_predicate.blocks[0]
        .ops
        .iter_mut()
        .find_map(|op| match &mut op.kind {
            OpKind::And { src2, .. } => Some(src2),
            _ => None,
        })
        .unwrap();
    *predicate = SrcOperand::Imm(2);
    assert_rejected("wrong K lane", &wrong_predicate);

    let mut wrong_hint = function.clone();
    wrong_hint.blocks[0].ops.last_mut().unwrap().x86_hint = Some(X86OpHint::MovImmModRm);
    assert_rejected("wrong semantic hint", &wrong_hint);

    let mut wrong_pc = function.clone();
    wrong_pc.blocks[0].ops.last_mut().unwrap().guest_pc += 1;
    assert_rejected("wrong semantic PC", &wrong_pc);

    let temporary = function.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .find(|register| matches!(register, VReg::Virtual(_)))
        .unwrap();
    let mut extra_use = function.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFE),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: SrcOperand::Reg(temporary),
            width: OpWidth::W64,
        },
    ));
    assert_rejected("extra SSA use", &extra_use);

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

    let store = PackedMoveMemoryCase {
        spec: SPECS[7],
        direction: Direction::Store,
        width: VecWidth::V512,
        vector: 25,
        base: 2,
        mask: 5,
        control: MaskControl::Merge,
    };
    let store_function = optimize(lift_case(store), OptLevel::O2);
    let mut wrong_vector = store_function.clone();
    let source = wrong_vector.blocks[0]
        .ops
        .iter_mut()
        .find_map(|op| match &mut op.kind {
            OpKind::VExtractLane { vec, .. } => Some(vec),
            _ => None,
        })
        .unwrap();
    *source = VReg::Arch(ArchReg::X86(X86Reg::Zmm(24)));
    assert_rejected("wrong store vector", &wrong_vector);
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let case = PackedMoveMemoryCase {
        spec: SPECS[9],
        direction: Direction::Load,
        width: VecWidth::V512,
        vector: 25,
        base: 2,
        mask: 5,
        control: MaskControl::Zero,
    };
    let function = lift_case(case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(lowerer.lower_function(&function).is_err());
}

#[test]
fn address_shapes_use_exact_tuple_scaling_and_alignment_address_identity() {
    let case = PackedMoveMemoryCase {
        spec: SPECS[5],
        direction: Direction::Store,
        width: VecWidth::V512,
        vector: 17,
        base: 2,
        mask: 3,
        control: MaskControl::Merge,
    };
    let mut bytes = case.bytes().to_vec();
    bytes[5] = (bytes[5] & 0x38) | 0x42;
    bytes.push(1);
    let function = optimize(
        function_from_bytes(&bytes, "disp8 tuple scaling"),
        OptLevel::O2,
    );
    let exact = sequence(&function, true).expect("disp8 aligned packed-move sequence");
    let OpKind::Lea { addr, .. } =
        &function.blocks[0].ops[sequence_index(&function) + exact.address_offset].kind
    else {
        unreachable!()
    };
    assert!(matches!(
        addr,
        Address::BaseOffset {
            offset: 64,
            disp_size: DispSize::Disp8,
            ..
        }
    ));
    let guard = function.blocks[0]
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::X86CheckAlignment { addr, .. } => Some(addr),
            _ => None,
        })
        .unwrap();
    assert_eq!(guard, addr);
}
