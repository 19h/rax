//! SSE3/AVX floating-point add-sub and horizontal lift contracts.

use super::*;

#[test]
fn lift_sse3_addsub_horizontal_covers_legacy_vex_widths_addresses_and_invalids() {
    for (bytes, expected_op, elem, lanes, expected_dst) in [
        (
            &[0xF2, 0x0F, 0xD0, 0xC1][..],
            X86FpBinaryOp::AddSub,
            VecElementType::F32,
            4,
            None,
        ),
        (
            &[0x66, 0x0F, 0x7C, 0xC1][..],
            X86FpBinaryOp::HorizontalAdd,
            VecElementType::F64,
            2,
            None,
        ),
        (
            &[0xC5, 0xEB, 0x7D, 0xCB][..],
            X86FpBinaryOp::HorizontalSub,
            VecElementType::F32,
            4,
            Some(X86Reg::Xmm(1)),
        ),
        (
            &[0xC4, 0x41, 0x75, 0x7C, 0xCA][..],
            X86FpBinaryOp::HorizontalAdd,
            VecElementType::F64,
            4,
            Some(X86Reg::Ymm(9)),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        let paired = result
            .ops
            .iter()
            .find_map(|op| match op.kind {
                OpKind::X86FpBinary {
                    dst,
                    mask,
                    elem,
                    lanes,
                    op,
                    round,
                    suppress_exceptions,
                    ..
                } => Some((dst, mask, elem, lanes, op, round, suppress_exceptions)),
                _ => None,
            })
            .expect("SSE3 paired FP semantic operation");
        assert_eq!(paired.1, None);
        assert_eq!(paired.2, elem);
        assert_eq!(paired.3, lanes);
        assert_eq!(paired.4, expected_op);
        assert_eq!(paired.5, FpRoundMode::Dynamic);
        assert!(!paired.6);
        if let Some(expected_dst) = expected_dst {
            assert_eq!(paired.0, VReg::Arch(ArchReg::X86(expected_dst)));
        } else {
            assert!(
                matches!(paired.0, VReg::Virtual(_)),
                "legacy compute must trap before architectural writeback"
            );
        }
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::FAdd { .. } | OpKind::FSub { .. }))
        );
    }

    let legacy_mem = lift_single(&[0x66, 0x0F, 0x7D, 0x00]).unwrap();
    assert!(
        legacy_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
    );
    let addr32 = lift_single(&[0x67, 0xC5, 0xFF, 0x7C, 0x54, 0x77, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 7);
    let addr = addr32
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad { addr, .. } => Some((addr, op.x86_hint)),
            _ => None,
        })
        .expect("horizontal-add addr32 memory source");
    super::addr32_assertions::sib(addr.0, Some(X86Reg::Rdi), X86Reg::Rsi, 2, 0x20);
    assert_eq!(addr.1, Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)));
    let paired = addr32
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86FpBinary { .. }))
        .expect("horizontal-add FP consumer");
    assert_eq!(
        paired.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::Repne,
            opcode: 0x7C,
            width: VecWidth::V256,
            w: false,
        })
    );
    assert!(
        !addr32
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    for bytes in [
        &[0x0F, 0x7C, 0xC1][..],
        &[0xF0, 0xF2, 0x0F, 0xD0, 0xC1][..],
        &[0xC5, 0xFC, 0x7C, 0xC1][..],
        &[0x62, 0xF1, 0x7D, 0x08, 0x7C, 0xC1][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
