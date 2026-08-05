use super::*;
use crate::smir::ir::X86EvexScalarMoveMemoryKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, OpId, OpWidth, SrcOperand, VirtualId, X86Reg,
};

#[test]
fn scalar_move_classifier_exhaustively_rewrites_26_496_control_register_and_apx_cells() {
    let mut accepted = 0usize;
    for format in ScalarFormat::ALL {
        for direction in Direction::ALL {
            for ll in 0..=2u8 {
                for vector in 0..32u8 {
                    for mask in 0..8u8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 || direction == Direction::Store && zeroing {
                                continue;
                            }
                            let canonical =
                                memory_encoding(format, direction, vector, ll, mask, zeroing, 11);
                            for base_high in [false, true] {
                                for index_high in [false, true] {
                                    let mut bytes = canonical;
                                    bytes[1] |= u8::from(base_high) << 3;
                                    if index_high {
                                        bytes[2] &= !0x04;
                                    }
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_scalar_move_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                    assert_eq!(encoding.vector, vector, "{bytes:02X?}");
                                    assert_eq!(encoding.writemask, (mask != 0).then_some(mask));
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(encoding.map, format.map(), "{bytes:02X?}");
                                    assert_eq!(encoding.pp, format.pp(), "{bytes:02X?}");
                                    assert_eq!(encoding.w, format.w(), "{bytes:02X?}");
                                    assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                    assert_eq!(encoding.opcode, direction.opcode(), "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.memory_width,
                                        format.memory_width(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.kind,
                                        match direction {
                                            Direction::Load => X86EvexScalarMoveMemoryKind::Load,
                                            Direction::Store => X86EvexScalarMoveMemoryKind::Store,
                                        },
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
                                            format, direction, vector, ll, mask, zeroing
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
    assert_eq!(accepted, 3 * 3 * 32 * (15 + 8) * 2 * 2);
}

#[test]
fn scalar_move_stack_encodings_match_six_independent_llvm_23_anchors() {
    // Generated independently with llvm-mc 23.0.0git in Intel syntax.
    for (source, expected_stack) in [
        (
            [0x62, 0xE1, 0x7E, 0x8B, 0x10, 0x0A],
            [0x62, 0xE1, 0x7E, 0x8B, 0x10, 0x0C, 0x24],
        ), // vmovss xmm17{k3}{z}, dword ptr [rdx]
        (
            [0x62, 0x61, 0xFF, 0x0D, 0x10, 0x0A],
            [0x62, 0x61, 0xFF, 0x0D, 0x10, 0x0C, 0x24],
        ), // vmovsd xmm25{k5}, qword ptr [rdx]
        (
            [0x62, 0x75, 0x7E, 0x89, 0x10, 0x0A],
            [0x62, 0x75, 0x7E, 0x89, 0x10, 0x0C, 0x24],
        ), // vmovsh xmm9{k1}{z}, word ptr [rdx]
        (
            [0x62, 0xE1, 0x7E, 0x0B, 0x11, 0x0A],
            [0x62, 0xE1, 0x7E, 0x0B, 0x11, 0x0C, 0x24],
        ), // vmovss dword ptr [rdx]{k3}, xmm17
        (
            [0x62, 0x61, 0xFF, 0x0D, 0x11, 0x0A],
            [0x62, 0x61, 0xFF, 0x0D, 0x11, 0x0C, 0x24],
        ), // vmovsd qword ptr [rdx]{k5}, xmm25
        (
            [0x62, 0x75, 0x7E, 0x09, 0x11, 0x0A],
            [0x62, 0x75, 0x7E, 0x09, 0x11, 0x0C, 0x24],
        ), // vmovsh word ptr [rdx]{k1}, xmm9
    ] {
        let actual = X86InstructionBytes::new(&source)
            .unwrap()
            .evex_scalar_move_memory_encoding()
            .unwrap();
        assert_eq!(actual.stack_instruction.as_slice(), expected_stack);
    }
}

#[test]
fn scalar_move_classifier_rejects_every_reserved_non_owned_and_trailing_shape() {
    let valid = memory_encoding(ScalarFormat::F32, Direction::Load, 17, 1, 3, false, 2).to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
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
    let mut store_zero = valid.clone();
    store_zero[3] |= 0x80;
    store_zero[4] = 0x11;
    malformed.push(store_zero);
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !7) | 2;
    malformed.push(wrong_map);
    let mut wrong_pp = valid.clone();
    wrong_pp[2] = (wrong_pp[2] & !3) | 1;
    malformed.push(wrong_pp);
    let mut wrong_w = valid.clone();
    wrong_w[2] |= 0x80;
    malformed.push(wrong_w);
    let mut wrong_opcode = valid.clone();
    wrong_opcode[4] = 0x12;
    malformed.push(wrong_opcode);
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    malformed.push(forbidden_prefix);

    for bytes in malformed {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .and_then(|instruction| instruction.evex_scalar_move_memory_encoding()),
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
                .evex_scalar_move_memory_encoding()
                .is_some(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_45_census_cells_admit_and_lower_at_o0_o1_o2_with_exact_features_and_bytes() {
    let cases = all_cases();
    assert_eq!(cases.len(), 45);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.consumed, function.blocks[0].ops.len(), "{case:?}");
            assert_eq!(exact.encoding.elem, case.format.elem(), "{case:?}");
            assert_eq!(exact.encoding.vector, case.vector, "{case:?}");
            assert_eq!(exact.encoding.ll, case.ll, "{case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{case:?}"
            );
            let (definitions, uses) = virtual_counts(&function);
            assert!(
                x86_jit_evex_scalar_move_memory_sequence(
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

            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.stack_instruction().len())
                    .any(|window| window == case.stack_instruction()),
                "{level:?} {case:?}: exact stack replay missing"
            );
            let size_register = match case.direction {
                Direction::Load => 0xBA,
                Direction::Store => 0xB9,
            };
            let helper_size = [size_register, case.format.memory_size() as u8, 0, 0, 0];
            assert!(
                code.windows(helper_size.len())
                    .any(|window| window == helper_size),
                "{level:?} {case:?}: exact helper width missing"
            );
            if case.mask() != 0 {
                let kmovq = [0xC4, 0xE1, 0xFB, 0x93, 0xC0 | case.mask()];
                assert!(
                    code.windows(kmovq.len()).any(|window| window == kmovq),
                    "{level:?} {case:?}: live K[0] guard missing"
                );
            }
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 45 * LEVELS.len());
}

#[test]
fn scalar_move_apx_r16_r17_sib_address_is_guarded_and_rewritten_to_plain_rsp() {
    // VMOVSS xmm17{k3}, dword ptr [r16+r17*2+4]. Tuple1 Scalar compresses
    // disp8=1 by 4 bytes. B4/X4 must remain helper-only.
    let bytes = [0x62, 0xE9, 0x7A, 0x0B, 0x10, 0x4C, 0x48, 0x01];
    let base = function_from_bytes(&bytes, "APX scalar move address");
    let expected_stack = [0x62, 0xE1, 0x7E, 0x0B, 0x10, 0x0C, 0x24];
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
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
        )));
        let exact = sequence(&function).expect("APX scalar move sequence");
        assert_eq!(exact.encoding.stack_instruction.as_slice(), expected_stack);
        let case = ScalarMoveCase {
            format: ScalarFormat::F32,
            direction: Direction::Load,
            vector: 17,
            ll: 0,
            control: MaskControl::Merge,
        };
        let (code, _) = lower(&function, case);
        assert!(code.windows(7).any(|window| window == expected_stack));
    }
}

#[test]
fn scalar_move_rip_addr32_segment_and_sib_addresses_remain_helper_owned() {
    let case = ScalarMoveCase {
        format: ScalarFormat::F64,
        direction: Direction::Store,
        vector: 25,
        ll: 2,
        control: MaskControl::Merge,
    };
    let mut rip = case.bytes().to_vec();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = case.bytes().to_vec();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes().to_vec();
    fs.insert(0, 0x64);
    let mut gs_sib = case.bytes().to_vec();
    gs_sib[5] = (gs_sib[5] & 0x38) | 0x44;
    gs_sib.push(0x8B);
    gs_sib.push(2); // Tuple1 Scalar disp8=2 scales to 16 bytes.
    gs_sib.insert(0, 0x65);

    for (name, bytes) in [
        ("RIP", rip),
        ("addr32", addr32),
        ("FS", fs),
        ("GS SIB", gs_sib),
    ] {
        let base = function_from_bytes(&bytes, name);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            let exact = sequence(&function)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{name} {level:?}"
            );
            lower(&function, case);
        }
    }
}

#[test]
fn masked_store_guard_skips_native_staging_and_helper_before_flag_neutral_stack_cleanup() {
    for format in ScalarFormat::ALL {
        let case = ScalarMoveCase {
            format,
            direction: Direction::Store,
            vector: 17,
            ll: 2,
            control: MaskControl::Merge,
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
        let displacement_at = matches[0] + guard.len();
        let active_at = displacement_at + 4;
        assert_eq!(&code[active_at..active_at + 2], &[0x58, 0x9D]);
        assert_eq!(
            &code[active_at + 2..active_at + 2 + case.stack_instruction().len()],
            case.stack_instruction(),
            "{case:?}: active path must stage the scalar before the helper"
        );

        let inactive = active_at as i64
            + i64::from(i32::from_le_bytes(
                code[displacement_at..active_at].try_into().unwrap(),
            ));
        let inactive = usize::try_from(inactive).expect("forward inactive target");
        assert_eq!(&code[inactive..inactive + 2], &[0x58, 0x9D]);
        assert_eq!(code[inactive - 5], 0xE9, "{case:?}: active cleanup jump");
        let cleanup = inactive as i64
            + i64::from(i32::from_le_bytes(
                code[inactive - 4..inactive].try_into().unwrap(),
            ));
        assert_eq!(usize::try_from(cleanup).unwrap(), inactive + 2);
        assert_eq!(
            &code[inactive + 2..inactive + 7],
            &[0x48, 0x8D, 0x64, 0x24, 0x10],
            "{case:?}: inactive path must reach LEA rsp,[rsp+16] directly"
        );
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function).is_none(),
        "{name}: exact matcher admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
}

#[test]
fn scalar_move_sequence_fails_closed_for_provenance_graph_hint_and_ssa_mutations() {
    for case in [
        ScalarMoveCase {
            format: ScalarFormat::F16,
            direction: Direction::Load,
            vector: 17,
            ll: 0,
            control: MaskControl::Merge,
        },
        ScalarMoveCase {
            format: ScalarFormat::F32,
            direction: Direction::Load,
            vector: 25,
            ll: 1,
            control: MaskControl::Merge,
        },
        ScalarMoveCase {
            format: ScalarFormat::F64,
            direction: Direction::Store,
            vector: 17,
            ll: 2,
            control: MaskControl::Merge,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function).is_some(), "{case:?}");

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_direction = function.clone();
        let mut bytes = case.bytes();
        bytes[4] = if bytes[4] == 0x10 { 0x11 } else { 0x10 };
        wrong_direction
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong direction provenance", &wrong_direction);

        let mut wrong_width = function.clone();
        let memory = wrong_width.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::Load { .. }
                        | OpKind::PredLoad { .. }
                        | OpKind::Store { .. }
                        | OpKind::PredStore { .. }
                )
            })
            .unwrap();
        match &mut memory.kind {
            OpKind::Load { width, .. }
            | OpKind::PredLoad { width, .. }
            | OpKind::Store { width, .. }
            | OpKind::PredStore { width, .. } => *width = MemWidth::B16,
            _ => unreachable!(),
        }
        assert_rejected("wrong memory width", &wrong_width);

        let mut wrong_hint = function.clone();
        let terminal = wrong_hint.blocks[0].ops.last_mut().unwrap();
        terminal.x86_hint = if case.format == ScalarFormat::F16 {
            Some(X86OpHint::MovImmModRm)
        } else {
            None
        };
        assert_rejected("wrong terminal hint", &wrong_hint);

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
        assert_rejected("wrong K bit", &wrong_condition);

        let mut wrong_vector = function.clone();
        let extract = wrong_vector.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .unwrap();
        let OpKind::VExtractLane { vec, .. } = &mut extract.kind else {
            unreachable!()
        };
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(case.vector ^ 1)));
        assert_rejected("wrong vector", &wrong_vector);

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
fn scalar_move_lowerer_rejects_the_avx_only_vector_bridge() {
    let case = ScalarMoveCase {
        format: ScalarFormat::F64,
        direction: Direction::Load,
        vector: 25,
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
