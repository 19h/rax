//! AArch64 barrier encodings.

use super::*;

#[test]
fn instruction_serialize_lowers_to_dsb_sy_followed_by_isb_sy() {
    let code = lower_single_op(OpKind::Fence {
        kind: FenceKind::InstructionSerialize,
    });
    let expected = [
        0x9F, 0x3F, 0x03, 0xD5, // DSB SY
        0xDF, 0x3F, 0x03, 0xD5, // ISB SY
    ];
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "missing DSB SY; ISB SY sequence: {code:02X?}"
    );
}

#[test]
fn existing_aarch64_fence_encodings_remain_exact_after_split() {
    for (kind, expected) in [
        (FenceKind::ISync, 0xD503_3FDFu32),
        (FenceKind::DSync, 0xD503_3F9F),
        (FenceKind::Full, 0xD503_3F9F),
        (FenceKind::LoadLoad, 0xD503_3FBF),
        (FenceKind::LoadStore, 0xD503_3FBF),
        (FenceKind::StoreLoad, 0xD503_3FBF),
        (FenceKind::StoreStore, 0xD503_3FBF),
    ] {
        let code = lower_single_op(OpKind::Fence { kind });
        let expected = expected.to_le_bytes();
        assert!(
            code.windows(4).any(|window| window == expected),
            "{kind:?}: missing {expected:02X?} in {code:02X?}"
        );
    }
}
