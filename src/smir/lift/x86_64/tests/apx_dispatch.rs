//! Intel APX Map 4 dispatch-frontier regression tests.

use super::*;

fn assert_map4_needs_modrm(bytes: &[u8]) {
    assert!(matches!(
        lift_single(bytes),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 5,
            need: 6
        })
    ));
}

fn assert_map4_opcode_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("terminal APX #UD form must strictly lift");
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

/// Opcode bytes assigned by Intel APX Architecture Specification revision 7.0:
/// the section 3.1.5 table plus the later-added MOVRS rows in section 6.38. An
/// assigned byte can still be reserved for a particular prefix or ModR/M value;
/// this predicate deliberately classifies only the first dispatch frontier.
fn apx_rev7_map4_opcode_is_assigned(opcode: u8) -> bool {
    matches!(
        opcode,
        0x00..=0x03
            | 0x08..=0x0B
            | 0x10..=0x13
            | 0x18..=0x1B
            | 0x20..=0x24
            | 0x28..=0x2C
            | 0x30..=0x33
            | 0x38..=0x3B
            | 0x40..=0x4F
            | 0x60
            | 0x61
            | 0x65
            | 0x66
            | 0x69
            | 0x6B
            | 0x80
            | 0x81
            | 0x83..=0x85
            | 0x88
            | 0x8A
            | 0x8B
            | 0x8F
            | 0xA5
            | 0xAD
            | 0xAF
            | 0xC0
            | 0xC1
            | 0xD0..=0xD3
            | 0xF0..=0xF2
            | 0xF4..=0xF9
            | 0xFC
            | 0xFE
            | 0xFF
    )
}

#[test]
fn non_target_map4_families_preserve_their_modrm_frontier() {
    // Valid ADC without NF still needs /r.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0xBC, 0x18, 0x11]);

    // A valid APX ADCX still needs /r.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0xBD, 0x18, 0x66]);

    // F2/F3 F8 families remain independent implementation gaps.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7F, 0x08, 0xF8]);
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7E, 0x08, 0xF8]);
}

#[test]
fn profile_disabled_apx_cet_forms_are_terminal_at_the_opcode_frontier() {
    for bytes in [
        &[0x62, 0xF4, 0x7C, 0x08, 0x66][..], // WRSSD
        &[0x62, 0xF4, 0xFC, 0x08, 0x66][..], // WRSSQ
        &[0x62, 0xF4, 0x7D, 0x08, 0x65][..], // WRUSSD
        &[0x62, 0xF4, 0xFD, 0x08, 0x65][..], // WRUSSQ
    ] {
        assert_map4_opcode_ud(bytes);
    }
}

#[test]
fn every_unassigned_apx_map4_opcode_is_terminal_at_the_opcode_frontier() {
    assert_eq!(
        (0..=u8::MAX)
            .filter(|opcode| apx_rev7_map4_opcode_is_assigned(*opcode))
            .count(),
        86,
        "Intel APX revision 7 assigns 86 distinct MAP4 opcode bytes"
    );

    for opcode in 0..=u8::MAX {
        if apx_rev7_map4_opcode_is_assigned(opcode) {
            continue;
        }

        assert_map4_opcode_ud(&[0x62, 0xF4, 0x7C, 0x08, opcode]);
    }
}

#[test]
fn rex2_lea_remains_available_in_legacy_map0() {
    // REX2 extends ordinary legacy-map0 LEA; it does not imply an EVEX MAP4
    // promotion. D5 48 8D 03 encodes LEA R16,[RBX].
    let result = lift_single(&[0xD5, 0x48, 0x8D, 0x03]).expect("strictly lift REX2 LEA");
    assert_eq!(result.bytes_consumed, 4);
    let ops = assert_rex2_guarded_ops(&result, 1);
    assert!(matches!(
        ops,
        [SmirOp {
            kind: OpKind::X86Lea {
                dst,
                addr: Address::Direct(base),
                width: OpWidth::W64,
            },
            ..
        }] if *dst == x86_gpr(16) && *base == x86_gpr(3)
    ));
}

#[test]
fn valid_unimplemented_f8_families_remain_explicit_fallbacks() {
    for bytes in [
        &[0x62, 0xF4, 0x7F, 0x08, 0xF8, 0xC0][..], // URDMSR rax,rax
        &[0x62, 0xF4, 0x7E, 0x08, 0xF8, 0xC0][..], // UWRMSR rax,rax
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Unsupported { .. })
        ));
    }
}
