//! EVEX vector/opmask conversion tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_evex_mask_vector_conversions_cover_elements_widths_high_vectors_and_invalids() {
    for (bytes, elem, width, mask_to_vector, vector, mask) in [
        (
            &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1][..],
            VecElementType::I8,
            VecWidth::V128,
            true,
            X86Reg::Xmm(2),
            X86Reg::K(1),
        ),
        (
            &[0x62, 0xF2, 0xFE, 0x28, 0x28, 0xE3][..],
            VecElementType::I16,
            VecWidth::V256,
            true,
            X86Reg::Ymm(4),
            X86Reg::K(3),
        ),
        (
            &[0x62, 0xE2, 0x7E, 0x48, 0x38, 0xD2][..],
            VecElementType::I32,
            VecWidth::V512,
            true,
            X86Reg::Zmm(18),
            X86Reg::K(2),
        ),
        (
            &[0x62, 0xF2, 0xFE, 0x08, 0x38, 0xD9][..],
            VecElementType::I64,
            VecWidth::V128,
            true,
            X86Reg::Xmm(3),
            X86Reg::K(1),
        ),
        (
            &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xCA][..],
            VecElementType::I8,
            VecWidth::V128,
            false,
            X86Reg::Xmm(2),
            X86Reg::K(1),
        ),
        (
            &[0x62, 0xF2, 0xFE, 0x28, 0x29, 0xDC][..],
            VecElementType::I16,
            VecWidth::V256,
            false,
            X86Reg::Ymm(4),
            X86Reg::K(3),
        ),
        (
            &[0x62, 0xB2, 0x7E, 0x48, 0x39, 0xD2][..],
            VecElementType::I32,
            VecWidth::V512,
            false,
            X86Reg::Zmm(18),
            X86Reg::K(2),
        ),
        (
            &[0x62, 0xF2, 0xFE, 0x08, 0x39, 0xCB][..],
            VecElementType::I64,
            VecWidth::V128,
            false,
            X86Reg::Xmm(3),
            X86Reg::K(1),
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        if mask_to_vector {
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    width: actual_width,
                    ..
                } if actual == vector && actual_width == width
            ));
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::VInsertLane { elem: actual, .. } if actual == elem))
                    .count(),
                width.lanes(elem) as usize
            );
            assert!(lifted.ops.iter().any(|op| {
                op.kind
                    .source_vregs()
                    .contains(&VReg::Arch(ArchReg::X86(mask)))
            }));
        } else {
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    width: OpWidth::W64,
                    ..
                } if actual == mask
            ));
            assert!(lifted.ops.iter().any(|op| {
                op.kind
                    .source_vregs()
                    .contains(&VReg::Arch(ArchReg::X86(vector)))
            }));
        }
    }

    // Intel SDM Table 2-41 defines EVEX.X/B as ignored when ModR/M.r/m
    // encodes a K register. All four encodings therefore select k1.
    for p0 in [0xF2, 0xD2, 0xB2, 0x92] {
        let bytes = [0x62, p0, 0x7E, 0x08, 0x28, 0xD1];
        let lifted = lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert!(lifted.ops.iter().any(|op| {
            op.kind
                .source_vregs()
                .contains(&VReg::Arch(ArchReg::X86(X86Reg::K(1))))
        }));
        assert!(matches!(
            lifted.ops.last().unwrap().kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                width: VecWidth::V128,
                ..
            }
        ));
    }

    for bytes in [
        &[0xC4, 0xE2, 0x7E, 0x28, 0xD1][..],       // EVEX-only
        &[0x62, 0xF2, 0x7E, 0x09, 0x28, 0xD1][..], // E7NM forbids writemasks
        &[0x62, 0xF2, 0x76, 0x08, 0x28, 0xD1][..], // EVEX.vvvv reserved
        &[0x62, 0xF2, 0x7E, 0x18, 0x28, 0xD1][..], // EVEX.b reserved
        &[0x62, 0xF2, 0x7E, 0x68, 0x28, 0xD1][..], // L'L=3
        &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0x11][..], // memory operand
        &[0x62, 0x72, 0x7E, 0x08, 0x29, 0xCA][..], // extended K destination
        &[0x62, 0xE2, 0x7E, 0x08, 0x29, 0xCA][..], // EVEX.R' K destination
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
