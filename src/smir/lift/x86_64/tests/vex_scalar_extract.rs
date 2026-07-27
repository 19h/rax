//! Strict-lift coverage for register-destination AVX VEX scalar lane extracts.

use super::*;

const IMMEDIATES: [u8; 18] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x80, 0xFF,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

impl WMode {
    fn values(self) -> &'static [bool] {
        match self {
            Self::Ignored => &[false, true],
            Self::W0 => &[false],
            Self::W1 => &[true],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GprField {
    Reg,
    Rm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtractKind {
    Vextractps,
    Vpextrb,
    Vpextrd,
    Vpextrq,
    VpextrwMap1,
    VpextrwMap3,
}

impl ExtractKind {
    const ALL: [Self; 6] = [
        Self::Vextractps,
        Self::Vpextrb,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VpextrwMap1,
        Self::VpextrwMap3,
    ];

    fn fields(
        self,
    ) -> (
        u8,
        u8,
        WMode,
        GprField,
        VecElementType,
        u8,
        OpWidth,
        MemWidth,
    ) {
        match self {
            Self::Vextractps => (
                3,
                0x17,
                WMode::Ignored,
                GprField::Rm,
                VecElementType::I32,
                0x03,
                OpWidth::W32,
                MemWidth::B4,
            ),
            Self::Vpextrb => (
                3,
                0x14,
                WMode::Ignored,
                GprField::Rm,
                VecElementType::I8,
                0x0F,
                OpWidth::W32,
                MemWidth::B1,
            ),
            Self::Vpextrd => (
                3,
                0x16,
                WMode::W0,
                GprField::Rm,
                VecElementType::I32,
                0x03,
                OpWidth::W32,
                MemWidth::B4,
            ),
            Self::Vpextrq => (
                3,
                0x16,
                WMode::W1,
                GprField::Rm,
                VecElementType::I64,
                0x01,
                OpWidth::W64,
                MemWidth::B8,
            ),
            Self::VpextrwMap1 => (
                1,
                0xC5,
                WMode::Ignored,
                GprField::Reg,
                VecElementType::I16,
                0x07,
                OpWidth::W32,
                MemWidth::B2,
            ),
            Self::VpextrwMap3 => (
                3,
                0x15,
                WMode::Ignored,
                GprField::Rm,
                VecElementType::I16,
                0x07,
                OpWidth::W32,
                MemWidth::B2,
            ),
        }
    }
}

fn gprs() -> impl Iterator<Item = u8> {
    0..16
}

fn c4_encoding(
    kind: ExtractKind,
    w: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
    immediate: u8,
) -> [u8; 6] {
    let (map, opcode, w_mode, gpr_field, ..) = kind.fields();
    assert!(w_mode.values().contains(&w));
    assert!(destination < 16 && source < 16);
    let (reg, rm) = match gpr_field {
        GprField::Reg => (destination, source),
        GprField::Rm => (source, destination),
    };
    let mut p0 = 0xE0 | map;
    if reg >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(w) << 7),
        opcode,
        0xC0 | ((reg & 7) << 3) | (rm & 7),
        immediate,
    ]
}

fn c5_encoding(destination: u8, source: u8, immediate: u8) -> [u8; 5] {
    assert!(destination < 16 && source < 8);
    [
        0xC5,
        (if destination < 8 { 0x80 } else { 0 }) | 0x79,
        0xC5,
        0xC0 | ((destination & 7) << 3) | source,
        immediate,
    ]
}

fn assert_exact_register_lift(
    bytes: &[u8],
    kind: ExtractKind,
    destination: u8,
    source: u8,
    immediate: u8,
) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
    assert_eq!(lifted.ops.len(), 2, "{bytes:02X?}: {:#?}", lifted.ops);
    assert!(
        lifted
            .ops
            .iter()
            .all(|op| op.kind.flags_written().is_empty()),
        "{bytes:02X?}"
    );

    let (_, _, _, _, element, lane_mask, width, _) = kind.fields();
    let [
        SmirOp {
            kind:
                OpKind::VExtractLane {
                    dst: scalar,
                    vec,
                    lane,
                    elem,
                    sign,
                },
            ..
        },
        SmirOp {
            kind:
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(actual_scalar),
                    width: actual_width,
                },
            ..
        },
    ] = lifted.ops.as_slice()
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(
        *vec,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(source))),
        "{bytes:02X?}"
    );
    assert_eq!(*lane, immediate & lane_mask, "{bytes:02X?}");
    assert_eq!(*elem, element, "{bytes:02X?}");
    assert_eq!(*sign, SignExtend::Zero, "{bytes:02X?}");
    assert_eq!(*dst, x86_gpr(destination), "{bytes:02X?}");
    assert_eq!(*actual_scalar, *scalar, "{bytes:02X?}");
    assert_eq!(*actual_width, width, "{bytes:02X?}");
}

fn assert_invalid_encoding(bytes: &[u8]) {
    let error = lift_single(bytes).unwrap_err();
    match error {
        LiftError::InvalidEncoding {
            addr,
            bytes: reported,
        } => {
            assert_eq!(addr, 0x1000, "{bytes:02X?}");
            assert_eq!(reported, bytes, "{bytes:02X?}");
        }
        other => panic!("{bytes:02X?}: {other:?}"),
    }
}

#[test]
fn all_94_464_structural_samples_strictly_lift_with_exact_lane_equations() {
    let mut lifted = 0usize;
    for kind in ExtractKind::ALL {
        let (_, _, w_mode, _, ..) = kind.fields();
        for &w in w_mode.values() {
            for ignored_x in [false, true] {
                for destination in gprs() {
                    for source in 0..16 {
                        for immediate in IMMEDIATES {
                            let bytes =
                                c4_encoding(kind, w, ignored_x, destination, source, immediate);
                            assert_exact_register_lift(
                                &bytes,
                                kind,
                                destination,
                                source,
                                immediate,
                            );
                            lifted += 1;
                        }
                    }
                }
            }
        }
    }
    for destination in gprs() {
        for source in 0..8 {
            for immediate in IMMEDIATES {
                let bytes = c5_encoding(destination, source, immediate);
                assert_exact_register_lift(
                    &bytes,
                    ExtractKind::VpextrwMap1,
                    destination,
                    source,
                    immediate,
                );
                lifted += 1;
            }
        }
    }
    assert_eq!(lifted, 94_464);
}

#[test]
fn reserved_vvvv_l_pp_and_exact_byte_shapes_are_precise_invalid_frontiers() {
    for kind in ExtractKind::ALL {
        let (_, _, w_mode, _, ..) = kind.fields();
        for &w in w_mode.values() {
            let base = c4_encoding(kind, w, true, 9, 11, 0xFF);
            for raw_vvvv in 0u8..15 {
                let mut bytes = base;
                bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
                assert_invalid_encoding(&bytes);
                assert!(
                    !X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .is_vex_register_scalar_extract(),
                    "{kind:?} {bytes:02X?}"
                );
            }
            for p1 in [
                (base[2] & !0x03) | 0,
                (base[2] & !0x03) | 2,
                (base[2] & !0x03) | 3,
                base[2] | 0x04,
            ] {
                let mut bytes = base;
                bytes[2] = p1;
                assert_invalid_encoding(&bytes);
                assert!(
                    !X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .is_vex_register_scalar_extract(),
                    "{kind:?} {bytes:02X?}"
                );
            }
        }
    }

    let compact = c5_encoding(9, 3, 0xFF);
    for p1 in [0x71, 0x75, 0x7A, 0x7D] {
        let mut bytes = compact;
        bytes[1] = p1;
        assert_invalid_encoding(&bytes);
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            "{bytes:02X?}"
        );
    }

    let bytes = c4_encoding(ExtractKind::Vextractps, true, true, 9, 11, 0xFF);
    assert!(
        matches!(lift_single(&bytes[..5]), Err(LiftError::Incomplete { .. })),
        "{:02X?}",
        &bytes[..5]
    );
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    let lifted =
        lift_single(&trailing).unwrap_or_else(|error| panic!("{trailing:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{trailing:02X?}");
    assert!(
        !X86InstructionBytes::new(&trailing)
            .unwrap()
            .is_vex_register_scalar_extract(),
        "{trailing:02X?}"
    );
}

#[test]
fn rsp_rbp_destinations_lift_and_rewrite_without_host_stack_aliasing() {
    for destination in [4, 5] {
        for kind in ExtractKind::ALL {
            let (_, _, w_mode, _, ..) = kind.fields();
            for &w in w_mode.values() {
                let bytes = c4_encoding(kind, w, true, destination, 11, 0xFF);
                assert_exact_register_lift(&bytes, kind, destination, 11, 0xFF);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert!(
                    instruction.is_vex_register_scalar_extract(),
                    "{kind:?} {bytes:02X?}"
                );
                assert_eq!(
                    instruction.vex_scalar_extract_destination_index(),
                    Some(destination),
                    "{kind:?} {bytes:02X?}"
                );
                assert_eq!(
                    instruction
                        .vex_scalar_extract_with_destination(0)
                        .unwrap()
                        .vex_scalar_extract_destination_index(),
                    Some(0),
                    "{kind:?} {bytes:02X?}"
                );
            }
        }
        let bytes = c5_encoding(destination, 3, 0xFF);
        assert_exact_register_lift(&bytes, ExtractKind::VpextrwMap1, destination, 3, 0xFF);
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert!(instruction.is_vex_register_scalar_extract(), "{bytes:02X?}");
        assert_eq!(
            instruction.vex_scalar_extract_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction
                .vex_scalar_extract_with_destination(0)
                .unwrap()
                .vex_scalar_extract_destination_index(),
            Some(0),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn memory_destinations_retain_store_or_invalid_semantics_outside_replay() {
    for kind in [
        ExtractKind::Vextractps,
        ExtractKind::Vpextrb,
        ExtractKind::Vpextrd,
        ExtractKind::Vpextrq,
        ExtractKind::VpextrwMap3,
    ] {
        let (_, _, w_mode, _, _, _, _, memory_width) = kind.fields();
        let &w = w_mode.values().first().unwrap();
        let mut bytes = c4_encoding(kind, w, true, 1, 11, 0xFF);
        bytes[4] = 0x08;
        let lifted =
            lift_single(&bytes).unwrap_or_else(|error| panic!("{kind:?} {bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{kind:?} {bytes:02X?}");
        assert!(
            lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Store {
                    width,
                    ..
                } if width == memory_width
            )),
            "{kind:?} {bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            "{kind:?} {bytes:02X?}"
        );
    }

    for compact in [false, true] {
        let bytes = if compact {
            let mut bytes = c5_encoding(1, 3, 0xFF);
            bytes[3] = 0x08;
            bytes.to_vec()
        } else {
            let mut bytes = c4_encoding(ExtractKind::VpextrwMap1, false, true, 1, 11, 0xFF);
            bytes[4] = 0x08;
            bytes.to_vec()
        };
        assert_invalid_encoding(&bytes);
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vextractps_l0_is_valid_and_l1_is_invalid_despite_manual_exception_row_typo() {
    // Intel SDM revision 092's encoding table and VEXTRACTPS description define
    // VEX.128 (L=0) and #UD for L=1. Its "Other Exceptions" row reverses that
    // condition; this test follows the encoding table, operation prose, and
    // established decoder behavior.
    for w in [false, true] {
        let valid = c4_encoding(ExtractKind::Vextractps, w, true, 9, 11, 0x03);
        assert_exact_register_lift(&valid, ExtractKind::Vextractps, 9, 11, 0x03);

        let mut invalid = valid;
        invalid[2] |= 0x04;
        assert_invalid_encoding(&invalid);
        assert!(
            !X86InstructionBytes::new(&invalid)
                .unwrap()
                .is_vex_register_scalar_extract(),
            "{invalid:02X?}"
        );
    }
}
