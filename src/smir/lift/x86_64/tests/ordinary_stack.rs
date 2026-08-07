//! Exhaustive ordinary PUSH/POP width, alias-ordering, and strict-lift coverage.

use super::*;

const SCANNER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

// These are the scanner's exact legacy-prefix images that still select RSP
// in `FF /6`. REX.B images select R12 instead and therefore do not exercise
// the architectural pre-decrement RSP alias.
const GROUP5_RSP_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn rsp() -> VReg {
    x86_gpr(4)
}

fn stack_mem_width(delta: i64) -> MemWidth {
    match delta {
        2 => MemWidth::B2,
        8 => MemWidth::B8,
        _ => panic!("non-architectural ordinary stack delta: {delta}"),
    }
}

fn assert_stack_shape(result: &LiftResult, bytes: &[u8], delta: i64) {
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        matches!(result.control_flow, ControlFlow::Fallthrough),
        "{bytes:02X?}"
    );

    let stack_adjustments = result
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op.kind,
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(got),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if dst == rsp() && src1 == rsp() && got == delta
            ) || matches!(
                op.kind,
                OpKind::Add {
                    src1,
                    src2: SrcOperand::Imm(got),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                    ..
                } if src1 == rsp() && got == delta
            )
        })
        .count();
    assert_eq!(stack_adjustments, 1, "{bytes:02X?}: stack adjustment");

    let expected_mem_width = stack_mem_width(delta);
    let stack_accesses = result
        .ops
        .iter()
        .filter(|op| match &op.kind {
            OpKind::Load {
                addr: Address::Direct(base),
                width,
                ..
            }
            | OpKind::Store {
                addr: Address::Direct(base),
                width,
                ..
            } => *base == rsp() && *width == expected_mem_width,
            _ => false,
        })
        .count();
    assert_eq!(stack_accesses, 1, "{bytes:02X?}: stack memory width");
}

fn lift_exact(bytes: &[u8]) -> LiftResult {
    lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"))
}

#[test]
fn push_imm32_66_rex_w_consumes_all_256_scanner_images() {
    for low in 0_u8..=u8::MAX {
        let bytes = [0x66, 0x48, 0x68, low, 0x00, 0x00, 0x00];
        let result = lift_exact(&bytes);
        assert_stack_shape(&result, &bytes, 8);
        assert_eq!(result.ops.len(), 2, "{bytes:02X?}");
        match &result.ops[1] {
            SmirOp {
                kind:
                    OpKind::Store {
                        src: VReg::Imm(value),
                        addr: Address::Direct(base),
                        width: MemWidth::B8,
                    },
                x86_hint: Some(X86OpHint::PushImm32),
                ..
            } => {
                assert_eq!(*value, i64::from(low), "{bytes:02X?}");
                assert_eq!(*base, rsp(), "{bytes:02X?}");
            }
            other => panic!("{bytes:02X?}: unexpected PUSH imm32 store: {other:?}"),
        }
    }
}

#[test]
fn all_scanner_prefixes_select_one_exact_stack_width_for_every_ordinary_form() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        let delta = if *prefix == [0x66] { 2 } else { 8 };
        let mut forms = vec![
            vec![0x50],
            vec![0x58],
            vec![0x6A, 0x80],
            vec![0xFF, 0xF0],
            vec![0x8F, 0xC0],
        ];
        forms.push(if delta == 2 {
            vec![0x68, 0x34, 0x80]
        } else {
            vec![0x68, 0x78, 0x56, 0x34, 0x80]
        });

        for form in forms {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&form);
            let result = lift_exact(&bytes);
            assert_stack_shape(&result, &bytes, delta);
            images += 1;
        }
    }
    assert_eq!(images, 84);
}

#[test]
fn group5_push_rsp_scanner_images_snapshot_the_predecrement_value() {
    let mut images = 0usize;
    for prefix in GROUP5_RSP_PREFIXES {
        let delta = if *prefix == [0x66] { 2 } else { 8 };
        let expected_width = if delta == 2 {
            OpWidth::W16
        } else {
            OpWidth::W64
        };
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(&[0xFF, 0xF4]);
        let result = lift_exact(&bytes);
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert_eq!(result.ops.len(), 3, "{bytes:02X?}");

        let snapshot = match result.ops[0].kind {
            OpKind::Mov {
                dst: temporary @ VReg::Virtual(_),
                src: SrcOperand::Reg(source),
                width,
            } if source == rsp() && width == expected_width => temporary,
            ref other => panic!("{bytes:02X?}: missing old-RSP snapshot: {other:?}"),
        };
        assert!(matches!(
            result.ops[1].kind,
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(got),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == rsp() && src1 == rsp() && got == delta
        ));
        assert!(matches!(
            result.ops[2].kind,
            OpKind::Store {
                src,
                addr: Address::Direct(base),
                width,
            } if src == snapshot && base == rsp() && width == stack_mem_width(delta)
        ));
        images += 1;
    }
    assert_eq!(images, 12);
}

#[test]
fn group5_memory_push_reads_old_rsp_before_decrement_at_the_exact_width() {
    for bytes in [
        &[0xFF, 0x34, 0x24][..],
        &[0x66, 0xFF, 0x34, 0x24][..],
        &[0x66, 0x48, 0xFF, 0x34, 0x24][..],
    ] {
        let delta = if bytes[0] == 0x66 && bytes[1] != 0x48 {
            2
        } else {
            8
        };
        let expected_width = stack_mem_width(delta);
        let result = lift_exact(bytes);
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert_eq!(result.ops.len(), 3, "{bytes:02X?}");
        let temporary = match result.ops[0].kind {
            OpKind::Load {
                dst: temporary @ VReg::Virtual(_),
                addr: Address::Direct(base),
                width,
                sign: SignExtend::Zero,
            } if base == rsp() && width == expected_width => temporary,
            ref other => panic!("{bytes:02X?}: source must load before SUB: {other:?}"),
        };
        assert!(matches!(
            result.ops[1].kind,
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(got),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == rsp() && src1 == rsp() && got == delta
        ));
        assert!(matches!(
            result.ops[2].kind,
            OpKind::Store {
                src,
                addr: Address::Direct(base),
                width,
            } if src == temporary && base == rsp() && width == expected_width
        ));
    }
}

#[test]
fn complete_rex2_payload_space_preserves_w_precedence_for_every_ordinary_form() {
    let mut images = 0usize;
    for payload in 0_u8..=0x7F {
        let delta = if payload & 0x08 == 0 { 2 } else { 8 };
        let mut forms = vec![
            vec![0x50],
            vec![0x58],
            vec![0x6A, 0x80],
            vec![0xFF, 0xF0],
            vec![0x8F, 0xC0],
        ];
        forms.push(if delta == 2 {
            vec![0x68, 0x34, 0x80]
        } else {
            vec![0x68, 0x78, 0x56, 0x34, 0x80]
        });

        for form in forms {
            let mut bytes = vec![0x66, 0xD5, payload];
            bytes.extend_from_slice(&form);
            let result = lift_exact(&bytes);
            assert_stack_shape(&result, &bytes, delta);
            images += 1;
        }
    }
    assert_eq!(images, 768);
}

#[test]
fn all_112_register_pop_rm_images_use_the_canonical_pop_graph() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        for rm in 0_u8..8 {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x8F, 0xC0 | rm]);
            let result = lift_exact(&bytes);

            let mut canonical = prefix.to_vec();
            canonical.push(0x58 | rm);
            let canonical = lift_exact(&canonical);
            assert_eq!(
                format!("{:?}", result.ops),
                format!("{:?}", canonical.ops),
                "{bytes:02X?}: 8F /0 and 58+rd must share one semantic graph"
            );
            images += 1;
        }
    }
    assert_eq!(images, 112);
}

#[test]
fn rex_must_be_the_final_legacy_prefix_to_override_operand_size() {
    for (bytes, delta) in [
        (&[0x48, 0x66, 0x50][..], 2),
        (&[0x66, 0x48, 0x50][..], 8),
        (&[0x48, 0x66, 0x58][..], 2),
        (&[0x66, 0x48, 0x58][..], 8),
        (&[0x48, 0x66, 0x6A, 0x80][..], 2),
        (&[0x66, 0x48, 0x6A, 0x80][..], 8),
        (&[0x48, 0x66, 0x68, 0x34, 0x80][..], 2),
        (&[0x66, 0x48, 0x68, 0x78, 0x56, 0x34, 0x80][..], 8),
        (&[0x48, 0x66, 0xFF, 0xF0][..], 2),
        (&[0x66, 0x48, 0xFF, 0xF0][..], 8),
        (&[0x48, 0x66, 0x8F, 0xC0][..], 2),
        (&[0x66, 0x48, 0x8F, 0xC0][..], 8),
    ] {
        let result = lift_exact(bytes);
        assert_stack_shape(&result, bytes, delta);
    }
}
