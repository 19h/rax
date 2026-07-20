//! Strict-lift coverage for Intel SGX root instructions in the non-SGX profile.

use super::*;

const SGX_ROOTS: [(u8, &str); 3] = [(0xC0, "ENCLV"), (0xCF, "ENCLS"), (0xD7, "ENCLU")];

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled SGX root must strictly lift to #UD");
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(result.ops.is_empty(), "{bytes:02X?}");
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}

#[test]
fn disabled_sgx_roots_strictly_lift_as_exact_invalid_opcode_traps() {
    for (modrm, _) in SGX_ROOTS {
        assert_ud(&[0x0F, 0x01, modrm]);
    }
}

#[test]
fn disabled_sgx_roots_preserve_prefix_and_feature_fault_equivalence() {
    for (modrm, _) in SGX_ROOTS {
        for prefix in [
            0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
            0x67, // address size
            0x40, 0x48, 0x4F, // representative ordinary REX forms
            0x66, 0xF2, 0xF3, // explicitly invalid SGX prefixes
        ] {
            assert_ud(&[prefix, 0x0F, 0x01, modrm]);
        }

        // SGX remains absent whether APX is disabled or enabled, so REX2
        // cannot make any of these fixed roots executable in RAX's profile.
        assert_ud(&[0xD5, 0x80, 0x01, modrm]);

        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn disabled_sgx_roots_terminate_strict_blocks_without_fallthrough() {
    for (modrm, name) in SGX_ROOTS {
        let mem = TestMemory::new(0x1000, vec![0x90, 0x0F, 0x01, modrm]);
        let mut lifter = X86_64Lifter::strict();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let block = lifter
            .lift_block(0x1000, &mem, &mut ctx)
            .expect("NOP followed by disabled SGX root must lift");

        assert!(block.ops.is_empty(), "{name}");
        assert!(matches!(
            block.terminator,
            Terminator::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}
