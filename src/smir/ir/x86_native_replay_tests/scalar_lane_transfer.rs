//! Exact classifier tests for register-only EVEX scalar lane transfers.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

impl WMode {
    fn values(self) -> &'static [bool] {
        match self {
            Self::Ignored => &[false, true],
            Self::W0 => &[false],
            Self::W1 => &[true],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GprField {
    None,
    Reg,
    Rm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferKind {
    Vextractps,
    Vinsertps,
    Vpextrb,
    Vpextrd,
    Vpextrq,
    VpextrwMap1,
    VpextrwMap3,
    Vpinsrb,
    Vpinsrd,
    Vpinsrq,
    Vpinsrw,
}

impl TransferKind {
    const ALL: [Self; 11] = [
        Self::Vextractps,
        Self::Vinsertps,
        Self::Vpextrb,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VpextrwMap1,
        Self::VpextrwMap3,
        Self::Vpinsrb,
        Self::Vpinsrd,
        Self::Vpinsrq,
        Self::Vpinsrw,
    ];

    fn fields(self) -> (u8, u8, WMode, bool, bool, GprField) {
        match self {
            Self::Vextractps => (3, 0x17, WMode::Ignored, false, true, GprField::Rm),
            Self::Vinsertps => (3, 0x21, WMode::W0, false, false, GprField::None),
            Self::Vpextrb => (3, 0x14, WMode::Ignored, false, true, GprField::Rm),
            Self::Vpextrd => (3, 0x16, WMode::W0, true, true, GprField::Rm),
            Self::Vpextrq => (3, 0x16, WMode::W1, true, true, GprField::Rm),
            Self::VpextrwMap1 => (1, 0xC5, WMode::Ignored, false, true, GprField::Reg),
            Self::VpextrwMap3 => (3, 0x15, WMode::Ignored, false, true, GprField::Rm),
            Self::Vpinsrb => (3, 0x20, WMode::Ignored, false, false, GprField::Rm),
            Self::Vpinsrd => (3, 0x22, WMode::W0, true, false, GprField::Rm),
            Self::Vpinsrq => (3, 0x22, WMode::W1, true, false, GprField::Rm),
            Self::Vpinsrw => (1, 0xC4, WMode::Ignored, false, false, GprField::Rm),
        }
    }
}

fn encoding(
    kind: TransferKind,
    w: bool,
    destination: u8,
    merge: u8,
    source: u8,
    immediate: u8,
) -> [u8; 7] {
    let (map, opcode, w_mode, _, reserved_vvvv, gpr_field) = kind.fields();
    assert!(w_mode.values().contains(&w));
    assert!(destination < 32 && merge < 32 && source < 32);
    match gpr_field {
        GprField::None => {}
        GprField::Reg => assert!(destination < 16),
        GprField::Rm if reserved_vvvv => assert!(destination < 16),
        GprField::Rm => assert!(source < 16),
    }

    let (reg, rm) = match gpr_field {
        GprField::Reg => (destination, source),
        GprField::Rm if reserved_vvvv => (source, destination),
        GprField::Rm | GprField::None => (destination, source),
    };
    let mut p0 = 0xF0 | map;
    if reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    let (vvvv, v_prime) = if reserved_vvvv {
        (0x78, 0x08)
    } else {
        (((!merge) & 0x0F) << 3, if merge < 16 { 0x08 } else { 0 })
    };

    [
        0x62,
        p0,
        vvvv | 0x04 | 0x01 | if w { 0x80 } else { 0 },
        v_prime,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
        immediate,
    ]
}

fn safe_gprs() -> impl Iterator<Item = u8> {
    (0..16).filter(|register| !matches!(register, 4 | 5))
}

#[test]
fn scalar_lane_transfer_classifier_covers_369792_legal_register_encodings() {
    let mut classified = 0usize;
    for kind in TransferKind::ALL {
        let (_, _, w_mode, needs_dq, reserved_vvvv, gpr_field) = kind.fields();
        for &w in w_mode.values() {
            match gpr_field {
                GprField::Reg => {
                    debug_assert!(reserved_vvvv);
                    for gpr in safe_gprs() {
                        for vector in 0..32 {
                            for immediate in [0, 0x5A, 0xFF] {
                                let bytes = encoding(kind, w, gpr, 0, vector, immediate);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_scalar_lane_transfer_requires_dq(),
                                    Some(needs_dq),
                                    "{kind:?} {bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
                GprField::Rm if reserved_vvvv => {
                    for gpr in safe_gprs() {
                        for vector in 0..32 {
                            for immediate in [0, 0x5A, 0xFF] {
                                let bytes = encoding(kind, w, gpr, 0, vector, immediate);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_scalar_lane_transfer_requires_dq(),
                                    Some(needs_dq),
                                    "{kind:?} {bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
                GprField::Rm => {
                    for destination in 0..32 {
                        for merge in 0..32 {
                            for source in safe_gprs() {
                                for immediate in [0, 0x5A, 0xFF] {
                                    let bytes =
                                        encoding(kind, w, destination, merge, source, immediate);
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_lane_transfer_requires_dq(),
                                        Some(needs_dq),
                                        "{kind:?} {bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
                GprField::None => {
                    for destination in 0..32 {
                        for merge in 0..32 {
                            for source in 0..32 {
                                for immediate in [0, 0x5A, 0xFF] {
                                    let bytes =
                                        encoding(kind, w, destination, merge, source, immediate);
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_lane_transfer_requires_dq(),
                                        Some(needs_dq),
                                        "{kind:?} {bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 369_792);
}

#[test]
fn scalar_lane_transfer_classifier_fails_closed_for_every_unsafe_boundary() {
    let extract = encoding(TransferKind::VpextrwMap1, false, 0, 0, 31, 0xFF);
    let insert = encoding(TransferKind::Vpinsrw, false, 31, 30, 0, 0xFF);
    let insertps = encoding(TransferKind::Vinsertps, false, 31, 30, 29, 0xFF);
    let mut invalid = Vec::new();

    let mut non_evex = extract;
    non_evex[0] = 0x61;
    invalid.push(non_evex.to_vec());
    for (index, mask) in [(1, 0x01), (2, 0x04)] {
        let mut bytes = extract;
        bytes[index] &= !mask;
        invalid.push(bytes.to_vec());
    }
    for mask in [0x01, 0x10, 0x20, 0x80] {
        let mut bytes = extract;
        bytes[3] |= mask;
        invalid.push(bytes.to_vec());
    }
    let mut nonreserved_vvvv = extract;
    nonreserved_vvvv[2] &= !0x08;
    invalid.push(nonreserved_vvvv.to_vec());
    let mut nonreserved_v_prime = extract;
    nonreserved_v_prime[3] &= !0x08;
    invalid.push(nonreserved_v_prime.to_vec());
    let mut memory = extract;
    memory[5] &= 0x3F;
    invalid.push(memory.to_vec());
    let mut fabricated_reg_gpr = extract;
    fabricated_reg_gpr[1] &= !0x10;
    invalid.push(fabricated_reg_gpr.to_vec());
    let mut fabricated_rm_gpr = insert;
    fabricated_rm_gpr[1] &= !0x40;
    invalid.push(fabricated_rm_gpr.to_vec());
    let mut wrong_map = extract;
    wrong_map[1] = (wrong_map[1] & 0xF0) | 2;
    invalid.push(wrong_map.to_vec());
    let mut wrong_pp = extract;
    wrong_pp[2] = (wrong_pp[2] & !0x03) | 2;
    invalid.push(wrong_pp.to_vec());
    let mut wrong_opcode = extract;
    wrong_opcode[4] = 0xC6;
    invalid.push(wrong_opcode.to_vec());
    let mut insertps_w1 = insertps;
    insertps_w1[2] |= 0x80;
    invalid.push(insertps_w1.to_vec());
    invalid.push(encoding(TransferKind::VpextrwMap1, false, 4, 0, 1, 0).to_vec());
    invalid.push(encoding(TransferKind::VpextrwMap1, true, 5, 0, 1, 0).to_vec());
    invalid.push(encoding(TransferKind::Vpextrb, false, 4, 0, 1, 0).to_vec());
    invalid.push(encoding(TransferKind::Vextractps, true, 5, 0, 1, 0).to_vec());
    invalid.push(encoding(TransferKind::Vpinsrb, false, 1, 2, 4, 0).to_vec());
    invalid.push(encoding(TransferKind::Vpinsrw, true, 1, 2, 5, 0).to_vec());
    invalid.push(extract[..6].to_vec());
    let mut trailing = extract.to_vec();
    trailing.push(0xA5);
    invalid.push(trailing);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_lane_transfer_requires_dq(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn scalar_lane_transfer_replay_spans_encode_exact_feature_requirements() {
    let pc = 0x1013;
    let mut block = SmirBlock::new(BlockId(35), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for kind in TransferKind::ALL {
        let (_, _, w_mode, needs_dq, reserved_vvvv, gpr_field) = kind.fields();
        for &w in w_mode.values() {
            let (destination, merge, source) = match gpr_field {
                GprField::Reg => {
                    debug_assert!(reserved_vvvv);
                    (15, 0, 31)
                }
                GprField::Rm if reserved_vvvv => (15, 0, 31),
                GprField::Rm => (31, 30, 15),
                GprField::None => (31, 30, 29),
            };
            let bytes = encoding(kind, w, destination, merge, source, 0xFF);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = HashMap::from([((BlockId(35), pc), instruction)]);
            for spans in [
                x86_evex_scalar_lane_transfer_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans
                    .get(&0)
                    .unwrap_or_else(|| panic!("{kind:?} {bytes:02X?}"));
                assert_eq!(span.end, 1, "{kind:?} {bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{kind:?} {bytes:02X?}");
                assert!(!span.needs_avx512vl, "{kind:?} {bytes:02X?}");
                assert_eq!(span.needs_avx512dq, needs_dq, "{kind:?} {bytes:02X?}");
                assert!(!span.needs_avx512fp16, "{kind:?} {bytes:02X?}");
            }
        }
    }
}
