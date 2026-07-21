//! Exhaustive strict-lift coverage for legacy Group 15 (`0F AE`).

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

const COMPLETE_LEGACY_GROUP15_LEADERS: [&[u8]; 22] = [
    &[0x0F, 0xAE],
    &[0x26, 0x0F, 0xAE],
    &[0x2E, 0x0F, 0xAE],
    &[0x36, 0x0F, 0xAE],
    &[0x3E, 0x0F, 0xAE],
    &[0x64, 0x0F, 0xAE],
    &[0x65, 0x0F, 0xAE],
    &[0x66, 0x0F, 0xAE],
    &[0x67, 0x0F, 0xAE],
    &[0xF2, 0x0F, 0xAE],
    &[0xF3, 0x0F, 0xAE],
    &[0x40, 0x0F, 0xAE],
    &[0x41, 0x0F, 0xAE],
    &[0x48, 0x0F, 0xAE],
    &[0x4F, 0x0F, 0xAE],
    &[0x66, 0xF2, 0x0F, 0xAE],
    &[0x66, 0xF3, 0x0F, 0xAE],
    &[0x64, 0xF3, 0x0F, 0xAE],
    &[0xF0, 0x0F, 0xAE],
    &[0xF0, 0xF2, 0x0F, 0xAE],
    &[0xF0, 0xF3, 0x0F, 0xAE],
    &[0xF0, 0x66, 0x0F, 0xAE],
];

fn fence_kind(result: &LiftResult) -> Option<FenceKind> {
    result.ops.last().and_then(|op| match op.kind {
        OpKind::Fence { kind } => Some(kind),
        _ => None,
    })
}

fn complete_modrm_form(leader: &[u8], modrm: u8) -> Vec<u8> {
    let mode = modrm >> 6;
    let rm = modrm & 7;
    let mut bytes = leader.to_vec();
    bytes.push(modrm);
    if mode != 3 && rm == 4 {
        bytes.push(0x24); // scale=1, no index, base=RSP/R12
    }
    match mode {
        0 if rm == 5 => bytes.extend_from_slice(&0x1234_5678_u32.to_le_bytes()),
        1 => bytes.push(0x80),
        2 => bytes.extend_from_slice(&0x89AB_CDEF_u32.to_le_bytes()),
        _ => {}
    }
    bytes
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
fn reserved_prefix_fence_aliases_follow_the_direct_engines_deterministic_policy() {
    // Intel reserves these otherwise-unused prefixes on operandless SSE
    // instructions. The direct engine deterministically ignores them, except
    // where F3 selects CET INCSSP and group /6 selects WAITPKG.
    for leader in [
        &[0x66, 0x0F, 0xAE][..],
        &[0xF2, 0x0F, 0xAE],
        &[0x66, 0xF2, 0x0F, 0xAE],
        &[0x66, 0xD5, 0x80, 0xAE],
        &[0xF2, 0xD5, 0x80, 0xAE],
    ] {
        for rm in 0..8 {
            let mut bytes = leader.to_vec();
            bytes.push(0xE8 | rm);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("direct-policy LFENCE alias must strictly lift: {bytes:02X?}: {error:?}")
            });
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert_eq!(
                fence_kind(&result),
                Some(FenceKind::LoadLoad),
                "{bytes:02X?}"
            );
        }
    }

    for leader in [
        &[0x66, 0x0F, 0xAE][..],
        &[0xF2, 0x0F, 0xAE],
        &[0xF3, 0x0F, 0xAE],
        &[0x66, 0xF2, 0x0F, 0xAE],
        &[0x66, 0xF3, 0x0F, 0xAE],
        &[0x66, 0xD5, 0x80, 0xAE],
        &[0xF2, 0xD5, 0x80, 0xAE],
        &[0xF3, 0xD5, 0x80, 0xAE],
    ] {
        for rm in 0..8 {
            let mut bytes = leader.to_vec();
            bytes.push(0xF8 | rm);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("direct-policy SFENCE alias must strictly lift: {bytes:02X?}: {error:?}")
            });
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert_eq!(
                fence_kind(&result),
                Some(FenceKind::StoreStore),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn every_rex2_payload_field_is_ignored_for_operandless_fence_aliases() {
    for payload in 0x80..=0xFF {
        for (base, expected) in [
            (0xE8, FenceKind::LoadLoad),
            (0xF0, FenceKind::Full),
            (0xF8, FenceKind::StoreStore),
        ] {
            for rm in 0..8 {
                let bytes = [0xD5, payload, 0xAE, base | rm];
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("REX2 fence alias must strictly lift: {bytes:02X?}: {error:?}")
                });
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert_eq!(fence_kind(&result), Some(expected), "{bytes:02X?}");
                assert_eq!(
                    result
                        .ops
                        .iter()
                        .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                        .count(),
                    1,
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn every_reserved_group15_register_slot_is_an_exact_invalid_opcode_trap() {
    for leader in [
        &[0x0F, 0xAE][..],
        &[0x66, 0x0F, 0xAE],
        &[0x67, 0x0F, 0xAE],
        &[0xF2, 0x0F, 0xAE],
        &[0x48, 0x0F, 0xAE],
        &[0xD5, 0x80, 0xAE],
    ] {
        for group in 0..=4 {
            for rm in 0..8 {
                let mut bytes = leader.to_vec();
                bytes.push(0xC0 | group << 3 | rm);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("reserved Group-15 register form must strictly lift: {bytes:02X?}: {error:?}")
                });
                assert_invalid_opcode_trap(&result, bytes.len());
            }
        }
    }
}

#[test]
fn every_complete_legacy_group15_form_avoids_an_interpreter_fallback() {
    for leader in COMPLETE_LEGACY_GROUP15_LEADERS {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = complete_modrm_form(leader, modrm);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("complete Group-15 form entered fallback: {bytes:02X?}: {error:?}")
            });
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
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
