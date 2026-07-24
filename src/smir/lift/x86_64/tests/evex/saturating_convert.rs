//! AVX10.2 MAP5 saturating-conversion lifting tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lifts_avx10_2_saturating_conversion_register_shapes() {
    for (
        bytes,
        fp_elem,
        int_elem,
        src_width,
        width,
        encoded_width,
        pp,
        signed,
        truncate,
        round,
        suppress_exceptions,
    ) in [
        (
            &[0x62, 0xF5, 0x7D, 0x08, 0x68, 0xCA][..],
            VecElementType::F32,
            VecElementType::I8,
            VecWidth::V128,
            VecWidth::V128,
            VecWidth::V128,
            X86SsePrefix::OpSize,
            true,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x28, 0x6A, 0xCA][..],
            VecElementType::F32,
            VecElementType::I8,
            VecWidth::V256,
            VecWidth::V256,
            VecWidth::V256,
            X86SsePrefix::OpSize,
            false,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x48, 0x6D, 0xCA][..],
            VecElementType::F64,
            VecElementType::I64,
            VecWidth::V512,
            VecWidth::V512,
            VecWidth::V512,
            X86SsePrefix::OpSize,
            true,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFC, 0x08, 0x6D, 0xCA][..],
            VecElementType::F64,
            VecElementType::I32,
            VecWidth::V128,
            VecWidth::V64,
            VecWidth::V128,
            X86SsePrefix::None,
            true,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x48, 0x6C, 0xCA][..],
            VecElementType::F32,
            VecElementType::I32,
            VecWidth::V512,
            VecWidth::V512,
            VecWidth::V512,
            X86SsePrefix::None,
            false,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x28, 0x6D, 0xCA][..],
            VecElementType::F32,
            VecElementType::I64,
            VecWidth::V128,
            VecWidth::V256,
            VecWidth::V256,
            X86SsePrefix::OpSize,
            true,
            true,
            FpRoundMode::RoundTowardZero,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x08, 0x69, 0xCA][..],
            VecElementType::F32,
            VecElementType::I8,
            VecWidth::V128,
            VecWidth::V128,
            VecWidth::V128,
            X86SsePrefix::OpSize,
            true,
            false,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x38, 0x6B, 0xCA][..],
            VecElementType::F32,
            VecElementType::I8,
            VecWidth::V512,
            VecWidth::V512,
            VecWidth::V512,
            X86SsePrefix::OpSize,
            false,
            false,
            FpRoundMode::RoundDown,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VCvtFpToIntSat {
                    dst,
                    src,
                    mask: None,
                    fp_elem: actual_fp,
                    int_elem: actual_int,
                    width: actual_width,
                    signed: actual_signed,
                    truncate: actual_truncate,
                    round: actual_round,
                    zeroing: false,
                    suppress_exceptions: actual_suppress,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map5,
                    pp: actual_pp,
                    width: hint_width,
                    ..
                }),
                ..
            }] if *actual_fp == fp_elem
                && *actual_int == int_elem
                && *actual_width == width
                && *hint_width == encoded_width
                && *actual_pp == pp
                && *actual_signed == signed
                && *actual_truncate == truncate
                && *actual_round == round
                && *actual_suppress == suppress_exceptions
                && *dst == match width {
                    VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    VecWidth::V64 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                }
                && *src == match src_width {
                    VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    VecWidth::V64 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                }
        ));
    }

    let extended = lift_single(&[0x62, 0xA5, 0xFD, 0xCB, 0x6C, 0xCA]).unwrap();
    assert!(matches!(
        extended.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VCvtFpToIntSat {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                fp_elem: VecElementType::F64,
                int_elem: VecElementType::I64,
                width: VecWidth::V512,
                signed: false,
                truncate: true,
                round: FpRoundMode::RoundTowardZero,
                zeroing: true,
                suppress_exceptions: false,
            },
            ..
        }]
    ));

    for bytes in [
        &[0x62, 0xF5, 0x7D, 0x18, 0x68, 0xCA][..],
        &[0x62, 0xF5, 0xFD, 0x18, 0x6D, 0xCA][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VCvtFpToIntSat {
                    width: VecWidth::V512,
                    suppress_exceptions: true,
                    ..
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    width: VecWidth::V512,
                    ..
                }),
                ..
            }]
        ));
    }

    for (p2, round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        let lifted = lift_single(&[0x62, 0xF5, 0x7D, p2, 0x69, 0xCA]).unwrap();
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VCvtFpToIntSat {
                    width: VecWidth::V512,
                    truncate: false,
                    round: actual_round,
                    suppress_exceptions: true,
                    ..
                },
                ..
            }] if *actual_round == round
        ));
    }
}

#[test]
fn lifts_avx10_2_saturating_conversion_memory_fault_suppression_and_tuples() {
    let full = lift_single(&[0x62, 0xF5, 0x7D, 0x48, 0x68, 0x48, 0x01]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: VecWidth::V512,
            ..
        }
    )));

    let narrowing = lift_single(&[0x62, 0xF5, 0xFC, 0x48, 0x6D, 0x48, 0x01]).unwrap();
    assert!(narrowing.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(matches!(
        narrowing.ops.last().map(|op| &op.kind),
        Some(OpKind::VCvtFpToIntSat {
            fp_elem: VecElementType::F64,
            int_elem: VecElementType::I32,
            width: VecWidth::V256,
            ..
        })
    ));

    let widening = lift_single(&[0x62, 0xF5, 0x7D, 0x48, 0x6D, 0x48, 0x01]).unwrap();
    assert!(widening.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset {
                offset: 32,
                disp_size: DispSize::Disp8,
                ..
            },
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(matches!(
        widening.ops.last().map(|op| &op.kind),
        Some(OpKind::VCvtFpToIntSat {
            fp_elem: VecElementType::F32,
            int_elem: VecElementType::I64,
            width: VecWidth::V512,
            ..
        })
    ));

    let masked_full = lift_single(&[0x62, 0xF5, 0x7D, 0x4A, 0x68, 0x48, 0x01]).unwrap();
    assert_eq!(
        masked_full
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
        16
    );
    assert!(
        !masked_full
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::VLoad { .. }))
    );

    let broadcast = lift_single(&[0x62, 0xF5, 0x7D, 0x58, 0x68, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                offset: 4,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));

    let masked_broadcast = lift_single(&[0x62, 0xF5, 0xFD, 0x5A, 0x6D, 0x48, 0x01]).unwrap();
    assert_eq!(
        masked_broadcast
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
        1,
        "a broadcast operand is one architectural memory read"
    );
    assert!(masked_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset {
                offset: 8,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));

    let rounded_broadcast = lift_single(&[0x62, 0xF5, 0x7D, 0x58, 0x6B, 0x48, 0x01]).unwrap();
    assert!(matches!(
        rounded_broadcast.ops.last().map(|op| &op.kind),
        Some(OpKind::VCvtFpToIntSat {
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        })
    ));
}

#[test]
fn rejects_reserved_avx10_2_saturating_conversion_encodings() {
    for bytes in [
        &[0x62, 0xF5, 0x75, 0x48, 0x68, 0xCA][..], // EVEX.vvvv
        &[0x62, 0xF5, 0x7D, 0x40, 0x68, 0xCA][..], // EVEX.V'
        &[0x62, 0xF5, 0x7D, 0x68, 0x68, 0xCA][..], // L'L=3
        &[0x62, 0xF5, 0x7D, 0xC8, 0x68, 0xCA][..], // {z} with k0
        &[0x62, 0xF5, 0x7D, 0x38, 0x68, 0xCA][..], // register b=1, L'L!=0
        &[0x62, 0xF5, 0x7D, 0x68, 0x69, 0xCA][..], // non-ER L'L=3
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "reserved saturation-conversion encoding accepted: {bytes:02X?}"
        );
    }

    // Other mandatory-prefix assignments are distinct AVX10.2 scalar or BF16
    // conversion families and must not be misdecoded as these packed forms.
    for bytes in [
        &[0x62, 0xF5, 0x7C, 0x48, 0x68, 0xCA][..],
        &[0x62, 0xF5, 0x7E, 0x08, 0x6D, 0xCA][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Unsupported { .. })
        ));
    }
}
