//! Strict-lift regressions for AMD SSE4A operations.

use super::*;

#[test]
fn strict_lifter_accepts_all_sse4a_bitfield_forms_and_exact_lengths() {
    for (name, bytes) in [
        ("EXTRQ immediate", &[0x66, 0x0F, 0x78, 0xC1, 0x08, 0x04][..]),
        ("EXTRQ register", &[0x66, 0x0F, 0x79, 0xCA][..]),
        (
            "INSERTQ immediate",
            &[0xF2, 0x0F, 0x78, 0xCA, 0x08, 0x10][..],
        ),
        ("INSERTQ register", &[0xF2, 0x0F, 0x79, 0xCA][..]),
        ("extended XMM", &[0x66, 0x45, 0x0F, 0x79, 0xCA][..]),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(!result.ops.is_empty(), "{name}");
    }
}

#[test]
fn strict_lifter_terminalizes_reserved_sse4a_shapes_without_fallback() {
    for (name, bytes, expected_len) in [
        ("EXTRQ memory", &[0x66, 0x0F, 0x79, 0x00][..], 4),
        ("INSERTQ memory", &[0xF2, 0x0F, 0x79, 0x00][..], 4),
        (
            "EXTRQ immediate nonzero group",
            &[0x66, 0x0F, 0x78, 0xC9, 1, 2][..],
            4,
        ),
        ("prefix-free", &[0x0F, 0x79, 0xC1][..], 2),
        ("REP", &[0xF3, 0x0F, 0x79, 0xC1][..], 3),
        ("LOCK", &[0xF0, 0x66, 0x0F, 0x79, 0xC1][..], 4),
        ("REX2", &[0x66, 0xD5, 0x00, 0x0F, 0x79, 0xC1][..], 4),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, expected_len);
    }
}

#[test]
fn strict_lifter_reports_both_missing_immediate_bytes_precisely() {
    for (bytes, have, need) in [
        (&[0x66, 0x0F, 0x78, 0xC0][..], 4, 5),
        (&[0x66, 0x0F, 0x78, 0xC0, 0x08][..], 5, 6),
        (&[0xF2, 0x0F, 0x78, 0xC1][..], 4, 5),
        (&[0xF2, 0x0F, 0x78, 0xC1, 0x08][..], 5, 6),
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::Incomplete {
                    have: got_have,
                    need: got_need,
                    ..
                }) if got_have == have && got_need == need
            ),
            "bytes={bytes:02X?}"
        );
    }
}

#[test]
fn strict_lifter_accepts_movntss_movntsd_with_exact_width_address_and_guard_order() {
    for (name, bytes, expected_src, expected_width, expected_addr) in [
        (
            "MOVNTSS",
            &[0xF3, 0x0F, 0x2B, 0x08][..],
            X86Reg::Xmm(1),
            MemWidth::B4,
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
        ),
        (
            "MOVNTSD extended XMM and disp8",
            &[0xF2, 0x44, 0x0F, 0x2B, 0x48, 0x10][..],
            X86Reg::Xmm(9),
            MemWidth::B8,
            Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                offset: 0x10,
                disp_size: DispSize::Disp8,
            },
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(
            matches!(
                result.ops.as_slice(),
                [
                    SmirOp { kind: OpKind::X86RequireSse4a, .. },
                    SmirOp {
                        kind: OpKind::X86Sse4aMovntStore { src: VReg::Arch(ArchReg::X86(src)), addr, width },
                        ..
                    }
                ] if *src == expected_src && *addr == expected_addr && *width == expected_width
            ),
            "{name}: {:#?}",
            result.ops
        );
    }
}

#[test]
fn strict_lifter_terminalizes_reserved_sse4a_movnt_shapes_without_fallback() {
    for (name, bytes, expected_len) in [
        ("MOVNTSS register", &[0xF3, 0x0F, 0x2B, 0xC1][..], 4),
        ("MOVNTSD register", &[0xF2, 0x0F, 0x2B, 0xC1][..], 4),
        ("combined 66 prefix", &[0x66, 0xF3, 0x0F, 0x2B, 0x08][..], 4),
        ("LOCK", &[0xF0, 0xF3, 0x0F, 0x2B, 0x08][..], 4),
        ("REX2", &[0xF3, 0xD5, 0x00, 0x0F, 0x2B, 0x08][..], 4),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, expected_len);
    }
}
