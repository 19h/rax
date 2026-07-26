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

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vex_bmi_semantic(op: &SmirOp) -> (&'static str, OpWidth) {
    match (&op.kind, op.x86_hint) {
        (OpKind::AndNot { width, .. }, None) => ("ANDN", *width),
        (
            OpKind::X86Bls {
                kind: X86BlsKind::Blsr,
                width,
                ..
            },
            None,
        ) => ("BLSR", *width),
        (
            OpKind::X86Bls {
                kind: X86BlsKind::Blsmsk,
                width,
                ..
            },
            None,
        ) => ("BLSMSK", *width),
        (
            OpKind::X86Bls {
                kind: X86BlsKind::Blsi,
                width,
                ..
            },
            None,
        ) => ("BLSI", *width),
        (OpKind::Bzhi { width, .. }, None) => ("BZHI", *width),
        (OpKind::Bextr { width, .. }, None) => ("BEXTR", *width),
        (OpKind::Pdep { width, .. }, None) => ("PDEP", *width),
        (OpKind::Pext { width, .. }, None) => ("PEXT", *width),
        (OpKind::Shl { width, .. }, None) => ("SHLX", *width),
        (OpKind::Shr { width, .. }, None) => ("SHRX", *width),
        (OpKind::Sar { width, .. }, None) => ("SARX", *width),
        (OpKind::Ror { width, .. }, None) => ("RORX", *width),
        (OpKind::MulU { width, .. }, Some(X86OpHint::Mulx)) => ("MULX", *width),
        (kind, hint) => panic!("unexpected VEX BMI semantic operation: {kind:?}, hint={hint:?}"),
    }
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
fn vex_bmi_memory_forms_preserve_address_size_segment_and_sib_metadata() {
    // Independently assembled as W1 with LLVM 23. Each instruction uses
    // `qword ptr fs:[ebx + 4*e{c,d}x + 32]`, combining the two legacy
    // prefixes whose state was previously lost by semantic re-decoding. W0 is
    // the same primary-spec encoding with VEX.W cleared and must preserve the
    // identical effective address while selecting a 32-bit data operand.
    let cases: &[(&str, &[u8], X86Reg)] = &[
        (
            "ANDN",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF0, 0xF2, 0x44, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "BLSR",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF8, 0xF3, 0x4C, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "BLSMSK",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF8, 0xF3, 0x54, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "BLSI",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF8, 0xF3, 0x5C, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "BZHI",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF0, 0xF5, 0x44, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "BEXTR",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF0, 0xF7, 0x44, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
        (
            "PDEP",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF3, 0xF5, 0x44, 0x93, 0x20],
            X86Reg::Rdx,
        ),
        (
            "PEXT",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF2, 0xF5, 0x44, 0x93, 0x20],
            X86Reg::Rdx,
        ),
        (
            "SHLX",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF1, 0xF7, 0x44, 0x93, 0x20],
            X86Reg::Rdx,
        ),
        (
            "SHRX",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF3, 0xF7, 0x44, 0x93, 0x20],
            X86Reg::Rdx,
        ),
        (
            "SARX",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF2, 0xF7, 0x44, 0x93, 0x20],
            X86Reg::Rdx,
        ),
        (
            "RORX",
            &[0x64, 0x67, 0xC4, 0xE3, 0xFB, 0xF0, 0x44, 0x8B, 0x20, 0x11],
            X86Reg::Rcx,
        ),
        (
            "MULX",
            &[0x64, 0x67, 0xC4, 0xE2, 0xF3, 0xF6, 0x44, 0x8B, 0x20],
            X86Reg::Rcx,
        ),
    ];

    for &(expected_name, w1_bytes, index) in cases {
        for w64 in [false, true] {
            let mut bytes = w1_bytes.to_vec();
            if !w64 {
                bytes[4] &= !0x80;
            }
            let expected_mem_width = if w64 { MemWidth::B8 } else { MemWidth::B4 };
            let expected_op_width = if w64 { OpWidth::W64 } else { OpWidth::W32 };

            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("{expected_name} {bytes:02X?}: {error:?}"));
            assert_eq!(result.bytes_consumed, bytes.len(), "{expected_name}");
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

            let (addr, width) = result
                .ops
                .iter()
                .find_map(|op| match &op.kind {
                    OpKind::Load { addr, width, .. } => Some((addr, *width)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{expected_name} did not emit an exact scalar load"));
            assert_eq!(width, expected_mem_width, "{expected_name}");
            assert_eq!(
                addr,
                &Address::X86Addr32(Box::new(Address::SegmentRel {
                    segment: x86(X86Reg::FsBase),
                    base: Some(x86(X86Reg::Rbx)),
                    index: Some(x86(index)),
                    scale: 4,
                    disp: 32,
                })),
                "{expected_name}"
            );
            assert_eq!(
                vex_bmi_semantic(result.ops.last().expect("semantic operation")),
                (expected_name, expected_op_width)
            );
        }
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
