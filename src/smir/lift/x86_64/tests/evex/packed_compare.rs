//! Fixed-predicate EVEX packed integer compare lifting tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn fixed_packed_compare_wig_and_fixed_w_frontiers_are_exact() {
    for opcode in [0x64, 0x65, 0x74, 0x75] {
        for w in [false, true] {
            for ll in 0u8..=2 {
                let bytes = [
                    0x62,
                    0xF1,
                    0x75 | if w { 0x80 } else { 0 },
                    (ll << 5) | 0x08,
                    opcode,
                    0xCA,
                ];
                let result =
                    lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            }
        }
    }

    for bytes in [
        &[0x62, 0xF1, 0x75, 0x08, 0x66, 0xCA][..], // VPCMPGTD W0
        &[0x62, 0xF1, 0x75, 0x08, 0x76, 0xCA],     // VPCMPEQD W0
        &[0x62, 0xF2, 0xF5, 0x08, 0x29, 0xCA],     // VPCMPEQQ W1
        &[0x62, 0xF2, 0xF5, 0x08, 0x37, 0xCA],     // VPCMPGTQ W1
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    }

    for bytes in [
        &[0x62, 0xF1, 0xF5, 0x08, 0x66, 0xCA][..], // VPCMPGTD W1
        &[0x62, 0xF1, 0xF5, 0x08, 0x76, 0xCA],     // VPCMPEQD W1
        &[0x62, 0xF2, 0x75, 0x08, 0x29, 0xCA],     // VPCMPEQQ W0
        &[0x62, 0xF2, 0x75, 0x08, 0x37, 0xCA],     // VPCMPGTQ W0
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "reserved fixed packed compare accepted: {bytes:02X?}"
        );
    }
}
