//! Byte classification, graph provenance, and address-frontier coverage.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{Address, OpId, VirtualId};

#[test]
fn llvm_23_memory_and_stack_replay_anchors_match_exactly() {
    // Independently produced by llvm-mc 23.0.0git. The memory operands use
    // compressed disp8 values at the maximum positive or negative Tuple1
    // Scalar scale-8 displacement; the replay operands use [RSP].
    let anchors: &[(HalfMoveCase, &[u8], &[u8])] = &[
        (
            HalfMoveCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Ps,
                destination: 1,
                source1: 2,
            },
            &[0x62, 0xD1, 0x6C, 0x08, 0x12, 0x4B, 0x7F],
            &[0x62, 0xF1, 0x6C, 0x08, 0x12, 0x0C, 0x24],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Pd,
                destination: 15,
                source1: 14,
            },
            &[0x62, 0x51, 0x8D, 0x08, 0x12, 0x7C, 0x24, 0x7F],
            &[0x62, 0x71, 0x8D, 0x08, 0x12, 0x3C, 0x24],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::High,
                format: MoveFormat::Ps,
                destination: 9,
                source1: 10,
            },
            &[0x62, 0x51, 0x2C, 0x08, 0x16, 0x4D, 0x80],
            &[0x62, 0x71, 0x2C, 0x08, 0x16, 0x0C, 0x24],
        ),
        (
            HalfMoveCase {
                lane: MemoryLane::High,
                format: MoveFormat::Pd,
                destination: 0,
                source1: 15,
            },
            &[0x62, 0xD1, 0x85, 0x08, 0x16, 0x46, 0x80],
            &[0x62, 0xF1, 0x85, 0x08, 0x16, 0x04, 0x24],
        ),
    ];
    for &(case, memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_half_move_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding, case.expected_encoding());
        assert_eq!(encoding.stack_instruction.as_slice(), replay);
        for level in [OptLevel::O0, OptLevel::O2] {
            lower(&optimize(function_from_bytes(memory), level), case);
        }
    }
}

fn sib_encoding(case: HalfMoveCase, base_high: bool, index_high: bool) -> Vec<u8> {
    let mut bytes = case.bytes().to_vec();
    bytes[5] = ((case.destination & 7) << 3) | 4;
    bytes.push(0x48); // [RAX + RCX*2], extended by APX B4/X4 when selected.
    bytes[1] |= u8::from(base_high) << 3;
    if index_high {
        bytes[2] &= !0x04;
    }
    bytes
}

#[test]
fn classifier_crosses_all_16384_operand_format_lane_and_apx_address_cells() {
    let mut cells = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    let case = HalfMoveCase {
                        lane,
                        format,
                        destination,
                        source1,
                    };
                    for base_high in [false, true] {
                        for index_high in [false, true] {
                            let bytes = sib_encoding(case, base_high, index_high);
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_half_move_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding, case.expected_encoding(), "{bytes:02X?}");
                            assert_eq!(encoding.stack_instruction.as_slice()[1] & 0x08, 0);
                            assert_eq!(encoding.stack_instruction.as_slice()[2] & 0x04, 0x04);
                            cells += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cells, 2 * 2 * 32 * 32 * 2 * 2);
}

#[test]
fn classifier_rejects_reserved_nonowned_register_truncated_and_trailing_images() {
    let case = HalfMoveCase {
        lane: MemoryLane::High,
        format: MoveFormat::Pd,
        destination: 31,
        source1: 30,
    };
    let valid = case.bytes().to_vec();
    let mut malformed = Vec::new();
    for (index, mask) in [
        (1, 0x02), // map 0F -> non-owned map
        (2, 0x80), // W1 -> W0 while pp remains 66
        (2, 0x01), // 66 -> NP while W remains W1
        (3, 0x80), // reserved z
        (3, 0x20), // reserved L'L
        (3, 0x10), // reserved b
        (3, 0x01), // reserved aaa
        (4, 0x01), // non-owned opcode 17H
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    malformed.push(forbidden_prefix);
    for end in 1..valid.len() {
        malformed.push(valid[..end].to_vec());
    }
    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .and_then(|instruction| instruction.evex_half_move_memory_encoding())
                .is_none(),
            "{bytes:02X?}"
        );
    }
    for opcode in [0x10, 0x11, 0x13, 0x14, 0x15, 0x17] {
        let mut bytes = valid.clone();
        bytes[4] = opcode;
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_half_move_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_exhausts_all_4194304_selector_mask_control_and_length_cells() {
    let mut cells = 0usize;
    let mut accepted = 0usize;
    for map in 0u8..=7 {
        for opcode in u8::MIN..=u8::MAX {
            for pp in 0u8..=3 {
                for w in [false, true] {
                    for ll in 0u8..=3 {
                        for embedded_control in [false, true] {
                            for zeroing in [false, true] {
                                for mask in 0u8..=7 {
                                    for trailing in [false, true] {
                                        let mut bytes = vec![
                                            0x62,
                                            0xF0 | map,
                                            (u8::from(w) << 7) | 0x7C | pp,
                                            (u8::from(zeroing) << 7)
                                                | (ll << 5)
                                                | (u8::from(embedded_control) << 4)
                                                | 0x08
                                                | mask,
                                            opcode,
                                            0x02,
                                        ];
                                        if trailing {
                                            bytes.push(0xA5);
                                        }
                                        let expected = map == 1
                                            && matches!(opcode, 0x12 | 0x16)
                                            && matches!((pp, w), (0, false) | (1, true))
                                            && ll == 0
                                            && !embedded_control
                                            && !zeroing
                                            && mask == 0
                                            && !trailing;
                                        let actual = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_half_move_memory_encoding();
                                        assert_eq!(actual.is_some(), expected, "{bytes:02X?}");
                                        accepted += usize::from(actual.is_some());
                                        cells += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cells, 8 * 256 * 4 * 2 * 4 * 2 * 2 * 8 * 2);
    assert_eq!(accepted, 4);
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function, true).is_none(), "{name}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}"
    );
    let mut lowerer = configured_lowerer(false);
    assert!(lowerer.lower_function(function).is_err(), "{name}");
}

#[test]
fn matcher_rejects_every_semantic_stage_provenance_and_virtual_escape_mutation() {
    let case = HalfMoveCase {
        lane: MemoryLane::Low,
        format: MoveFormat::Pd,
        destination: 17,
        source1: 18,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&base, true).is_some());
    let preserved = match base.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let loaded = match base.blocks[0].ops[1].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let zero = match base.blocks[0].ops[2].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut wrong_extract_source = base.clone();
    let OpKind::VExtractLane { vec, .. } = &mut wrong_extract_source.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *vec = xmm(19);
    malformed.push(("extract source", wrong_extract_source));

    let mut wrong_extract_lane = base.clone();
    let OpKind::VExtractLane { lane, .. } = &mut wrong_extract_lane.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *lane = 0;
    malformed.push(("extract lane", wrong_extract_lane));

    let mut wrong_width = base.clone();
    let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    malformed.push(("load width", wrong_width));

    let mut wrong_address = base.clone();
    let OpKind::Load { addr, .. } = &mut wrong_address.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *addr = Address::GpRel { offset: 1 };
    malformed.push(("address shape", wrong_address));

    let mut wrong_zero = base.clone();
    let OpKind::Mov { src, .. } = &mut wrong_zero.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *src = SrcOperand::Imm(1);
    malformed.push(("zero seed", wrong_zero));

    let mut wrong_clear = base.clone();
    let OpKind::VBroadcast { dst, .. } = &mut wrong_clear.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *dst = xmm(19);
    malformed.push(("clear destination", wrong_clear));

    let mut wrong_preserved = base.clone();
    let OpKind::VInsertLane { scalar, .. } = &mut wrong_preserved.blocks[0].ops[4].kind else {
        unreachable!()
    };
    *scalar = loaded;
    malformed.push(("preserved insertion", wrong_preserved));

    let mut wrong_memory = base.clone();
    let OpKind::VInsertLane { lane, .. } = &mut wrong_memory.blocks[0].ops[5].kind else {
        unreachable!()
    };
    *lane = 1;
    malformed.push(("memory insertion", wrong_memory));

    for index in 0..6 {
        let mut hinted = base.clone();
        hinted.blocks[0].ops[index].x86_hint = Some(crate::smir::ir::ops::X86OpHint::VecAlign(
            crate::smir::ir::ops::X86VecAlign::Unaligned,
        ));
        malformed.push(("invented hint", hinted));
    }
    for index in 1..6 {
        let mut split = base.clone();
        split.blocks[0].ops[index].guest_pc += 1;
        malformed.push(("split guest PC", split));
    }
    for (name, register) in [
        ("preserved escape", preserved),
        ("load escape", loaded),
        ("zero escape", zero),
    ] {
        let mut escaped = base.clone();
        escaped.blocks[0].ops.push(SmirOp::new(
            OpId(0x7FF0),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFF00)),
                src: SrcOperand::Reg(register),
                width: OpWidth::W64,
            },
        ));
        malformed.push((name, escaped));
    }
    let mut trailing_same_pc = base.clone();
    trailing_same_pc.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFE), PC, OpKind::Nop));
    malformed.push(("same-PC tail", trailing_same_pc));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut wrong_provenance = base.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(
            &HalfMoveCase {
                destination: 19,
                ..case
            }
            .bytes(),
        )
        .unwrap(),
    );
    assert_rejected("source-byte destination", &wrong_provenance);

    let mut missing_provenance = base;
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing source bytes", &missing_provenance);
}

#[test]
fn segment_addr32_rip_sib_and_apx_addresses_admit_and_lower_exactly() {
    let case = HalfMoveCase {
        lane: MemoryLane::High,
        format: MoveFormat::Ps,
        destination: 31,
        source1: 30,
    };
    let mut fs = case.bytes().to_vec();
    fs.insert(0, 0x64);
    let mut addr32 = case.bytes().to_vec();
    addr32.insert(0, 0x67);
    let mut rip = case.bytes().to_vec();
    rip[5] = ((case.destination & 7) << 3) | 5;
    rip.extend_from_slice(&0x1234i32.to_le_bytes());
    let sib = sib_encoding(case, false, false);
    for (name, bytes) in [("FS", fs), ("addr32", addr32), ("RIP", rip), ("SIB", sib)] {
        let base = function_from_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let apx = sib_encoding(case, true, true);
    let base = function_from_bytes(&apx);
    assert!(matches!(base.blocks[0].ops[0].kind, OpKind::X86RequireApx));
    let mut missing_guard = base.clone();
    missing_guard.blocks[0].ops.remove(0);
    assert!(sequence(&missing_guard, true).is_none());
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        sequence(&function, true)
            .unwrap_or_else(|| panic!("APX {level:?}: {:#?}", function.blocks[0].ops));
        lower(&function, case);
    }
}
