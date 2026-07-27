//! Strict-lift coverage for VEX vector sign-mask extracts targeting guest RSP
//! or RBP.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskKind {
    Vmovmskps,
    Vmovmskpd,
    Vpmovmskb,
}

impl MaskKind {
    const ALL: [Self; 3] = [Self::Vmovmskps, Self::Vmovmskpd, Self::Vpmovmskb];

    fn fields(self) -> (X86SsePrefix, u8, VecElementType) {
        match self {
            Self::Vmovmskps => (X86SsePrefix::None, 0x50, VecElementType::F32),
            Self::Vmovmskpd => (X86SsePrefix::OpSize, 0x50, VecElementType::F64),
            Self::Vpmovmskb => (X86SsePrefix::OpSize, 0xD7, VecElementType::I8),
        }
    }
}

fn c4_encoding(
    kind: MaskKind,
    w: bool,
    wide: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
) -> [u8; 5] {
    assert!(matches!(destination, 4 | 5));
    assert!(source < 16);
    let (pp, opcode, _) = kind.fields();
    let mut p0 = 0xE1;
    if ignored_x {
        p0 &= !0x40;
    }
    if source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7)
            | 0x78
            | (u8::from(wide) << 2)
            | match pp {
                X86SsePrefix::None => 0,
                X86SsePrefix::OpSize => 1,
                _ => unreachable!(),
            },
        opcode,
        0xC0 | (destination << 3) | (source & 7),
    ]
}

fn c5_encoding(kind: MaskKind, wide: bool, destination: u8, source: u8) -> [u8; 4] {
    assert!(matches!(destination, 4 | 5));
    assert!(source < 8);
    let (pp, opcode, _) = kind.fields();
    [
        0xC5,
        0xF8 | (u8::from(wide) << 2)
            | match pp {
                X86SsePrefix::None => 0,
                X86SsePrefix::OpSize => 1,
                _ => unreachable!(),
            },
        opcode,
        0xC0 | (destination << 3) | source,
    ]
}

fn assert_exact_lift(
    bytes: &[u8],
    kind: MaskKind,
    w: bool,
    wide: bool,
    destination: u8,
    source: u8,
) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
    let (pp, opcode, elem) = kind.fields();
    let width = if wide { VecWidth::V256 } else { VecWidth::V128 };
    let expected_source = if wide {
        X86Reg::Ymm(source)
    } else {
        X86Reg::Xmm(source)
    };
    assert!(
        matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86MovMask {
                    dst: VReg::Arch(ArchReg::X86(actual_destination)),
                    src: VReg::Arch(ArchReg::X86(actual_source)),
                    elem: actual_elem,
                    lanes,
                    dst_width: OpWidth::W32,
                },
                x86_hint: Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: actual_pp,
                    opcode: actual_opcode,
                    width: actual_width,
                    w: actual_w,
                }),
                ..
            }] if actual_destination.gpr_index() == Some(destination)
                && *actual_source == expected_source
                && *actual_elem == elem
                && u32::from(*lanes) == width.lanes(elem)
                && *actual_pp == pp
                && *actual_opcode == opcode
                && *actual_width == width
                && *actual_w == w
        ),
        "{bytes:02X?}: {:#?}",
        lifted.ops
    );
    assert!(
        lifted
            .ops
            .iter()
            .all(|op| op.kind.flags_written().is_empty()),
        "{bytes:02X?}"
    );
}

#[test]
fn all_864_stack_destination_byte_encodings_strictly_lift_with_exact_hints() {
    let mut lifted = 0usize;
    for kind in MaskKind::ALL {
        for w in [false, true] {
            for wide in [false, true] {
                for ignored_x in [false, true] {
                    for destination in [4, 5] {
                        for source in 0..16 {
                            let bytes = c4_encoding(kind, w, wide, ignored_x, destination, source);
                            assert_exact_lift(&bytes, kind, w, wide, destination, source);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_mov_mask_stack_destination_needs_avx2(),
                                Some(kind == MaskKind::Vpmovmskb && wide),
                                "{bytes:02X?}"
                            );
                            lifted += 1;
                        }
                    }
                }
            }
        }
        for wide in [false, true] {
            for destination in [4, 5] {
                for source in 0..8 {
                    let bytes = c5_encoding(kind, wide, destination, source);
                    assert_exact_lift(&bytes, kind, false, wide, destination, source);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert_eq!(
                        instruction.vex_mov_mask_stack_destination_needs_avx2(),
                        Some(kind == MaskKind::Vpmovmskb && wide),
                        "{bytes:02X?}"
                    );
                    lifted += 1;
                }
            }
        }
    }
    assert_eq!(lifted, 864);
}

#[test]
fn replay_classifier_is_narrower_than_valid_non_stack_movmask_lifting() {
    for (destination, source) in [(0, 1), (3, 15), (8, 0), (15, 14)] {
        let kind = MaskKind::Vpmovmskb;
        let wide = destination & 1 != 0;
        let mut bytes = c4_encoding(kind, true, wide, true, 4, source);
        if destination < 8 {
            bytes[1] |= 0x80;
        } else {
            bytes[1] &= !0x80;
        }
        bytes[4] = (bytes[4] & !0x38) | ((destination & 7) << 3);
        assert_exact_lift(&bytes, kind, true, wide, destination, source);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn reserved_memory_and_exact_byte_shape_frontiers_fail_closed() {
    let base = c4_encoding(MaskKind::Vpmovmskb, true, true, true, 5, 15);
    for raw_vvvv in 0u8..15 {
        let mut bytes = base;
        bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
        assert!(matches!(
            lift_single(&bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
    }

    for bytes in [
        &[0xC4, 0xA1, 0xFD, 0xD7, 0x2F][..],
        &[0xC5, 0xFD, 0xD7, 0x2F][..],
        &[0xC4, 0xA1, 0xFC, 0xD7, 0xEF][..],
        &[0xC4, 0xA1, 0xFE, 0x50, 0xEF][..],
        &[0x62, 0xF1, 0x7D, 0x28, 0xD7, 0xEF][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "{bytes:02X?}"
        );
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
    }

    assert!(matches!(
        lift_single(&base[..4]),
        Err(LiftError::Incomplete { .. })
    ));
    let mut trailing = base.to_vec();
    trailing.push(0);
    let lifted =
        lift_single(&trailing).unwrap_or_else(|error| panic!("{trailing:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, base.len());
    assert_eq!(
        X86InstructionBytes::new(&trailing)
            .unwrap()
            .vex_mov_mask_stack_destination_needs_avx2(),
        None
    );
}
