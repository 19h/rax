//! EVEX scalar, tuple, GPR, and opmask broadcast tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_load_broadcasts_cover_scalar_tuple_gpr_masks_compressed_disp_and_invalids() {
    for (bytes, elem, lanes, destination) in [
        (
            &[0x62, 0xE2, 0x7D, 0xCB, 0x18, 0xCA][..],
            VecElementType::F32,
            16u8,
            X86Reg::Zmm(17),
        ),
        (
            &[0x62, 0xE2, 0x7D, 0xCB, 0x78, 0xCA][..],
            VecElementType::I8,
            64,
            X86Reg::Zmm(17),
        ),
        (
            &[0x62, 0xC2, 0xFD, 0xCB, 0x7C, 0xC9][..],
            VecElementType::I64,
            8,
            X86Reg::Zmm(17),
        ),
        (
            &[0xC4, 0xE2, 0x7D, 0x58, 0xC1][..],
            VecElementType::I32,
            8,
            X86Reg::Ymm(0),
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: actual_elem,
                lanes: actual_lanes,
                ..
            } if actual_elem == elem && actual_lanes == lanes
        )));
        assert!(
            lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    ..
                } if actual_dst == destination
            )) || lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    ..
                } if actual_dst == destination
            ))
        );
    }

    let pair = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0x19, 0xCA]).unwrap();
    let extracted = pair
        .ops
        .iter()
        .filter_map(|op| match op.kind {
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                lane,
                elem: VecElementType::F32,
                ..
            } => Some(lane),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(extracted, (0..16).map(|lane| lane % 2).collect::<Vec<_>>());

    let tuple = lift_single(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x48, 0x08]).unwrap();
    assert_eq!(
        tuple
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4
    );
    assert!(tuple.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 128,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));

    for (bytes, elem, source, destination, source_mask) in [
        (
            &[0x62, 0xE2, 0xFE, 0x48, 0x2A, 0xCF][..],
            VecElementType::I64,
            7,
            X86Reg::Zmm(17),
            0xFF,
        ),
        (
            &[0x62, 0xF2, 0x7E, 0x28, 0x3A, 0xD3][..],
            VecElementType::I32,
            3,
            X86Reg::Ymm(2),
            0xFFFF,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::And {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::K(actual_source))),
                        src2: SrcOperand::Imm(actual_mask),
                        flags: FlagUpdate::None,
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(actual_destination)),
                        elem: actual_elem,
                        ..
                    },
                    ..
                }
            ] if *actual_source == source
                && *actual_mask == source_mask
                && *actual_destination == destination
                && *actual_elem == elem
        ));
    }

    // Intel SDM Table 2-41 marks EVEX.X and EVEX.B ignored when ModRM.r/m
    // selects an opmask register. All four encodings therefore read K1.
    for p0 in [0xF2, 0xD2, 0xB2, 0x92] {
        let lifted = lift_single(&[0x62, p0, 0xFE, 0x08, 0x2A, 0xD1]).unwrap();
        assert!(matches!(
            lifted.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::And {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::VBroadcast {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        ..
                    },
                    ..
                }
            ]
        ));
    }

    for bytes in [
        &[0x62, 0xF2, 0x7D, 0x68, 0x18, 0xCA][..], // EVEX.L'L=3
        &[0x62, 0xF2, 0x7D, 0x88, 0x18, 0xCA][..], // {z} with k0
        &[0x62, 0xF2, 0x7D, 0x58, 0x18, 0xCA][..], // EVEX.b reserved
        &[0x62, 0xF2, 0x75, 0x48, 0x18, 0xCA][..], // vvvv reserved field
        &[0x62, 0xF2, 0x7D, 0x08, 0x19, 0xCA][..], // F32X2 has no VL=128
        &[0x62, 0xF2, 0x7D, 0x48, 0x1A, 0xCA][..], // F32X4 is memory-only
        &[0x62, 0xF2, 0x7D, 0x28, 0x5B, 0x08][..], // I32X8 requires VL=512
        &[0x62, 0xF2, 0x7D, 0x48, 0x7C, 0x08][..], // GPR form rejects memory
        &[0xC4, 0xE2, 0xFD, 0x58, 0xC1][..],       // VEX VPBROADCASTD requires W0
        &[0xC4, 0xE2, 0x79, 0x5A, 0x08][..],       // VEX I128 requires VL=256
        &[0x62, 0xF2, 0xFE, 0x09, 0x2A, 0xC9][..], // mask broadcast has no writemask
        &[0x62, 0xF2, 0x7E, 0x08, 0x2A, 0xC9][..], // MB2Q requires W1
        &[0x62, 0xF2, 0xFE, 0x08, 0x3A, 0xC9][..], // MW2D requires W0
        &[0x62, 0xF2, 0xFE, 0x08, 0x2A, 0x09][..], // mask source is register-only
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved broadcast encoding {bytes:02X?}"
        );
    }
}

fn gpr_broadcast_encoding(
    opcode: u8,
    w: bool,
    ll: u8,
    destination: u8,
    source: u8,
    ignored_x: bool,
) -> [u8; 6] {
    assert!(destination < 32 && source < 16 && ll < 3);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn lift_gpr_broadcasts_cover_all_widths_extensions_and_ignored_x() {
    for (opcode, w, elem) in [
        (0x7A, false, VecElementType::I8),
        (0x7B, false, VecElementType::I16),
        (0x7C, false, VecElementType::I32),
        (0x7C, true, VecElementType::I64),
    ] {
        for (ll, width) in [
            (0u8, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            for destination in [1u8, 9, 17, 25] {
                for source in [0u8, 4, 5, 8, 12, 13, 15] {
                    for ignored_x in [false, true] {
                        let bytes =
                            gpr_broadcast_encoding(opcode, w, ll, destination, source, ignored_x);
                        let lifted = lift_single(&bytes).unwrap_or_else(|error| {
                            panic!("failed to lift {bytes:02X?}: {error:?}")
                        });
                        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                        assert!(
                            lifted.ops.iter().any(|op| matches!(
                                op.kind,
                                OpKind::VBroadcast {
                                    scalar: VReg::Arch(ArchReg::X86(actual_source)),
                                    elem: actual_elem,
                                    lanes,
                                    ..
                                } if actual_source == X86Reg::gpr(source)
                                    && actual_elem == elem
                                    && lanes == width.lanes(elem) as u8
                            )),
                            "{bytes:02X?}: missing exact GPR broadcast"
                        );
                        let expected_destination = match ll {
                            0 => X86Reg::Xmm(destination),
                            1 => X86Reg::Ymm(destination),
                            2 => X86Reg::Zmm(destination),
                            _ => unreachable!(),
                        };
                        assert!(
                            lifted.ops.iter().any(|op| matches!(
                                op.kind,
                                OpKind::VMov {
                                    dst: VReg::Arch(ArchReg::X86(actual_destination)),
                                    width: actual_width,
                                    ..
                                } if actual_destination == expected_destination
                                    && actual_width == width
                            )),
                            "{bytes:02X?}: missing exact destination write"
                        );
                    }
                }
            }
        }
    }
}
