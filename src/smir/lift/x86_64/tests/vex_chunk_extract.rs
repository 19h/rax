//! Exhaustive strict-lift coverage for register-destination AVX/AVX2 VEX
//! 128-bit chunk extracts.

use super::*;

const IMMEDIATES: [u8; 6] = [0x00, 0x01, 0x7E, 0x81, 0xFE, 0xFF];

fn encoding(
    needs_avx2: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
    immediate: u8,
) -> [u8; 6] {
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE3;
    if source >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if destination >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x7D,
        if needs_avx2 { 0x39 } else { 0x19 },
        0xC0 | ((source & 7) << 3) | (destination & 7),
        immediate,
    ]
}

fn assert_exact_register_lift(bytes: &[u8], destination: u8, source: u8, immediate: u8) {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
    assert!(
        lifted
            .ops
            .iter()
            .all(|op| op.kind.flags_written().is_empty()),
        "{bytes:02X?}"
    );

    let final_move = lifted
        .ops
        .last()
        .unwrap_or_else(|| panic!("{bytes:02X?}: missing VEX chunk extract"));
    let OpKind::VMov {
        dst,
        src: assembled,
        width,
    } = final_move.kind
    else {
        panic!("{bytes:02X?}: {:#?}", lifted.ops)
    };
    assert_eq!(
        dst,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(destination))),
        "{bytes:02X?}"
    );
    assert_eq!(width, VecWidth::V128, "{bytes:02X?}");

    let first_lane = (immediate & 1) * 2;
    for output_lane in 0..2 {
        let expected_source_lane = first_lane + output_lane;
        let pair = lifted.ops.windows(2).find(|pair| {
            matches!(
                pair,
                [
                    SmirOp {
                        kind: OpKind::VExtractLane {
                            dst: scalar,
                            vec: VReg::Arch(ArchReg::X86(X86Reg::Ymm(actual_source))),
                            lane: actual_source_lane,
                            elem: VecElementType::I64,
                            sign: SignExtend::Zero,
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VInsertLane {
                            dst: actual_assembled,
                            vec,
                            scalar: actual_scalar,
                            lane: actual_output_lane,
                            elem: VecElementType::I64,
                        },
                        ..
                    },
                ] if *actual_source == source
                    && *actual_source_lane == expected_source_lane
                    && *actual_assembled == assembled
                    && *vec == assembled
                    && *actual_scalar == *scalar
                    && *actual_output_lane == output_lane
            )
        });
        assert!(
            pair.is_some(),
            "{bytes:02X?}: missing lane {expected_source_lane}->{output_lane}: {:#?}",
            lifted.ops
        );
    }
}

#[test]
fn all_6144_structural_samples_strictly_lift_with_exact_lane_equations() {
    let mut lifted = 0usize;
    for needs_avx2 in [false, true] {
        for ignored_x in [false, true] {
            for destination in 0u8..16 {
                for source in 0u8..16 {
                    for immediate in IMMEDIATES {
                        let bytes = encoding(needs_avx2, ignored_x, destination, source, immediate);
                        assert_exact_register_lift(&bytes, destination, source, immediate);
                        lifted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 6_144);
}

#[test]
fn reserved_vvvv_w_l_pp_and_byte_shapes_are_precise_invalid_frontiers() {
    for needs_avx2 in [false, true] {
        for ignored_x in [false, true] {
            let base = encoding(needs_avx2, ignored_x, 9, 11, 0xFE);
            for raw_vvvv in 0u8..15 {
                let mut bytes = base;
                bytes[2] = (bytes[2] & !0x78) | (raw_vvvv << 3);
                assert_invalid_opcode_trap(&lift_single(&bytes).unwrap(), 4);
                assert!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_chunk_extract_needs_avx2()
                        .is_none(),
                    "{bytes:02X?}"
                );
            }

            for p1 in [base[2] | 0x80, base[2] & !0x04, 0x7C, 0x7E, 0x7F] {
                let mut bytes = base;
                bytes[2] = p1;
                assert_invalid_opcode_trap(&lift_single(&bytes).unwrap(), 4);
                assert!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_chunk_extract_needs_avx2()
                        .is_none(),
                    "{bytes:02X?}"
                );
            }
        }
    }

    let bytes = encoding(false, true, 9, 11, 0xFF);
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
        X86InstructionBytes::new(&trailing)
            .unwrap()
            .vex_register_chunk_extract_needs_avx2()
            .is_none(),
        "{trailing:02X?}"
    );
}

#[test]
fn representative_memory_destinations_lift_but_never_enter_native_replay() {
    let cases: &[&[u8]] = &[
        &[0xC4, 0xE3, 0x7D, 0x19, 0x00, 0x00],
        &[0xC4, 0x63, 0x7D, 0x19, 0x48, 0x20, 0xFF],
        &[0xC4, 0xE3, 0x7D, 0x39, 0x10, 0x01],
        &[0xC4, 0x63, 0x7D, 0x39, 0x58, 0x20, 0xFE],
    ];
    for &bytes in cases {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VStore { .. })),
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_chunk_extract_needs_avx2()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}
