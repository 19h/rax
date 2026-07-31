//! Packed integer minimum/maximum lift tests.

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_packed_minmax_covers_signedness_widths_masks_broadcasts_and_invalids() {
    let shapes = [
        (0x38, VecElementType::I8, VLaneOp::Min, true),
        (0x39, VecElementType::I32, VLaneOp::Min, true),
        (0x3A, VecElementType::I16, VLaneOp::Min, false),
        (0x3B, VecElementType::I32, VLaneOp::Min, false),
        (0x3C, VecElementType::I8, VLaneOp::Max, true),
        (0x3D, VecElementType::I32, VLaneOp::Max, true),
        (0x3E, VecElementType::I16, VLaneOp::Max, false),
        (0x3F, VecElementType::I32, VLaneOp::Max, false),
    ];
    for (opcode, elem, lane_op, signed) in shapes {
        let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert_eq!(legacy.ops.len(), 1);
        assert!(matches!(
            (&legacy.ops[0].kind, legacy.ops[0].x86_hint),
            (
                OpKind::VLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: actual_elem,
                    lanes,
                    op: actual_op,
                    signed: actual_signed,
                    set_ovf: false,
                },
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                })
            ) if *actual_elem == elem
                && *lanes == VecWidth::V128.lanes(elem) as u8
                && *actual_op == lane_op
                && *actual_signed == signed
                && actual_opcode == opcode
        ));

        for (p2, width, dst, src1, src2) in [
            (
                0x71,
                VecWidth::V128,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                0x75,
                VecWidth::V256,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
        ] {
            let vex = lift_single(&[0xC4, 0xE2, p2, opcode, 0xC2]).unwrap();
            assert_eq!(vex.ops.len(), 1);
            assert!(matches!(
                (&vex.ops[0].kind, vex.ops[0].x86_hint),
                (
                    OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        src1: VReg::Arch(ArchReg::X86(actual_src1)),
                        src2: VReg::Arch(ArchReg::X86(actual_src2)),
                        elem: actual_elem,
                        lanes,
                        op: actual_op,
                        signed: actual_signed,
                        set_ovf: false,
                    },
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: actual_opcode,
                        width: actual_width,
                        ..
                    })
                ) if *actual_dst == dst
                    && *actual_src1 == src1
                    && *actual_src2 == src2
                    && *actual_elem == elem
                    && *lanes == width.lanes(elem) as u8
                    && *actual_op == lane_op
                    && *actual_signed == signed
                    && actual_opcode == opcode
                    && actual_width == width
            ));
        }
    }

    let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x38, 0x00]).unwrap();
    let alignment = legacy_mem
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = legacy_mem
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .unwrap();
    assert!(alignment < load);

    let vex_mem = lift_single(&[0xC4, 0xE2, 0x75, 0x3A, 0x00]).unwrap();
    assert!(vex_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !vex_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // EVEX.W selects dword/qword for odd opcodes and preserves all three
    // high-register extension paths.
    let high = lift_single(&[0x62, 0xA2, 0xF5, 0x40, 0x39, 0xC2]).unwrap();
    assert_eq!(high.ops.len(), 1);
    assert!(matches!(
        (&high.ops[0].kind, high.ops[0].x86_hint),
        (
            OpKind::VLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                elem: VecElementType::I64,
                lanes: 8,
                op: VLaneOp::Min,
                signed: true,
                set_ovf: false,
            },
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x39,
                width: VecWidth::V512,
                w: true,
            })
        )
    ));

    let qword_broadcast = lift_single(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x40, 0x01]).unwrap();
    assert!(qword_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset {
                offset: 8,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B8,
            ..
        }
    )));
    assert_eq!(
        qword_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    let dword_broadcast = lift_single(&[0x62, 0xF2, 0x75, 0xD9, 0x39, 0x40, 0x01]).unwrap();
    assert!(dword_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset {
                offset: 4,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B4,
            ..
        }
    )));
    assert_eq!(
        dword_broadcast
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
        1
    );

    let word_full = lift_single(&[0x62, 0xF2, 0x75, 0x49, 0x3E, 0x40, 0x01]).unwrap();
    assert!(word_full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));
    assert_eq!(
        word_full
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );

    // W is ignored for byte/word forms.
    assert!(lift_single(&[0x62, 0xF2, 0xF5, 0x48, 0x38, 0xC2]).is_ok());
    assert!(lift_single(&[0x62, 0xF2, 0xF5, 0x48, 0x3E, 0xC2]).is_ok());
    for bytes in [
        &[0x0F, 0x38, 0x38, 0xC1][..],
        &[0xF0, 0x66, 0x0F, 0x38, 0x39, 0xC1][..],
        &[0xC4, 0xE2, 0x74, 0x38, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0xC8, 0x38, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0x58, 0x38, 0x00][..],
        &[0x62, 0xF2, 0x75, 0x58, 0x3A, 0x00][..],
        &[0x62, 0xF2, 0x75, 0x58, 0x39, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0x68, 0x39, 0xC2][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid packed min/max encoding accepted: {bytes:02X?}",
        );
    }
}
