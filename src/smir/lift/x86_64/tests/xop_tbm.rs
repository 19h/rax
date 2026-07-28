//! AMD XOP/TBM strict-lifting contracts.

use super::*;
use crate::smir::ir::ops::X86TbmKind;

fn xop(map: u8, w: bool, l: bool, pp: u8, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    assert!((8..=31).contains(&map));
    assert!(pp < 4 && vvvv < 16);
    let mut bytes = vec![
        0x8F,
        0xE0 | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

fn tbm_flags() -> FlagUpdate {
    FlagUpdate::Specific(
        FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF),
    )
}

#[test]
fn strict_lifter_accepts_every_map9_tbm_operation_at_both_widths() {
    for (opcode, group, expected_kind) in [
        (0x01, 1, X86TbmKind::Blcfill),
        (0x02, 6, X86TbmKind::Blci),
        (0x01, 5, X86TbmKind::Blcic),
        (0x02, 1, X86TbmKind::Blcmsk),
        (0x01, 3, X86TbmKind::Blcs),
        (0x01, 2, X86TbmKind::Blsfill),
        (0x01, 6, X86TbmKind::Blsic),
        (0x01, 7, X86TbmKind::T1mskc),
        (0x01, 4, X86TbmKind::Tzmsk),
    ] {
        for w in [false, true] {
            let bytes = xop(9, w, false, 0, 5, opcode, &[0xC0 | (group << 3) | 3]);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("{expected_kind:?}, W={w}: {error:?}"));
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
            assert!(matches!(
                result.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86RequireTbm,
                        ..
                    },
                    SmirOp {
                        kind: OpKind::X86Tbm {
                            dst,
                            src,
                            width,
                            kind,
                            flags,
                        },
                        ..
                    }
                ] if *dst == x86_gpr(5)
                    && *src == x86_gpr(3)
                    && *width == if w { OpWidth::W64 } else { OpWidth::W32 }
                    && *kind == expected_kind
                    && *flags == tbm_flags()
            ));
        }
    }
}

#[test]
fn strict_lifter_accepts_immediate_bextr_with_exact_control_and_length() {
    for w in [false, true] {
        let bytes = xop(10, w, false, 0, 0, 0x10, &[0xEB, 0x04, 0x08, 0x00, 0xA5]);
        let result = lift_single(&bytes).expect("lift immediate-control BEXTR");
        assert_eq!(result.bytes_consumed, 9);
        assert!(matches!(
            result.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86RequireTbm,
                    ..
                },
                SmirOp {
                    kind: OpKind::Bextr {
                        dst,
                        src,
                        control: VReg::Imm(imm),
                        width,
                        flags,
                    },
                    ..
                }
            ] if *dst == x86_gpr(5)
                && *src == x86_gpr(3)
                && *imm == i64::from(0xA500_0804_u32)
                && *width == if w { OpWidth::W64 } else { OpWidth::W32 }
                && *flags == FlagUpdate::Specific(
                    FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)
                )
        ));
    }
}

#[test]
fn memory_tbm_preserves_segment_addr32_sib_and_guard_order() {
    // BLCFILL RBP,qword ptr FS:[EBX + ECX*4 + 0x20].
    let bytes = [0x64, 0x67, 0x8F, 0xE9, 0xD0, 0x01, 0x4C, 0x8B, 0x20];
    let result = lift_single(&bytes).expect("lift address-rich XOP memory form");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireTbm));

    let load_index = result
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Load { .. }))
        .expect("TBM memory source load");
    assert!(
        load_index > 0,
        "feature guard must precede address/load work"
    );
    assert!(matches!(
        &result.ops[load_index].kind,
        OpKind::Load {
            addr: Address::X86Addr32(inner),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        } if **inner == Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(x86_gpr(3)),
            index: Some(x86_gpr(1)),
            scale: 4,
            disp: 0x20,
        }
    ));
    assert!(matches!(
        result.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Tbm {
            dst,
            width: OpWidth::W64,
            kind: X86TbmKind::Blcfill,
            ..
        }) if *dst == x86_gpr(5)
    ));
}

#[test]
fn xop_prefix_reservations_terminalize_at_the_precise_frontier() {
    for (name, bytes, expected_len) in [
        (
            "operand-size prefix",
            &[0x66, 0x8F, 0xE9, 0x78, 0x01, 0xCB][..],
            5,
        ),
        ("REP", &[0xF3, 0x8F, 0xE9, 0x78, 0x01, 0xCB], 5),
        ("LOCK", &[0xF0, 0x8F, 0xE9, 0x78, 0x01, 0xCB], 5),
        ("REX", &[0x48, 0x8F, 0xE9, 0x78, 0x01, 0xCB], 5),
        (
            "REX before address-size prefix",
            &[0x48, 0x67, 0x8F, 0xE9, 0x78, 0x01, 0xCB],
            6,
        ),
        (
            "REX before segment prefix",
            &[0x48, 0x64, 0x8F, 0xE9, 0x78, 0x01, 0xCB],
            6,
        ),
        ("L=1", &[0x8F, 0xE9, 0x7C, 0x01, 0xCB], 4),
        ("pp=01", &[0x8F, 0xE9, 0x79, 0x01, 0xCB], 4),
        ("reserved map 11", &[0x8F, 0xEB, 0x78, 0x01, 0xCB], 4),
        (
            "BEXTR reserved vvvv",
            &[0x8F, 0xEA, 0x70, 0x10, 0xC3, 0, 0, 0, 0],
            4,
        ),
        ("reserved map9 group", &[0x8F, 0xE9, 0x78, 0x01, 0xC3], 5),
    ] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("{name}: expected #UD, got {error:?}"));
        assert_invalid_opcode_trap(&result, expected_len);
    }
}

#[test]
fn xop_fetch_and_pop_disambiguation_frontiers_are_exact() {
    for (bytes, have, need) in [
        (&[0x8F, 0xE9][..], 2, 4),
        (&[0x8F, 0xE9, 0x78][..], 3, 4),
        (&[0x8F, 0xE9, 0x78, 0x01][..], 4, 5),
        (&[0x8F, 0xEA, 0x78, 0x10, 0xC3][..], 5, 9),
        (&[0x8F, 0xEA, 0x78, 0x10, 0xC3, 1, 2, 3][..], 8, 9),
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::Incomplete {
                    have: actual_have,
                    need: actual_need,
                    ..
                }) if actual_have == have && actual_need == need
            ),
            "bytes={bytes:02X?}"
        );
    }

    let pop = lift_single(&[0x8F, 0xC0]).expect("8F /0 must remain legacy POP");
    assert_eq!(pop.bytes_consumed, 2);
    assert!(matches!(pop.control_flow, ControlFlow::Fallthrough));
    assert!(
        pop.ops
            .iter()
            .all(|op| !matches!(op.kind, OpKind::X86RequireTbm))
    );
}

#[test]
fn removed_vex_tbm_aliases_are_reserved_without_hiding_avx2_cells() {
    for opcode in [0x01, 0x02] {
        // VEX.128.0F38.W0 01/02 with pp=00 is unassigned. The old TBM
        // implementation incorrectly treated these cells as BLCFILL/BLCMSK.
        let bytes = [0xC4, 0xE2, 0x78, opcode, 0xCB];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("opcode={opcode:#04x}: expected #UD, got {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }

    // pp=01 remains the assigned AVX2 VPHADDW cell.
    let valid = lift_single(&[0xC4, 0xE2, 0x79, 0x01, 0xCB])
        .expect("VEX.128.66.0F38.WIG 01 /r must remain VPHADDW");
    assert_eq!(valid.bytes_consumed, 5);
    assert!(matches!(valid.control_flow, ControlFlow::Fallthrough));
}

#[test]
fn assigned_non_tbm_xop_remains_an_explicit_unsupported_frontier() {
    // Iced-x86 1.21 independently encodes this as VPROTB XMM1,XMM2,3.
    let bytes = [0x8F, 0xE8, 0x78, 0xC0, 0xCA, 0x03];
    assert!(matches!(
        lift_single(&bytes),
        Err(LiftError::Unsupported { mnemonic, .. })
            if mnemonic == "XOP map 0x08 opcode 0xc0"
    ));
}
