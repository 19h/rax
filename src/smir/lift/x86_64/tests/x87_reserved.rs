//! Exhaustive strict-lifting coverage for reserved x87 escape-map cells.

use super::*;

const LEGACY_PREFIX_LEADERS: [&[u8]; 19] = [
    &[],
    &[0x26],
    &[0x2E],
    &[0x36],
    &[0x3E],
    &[0x64],
    &[0x65],
    &[0x66],
    &[0x67],
    &[0xF2],
    &[0xF3],
    &[0x40],
    &[0x41],
    &[0x48],
    &[0x4F],
    &[0x66, 0xF2],
    &[0x66, 0xF3],
    &[0x64, 0xF3],
    &[0xD5, 0x00],
];

const VALID_UNIMPLEMENTED_D9: [u8; 8] = [0xF0, 0xF1, 0xF2, 0xF3, 0xF9, 0xFB, 0xFE, 0xFF];

fn complete_x87_form(leader: &[u8], opcode: u8, modrm: u8) -> Vec<u8> {
    let mode = modrm >> 6;
    let rm = modrm & 7;
    let mut bytes = leader.to_vec();
    bytes.extend_from_slice(&[opcode, modrm]);
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

/// Intel SDM Tables A-7 through A-22 leave these cells blank. The direct
/// engine deterministically injects #UD for them; its accepted DD/DE/DF legacy
/// aliases and DB obsolete no-operations are therefore deliberately absent.
fn is_direct_rejected_reserved_cell(opcode: u8, modrm: u8) -> bool {
    if modrm < 0xC0 {
        let group = (modrm >> 3) & 7;
        return matches!((opcode, group), (0xD9, 1) | (0xDB, 4 | 6) | (0xDD, 5));
    }

    match opcode {
        0xD9 => matches!(modrm, 0xD1..=0xDF | 0xE2..=0xE3 | 0xE6..=0xE7 | 0xEF),
        0xDA => matches!(modrm, 0xE0..=0xE8 | 0xEA..=0xFF),
        0xDB => matches!(modrm, 0xE5..=0xE7 | 0xF8..=0xFF),
        0xDD => matches!(modrm, 0xF0..=0xFF),
        0xDE => matches!(modrm, 0xD8 | 0xDA..=0xDF),
        0xDF => matches!(modrm, 0xC8..=0xCF | 0xD8..=0xDF | 0xE1..=0xE7 | 0xF8..=0xFF),
        _ => false,
    }
}

fn assert_invalid_opcode(bytes: &[u8], result: &LiftResult) {
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(result.ops.is_empty(), "{bytes:02X?}: {:?}", result.ops);
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(
        matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ),
        "{bytes:02X?}: {:?}",
        result.control_flow
    );
}

#[test]
fn every_direct_rejected_reserved_x87_cell_is_an_exact_invalid_opcode_trap() {
    let mut cells = 0usize;
    for opcode in 0xD8..=0xDF {
        for modrm in u8::MIN..=u8::MAX {
            if !is_direct_rejected_reserved_cell(opcode, modrm) {
                continue;
            }
            cells += 1;
            for leader in LEGACY_PREFIX_LEADERS {
                let bytes = complete_x87_form(leader, opcode, modrm);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("reserved x87 cell entered fallback: {bytes:02X?}: {error:?}")
                });
                assert_invalid_opcode(&bytes, &result);
            }
        }
    }
    assert_eq!(cells, 212, "reserved-cell oracle drifted");
}

#[test]
fn every_map0_rex2_payload_preserves_reserved_x87_invalid_opcode_results() {
    let representatives = [
        (0xD9, 0x08),
        (0xD9, 0xD1),
        (0xDA, 0xE0),
        (0xDB, 0x20),
        (0xDB, 0xE5),
        (0xDD, 0x28),
        (0xDD, 0xF0),
        (0xDE, 0xD8),
        (0xDF, 0xC8),
    ];
    for payload in 0x00..=0x7F {
        for (opcode, modrm) in representatives {
            let bytes = complete_x87_form(&[0xD5, payload], opcode, modrm);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("REX2 reserved x87 cell entered fallback: {bytes:02X?}: {error:?}")
            });
            assert_invalid_opcode(&bytes, &result);
        }
    }
}

#[test]
fn every_complete_non_lock_x87_form_has_only_eight_known_semantic_gaps() {
    for leader in LEGACY_PREFIX_LEADERS {
        for opcode in 0xD8..=0xDF {
            for modrm in u8::MIN..=u8::MAX {
                let bytes = complete_x87_form(leader, opcode, modrm);
                let result = lift_single(&bytes);
                if opcode == 0xD9
                    && VALID_UNIMPLEMENTED_D9.contains(&modrm)
                    && !is_direct_rejected_reserved_cell(opcode, modrm)
                {
                    assert!(
                        matches!(result, Err(LiftError::Unsupported { .. })),
                        "valid x87 semantic gap changed classification: {bytes:02X?}: {result:?}"
                    );
                } else {
                    let lifted = result.unwrap_or_else(|error| {
                        panic!("unexpected complete x87 fallback: {bytes:02X?}: {error:?}")
                    });
                    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                }
            }
        }
    }
}

#[test]
fn every_complete_lock_prefixed_x87_form_is_an_exact_invalid_opcode_trap() {
    for opcode in 0xD8..=0xDF {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = complete_x87_form(&[0xF0], opcode, modrm);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("LOCK x87 form entered fallback: {bytes:02X?}: {error:?}")
            });
            assert_invalid_opcode(&bytes, &result);
        }
    }
}

#[test]
fn incomplete_reserved_x87_memory_address_remains_incomplete() {
    assert!(matches!(
        lift_single(&[0xD9, 0x0D]),
        Err(LiftError::Incomplete {
            have: 1,
            need: 5,
            ..
        })
    ));
}
