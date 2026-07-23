//! Legacy/VEX/EVEX scalar integer/floating-point conversion lift coverage.

use super::*;

#[test]
fn lift_legacy_vex_evex_scalar_fp_to_integer_conversions() {
    for (bytes, dst, elem, width, truncate) in [
        (
            &[0xF3, 0x0F, 0x2D, 0xC1][..],
            X86Reg::Rax,
            VecElementType::F32,
            OpWidth::W32,
            false,
        ),
        (
            &[0xF2, 0x48, 0x0F, 0x2C, 0xC1][..],
            X86Reg::Rax,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
        (
            &[0xC4, 0x61, 0xFA, 0x2D, 0xC9][..],
            X86Reg::R9,
            VecElementType::F32,
            OpWidth::W64,
            false,
        ),
        (
            &[0xC4, 0x61, 0xFB, 0x2C, 0xD1][..],
            X86Reg::R10,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                elem: actual_elem,
                int_width: actual_width,
                truncate: actual_truncate,
                ..
            } if actual_dst == dst && actual_elem == elem && actual_width == width
                && actual_truncate == truncate
        ));
    }

    let memory = lift_single(&[0xF2, 0x0F, 0x2D, 0x00]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            src: VReg::Virtual(_),
            elem: VecElementType::F64,
            ..
        }
    ));

    let evex_high = lift_single(&[0x62, 0x31, 0xFE, 0x08, 0x2D, 0xD1]).unwrap();
    assert!(matches!(
        evex_high.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R10)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            elem: VecElementType::F32,
            int_width: OpWidth::W64,
            signed: true,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        }
    ));

    for (bytes, elem, width, round, sae) in [
        (
            &[0x62, 0xF1, 0xEE, 0x78, 0x7B, 0xC8][..],
            VecElementType::F32,
            OpWidth::W64,
            FpRoundMode::RoundTowardZero,
            true,
        ),
        (
            &[0x62, 0xF1, 0xEF, 0x38, 0x7B, 0xC8][..],
            VecElementType::F64,
            OpWidth::W64,
            FpRoundMode::RoundDown,
            true,
        ),
        (
            &[0x62, 0xF1, 0x6F, 0x58, 0x7B, 0xC8][..],
            VecElementType::F64,
            OpWidth::W32,
            FpRoundMode::Dynamic,
            false,
        ),
    ] {
        let unsigned = lift_single(bytes).unwrap();
        assert!(matches!(
            unsigned.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                elem: actual_elem,
                int_width,
                signed: false,
                round: actual_round,
                suppress_exceptions,
                zero_upper: true,
            } if actual_elem == elem
                && int_width == width
                && actual_round == round
                && suppress_exceptions == sae
        ));
    }

    let unsigned_memory = lift_single(&[0x62, 0xF1, 0x6E, 0x08, 0x7B, 0x48, 0x7F]).unwrap();
    assert!(unsigned_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 508, .. },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
            ..
        }
    )));

    let evex_er = lift_single(&[0x62, 0xF1, 0x7E, 0x38, 0x2D, 0xC3]).unwrap();
    assert!(matches!(
        evex_er.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            elem: VecElementType::F32,
            signed: true,
            truncate: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
            ..
        }
    ));

    for (bytes, elem, width, truncate, round, sae) in [
        (
            &[0x62, 0xF1, 0x7E, 0x08, 0x79, 0xC3][..],
            VecElementType::F32,
            OpWidth::W32,
            false,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF1, 0xFF, 0x18, 0x78, 0xC3][..],
            VecElementType::F64,
            OpWidth::W64,
            true,
            FpRoundMode::RoundTowardZero,
            true,
        ),
    ] {
        let unsigned = lift_single(bytes).unwrap();
        assert!(matches!(
            unsigned.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                elem: actual_elem,
                int_width,
                signed: false,
                truncate: actual_truncate,
                round: actual_round,
                suppress_exceptions,
                ..
            } if actual_elem == elem
                && int_width == width
                && actual_truncate == truncate
                && actual_round == round
                && suppress_exceptions == sae
        ));
    }

    let unsigned_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x08, 0x79, 0x40, 0x7F]).unwrap();
    assert!(unsigned_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 508, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0x2D, 0xC1][..],       // LOCK
        &[0xC5, 0xF2, 0x2D, 0xC1][..],             // reserved VEX.vvvv
        &[0xC5, 0xFA, 0x79, 0xC1][..],             // unsigned is EVEX-only
        &[0x62, 0xE1, 0x7E, 0x08, 0x2D, 0xC1][..], // EVEX GPR R'
        &[0x62, 0xF1, 0x7E, 0x18, 0x79, 0x00][..], // EVEX.b memory
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_evex_scalar_integer_to_fp_conversions() {
    for (bytes, dst, merge, src, elem, width, zero_upper) in [
        (
            &[0xF3, 0x0F, 0x2A, 0xC8][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(1),
            X86Reg::Rax,
            VecElementType::F32,
            OpWidth::W32,
            false,
        ),
        (
            &[0xF2, 0x48, 0x0F, 0x2A, 0xC8][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(1),
            X86Reg::Rax,
            VecElementType::F64,
            OpWidth::W64,
            false,
        ),
        (
            &[0xC4, 0xC1, 0xEA, 0x2A, 0xC9][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            X86Reg::R9,
            VecElementType::F32,
            OpWidth::W64,
            true,
        ),
        (
            &[0xC4, 0xC1, 0xE3, 0x2A, 0xD2][..],
            X86Reg::Xmm(2),
            X86Reg::Xmm(3),
            X86Reg::R10,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                merge: VReg::Arch(ArchReg::X86(actual_merge)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                elem: actual_elem,
                int_width: actual_width,
                signed: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: actual_zero,
            } if actual_dst == dst && actual_merge == merge && actual_src == src
                && actual_elem == elem && actual_width == width && actual_zero == zero_upper
        ));
    }

    let evex_high = lift_single(&[0x62, 0xC1, 0xFE, 0x00, 0x2A, 0xCA]).unwrap();
    assert!(matches!(
        evex_high.ops.last().unwrap().kind,
        OpKind::X86IntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
            src: VReg::Arch(ArchReg::X86(X86Reg::R10)),
            elem: VecElementType::F32,
            int_width: OpWidth::W64,
            signed: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        }
    ));

    for (bytes, width, offset) in [
        (
            &[0x62, 0xE1, 0x7E, 0x00, 0x2A, 0x48, 0x10][..],
            MemWidth::B4,
            64i64,
        ),
        (
            &[0x62, 0xE1, 0xFE, 0x00, 0x2A, 0x48, 0x08][..],
            MemWidth::B8,
            64i64,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset: actual_offset,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: actual_width,
                sign: SignExtend::Sign,
                ..
            } if actual_offset == offset && actual_width == width
        )));
    }

    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0x2A, 0xC8][..],       // LOCK
        &[0xC5, 0xEA, 0x7B, 0xC8][..],             // unsigned is EVEX-only
        &[0x62, 0xF1, 0x6E, 0x18, 0x7B, 0x00][..], // EVEX.b memory
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
