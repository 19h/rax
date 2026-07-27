//! Exhaustive strict-lift coverage for VEX one-source lane shuffles.

use super::*;

#[derive(Clone, Copy, Debug)]
enum Family {
    MoveSlDup,
    MoveShDup,
    MoveDDup,
    ShuffleDword,
    ShuffleHighWord,
    ShuffleLowWord,
}

impl Family {
    const ALL: [Self; 6] = [
        Self::MoveSlDup,
        Self::MoveShDup,
        Self::MoveDDup,
        Self::ShuffleDword,
        Self::ShuffleHighWord,
        Self::ShuffleLowWord,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::MoveSlDup | Self::MoveDDup => 0x12,
            Self::MoveShDup => 0x16,
            Self::ShuffleDword | Self::ShuffleHighWord | Self::ShuffleLowWord => 0x70,
        }
    }

    fn pp(self) -> u8 {
        match self {
            Self::MoveSlDup | Self::MoveShDup | Self::ShuffleHighWord => 2,
            Self::MoveDDup | Self::ShuffleLowWord => 3,
            Self::ShuffleDword => 1,
        }
    }

    fn immediate(self) -> bool {
        matches!(
            self,
            Self::ShuffleDword | Self::ShuffleHighWord | Self::ShuffleLowWord
        )
    }

    fn element(self) -> VecElementType {
        match self {
            Self::MoveSlDup | Self::MoveShDup => VecElementType::F32,
            Self::MoveDDup => VecElementType::F64,
            Self::ShuffleDword => VecElementType::I32,
            Self::ShuffleHighWord | Self::ShuffleLowWord => VecElementType::I16,
        }
    }
}

fn vector(register: u8, l: bool) -> VReg {
    VReg::Arch(ArchReg::X86(if l {
        X86Reg::Ymm(register)
    } else {
        X86Reg::Xmm(register)
    }))
}

fn vex_c5_encoding(family: Family, l: bool, reg: u8, rm: u8, immediate: Option<u8>) -> Vec<u8> {
    assert!(reg < 16 && rm < 8);
    let mut bytes = vec![
        0xC5,
        (if reg < 8 { 0x80 } else { 0 }) | 0x78 | (u8::from(l) << 2) | family.pp(),
        family.opcode(),
        0xC0 | ((reg & 7) << 3) | rm,
    ];
    if let Some(immediate) = immediate {
        bytes.push(immediate);
    }
    bytes
}

fn vex_c4_encoding(
    family: Family,
    l: bool,
    w: bool,
    ignored_x: bool,
    reg: u8,
    rm: u8,
    immediate: Option<u8>,
) -> Vec<u8> {
    assert!(reg < 16 && rm < 16);
    let mut p0 = 0xE1;
    if reg >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    let mut bytes = vec![
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x78 | (u8::from(l) << 2) | family.pp(),
        family.opcode(),
        0xC0 | ((reg & 7) << 3) | (rm & 7),
    ];
    if let Some(immediate) = immediate {
        bytes.push(immediate);
    }
    bytes
}

fn assert_exact_register_lift(bytes: &[u8], family: Family, l: bool, reg: u8, rm: u8) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let expected_lanes = if l {
        match family.element() {
            VecElementType::F64 => 4,
            VecElementType::F32 | VecElementType::I32 => 8,
            VecElementType::I16 => 16,
            _ => unreachable!(),
        }
    } else {
        match family.element() {
            VecElementType::F64 => 2,
            VecElementType::F32 | VecElementType::I32 => 4,
            VecElementType::I16 => 8,
            _ => unreachable!(),
        }
    };
    assert!(
        matches!(
            lifted.ops.last(),
            Some(SmirOp {
                kind: OpKind::VShuffle {
                    dst,
                    src1,
                    src2: None,
                    elem,
                    lanes,
                    ..
                },
                ..
            }) if *dst == vector(reg, l)
                && *src1 == vector(rm, l)
                && *elem == family.element()
                && *lanes == expected_lanes
        ),
        "{bytes:02X?}: {:#?}",
        lifted.ops
    );
}

#[test]
fn all_48384_structural_register_samples_strictly_lift_with_exact_operands() {
    const IMMEDIATES: [u8; 6] = [0x00, 0x1B, 0x4E, 0xA5, 0xE4, 0xFF];
    let mut lifted = 0usize;

    for family in Family::ALL {
        let immediate_samples: &[Option<u8>] = if family.immediate() {
            &[
                Some(IMMEDIATES[0]),
                Some(IMMEDIATES[1]),
                Some(IMMEDIATES[2]),
                Some(IMMEDIATES[3]),
                Some(IMMEDIATES[4]),
                Some(IMMEDIATES[5]),
            ]
        } else {
            &[None]
        };
        for l in [false, true] {
            for reg in 0u8..16 {
                for rm in 0u8..8 {
                    for &immediate in immediate_samples {
                        let bytes = vex_c5_encoding(family, l, reg, rm, immediate);
                        assert_exact_register_lift(&bytes, family, l, reg, rm);
                        lifted += 1;
                    }
                }
            }
        }
    }

    for family in Family::ALL {
        let immediate_samples: &[Option<u8>] = if family.immediate() {
            &[
                Some(IMMEDIATES[0]),
                Some(IMMEDIATES[1]),
                Some(IMMEDIATES[2]),
                Some(IMMEDIATES[3]),
                Some(IMMEDIATES[4]),
                Some(IMMEDIATES[5]),
            ]
        } else {
            &[None]
        };
        for l in [false, true] {
            for w in [false, true] {
                for ignored_x in [false, true] {
                    for reg in 0u8..16 {
                        for rm in 0u8..16 {
                            for &immediate in immediate_samples {
                                let bytes =
                                    vex_c4_encoding(family, l, w, ignored_x, reg, rm, immediate);
                                assert_exact_register_lift(&bytes, family, l, reg, rm);
                                lifted += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(lifted, 48_384);
}

#[test]
fn nonreserved_vvvv_and_wrong_immediate_shapes_are_precise_invalid_encodings() {
    let mut rejected = 0usize;
    for family in Family::ALL {
        let immediate = family.immediate().then_some(0x4E);
        for l in [false, true] {
            for raw_vvvv in 0u8..15 {
                let mut c5 = vex_c5_encoding(family, l, 9, 3, immediate);
                c5[1] = (c5[1] & !0x78) | (raw_vvvv << 3);
                assert!(
                    matches!(lift_single(&c5), Err(LiftError::InvalidEncoding { .. })),
                    "{c5:02X?}"
                );
                rejected += 1;

                for w in [false, true] {
                    let mut c4 = vex_c4_encoding(family, l, w, true, 9, 11, immediate);
                    c4[2] = (c4[2] & !0x78) | (raw_vvvv << 3);
                    assert!(
                        matches!(lift_single(&c4), Err(LiftError::InvalidEncoding { .. })),
                        "{c4:02X?}"
                    );
                    rejected += 1;
                }
            }
        }
    }
    assert_eq!(rejected, 540);

    for family in Family::ALL {
        let mut bytes = vex_c4_encoding(
            family,
            false,
            true,
            true,
            9,
            11,
            family.immediate().then_some(0x1B),
        );
        if family.immediate() {
            bytes.pop();
            assert!(
                matches!(lift_single(&bytes), Err(LiftError::Incomplete { .. })),
                "{bytes:02X?}"
            );
        } else {
            bytes.push(0);
            let lifted =
                lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            assert_eq!(lifted.bytes_consumed, bytes.len() - 1, "{bytes:02X?}");
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_lane_shuffle_needs_avx2()
                    .is_none(),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn representative_memory_forms_lift_but_never_enter_native_replay() {
    let cases: &[&[u8]] = &[
        &[0xC5, 0xFA, 0x12, 0x00],
        &[0xC5, 0xFE, 0x16, 0x48, 0x20],
        &[0xC4, 0x21, 0xFB, 0x12, 0x84, 0x8A, 0x40, 0x00, 0x00, 0x00],
        &[0xC5, 0xF9, 0x70, 0x00, 0x1B],
        &[0xC4, 0x21, 0xFE, 0x70, 0x48, 0x20, 0x4E],
        &[
            0xC4, 0x01, 0xFB, 0x70, 0x84, 0x8A, 0x40, 0x00, 0x00, 0x00, 0xE4,
        ],
    ];
    for &bytes in cases {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. } | OpKind::Load { .. })),
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}
