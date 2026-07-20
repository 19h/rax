//! Strict-lift coverage for AMD SVM controls in the non-SVM guest profile.

use super::*;

const SVM_CONTROLS: [(u8, &str); 7] = [
    (0xD8, "VMRUN"),
    (0xDA, "VMLOAD"),
    (0xDB, "VMSAVE"),
    (0xDC, "STGI"),
    (0xDD, "CLGI"),
    (0xDE, "SKINIT"),
    (0xDF, "INVLPGA"),
];

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled SVM control must strictly lift to #UD");
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
fn disabled_svm_controls_strictly_lift_as_exact_invalid_opcode_traps() {
    for (modrm, _) in SVM_CONTROLS {
        assert_ud(&[0x0F, 0x01, modrm]);
    }
}

#[test]
fn disabled_svm_controls_preserve_prefix_and_vendor_fault_equivalence() {
    for (modrm, _) in SVM_CONTROLS {
        for prefix in [
            0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
            0x66, 0x67, // operand/address size
            0x40, 0x48, 0x4F, // representative ordinary REX forms
            0xF2, 0xF3, // repeat prefixes
        ] {
            assert_ud(&[prefix, 0x0F, 0x01, modrm]);
        }

        // REX2 is an Intel APX prefix while these controls are AMD-only. The
        // encoding is #UD whether APX is disabled or enabled, so no dynamic
        // APX feature guard is required for the terminal trap.
        assert_ud(&[0xD5, 0x80, 0x01, modrm]);

        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn disabled_svm_controls_terminate_strict_blocks_without_fallthrough() {
    for (modrm, name) in SVM_CONTROLS {
        let mem = TestMemory::new(0x1000, vec![0x90, 0x0F, 0x01, modrm]);
        let mut lifter = X86_64Lifter::strict();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let block = lifter
            .lift_block(0x1000, &mem, &mut ctx)
            .expect("NOP followed by disabled SVM control must lift");

        assert!(block.ops.is_empty(), "{name}");
        assert!(matches!(
            block.terminator,
            Terminator::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}
