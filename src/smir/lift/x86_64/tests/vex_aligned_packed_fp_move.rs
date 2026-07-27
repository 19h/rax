//! Exhaustive strict-lift coverage for VEX `VMOVAPS`/`VMOVAPD`.

use super::*;

#[derive(Clone, Copy, Debug)]
enum MoveKind {
    F32,
    F64,
}

impl MoveKind {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 0,
            Self::F64 => 1,
        }
    }

    fn prefix(self) -> X86SsePrefix {
        match self {
            Self::F32 => X86SsePrefix::None,
            Self::F64 => X86SsePrefix::OpSize,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Direction {
    Load,
    Store,
}

impl Direction {
    const ALL: [Self; 2] = [Self::Load, Self::Store];

    fn opcode(self) -> u8 {
        match self {
            Self::Load => 0x28,
            Self::Store => 0x29,
        }
    }

    fn operands(self, reg: u8, rm: u8) -> (u8, u8) {
        match self {
            Self::Load => (reg, rm),
            Self::Store => (rm, reg),
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

fn vex_c5_encoding(kind: MoveKind, direction: Direction, l: bool, reg: u8, rm: u8) -> [u8; 4] {
    assert!(reg < 16 && rm < 8);
    [
        0xC5,
        (if reg < 8 { 0x80 } else { 0 }) | 0x78 | (u8::from(l) << 2) | kind.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | rm,
    ]
}

fn vex_c4_encoding(
    kind: MoveKind,
    direction: Direction,
    l: bool,
    w: bool,
    ignored_x: bool,
    reg: u8,
    rm: u8,
) -> [u8; 5] {
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
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x78 | (u8::from(l) << 2) | kind.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | (rm & 7),
    ]
}

fn assert_exact_register_lift(
    bytes: &[u8],
    kind: MoveKind,
    direction: Direction,
    l: bool,
    w: bool,
    reg: u8,
    rm: u8,
) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let (dst, src) = direction.operands(reg, rm);
    assert_eq!(lifted.ops.len(), 1, "{bytes:02X?}: {:#?}", lifted.ops);
    assert!(
        matches!(
            &lifted.ops[0],
            SmirOp {
                kind: OpKind::VMov {
                    dst: actual_dst,
                    src: actual_src,
                    width,
                },
                x86_hint: Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp,
                    opcode,
                    width: hint_width,
                    w: hint_w,
                }),
                ..
            } if *actual_dst == vector(dst, l)
                && *actual_src == vector(src, l)
                && *width == if l { VecWidth::V256 } else { VecWidth::V128 }
                && *pp == kind.prefix()
                && *opcode == direction.opcode()
                && *hint_width == *width
                && *hint_w == w
        ),
        "{bytes:02X?}: {:#?}",
        lifted.ops
    );
}

#[test]
fn every_9216_register_encoding_strictly_lifts_with_exact_direction_and_width() {
    let mut lifted = 0usize;

    for kind in MoveKind::ALL {
        for direction in Direction::ALL {
            for l in [false, true] {
                for reg in 0u8..16 {
                    for rm in 0u8..8 {
                        let bytes = vex_c5_encoding(kind, direction, l, reg, rm);
                        assert_exact_register_lift(&bytes, kind, direction, l, false, reg, rm);
                        lifted += 1;
                    }
                }
            }
        }
    }

    for kind in MoveKind::ALL {
        for direction in Direction::ALL {
            for l in [false, true] {
                for w in [false, true] {
                    for ignored_x in [false, true] {
                        for reg in 0u8..16 {
                            for rm in 0u8..16 {
                                let bytes =
                                    vex_c4_encoding(kind, direction, l, w, ignored_x, reg, rm);
                                assert_exact_register_lift(&bytes, kind, direction, l, w, reg, rm);
                                lifted += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(lifted, 9_216);
}

#[test]
fn every_nonreserved_vvvv_value_is_rejected_before_register_move_execution() {
    let mut rejected = 0usize;
    for kind in MoveKind::ALL {
        for direction in Direction::ALL {
            for l in [false, true] {
                for reg in [1, 9] {
                    for raw_vvvv in 0u8..15 {
                        let mut bytes = vex_c5_encoding(kind, direction, l, reg, 3);
                        bytes[1] = (bytes[1] & !0x78) | (raw_vvvv << 3);
                        assert!(
                            matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                            "{bytes:02X?}"
                        );
                        rejected += 1;
                    }
                }
                for w in [false, true] {
                    for ignored_x in [false, true] {
                        for reg in [1, 9] {
                            for rm in [3, 11] {
                                for raw_vvvv in 0u8..15 {
                                    let mut bytes =
                                        vex_c4_encoding(kind, direction, l, w, ignored_x, reg, rm);
                                    bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
                                    assert!(
                                        matches!(
                                            lift_single(&bytes),
                                            Err(LiftError::InvalidEncoding { .. })
                                        ),
                                        "{bytes:02X?}"
                                    );
                                    rejected += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(rejected, 2_160);
}

#[test]
fn rep_and_repne_prefix_selectors_are_rejected_for_both_directions_and_lengths() {
    for direction in Direction::ALL {
        for l in [false, true] {
            for pp in [2, 3] {
                let bytes = [
                    0xC5,
                    0xF8 | (u8::from(l) << 2) | pp,
                    direction.opcode(),
                    0xCB,
                ];
                assert!(
                    matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                    "{bytes:02X?}"
                );
                assert!(
                    !X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .is_vex_register_aligned_packed_fp_move(),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn representative_memory_forms_keep_alignment_checks_and_reject_native_replay() {
    let cases: &[(&[u8], u8)] = &[
        (&[0xC5, 0xF8, 0x28, 0x00], 16),
        (&[0xC5, 0xFD, 0x29, 0x48, 0x20], 32),
        (
            &[0xC4, 0x01, 0xF9, 0x28, 0x84, 0x8A, 0x40, 0x00, 0x00, 0x00],
            16,
        ),
        (&[0xC4, 0x21, 0xFC, 0x29, 0x85, 0x60, 0x00, 0x00, 0x00], 32),
    ];

    for &(bytes, alignment) in cases {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            lifted.ops.iter().any(|op| {
                matches!(
                    op.kind,
                    OpKind::X86CheckAlignment {
                        alignment: actual,
                        ..
                    } if actual == alignment
                )
            }),
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(
            !instruction.is_vex_register_aligned_packed_fp_move(),
            "{bytes:02X?}"
        );
    }
}
