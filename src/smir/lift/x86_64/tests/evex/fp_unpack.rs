//! Legacy, VEX, and EVEX floating-point unpack lift tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_legacy_vex_evex_fp_unpack_family() {
    for (bytes, elem, width, high) in [
        (
            &[0x0F, 0x14, 0xCA][..],
            VecElementType::F32,
            VecWidth::V128,
            false,
        ),
        (
            &[0x66, 0x0F, 0x15, 0x18][..],
            VecElementType::F64,
            VecWidth::V128,
            true,
        ),
        (
            &[0xC5, 0xEC, 0x14, 0xCB][..],
            VecElementType::F32,
            VecWidth::V256,
            false,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        if bytes[0] == 0xC5 {
            assert!(matches!(
                lifted.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VInterleave {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                        elem: VecElementType::F32,
                        lanes: 8,
                        block_lanes: 4,
                        high: false,
                    },
                    x86_hint: Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::None,
                        opcode: 0x14,
                        width: VecWidth::V256,
                        w: false,
                    }),
                    ..
                }]
            ));
        } else {
            let expected_first = if high { width.lanes(elem) as u8 / 2 } else { 0 };
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Mov { src: SrcOperand::Imm(index), .. }
                    if index == i64::from(expected_first)
            )));
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VShuffle { elem: actual, lanes, .. }
                    if actual == elem && lanes == width.lanes(elem) as u8
            )));
        }
    }

    let evex = lift_single(&[0x62, 0xA1, 0xED, 0x83, 0x15, 0xCB]).unwrap();
    assert!(evex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VShuffle {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(19)))),
            elem: VecElementType::F64,
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xE1, 0x6C, 0x53, 0x14, 0x48, 0x10]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 64, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(
        !broadcast
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    for bytes in [
        &[0xF3, 0x0F, 0x14, 0xC1][..],
        &[0xF0, 0x66, 0x0F, 0x15, 0xC1][..],
        &[0x62, 0xF1, 0x6C, 0x18, 0x14, 0xC1][..],
        &[0x62, 0xF1, 0xEC, 0x08, 0x14, 0xC1][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
