//! Original VEX and Intel APX-promoted EVEX CMPccXADD lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

fn assert_cmpccxadd_ud(bytes: &[u8], expected_len: usize, name: &str) {
    let result = lift_single(bytes)
        .unwrap_or_else(|error| panic!("{name}: reserved CMPccXADD must lift to #UD: {error:?}"));
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn lift_cmpccxadd_vex_conditions_like_llvm() {
    for (opcode, cond) in [
        (0xE0, Condition::Overflow),
        (0xE1, Condition::NoOverflow),
        (0xE2, Condition::Ult),
        (0xE3, Condition::Uge),
        (0xE4, Condition::Eq),
        (0xE5, Condition::Ne),
        (0xE6, Condition::Ule),
        (0xE7, Condition::Ugt),
        (0xE8, Condition::Negative),
        (0xE9, Condition::Positive),
        (0xEA, Condition::Parity),
        (0xEB, Condition::NoParity),
        (0xEC, Condition::Slt),
        (0xED, Condition::Sge),
        (0xEE, Condition::Sle),
        (0xEF, Condition::Sgt),
    ] {
        let bytes = [0xC4, 0xE2, 0x71, opcode, 0x18];
        let result = lift_single(&bytes).unwrap();
        assert_eq!(result.bytes_consumed, 5, "opcode {opcode:02x}");
        assert_eq!(result.ops.len(), 1, "opcode {opcode:02x}");
        match &result.ops[0].kind {
            OpKind::AtomicCmpXadd {
                dst_old,
                addr: Address::Direct(base),
                cmp,
                add,
                cond: got_cond,
                width,
                order: MemoryOrder::SeqCst,
            } => {
                assert_eq!(*dst_old, x86_gpr(3), "opcode {opcode:02x}");
                assert_eq!(*cmp, x86_gpr(3), "opcode {opcode:02x}");
                assert_eq!(*add, x86_gpr(1), "opcode {opcode:02x}");
                assert_eq!(*base, x86_gpr(0), "opcode {opcode:02x}");
                assert_eq!(*got_cond, cond, "opcode {opcode:02x}");
                assert_eq!(*width, MemWidth::B4, "opcode {opcode:02x}");
            }
            other => panic!("expected CMPccXADD op for {opcode:02x}, got {other:?}"),
        }
    }
}

#[test]
fn lift_cmpccxadd_vex_width_and_high_regs_like_llvm() {
    for (bytes, name, width, dst, base, add) in [
        (
            &[0xC4, 0xE2, 0xF1, 0xE2, 0x18][..],
            "cmpbxadd64",
            MemWidth::B8,
            x86_gpr(3),
            x86_gpr(0),
            x86_gpr(1),
        ),
        (
            &[0xC4, 0x42, 0x29, 0xE2, 0x08][..],
            "cmpbxadd32_r8_r9_r10",
            MemWidth::B4,
            x86_gpr(9),
            x86_gpr(8),
            x86_gpr(10),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, 5, "{name}");
        match &result.ops[0].kind {
            OpKind::AtomicCmpXadd {
                dst_old,
                addr: Address::Direct(got_base),
                cmp,
                add: got_add,
                cond: Condition::Ult,
                width: got_width,
                order: MemoryOrder::SeqCst,
            } => {
                assert_eq!(*dst_old, dst, "{name}");
                assert_eq!(*cmp, dst, "{name}");
                assert_eq!(*got_base, base, "{name}");
                assert_eq!(*got_add, add, "{name}");
                assert_eq!(*got_width, width, "{name}");
            }
            other => panic!("expected VEX {name} AtomicCmpXadd, got {other:?}"),
        }
    }
}

#[test]
fn vex_cmpccxadd_preserves_allowed_address_size_and_segment_prefixes() {
    // Type 14 forbids LOCK, REX, 66, F2, and F3 before VEX. Address-size and
    // segment overrides remain legal and must reach effective-address lifting.
    let addr32 = lift_single(&[0x67, 0xC4, 0xE2, 0x71, 0xE2, 0x18]).unwrap();
    assert_eq!(addr32.bytes_consumed, 6);
    assert!(matches!(
        &addr32.ops[0].kind,
        OpKind::AtomicCmpXadd {
            addr: Address::X86Addr32(inner),
            ..
        } if matches!(inner.as_ref(), Address::Direct(base) if *base == x86_gpr(0))
    ));

    let gs = lift_single(&[0x65, 0xC4, 0xE2, 0x71, 0xE2, 0x18]).unwrap();
    assert_eq!(gs.bytes_consumed, 6);
    assert!(matches!(
        &gs.ops[0].kind,
        OpKind::AtomicCmpXadd {
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: Some(base),
                index: None,
                scale: 1,
                disp: 0,
            },
            ..
        } if *base == x86_gpr(0)
    ));

    assert_cmpccxadd_ud(&[0x67, 0xC4, 0xE2, 0x72, 0xE2], 5, "addr32 VEX reserved pp");
    assert_cmpccxadd_ud(
        &[0x65, 0x62, 0xEA, 0x61, 0x04, 0xE2],
        6,
        "GS APX reserved payload",
    );
}

#[test]
fn every_reserved_vex_cmpccxadd_prefix_and_register_cell_is_precise_ud() {
    // Intel SDM Vol. 2A specifies VEX.128.66.0F38.W{0,1}. A wrong pp or
    // VEX.L=1 is known at the opcode and must not demand ModR/M.
    for opcode in 0xE0..=0xEF {
        for (p1, name) in [
            (0x70, "pp=NP"),
            (0x72, "pp=F3"),
            (0x73, "pp=F2"),
            (0x75, "L=1"),
        ] {
            assert_cmpccxadd_ud(&[0xC4, 0xE2, p1, opcode], 4, name);
        }

        // MOD=3 is reserved for every condition, W value, REG field, and R/M
        // field. It is terminal once ModR/M is available.
        for w in [0x00, 0x80] {
            for low_six in 0..=0x3F {
                let bytes = [0xC4, 0xE2, 0x71 | w, opcode, 0xC0 | low_six];
                assert_cmpccxadd_ud(&bytes, 5, "VEX MOD=3");
            }
        }
    }
}

#[test]
fn every_reserved_apx_cmpccxadd_payload_pp_and_register_cell_is_precise_ud() {
    // APX revision 5.0 permits only V4 (bit 3) in payload byte 2. Exercise
    // every forbidden bit with both legal V4 values and every condition.
    for opcode in 0xE0..=0xEF {
        for v4 in [0x00, 0x08] {
            for bit in [0, 1, 2, 4, 5, 6, 7] {
                let p2 = v4 | (1 << bit);
                assert_cmpccxadd_ud(&[0x62, 0xEA, 0x61, p2, opcode], 5, "APX reserved P2");
            }
            for pp in [0x00, 0x02, 0x03] {
                assert_cmpccxadd_ud(&[0x62, 0xEA, 0x60 | pp, v4, opcode], 5, "APX reserved pp");
            }
        }

        // MOD=3 is reserved independently of W, V4, REG, and R/M.
        for w in [0x00, 0x80] {
            for v4 in [0x00, 0x08] {
                for low_six in 0..=0x3F {
                    let bytes = [0x62, 0xEA, 0x61 | w, v4, opcode, 0xC0 | low_six];
                    assert_cmpccxadd_ud(&bytes, 6, "APX MOD=3");
                }
            }
        }
    }
}

#[test]
fn cmpccxadd_valid_frontiers_preserve_incomplete_addressing_errors() {
    for (bytes, have, need, name) in [
        (&[0xC4, 0xE2, 0x71, 0xE2][..], 4, 5, "VEX missing ModR/M"),
        (
            &[0x62, 0xEA, 0x61, 0x00, 0xE2][..],
            5,
            6,
            "APX missing ModR/M",
        ),
    ] {
        let error = lift_single(bytes).expect_err(name);
        assert!(
            matches!(error, LiftError::Incomplete { have: got_have, need: got_need, .. }
                if got_have == have && got_need == need),
            "{name}: {error:?}"
        );
    }

    for (bytes, name) in [
        (&[0xC4, 0xE2, 0x71, 0xE2, 0x04][..], "VEX missing SIB"),
        (&[0x62, 0xEA, 0x61, 0x00, 0xE2, 0x04][..], "APX missing SIB"),
        (&[0xC4, 0xE2, 0x71, 0xE2, 0x05][..], "VEX missing disp32"),
        (
            &[0x62, 0xEA, 0x61, 0x00, 0xE2, 0x05][..],
            "APX missing disp32",
        ),
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "{name}"
        );
    }
}
