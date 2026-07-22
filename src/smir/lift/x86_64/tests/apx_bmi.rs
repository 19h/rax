//! Intel APX promoted-BMI strict-lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn bmi_flags(kind: &OpKind) -> FlagUpdate {
    match kind {
        OpKind::AndNot { flags, .. }
        | OpKind::Bextr { flags, .. }
        | OpKind::Bzhi { flags, .. }
        | OpKind::X86Bls { flags, .. } => *flags,
        other => panic!("expected flag-controlling APX BMI operation, got {other:?}"),
    }
}

fn assert_apx_bmi_ud(bytes: &[u8], expected_len: usize, name: &str) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!("{name}: reserved APX BMI form must lift to #UD: {error:?}")
    });
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn lift_apx_bmi1_accepts_nf0_and_nf1_with_exact_operands_and_flags() {
    let andn_flags = FlagUpdate::Specific(
        FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF),
    );
    let bextr_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF));
    let bls_bzhi_flags = andn_flags;

    // Intel APX revision 5.0 Table 3.1.5 specifies NF=0/1 for all six
    // promoted flag-controlling BMI operations. LLVM 23 independently accepts
    // each NF=0 encoding below; setting payload bit 2 produces its NF=1 peer.
    for (mut bytes, name, expected_flags, expected_bls) in [
        (
            [0x62, 0x72, 0xFC, 0x08, 0xF2, 0xC3],
            "andn",
            andn_flags,
            None,
        ),
        (
            [0x62, 0xEA, 0xEC, 0x00, 0xF7, 0xC1],
            "bextr",
            bextr_flags,
            None,
        ),
        (
            [0x62, 0xEA, 0xEC, 0x00, 0xF5, 0xC1],
            "bzhi",
            bls_bzhi_flags,
            None,
        ),
        (
            [0x62, 0xFA, 0xFC, 0x00, 0xF3, 0xC9],
            "blsr",
            bls_bzhi_flags,
            Some(X86BlsKind::Blsr),
        ),
        (
            [0x62, 0xFA, 0xFC, 0x00, 0xF3, 0xD1],
            "blsmsk",
            bls_bzhi_flags,
            Some(X86BlsKind::Blsmsk),
        ),
        (
            [0x62, 0xFA, 0xFC, 0x00, 0xF3, 0xD9],
            "blsi",
            bls_bzhi_flags,
            Some(X86BlsKind::Blsi),
        ),
    ] {
        for nf in [false, true] {
            bytes[3] = (bytes[3] & !0x04) | if nf { 0x04 } else { 0 };
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("{name} NF={}: {error:?}", u8::from(nf)));
            assert_eq!(result.bytes_consumed, 6, "{name} NF={}", u8::from(nf));
            assert_eq!(result.ops.len(), 1, "{name} NF={}", u8::from(nf));
            assert_eq!(
                bmi_flags(&result.ops[0].kind),
                if nf { FlagUpdate::None } else { expected_flags },
                "{name} NF={}",
                u8::from(nf)
            );

            match (&result.ops[0].kind, name) {
                (
                    OpKind::AndNot {
                        dst,
                        src1,
                        src2: SrcOperand::Reg(src2),
                        width: OpWidth::W64,
                        ..
                    },
                    "andn",
                ) => {
                    assert_eq!(*dst, x86_gpr(8));
                    assert_eq!(*src1, x86_gpr(3));
                    assert_eq!(*src2, x86_gpr(0));
                }
                (
                    OpKind::Bextr {
                        dst,
                        src,
                        control,
                        width: OpWidth::W64,
                        ..
                    },
                    "bextr",
                )
                | (
                    OpKind::Bzhi {
                        dst,
                        src,
                        index: control,
                        width: OpWidth::W64,
                        ..
                    },
                    "bzhi",
                ) => {
                    assert_eq!(*dst, x86_gpr(16));
                    assert_eq!(*src, x86_gpr(17));
                    assert_eq!(*control, x86_gpr(18));
                }
                (
                    OpKind::X86Bls {
                        dst,
                        src,
                        width: OpWidth::W64,
                        kind,
                        ..
                    },
                    "blsr" | "blsmsk" | "blsi",
                ) => {
                    assert_eq!(*dst, x86_gpr(16));
                    assert_eq!(*src, x86_gpr(17));
                    assert_eq!(Some(*kind), expected_bls);
                }
                (other, _) => panic!("{name}: unexpected APX BMI operation {other:?}"),
            }
        }
    }
}

#[test]
fn lift_apx_bmi1_covers_w32_memory_alias_and_x4_address_forms() {
    // W=0 selects 32-bit operation and architectural zero-extension.
    let w32 = lift_single(&[0x62, 0x72, 0x7C, 0x08, 0xF2, 0xC3]).unwrap();
    assert!(matches!(
        w32.ops.as_slice(),
        [SmirOp {
            kind: OpKind::AndNot {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(_),
            },
            ..
        }] if *dst == x86_gpr(8) && *src1 == x86_gpr(3) && *src2 == x86_gpr(0)
    ));

    // Destination/source aliasing must retain both inputs before committing.
    let alias = lift_single(&[0x62, 0xF2, 0xE4, 0x08, 0xF2, 0xC0]).unwrap();
    assert!(matches!(
        alias.ops.as_slice(),
        [SmirOp {
            kind: OpKind::AndNot {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                ..
            },
            ..
        }] if *dst == x86_gpr(0) && *src1 == x86_gpr(0) && *src2 == x86_gpr(3)
    ));

    for (bytes, name) in [
        (
            &[0x62, 0x72, 0xF4, 0x08, 0xF7, 0x03][..],
            "bextr memory NF=0",
        ),
        (
            &[0x62, 0x72, 0xF4, 0x0C, 0xF5, 0x03][..],
            "bzhi memory NF=1",
        ),
        (
            &[0x62, 0xF2, 0xBC, 0x08, 0xF3, 0x0B][..],
            "blsr memory NF=0",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");
        assert!(matches!(
            result.ops[0].kind,
            OpKind::Load {
                addr: Address::Direct(base),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
                ..
            } if base == x86_gpr(3)
        ));
    }

    // In a memory form P1 bit 2 is the inverted X4 extension, not U. Clearing
    // it selects an EGPR index; setting it keeps the low 16-register bank.
    for (p1, expected_index) in [(0xF0, x86_gpr(17)), (0xF4, x86_gpr(1))] {
        let result = lift_single(&[0x62, 0xF2, p1, 0x08, 0xF7, 0x04, 0x4B]).unwrap();
        assert!(matches!(
            &result.ops[0].kind,
            OpKind::Load {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 2,
                    ..
                },
                ..
            } if *base == x86_gpr(3) && *index == expected_index
        ));
    }
}

#[test]
fn lift_apx_bmi2_register_and_memory_forms_match_llvm_encodings() {
    let pdep = lift_single(&[0x62, 0xE2, 0xE7, 0x00, 0xF5, 0xE3]).unwrap();
    assert!(matches!(
        pdep.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Pdep {
                dst,
                src,
                mask,
                width: OpWidth::W64,
            },
            ..
        }] if *dst == x86_gpr(20) && *src == x86_gpr(19) && *mask == x86_gpr(3)
    ));

    let pext = lift_single(&[0x62, 0xE2, 0xE6, 0x00, 0xF5, 0xE3]).unwrap();
    assert!(matches!(
        pext.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Pext {
                dst,
                src,
                mask,
                width: OpWidth::W64,
            },
            ..
        }] if *dst == x86_gpr(20) && *src == x86_gpr(19) && *mask == x86_gpr(3)
    ));

    let mulx = lift_single(&[0x62, 0xE2, 0xE7, 0x00, 0xF6, 0xE3]).unwrap();
    assert_eq!(mulx.ops[0].x86_hint, Some(X86OpHint::Mulx));
    assert!(matches!(
        mulx.ops.as_slice(),
        [SmirOp {
            kind: OpKind::MulU {
                dst_lo,
                dst_hi: Some(dst_hi),
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            ..
        }] if *dst_lo == x86_gpr(19)
            && *dst_hi == x86_gpr(20)
            && *src1 == x86_gpr(2)
            && *src2 == x86_gpr(3)
    ));

    for (bytes, name, width) in [
        (
            &[0x62, 0xEA, 0xE6, 0x08, 0xF7, 0xE3][..],
            "sarx",
            OpWidth::W64,
        ),
        (
            &[0x62, 0xEA, 0xE7, 0x08, 0xF7, 0xE3][..],
            "shrx",
            OpWidth::W64,
        ),
        (
            &[0x62, 0xEA, 0x65, 0x08, 0xF7, 0xE3][..],
            "shlx",
            OpWidth::W32,
        ),
    ] {
        assert_apx_bmi2_shift(bytes, name, x86_gpr(20), x86_gpr(19), x86_gpr(3), width);
    }

    let rorx = lift_single(&[0x62, 0xE3, 0xFF, 0x08, 0xF0, 0xE3, 0x0D]).unwrap();
    assert_eq!(rorx.bytes_consumed, 7);
    assert_vex_rorx_op(&rorx.ops, 0, x86_gpr(20), x86_gpr(3), 13, OpWidth::W64);

    for (bytes, name) in [
        (
            &[0x62, 0xEA, 0xE3, 0x00, 0xF5, 0x64, 0x91, 0x20][..],
            "pdep",
        ),
        (
            &[0x62, 0xEA, 0xE2, 0x00, 0xF5, 0x64, 0x91, 0x20][..],
            "pext",
        ),
        (
            &[0x62, 0xEA, 0xE3, 0x00, 0xF6, 0x64, 0x91, 0x20][..],
            "mulx",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, 8, "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");
        let loaded = assert_apx_bmi2_memory_load(&result.ops[0], name);
        match (&result.ops[1].kind, name) {
            (OpKind::Pdep { mask, .. }, "pdep") | (OpKind::Pext { mask, .. }, "pext") => {
                assert_eq!(*mask, loaded, "{name}");
            }
            (
                OpKind::MulU {
                    src2: SrcOperand::Reg(src2),
                    ..
                },
                "mulx",
            ) => assert_eq!(*src2, loaded),
            (other, _) => panic!("{name}: unexpected memory operation {other:?}"),
        }
    }

    for (bytes, name) in [
        (
            &[0x62, 0xEA, 0xE2, 0x00, 0xF7, 0x64, 0x91, 0x20][..],
            "sarx",
        ),
        (
            &[0x62, 0xEA, 0xE3, 0x00, 0xF7, 0x64, 0x91, 0x20][..],
            "shrx",
        ),
        (
            &[0x62, 0xEA, 0xE1, 0x00, 0xF7, 0x64, 0x91, 0x20][..],
            "shlx",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let loaded = assert_apx_bmi2_memory_load(&result.ops[0], name);
        assert_apx_bmi2_shift_ops(
            &result.ops,
            1,
            name,
            x86_gpr(20),
            loaded,
            x86_gpr(19),
            OpWidth::W64,
        );
    }

    let rorx_mem = lift_single(&[0x62, 0xEB, 0xFB, 0x08, 0xF0, 0x64, 0x91, 0x20, 0x0D]).unwrap();
    let loaded = assert_apx_bmi2_memory_load(&rorx_mem.ops[0], "rorx");
    assert_vex_rorx_op(&rorx_mem.ops, 1, x86_gpr(20), loaded, 13, OpWidth::W64);
}

#[test]
fn lift_apx_bmi_reserved_payload_and_opcode_cells_are_precise_ud() {
    for bit in [0, 1, 4, 5, 6, 7] {
        let bytes = [0x62, 0x72, 0xFC, 0x08 | (1 << bit), 0xF2];
        assert_apx_bmi_ud(&bytes, 5, &format!("ANDN reserved P2 bit {bit}"));
    }
    for bit in [0, 1, 2, 4, 5, 6, 7] {
        let bytes = [0x62, 0xE2, 0xE7, 0x08 | (1 << bit), 0xF5];
        assert_apx_bmi_ud(&bytes, 5, &format!("PDEP reserved P2 bit {bit}"));
    }

    // Wrong mandatory-prefix combinations are known once the opcode arrives;
    // they must not demand or interpret a following ModR/M byte.
    for (p1, opcode, name) in [
        (0xFD, 0xF2, "ANDN 66"),
        (0xFE, 0xF2, "ANDN F3"),
        (0xFF, 0xF2, "ANDN F2"),
        (0xFD, 0xF3, "BLS 66"),
        (0xFE, 0xF3, "BLS F3"),
        (0xFF, 0xF3, "BLS F2"),
        (0xE5, 0xF5, "F5 66"),
        (0xE4, 0xF6, "MULX NP"),
        (0xE5, 0xF6, "MULX 66"),
        (0xE6, 0xF6, "MULX F3"),
    ] {
        assert_apx_bmi_ud(&[0x62, 0xE2, p1, 0x08, opcode], 5, name);
    }

    // NF is reserved for every flagless BMI2 instruction.
    for (p0, p1, opcode, name) in [
        (0xE2, 0xE7, 0xF5, "pdep"),
        (0xE2, 0xE6, 0xF5, "pext"),
        (0xE2, 0xE7, 0xF6, "mulx"),
        (0xEA, 0xE6, 0xF7, "sarx"),
        (0xEA, 0xE5, 0xF7, "shlx"),
        (0xEA, 0xE7, 0xF7, "shrx"),
    ] {
        assert_apx_bmi_ud(&[0x62, p0, p1, 0x0C, opcode], 5, name);
    }
    assert_apx_bmi_ud(&[0x62, 0xE3, 0xFF, 0x0C, 0xF0], 5, "rorx NF");

    // RORX has no V operand; both encoded V fields must identify no register.
    assert_apx_bmi_ud(&[0x62, 0xE3, 0xEF, 0x08, 0xF0], 5, "rorx VVVV");
    assert_apx_bmi_ud(&[0x62, 0xE3, 0xFF, 0x00, 0xF0], 5, "rorx V4");
    for p1 in [0xFC, 0xFD, 0xFE] {
        assert_apx_bmi_ud(&[0x62, 0xE3, p1, 0x08, 0xF0], 5, "rorx pp");
    }
}

#[test]
fn lift_apx_bmi_modrm_frontiers_validate_group_u_and_incompleteness() {
    // Every reserved BLS opcode extension is #UD at ModR/M, even when its r/m
    // bits would otherwise require an absent SIB or displacement byte.
    for group in [0, 4, 5, 6, 7] {
        for mod_bits in 0..=3 {
            let modrm = (mod_bits << 6) | (group << 3) | 4;
            let bytes = [0x62, 0xF2, 0xFC, 0x08, 0xF3, modrm];
            assert_apx_bmi_ud(&bytes, 6, &format!("BLS reserved /{group} mod {mod_bits}"));
        }
    }

    // Register forms reinterpret P1 bit 2 as U and require U=1. Exercise all
    // promoted BMI operation classes, including each BMI2 shift and RORX.
    for (mut bytes, name) in [
        (vec![0x62, 0x72, 0xFC, 0x08, 0xF2, 0xC3], "andn"),
        (vec![0x62, 0xF2, 0xFC, 0x08, 0xF3, 0xC8], "blsr"),
        (vec![0x62, 0xF2, 0xF4, 0x08, 0xF5, 0xC3], "bzhi"),
        (vec![0x62, 0xF2, 0xF4, 0x08, 0xF7, 0xC3], "bextr"),
        (vec![0x62, 0xE2, 0xE7, 0x00, 0xF5, 0xE3], "pdep"),
        (vec![0x62, 0xE2, 0xE6, 0x00, 0xF5, 0xE3], "pext"),
        (vec![0x62, 0xE2, 0xE7, 0x00, 0xF6, 0xE3], "mulx"),
        (vec![0x62, 0xEA, 0xE6, 0x08, 0xF7, 0xE3], "sarx"),
        (vec![0x62, 0xEA, 0xE5, 0x08, 0xF7, 0xE3], "shlx"),
        (vec![0x62, 0xEA, 0xE7, 0x08, 0xF7, 0xE3], "shrx"),
        (vec![0x62, 0xE3, 0xFF, 0x08, 0xF0, 0xE3, 0x0D], "rorx"),
    ] {
        bytes[2] &= !0x04;
        assert_apx_bmi_ud(&bytes, 6, &format!("{name} U=0"));
    }

    for (bytes, have, need, name) in [
        (
            &[0x62, 0x72, 0xFC, 0x08, 0xF2][..],
            5,
            6,
            "ANDN missing ModR/M",
        ),
        (
            &[0x62, 0xE3, 0xFF, 0x08, 0xF0][..],
            5,
            6,
            "RORX missing ModR/M",
        ),
        (
            &[0x62, 0xE3, 0xFF, 0x08, 0xF0, 0xE3][..],
            6,
            7,
            "RORX missing imm8",
        ),
    ] {
        let error = lift_single(bytes).expect_err(name);
        assert!(
            matches!(error, LiftError::Incomplete { have: got_have, need: got_need, .. }
                if got_have == have && got_need == need),
            "{name}: {error:?}"
        );
    }

    let missing_sib = lift_single(&[0x62, 0xF2, 0xFC, 0x08, 0xF3, 0x0C])
        .expect_err("valid BLSR memory form must require its SIB");
    assert!(matches!(missing_sib, LiftError::Incomplete { .. }));
}
