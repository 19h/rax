//! EVEX exact packed I32-to-F64 conversion lift coverage.

use super::*;

#[allow(clippy::too_many_arguments)]
fn encoding(
    signed: bool,
    ll: u8,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    embedded_control: bool,
    memory: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if !memory && source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if !memory && source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7E,
        (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(embedded_control) << 4) | 0x08 | mask,
        if signed { 0xE6 } else { 0x7A },
        (if memory { 0 } else { 0xC0 }) | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn ignored_embedded_rounding_lifts_all_24_scanner_cells_as_exact_512_bit_conversions() {
    let mut lifted = 0usize;
    for signed in [false, true] {
        for ll in 0..=3 {
            for (mask, zeroing) in [(0u8, false), (1, false), (1, true)] {
                let bytes = encoding(signed, ll, 17, 18, mask, zeroing, true, false);
                let result =
                    lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                let op = result.ops.last().unwrap();
                assert!(
                    matches!(
                        op.kind,
                        OpKind::X86PackedIntToFp {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                            src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(18))),
                            mask: actual_mask,
                            int_elem: VecElementType::I32,
                            fp_elem: VecElementType::F64,
                            signed: actual_signed,
                            lanes: 8,
                            src_width: VecWidth::V256,
                            dst_width: VecWidth::V512,
                            mask_zeroing: actual_zeroing,
                            zero_upper: true,
                            round: FpRoundMode::Dynamic,
                            suppress_exceptions: false,
                        } if actual_signed == signed
                            && actual_zeroing == zeroing
                            && actual_mask
                                == (mask != 0)
                                    .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(mask))))
                    ),
                    "{bytes:02X?}: {:?}",
                    op.kind
                );
                assert!(matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::Rep,
                        opcode,
                        width: VecWidth::V512,
                        w: false,
                    }) if opcode == if signed { 0xE6 } else { 0x7A }
                ));
                lifted += 1;
            }
        }
    }
    assert_eq!(lifted, 24);
}

#[test]
fn ordinary_ll3_and_memory_broadcast_ll3_remain_invalid() {
    for signed in [false, true] {
        for bytes in [
            encoding(signed, 3, 1, 2, 0, false, false, false),
            encoding(signed, 3, 1, 0, 1, false, true, true),
        ] {
            assert!(
                matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                "{bytes:02X?}"
            );
        }
    }
}
