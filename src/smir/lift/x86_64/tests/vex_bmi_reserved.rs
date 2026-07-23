//! Exhaustive strict-lift frontiers for VEX-encoded BMI1/BMI2 instructions.

use super::*;

fn vex3(map: u8, pp: u8, l: u8, w: u8, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![
        0xC4,
        0xE0 | map,
        (w << 7) | (((!vvvv) & 0x0F) << 3) | (l << 2) | pp,
        opcode,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn assert_terminal_ud(bytes: &[u8], expected_len: usize) {
    let strict =
        lift_single(bytes).unwrap_or_else(|error| panic!("strict {bytes:02X?}: {error:?}"));
    assert_invalid_opcode_trap(&strict, expected_len);

    let nonstrict =
        lift_nonstrict(bytes).unwrap_or_else(|error| panic!("nonstrict {bytes:02X?}: {error:?}"));
    assert_invalid_opcode_trap(&nonstrict, expected_len);
}

fn valid_0f38_pp(opcode: u8, pp: u8) -> bool {
    match opcode {
        0xF2 | 0xF3 => pp == 0,
        0xF5 => matches!(pp, 0 | 2 | 3),
        0xF6 => pp == 3,
        0xF7 => pp <= 3,
        _ => false,
    }
}

#[test]
fn every_vex_bmi_0f38_prefix_cell_is_lifted_or_terminal_ud() {
    for opcode in [0xF2_u8, 0xF3, 0xF5, 0xF6, 0xF7] {
        for pp in 0_u8..=3 {
            for l in 0_u8..=1 {
                for w in 0_u8..=1 {
                    for vvvv in 0_u8..=15 {
                        let modrm = if opcode == 0xF3 { 0xC8 } else { 0xC0 };
                        let bytes = vex3(2, pp, l, w, vvvv, opcode, &[modrm]);
                        if l == 0 && valid_0f38_pp(opcode, pp) {
                            let result = lift_single(&bytes).unwrap_or_else(|error| {
                                panic!("assigned VEX BMI cell {bytes:02X?}: {error:?}")
                            });
                            assert!(
                                matches!(result.control_flow, ControlFlow::Fallthrough),
                                "assigned VEX BMI cell trapped: {bytes:02X?}: {result:?}"
                            );
                            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                        } else {
                            assert_terminal_ud(&bytes, 4);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_vex_rorx_prefix_cell_is_lifted_or_terminal_ud() {
    for pp in 0_u8..=3 {
        for l in 0_u8..=1 {
            for w in 0_u8..=1 {
                for vvvv in 0_u8..=15 {
                    let bytes = vex3(3, pp, l, w, vvvv, 0xF0, &[0xC0, 0x0D]);
                    if pp == 3 && l == 0 && vvvv == 0 {
                        let result = lift_single(&bytes).unwrap_or_else(|error| {
                            panic!("assigned RORX cell {bytes:02X?}: {error:?}")
                        });
                        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
                        assert_eq!(result.bytes_consumed, bytes.len());
                    } else {
                        assert_terminal_ud(&bytes, 4);
                    }
                }
            }
        }
    }
}

#[test]
fn vex_bmi_reserved_group_and_fetch_frontiers_are_exact() {
    for group in [0_u8, 4, 5, 6, 7] {
        let register = vex3(2, 0, 0, 0, 0, 0xF3, &[0xC0 | (group << 3)]);
        assert_terminal_ud(&register, 5);
    }

    // The invalid BLS /0 group is known from ModR/M.reg before the missing SIB
    // or any displacement can be fetched. A valid /1 group must continue
    // decoding the same memory address.
    let invalid_memory = vex3(2, 0, 0, 0, 0, 0xF3, &[0x04]);
    assert_terminal_ud(&invalid_memory, 5);
    let valid_memory = vex3(2, 0, 0, 0, 0, 0xF3, &[0x0C]);
    assert!(matches!(
        lift_single(&valid_memory),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 5,
            need: 6,
        })
    ));

    // Prefix-level reservations are final at the opcode byte. Assigned cells
    // still require ModR/M, and RORX additionally requires imm8.
    assert_terminal_ud(&vex3(2, 0, 1, 0, 0, 0xF2, &[]), 4);
    assert_terminal_ud(&vex3(3, 0, 0, 0, 0, 0xF0, &[]), 4);
    assert!(matches!(
        lift_single(&vex3(2, 0, 0, 0, 0, 0xF2, &[])),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 4,
            need: 5,
        })
    ));
    assert!(matches!(
        lift_single(&vex3(3, 3, 0, 0, 0, 0xF0, &[])),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 4,
            need: 5,
        })
    ));
    assert!(matches!(
        lift_single(&vex3(3, 3, 0, 0, 0, 0xF0, &[0xC0])),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 5,
            need: 6,
        })
    ));
}
