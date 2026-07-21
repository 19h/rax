//! Strict-lift coverage for the r/m-ignored Group 15 fence encodings.

use super::*;

const NO_MANDATORY_PREFIX_LEADERS: [&[u8]; 12] = [
    &[0x0F, 0xAE],
    &[0x26, 0x0F, 0xAE],
    &[0x2E, 0x0F, 0xAE],
    &[0x36, 0x0F, 0xAE],
    &[0x3E, 0x0F, 0xAE],
    &[0x64, 0x0F, 0xAE],
    &[0x65, 0x0F, 0xAE],
    &[0x67, 0x0F, 0xAE],
    &[0x40, 0x0F, 0xAE],
    &[0x48, 0x0F, 0xAE],
    &[0x4F, 0x0F, 0xAE],
    &[0xD5, 0x80, 0xAE],
];

fn fence_kind(result: &LiftResult) -> Option<FenceKind> {
    result.ops.last().and_then(|op| match op.kind {
        OpKind::Fence { kind } => Some(kind),
        _ => None,
    })
}

#[test]
fn every_documented_fence_alias_and_non_mandatory_prefix_class_lifts() {
    for leader in NO_MANDATORY_PREFIX_LEADERS {
        for (base, expected) in [
            (0xE8, FenceKind::LoadLoad),
            (0xF0, FenceKind::Full),
            (0xF8, FenceKind::StoreStore),
        ] {
            for rm in 0..8 {
                let mut bytes = leader.to_vec();
                bytes.push(base | rm);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("documented fence alias must strictly lift: {bytes:02X?}: {error:?}")
                });

                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert_eq!(fence_kind(&result), Some(expected), "{bytes:02X?}");
                assert!(result.ops.last().unwrap().is_jit_safe(), "{bytes:02X?}");
                assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
                assert!(result.branch_targets.is_empty(), "{bytes:02X?}");

                let apx_guards = result
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                    .count();
                assert_eq!(
                    apx_guards,
                    usize::from(leader == [0xD5, 0x80, 0xAE]),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn cet_incssp_aliases_are_terminal_ud_not_lfence() {
    for rm in 0..8 {
        let bytes = [0xF3, 0x0F, 0xAE, 0xE8 | rm];
        let result = lift_single(&bytes).expect("disabled INCSSP form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}

#[test]
fn waitpkg_mandatory_prefix_forms_are_never_mfence() {
    for leader in [
        &[0x66, 0x0F, 0xAE][..],
        &[0xF2, 0x0F, 0xAE],
        &[0xF3, 0x0F, 0xAE],
    ] {
        for rm in 0..8 {
            let mut bytes = leader.to_vec();
            bytes.push(0xF0 | rm);
            match lift_single(&bytes) {
                Ok(result) => assert!(
                    result
                        .ops
                        .iter()
                        .all(|op| !matches!(op.kind, OpKind::Fence { .. })),
                    "WAITPKG form was misclassified as MFENCE: {bytes:02X?}"
                ),
                Err(LiftError::Unsupported { .. }) => {}
                other => panic!("unexpected WAITPKG classification for {bytes:02X?}: {other:?}"),
            }
        }
    }
}

#[test]
fn fence_aliases_remain_ordered_through_strict_block_lifting() {
    let bytes = [
        0x0F, 0xAE, 0xEF, // LFENCE alias
        0x0F, 0xAE, 0xF7, // MFENCE alias
        0x0F, 0xAE, 0xFF, // SFENCE alias
        0xF4,
    ];
    let mem = TestMemory::new(0x1000, bytes.to_vec());
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &mem, &mut ctx)
        .expect("all fence aliases must remain in one strict block");

    assert_eq!(
        block
            .ops
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Fence { kind } => Some(kind),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [FenceKind::LoadLoad, FenceKind::Full, FenceKind::StoreStore]
    );
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::Halt
        }
    ));
}
