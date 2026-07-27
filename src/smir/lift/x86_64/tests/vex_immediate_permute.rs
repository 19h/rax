//! Exhaustive strict-lift coverage for register-source AVX/AVX2 VEX
//! immediate permutes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermuteKind {
    PermilPs,
    PermilPd,
    PermQ,
    PermPd,
}

impl PermuteKind {
    fn opcode(self) -> u8 {
        match self {
            Self::PermilPs => 0x04,
            Self::PermilPd => 0x05,
            Self::PermQ => 0x00,
            Self::PermPd => 0x01,
        }
    }

    fn element(self) -> VecElementType {
        match self {
            Self::PermilPs => VecElementType::F32,
            Self::PermilPd | Self::PermPd => VecElementType::F64,
            Self::PermQ => VecElementType::I64,
        }
    }

    fn w(self) -> bool {
        matches!(self, Self::PermQ | Self::PermPd)
    }
}

const SHAPES: [(PermuteKind, bool); 6] = [
    (PermuteKind::PermilPs, false),
    (PermuteKind::PermilPs, true),
    (PermuteKind::PermilPd, false),
    (PermuteKind::PermilPd, true),
    (PermuteKind::PermQ, true),
    (PermuteKind::PermPd, true),
];

fn vector(register: u8, ymm: bool) -> VReg {
    VReg::Arch(ArchReg::X86(if ymm {
        X86Reg::Ymm(register)
    } else {
        X86Reg::Xmm(register)
    }))
}

fn encoding(
    kind: PermuteKind,
    ymm: bool,
    ignored_x: bool,
    dst: u8,
    src: u8,
    immediate: u8,
) -> [u8; 6] {
    assert!(dst < 16 && src < 16);
    let mut p0 = 0xE3;
    if dst >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if src >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(kind.w()) << 7) | 0x78 | (u8::from(ymm) << 2) | 1,
        kind.opcode(),
        0xC0 | ((dst & 7) << 3) | (src & 7),
        immediate,
    ]
}

fn expected_source_lane(kind: PermuteKind, lane: u8, immediate: u8) -> u8 {
    match kind {
        PermuteKind::PermilPs => {
            let domain = lane / 4 * 4;
            domain + ((immediate >> ((lane % 4) * 2)) & 3)
        }
        PermuteKind::PermilPd => {
            let domain = lane / 2 * 2;
            domain + ((immediate >> lane) & 1)
        }
        PermuteKind::PermQ | PermuteKind::PermPd => (immediate >> (lane * 2)) & 3,
    }
}

fn assert_exact_register_lift(
    bytes: &[u8],
    kind: PermuteKind,
    ymm: bool,
    dst: u8,
    src: u8,
    immediate: u8,
) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let OpKind::VPermute {
        dst: actual_dst,
        src1,
        src2,
        indices,
        elem,
        width,
        overwrite_table,
    } = &lifted.ops.last().expect("VEX immediate permute op").kind
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(*actual_dst, vector(dst, ymm), "{bytes:02X?}");
    assert_eq!(*src1, vector(src, ymm), "{bytes:02X?}");
    assert_eq!(*src2, None, "{bytes:02X?}");
    assert_eq!(*elem, kind.element(), "{bytes:02X?}");
    assert_eq!(
        *width,
        if ymm { VecWidth::V256 } else { VecWidth::V128 },
        "{bytes:02X?}"
    );
    assert!(!overwrite_table, "{bytes:02X?}");

    let lanes = width.lanes(*elem) as u8;
    for lane in 0..lanes {
        let expected = i64::from(expected_source_lane(kind, lane, immediate));
        let pair = lifted.ops.windows(2).find(|pair| {
            matches!(
                &pair[1].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem: actual_elem,
                    ..
                } if *dst == *indices
                    && *vec == *indices
                    && *actual_lane == lane
                    && *actual_elem == *elem
            )
        });
        let Some([move_op, insert_op]) = pair else {
            panic!("{bytes:02X?}: missing index lane {lane}: {:#?}", lifted.ops)
        };
        let OpKind::VInsertLane { scalar, .. } = insert_op.kind else {
            unreachable!()
        };
        assert!(
            matches!(
                move_op.kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(value),
                    width: OpWidth::W64,
                } if dst == scalar && value == expected
            ),
            "{bytes:02X?}: index lane {lane}, expected {expected}: {:#?}",
            lifted.ops
        );
    }
}

#[test]
fn all_18432_structural_samples_strictly_lift_with_exact_index_equations() {
    const IMMEDIATES: [u8; 6] = [0x00, 0x1B, 0x4E, 0xA5, 0xE4, 0xFF];
    let mut lifted = 0usize;
    for (kind, ymm) in SHAPES {
        for ignored_x in [false, true] {
            for dst in 0u8..16 {
                for src in 0u8..16 {
                    for immediate in IMMEDIATES {
                        let bytes = encoding(kind, ymm, ignored_x, dst, src, immediate);
                        assert_exact_register_lift(&bytes, kind, ymm, dst, src, immediate);
                        lifted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 18_432);
}

#[test]
fn reserved_vvvv_w_l_and_byte_shapes_are_precise_invalid_frontiers() {
    for (kind, ymm) in SHAPES {
        for ignored_x in [false, true] {
            let base = encoding(kind, ymm, ignored_x, 9, 11, 0x4E);
            for raw_vvvv in 0u8..15 {
                let mut bytes = base;
                bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
                assert!(
                    matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                    "{bytes:02X?}"
                );
                assert!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_immediate_permute_needs_avx2()
                        .is_none(),
                    "{bytes:02X?}"
                );
            }

            let mut wrong_w = base;
            wrong_w[2] ^= 0x80;
            assert!(
                matches!(
                    lift_single(&wrong_w),
                    Err(LiftError::InvalidEncoding { .. })
                ),
                "{wrong_w:02X?}"
            );
            assert!(
                X86InstructionBytes::new(&wrong_w)
                    .unwrap()
                    .vex_register_immediate_permute_needs_avx2()
                    .is_none(),
                "{wrong_w:02X?}"
            );
        }
    }

    for kind in [PermuteKind::PermQ, PermuteKind::PermPd] {
        for ignored_x in [false, true] {
            let bytes = encoding(kind, false, ignored_x, 9, 11, 0xA5);
            assert!(
                matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                "{bytes:02X?}"
            );
        }
    }

    let bytes = encoding(PermuteKind::PermilPs, true, true, 9, 11, 0xE4);
    assert!(
        matches!(lift_single(&bytes[..5]), Err(LiftError::Incomplete { .. })),
        "{:02X?}",
        &bytes[..5]
    );
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    let lifted =
        lift_single(&trailing).unwrap_or_else(|error| panic!("{trailing:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{trailing:02X?}");
    assert!(
        X86InstructionBytes::new(&trailing)
            .unwrap()
            .vex_register_immediate_permute_needs_avx2()
            .is_none(),
        "{trailing:02X?}"
    );
}

#[test]
fn representative_memory_forms_lift_but_never_enter_native_replay() {
    let cases: &[&[u8]] = &[
        &[0xC4, 0xE3, 0x79, 0x04, 0x00, 0x1B],
        &[0xC4, 0xE3, 0x7D, 0x04, 0x48, 0x20, 0xE4],
        &[0xC4, 0xE3, 0x79, 0x05, 0x10, 0x02],
        &[0xC4, 0xE3, 0x7D, 0x05, 0x18, 0x0D],
        &[0xC4, 0xE3, 0xFD, 0x00, 0x20, 0x1B],
        &[0xC4, 0xE3, 0xFD, 0x01, 0x28, 0xE4],
    ];
    for &bytes in cases {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. })),
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_immediate_permute_needs_avx2()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}
