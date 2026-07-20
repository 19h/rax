//! Strict-lift coverage for VMX controls in the non-VMX guest profile.

use super::*;

const VMX_CONTROLS: [(u8, &str); 4] = [
    (0xC2, "VMLAUNCH"),
    (0xC3, "VMRESUME"),
    (0xC4, "VMXOFF"),
    (0xD4, "VMFUNC"),
];

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled VMX control must strictly lift to #UD");
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
fn disabled_vmx_controls_strictly_lift_as_exact_invalid_opcode_traps() {
    for (modrm, name) in VMX_CONTROLS {
        assert_ud(&[0x0F, 0x01, modrm]);
        assert_eq!(
            lift_single(&[0x0F, 0x01, modrm])
                .expect("exact VMX trap")
                .bytes_consumed,
            3,
            "{name}"
        );
    }
}

#[test]
fn disabled_vmx_controls_preserve_prefix_and_apx_fault_equivalence() {
    for (modrm, _) in VMX_CONTROLS {
        for prefix in [
            0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
            0x66, 0x67, // operand/address size
            0x40, 0x48, 0x4F, // representative ordinary REX forms
            0xF2, 0xF3, // repeat prefixes / VMFUNC's invalid NP aliases
        ] {
            assert_ud(&[prefix, 0x0F, 0x01, modrm]);
        }

        // With APX disabled, the direct decoder faults on REX2 first. With APX
        // enabled, the absent VMX execution state faults next. Both paths are
        // the same precise #UD and therefore need no dynamic feature guard.
        assert_ud(&[0xD5, 0x80, 0x01, modrm]);

        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn disabled_vmx_controls_terminate_strict_blocks_without_fallthrough() {
    for (modrm, name) in VMX_CONTROLS {
        let mem = TestMemory::new(0x1000, vec![0x90, 0x0F, 0x01, modrm]);
        let mut lifter = X86_64Lifter::strict();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let block = lifter
            .lift_block(0x1000, &mem, &mut ctx)
            .expect("NOP followed by disabled VMX control must lift");

        assert!(block.ops.is_empty(), "{name}");
        assert!(matches!(
            block.terminator,
            Terminator::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}
