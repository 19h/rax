//! Byte classification, graph provenance, and APX address-frontier coverage.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{Address, OpId, VecWidth, VirtualId};

#[test]
fn llvm_23_store_and_stack_replay_anchors_match_exactly() {
    // Independently produced by llvm-mc 23.0.0git. The memory operands use
    // compressed disp8 values at the maximum positive or negative scale-8
    // displacement; the replay operands use [RSP].
    let anchors: &[(HalfMoveStoreCase, &[u8], &[u8])] = &[
        (
            HalfMoveStoreCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Ps,
                source: 17,
            },
            &[0x62, 0xC1, 0x7C, 0x08, 0x13, 0x4B, 0x7F],
            &[0x62, 0xE1, 0x7C, 0x08, 0x13, 0x0C, 0x24],
        ),
        (
            HalfMoveStoreCase {
                lane: MemoryLane::Low,
                format: MoveFormat::Pd,
                source: 31,
            },
            &[0x62, 0x61, 0xFD, 0x08, 0x13, 0x7C, 0x24, 0x7F],
            &[0x62, 0x61, 0xFD, 0x08, 0x13, 0x3C, 0x24],
        ),
        (
            HalfMoveStoreCase {
                lane: MemoryLane::High,
                format: MoveFormat::Ps,
                source: 25,
            },
            &[0x62, 0x41, 0x7C, 0x08, 0x17, 0x4D, 0x80],
            &[0x62, 0x61, 0x7C, 0x08, 0x17, 0x0C, 0x24],
        ),
        (
            HalfMoveStoreCase {
                lane: MemoryLane::High,
                format: MoveFormat::Pd,
                source: 16,
            },
            &[0x62, 0xE1, 0xFD, 0x08, 0x17, 0x46, 0x80],
            &[0x62, 0xE1, 0xFD, 0x08, 0x17, 0x04, 0x24],
        ),
    ];
    for &(case, memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_half_move_store_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding, case.expected_encoding());
        assert_eq!(encoding.stack_instruction.as_slice(), replay);
        for level in [OptLevel::O0, OptLevel::O2] {
            lower_store(&optimize(function_from_bytes(memory), level), case);
        }
    }
}

fn sib_encoding(case: HalfMoveStoreCase, base_high: bool, index_high: bool) -> Vec<u8> {
    let mut bytes = case.bytes().to_vec();
    bytes[5] = ((case.source & 7) << 3) | 4;
    bytes.push(0x48); // [RAX + RCX*2], extended by APX B4/X4 when selected.
    bytes[1] |= u8::from(base_high) << 3;
    if index_high {
        bytes[2] &= !0x04;
    }
    bytes
}

#[test]
fn store_classifier_crosses_all_512_source_format_lane_and_apx_address_cells() {
    let mut cells = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for source in 0..32u8 {
                let case = HalfMoveStoreCase {
                    lane,
                    format,
                    source,
                };
                for base_high in [false, true] {
                    for index_high in [false, true] {
                        let bytes = sib_encoding(case, base_high, index_high);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_half_move_store_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(encoding, case.expected_encoding(), "{bytes:02X?}");
                        assert_eq!(encoding.stack_instruction.as_slice()[1] & 0x08, 0);
                        assert_eq!(encoding.stack_instruction.as_slice()[2] & 0x04, 0x04);
                        for level in [OptLevel::O0, OptLevel::O2] {
                            lower_store(&optimize(function_from_bytes(&bytes), level), case);
                        }
                        cells += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cells, 2 * 2 * 32 * 2 * 2);
}

#[test]
fn store_classifier_rejects_reserved_nonowned_register_truncated_and_trailing_images() {
    let case = HalfMoveStoreCase {
        lane: MemoryLane::High,
        format: MoveFormat::Pd,
        source: 31,
    };
    let valid = case.bytes().to_vec();
    let mut malformed = Vec::new();
    for (index, mask) in [
        (1, 0x02), // map 0F -> non-owned map
        (2, 0x80), // W1 -> W0 while pp remains 66
        (2, 0x01), // 66 -> NP while W remains W1
        (2, 0x08), // reserved EVEX.vvvv no longer encodes 1111b
        (3, 0x08), // reserved EVEX.V' no longer encodes 1b
        (3, 0x80), // reserved z
        (3, 0x20), // reserved L'L
        (3, 0x10), // reserved b
        (3, 0x01), // reserved aaa
        (4, 0x01), // non-owned load opcode 16H
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
                .and_then(|instruction| instruction.evex_half_move_store_encoding())
                .is_none(),
            "{bytes:02X?}"
        );
    }
    for opcode in [0x10, 0x11, 0x12, 0x14, 0x15, 0x16] {
        let mut bytes = valid.clone();
        bytes[4] = opcode;
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_half_move_store_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn store_classifier_exhausts_all_4194304_control_and_length_cells() {
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
                                            && matches!(opcode, 0x13 | 0x17)
                                            && matches!((pp, w), (0, false) | (1, true))
                                            && ll == 0
                                            && !embedded_control
                                            && !zeroing
                                            && mask == 0
                                            && !trailing;
                                        let actual = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_half_move_store_encoding();
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

#[test]
fn only_reserved_all_ones_evex_vvvv_and_v_prime_image_is_accepted() {
    let case = HalfMoveStoreCase {
        lane: MemoryLane::Low,
        format: MoveFormat::Ps,
        source: 1,
    };
    let mut accepted = 0usize;
    for encoded_vvvv in 0u8..=15 {
        for encoded_v_prime in [false, true] {
            let mut bytes = case.bytes();
            bytes[2] = (bytes[2] & !0x78) | (encoded_vvvv << 3);
            bytes[3] = (bytes[3] & !0x08) | (u8::from(encoded_v_prime) << 3);
            let actual = X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_half_move_store_encoding();
            let expected = encoded_vvvv == 15 && encoded_v_prime;
            assert_eq!(actual.is_some(), expected, "{bytes:02X?}");
            accepted += usize::from(actual.is_some());
        }
    }
    assert_eq!(accepted, 1);
}

fn assert_store_rejected(name: &str, function: &SmirFunction) {
    assert!(store_sequence(function, true).is_none(), "{name}");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}"
    );
    let mut lowerer = configured_lowerer(false);
    assert!(lowerer.lower_function(function).is_err(), "{name}");
}

#[test]
fn store_matcher_rejects_every_graph_field_provenance_and_virtual_escape_mutation() {
    let case = HalfMoveStoreCase {
        lane: MemoryLane::High,
        format: MoveFormat::Pd,
        source: 17,
    };
    let base = optimize(lift_store_case(case), OptLevel::O2);
    assert!(store_sequence(&base, true).is_some());
    let extracted = match base.blocks[0].ops[0].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    macro_rules! mutate_extract {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::VExtractLane { $field, .. } = &mut function.blocks[0].ops[0].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_extract!("extract destination", dst, VReg::Virtual(VirtualId(0x7F00)));
    mutate_extract!("extract source", vec, xmm(18));
    mutate_extract!("extract lane", lane, 0);
    mutate_extract!("extract element", elem, VecElementType::I32);
    mutate_extract!("extract extension", sign, SignExtend::Sign);

    macro_rules! mutate_store {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = base.clone();
            let OpKind::Store { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            malformed.push(($name, function));
        }};
    }
    mutate_store!("store source", src, xmm(0));
    mutate_store!(
        "store address",
        addr,
        Address::Direct(VReg::Virtual(VirtualId(0x7F01)))
    );
    mutate_store!("store width", width, MemWidth::B4);

    for index in 0..2 {
        let mut function = base.clone();
        function.blocks[0].ops[index].x86_hint = Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode(),
            width: VecWidth::V128,
            w: true,
        });
        malformed.push(("invented operation hint", function));
    }

    let mut split_pc = base.clone();
    split_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest provenance", split_pc));

    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FE0),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0x7F02)),
            src: SrcOperand::Reg(extracted),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value escapes", escaped));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FE1),
        PC + 1,
        OpKind::Mov {
            dst: extracted,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extracted value redefined", duplicate_definition));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FE2), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    for (name, function) in malformed {
        assert_store_rejected(name, &function);
    }

    let mut same_pc_head = base.clone();
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7FE3), PC, OpKind::Nop));
    let (definitions, uses) = virtual_counts(&same_pc_head);
    assert!(
        x86_jit_evex_half_move_store_sequence(
            &same_pc_head.blocks[0],
            1,
            true,
            &same_pc_head.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "same-PC head must prevent mid-instruction admission"
    );

    let mut missing = base.clone();
    missing.x86_instruction_bytes.clear();
    assert_store_rejected("missing source metadata", &missing);

    let mut load_metadata = base;
    let mut bytes = case.bytes();
    bytes[4] = 0x16;
    load_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert_store_rejected("load source metadata", &load_metadata);
}
