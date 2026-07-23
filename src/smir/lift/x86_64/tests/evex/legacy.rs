//! evex::legacy tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_legacy_vex_evex_saturating_add_subtract_covers_all_eight_opcodes() {
    for (opcode, elem, subtract, signed, legacy_lanes) in [
        (0xEC, VecElementType::I8, false, true, 16u8),
        (0xED, VecElementType::I16, false, true, 8),
        (0xDC, VecElementType::I8, false, false, 16),
        (0xDD, VecElementType::I16, false, false, 8),
        (0xE8, VecElementType::I8, true, true, 16),
        (0xE9, VecElementType::I16, true, true, 8),
        (0xD8, VecElementType::I8, true, false, 16),
        (0xD9, VecElementType::I16, true, false, 8),
    ] {
        let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::VAddSubSat {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: actual_elem,
                        lanes,
                        subtract: actual_subtract,
                        signed: actual_signed,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                }
            ] if *actual_elem == elem && *lanes == legacy_lanes / 2
                && *actual_subtract == subtract && *actual_signed == signed
                && *actual_opcode == opcode
        ));

        let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
        assert!(matches!(
            legacy.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VAddSubSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: actual_elem,
                    lanes,
                    subtract: actual_subtract,
                    signed: actual_signed,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                }),
                ..
            }] if *actual_elem == elem && *lanes == legacy_lanes
                && *actual_subtract == subtract && *actual_signed == signed
                && *actual_opcode == opcode
        ));

        let vex = lift_single(&[0xC5, 0xF5, opcode, 0xC2]).unwrap();
        assert!(matches!(
            vex.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VAddSubSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    elem: actual_elem,
                    lanes,
                    subtract: actual_subtract,
                    signed: actual_signed,
                },
                ..
            }] if *actual_elem == elem && *lanes == legacy_lanes * 2
                && *actual_subtract == subtract && *actual_signed == signed
        ));

        let evex = lift_single(&[0x62, 0xF1, 0x7D, 0xC9, opcode, 0xD1]).unwrap();
        let expected_lanes = if elem == VecElementType::I8 { 64 } else { 32 };
        let select_width = if elem == VecElementType::I8 {
            OpWidth::W8
        } else {
            OpWidth::W16
        };
        assert!(evex.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAddSubSat {
                elem: actual_elem,
                lanes,
                subtract: actual_subtract,
                signed: actual_signed,
                ..
            } if actual_elem == elem && usize::from(lanes) == expected_lanes
                && actual_subtract == subtract && actual_signed == signed
        )));
        assert_eq!(
            evex.ops
                .iter()
                .filter(
                    |op| matches!(op.kind, OpKind::Select { width, .. } if width == select_width)
                )
                .count(),
            expected_lanes,
        );
    }

    // EVEX.W is ignored for all byte/word saturating forms.
    let wig = lift_single(&[0x62, 0xF1, 0xFD, 0x48, 0xEC, 0xC1]).unwrap();
    assert!(matches!(
        wig.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VAddSubSat {
                elem: VecElementType::I8,
                lanes: 64,
                ..
            },
            ..
        }]
    ));

    let masked_memory = lift_single(&[0x62, 0xF1, 0x7D, 0x49, 0xED, 0x50, 0x02]).unwrap();
    assert_eq!(
        masked_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32,
    );
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 128, .. },
            ..
        }
    )));

    for bytes in [
        &[0xF3, 0x0F, 0xEC, 0xC1][..],             // invalid mandatory prefix
        &[0xF0, 0x66, 0x0F, 0xEC, 0xC1][..],       // LOCK is undefined
        &[0xC5, 0xF4, 0xEC, 0xC1][..],             // VEX requires 66
        &[0x62, 0xF1, 0x7D, 0x58, 0xEC, 0x00][..], // no broadcast form
        &[0x62, 0xF1, 0x7D, 0xC8, 0xEC, 0xC1][..], // {z} requires k1-k7
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid saturating packed arithmetic accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_vex_evex_comi_ucomi() {
    for (bytes, elem, signaling) in [
        (&[0x0F, 0x2E, 0xC1][..], VecElementType::F32, false),
        (&[0x66, 0x0F, 0x2F, 0xC1][..], VecElementType::F64, true),
        (&[0xC5, 0xF8, 0x2E, 0xC1][..], VecElementType::F32, false),
        (&[0xC5, 0xF9, 0x2F, 0xC1][..], VecElementType::F64, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86FpCompare {
                elem: actual_elem,
                signaling: actual_signaling,
                ..
            } if actual_elem == elem && actual_signaling == signaling
        ));
    }

    let high = lift_single(&[0x62, 0xA1, 0x7C, 0x08, 0x2E, 0xD1]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86FpCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            elem: VecElementType::F32,
            signaling: false,
        }
    ));

    let compressed = lift_single(&[0x62, 0xE1, 0xFD, 0x08, 0x2F, 0x50, 0x08]).unwrap();
    assert!(compressed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(matches!(
        compressed.ops.last().unwrap().kind,
        OpKind::X86FpCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            elem: VecElementType::F64,
            signaling: true,
            ..
        }
    ));

    for bytes in [
        &[0xF0, 0x0F, 0x2E, 0xC1][..],             // LOCK
        &[0xF3, 0x0F, 0x2E, 0xC1][..],             // reserved legacy prefix
        &[0xC5, 0xF0, 0x2E, 0xC1][..],             // reserved VEX.vvvv
        &[0x62, 0xF1, 0xFC, 0x08, 0x2E, 0xC1][..], // VUCOMISS W=1
        &[0x62, 0xF1, 0x7D, 0x08, 0x2F, 0xC1][..], // VCOMISD W=0
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_evex_movd_movq_covers_widths_extensions_and_reserved_fields() {
    let legacy_d = lift_single(&[0x66, 0x0F, 0x6E, 0xC1]).unwrap();
    assert_eq!(legacy_d.bytes_consumed, 4);
    assert!(matches!(
        legacy_d.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                width: OpWidth::W32,
                zero_upper: false,
            },
            x86_hint: Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            }),
            ..
        }]
    ));

    let legacy_q_reg = lift_single(&[0x66, 0x4D, 0x0F, 0x6E, 0xCA]).unwrap();
    assert!(matches!(
        legacy_q_reg.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                src: VReg::Arch(ArchReg::X86(X86Reg::R10)),
                width: OpWidth::W64,
                zero_upper: false,
            },
            ..
        }]
    ));

    let legacy_q_mem = lift_single(&[0x66, 0x48, 0x0F, 0x6E, 0x00]).unwrap();
    assert!(legacy_q_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(legacy_q_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I64,
            ..
        }
    )));

    let legacy_d_store = lift_single(&[0x66, 0x0F, 0x7E, 0xC1]).unwrap();
    assert!(matches!(
        legacy_d_store.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: OpWidth::W32,
                zero_upper: false,
            },
            ..
        }]
    ));
    let legacy_q_store = lift_single(&[0x66, 0x48, 0x0F, 0x7E, 0x00]).unwrap();
    assert!(matches!(
        legacy_q_store.ops.last().unwrap().kind,
        OpKind::Store {
            width: MemWidth::B8,
            ..
        }
    ));

    let vex_d = lift_single(&[0xC5, 0xF9, 0x6E, 0xC1]).unwrap();
    assert!(matches!(
        vex_d.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                width: OpWidth::W32,
                zero_upper: true,
            },
            x86_hint: Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: false,
            }),
            ..
        }]
    ));
    let vex_q_store = lift_single(&[0xC4, 0xE1, 0xF9, 0x7E, 0xC1]).unwrap();
    assert!(matches!(
        vex_q_store.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: OpWidth::W64,
                zero_upper: false,
            },
            ..
        }]
    ));

    // EVEX.R' selects XMM17 and EVEX.B selects R8. The register form must
    // not interpret EVEX.X as a fifth GPR index bit.
    let evex_high = lift_single(&[0x62, 0xC1, 0x7D, 0x08, 0x6E, 0xC8]).unwrap();
    assert!(matches!(
        evex_high.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86MovdQ {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                src: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                width: OpWidth::W32,
                zero_upper: true,
            },
            ..
        }]
    ));

    let evex_q_mem = lift_single(&[0x62, 0xF1, 0xFD, 0x08, 0x6E, 0x40, 0x10]).unwrap();
    assert!(evex_q_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                offset: 128,
                disp_size: DispSize::Disp8,
            },
            width: MemWidth::B8,
            ..
        }
    )));

    for bytes in [
        &[0xF3, 0x0F, 0x6E, 0xC1][..],             // mandatory prefix is 66
        &[0xF0, 0x66, 0x0F, 0x7E, 0xC1][..],       // LOCK is undefined
        &[0xC5, 0xFD, 0x6E, 0xC1][..],             // VEX.L=1
        &[0xC5, 0xF1, 0x6E, 0xC1][..],             // VEX.vvvv != 1111b
        &[0xC5, 0xF8, 0x6E, 0xC1][..],             // VEX.pp != 66
        &[0x62, 0xF1, 0x7D, 0x28, 0x6E, 0xC1][..], // EVEX.L'L != 00b
        &[0x62, 0xF1, 0x75, 0x08, 0x6E, 0xC1][..], // EVEX.vvvv != 1111b
        &[0x62, 0xF1, 0x7D, 0x00, 0x6E, 0xC1][..], // EVEX.V' != 1b
        &[0x62, 0xB1, 0x7D, 0x08, 0x6E, 0xC1][..], // no GPR bit 4
        &[0x62, 0xF1, 0x7D, 0x09, 0x6E, 0xC1][..], // writemasks reserved
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "reserved MOVD/MOVQ encoding accepted: {bytes:02X?}",
        );
    }
    let mmx_d = lift_single(&[0x0F, 0x6E, 0xC1]).unwrap();
    assert_eq!(mmx_d.bytes_consumed, 3);
    assert!(matches!(
        mmx_d.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86MovdQ {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                    width: OpWidth::W32,
                    zero_upper: false,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x6E,
                }),
                ..
            }
        ]
    ));

    // REX.W selects the 64-bit transfer; REX.R does not extend the
    // three-bit MMX register namespace.
    let mmx_q_reg = lift_single(&[0x4C, 0x0F, 0x6E, 0xCA]).unwrap();
    assert!(matches!(
        mmx_q_reg.ops.last().unwrap().kind,
        OpKind::X86MovdQ {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            width: OpWidth::W64,
            ..
        }
    ));

    let mmx_q_load = lift_single(&[0x48, 0x0F, 0x6E, 0x00]).unwrap();
    assert!(matches!(
        mmx_q_load.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::Load {
                    width: MemWidth::B8,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86MovdQ {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    width: OpWidth::W64,
                    ..
                },
                ..
            }
        ]
    ));

    let mmx_q_store = lift_single(&[0x48, 0x0F, 0x7E, 0x00]).unwrap();
    assert!(matches!(
        mmx_q_store.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86MovdQ {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    width: OpWidth::W64,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::Store {
                    width: MemWidth::B8,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            }
        ]
    ));
}
#[test]
fn lift_legacy_vex_evex_scalar_vector_movq_covers_load_store_and_invalid_forms() {
    let legacy_load = lift_single(&[0xF3, 0x0F, 0x7E, 0xC1]).unwrap();
    assert_eq!(legacy_load.bytes_consumed, 4);
    assert!(matches!(
        legacy_load.ops.first().unwrap().kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            elem: VecElementType::I64,
            lane: 0,
            ..
        }
    ));
    assert!(legacy_load.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I64,
            lane: 0,
            ..
        }
    )));

    let legacy_load_mem = lift_single(&[0xF3, 0x0F, 0x7E, 0x00]).unwrap();
    assert!(matches!(
        legacy_load_mem.ops.first().unwrap().kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    ));
    let legacy_store = lift_single(&[0x66, 0x0F, 0xD6, 0xC1]).unwrap();
    assert!(matches!(
        legacy_store.ops.first().unwrap().kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I64,
            ..
        }
    ));
    let legacy_store_mem = lift_single(&[0x66, 0x0F, 0xD6, 0x00]).unwrap();
    assert!(matches!(
        legacy_store_mem.ops.last().unwrap().kind,
        OpKind::Store {
            width: MemWidth::B8,
            ..
        }
    ));

    let vex_load = lift_single(&[0xC5, 0xFA, 0x7E, 0xC1]).unwrap();
    assert!(vex_load.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I64,
            lanes: 1,
            ..
        }
    )));
    let vex_store = lift_single(&[0xC5, 0xF9, 0xD6, 0xC1]).unwrap();
    assert!(vex_store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            elem: VecElementType::I64,
            lane: 0,
            ..
        }
    )));
    // VEX.W is ignored for both forms.
    assert!(lift_single(&[0xC4, 0xE1, 0xFA, 0x7E, 0xC1]).is_ok());
    assert!(lift_single(&[0xC4, 0xE1, 0xF9, 0xD6, 0xC1]).is_ok());

    // EVEX.R' selects XMM17, EVEX.X selects XMM18 for a register
    // operand, and disp8*N uses the scalar tuple size N=8 bytes.
    let evex_high_load = lift_single(&[0x62, 0xA1, 0xFE, 0x08, 0x7E, 0xCA]).unwrap();
    assert!(evex_high_load.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            ..
        }
    )));
    assert!(evex_high_load.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            ..
        }
    )));
    let evex_mem_store = lift_single(&[0x62, 0xF1, 0xFD, 0x08, 0xD6, 0x40, 0x10]).unwrap();
    assert!(evex_mem_store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Store {
            addr: Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                offset: 128,
                disp_size: DispSize::Disp8,
            },
            width: MemWidth::B8,
            ..
        }
    )));

    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0x7E, 0xC1][..], // LOCK
        &[0x66, 0xF3, 0x0F, 0x7E, 0xC1][..], // conflicting mandatory prefixes
        &[0xF3, 0x66, 0x0F, 0xD6, 0xC1][..],
        &[0xC5, 0xFE, 0x7E, 0xC1][..],             // VEX.L=1
        &[0xC5, 0xF2, 0x7E, 0xC1][..],             // VEX.vvvv != 1111b
        &[0xC5, 0xFA, 0xD6, 0xC1][..],             // wrong pp
        &[0x62, 0xF1, 0x7E, 0x08, 0x7E, 0xC1][..], // EVEX.W=0
        &[0x62, 0xF1, 0xFE, 0x28, 0x7E, 0xC1][..], // EVEX.L'L != 00b
        &[0x62, 0xF1, 0xF6, 0x08, 0x7E, 0xC1][..], // EVEX.vvvv != 1111b
        &[0x62, 0xF1, 0xFE, 0x00, 0x7E, 0xC1][..], // EVEX.V' != 1b
        &[0x62, 0xF1, 0xFE, 0x09, 0x7E, 0xC1][..], // writemask
        &[0x62, 0xF1, 0xFD, 0x18, 0xD6, 0xC1][..], // EVEX.b
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "reserved scalar vector MOVQ encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_duplicate_moves_covers_legacy_vex_evex_memory_and_invalids() {
    for bytes in [
        &[0xF3, 0x45, 0x0F, 0x12, 0xCA][..],
        &[0xF3, 0x44, 0x0F, 0x16, 0x48, 0x11][..],
        &[0xF2, 0x44, 0x0F, 0x12, 0x48, 0x11][..],
        &[0xC4, 0x41, 0x7E, 0x12, 0xCA][..],
        &[0xC4, 0x41, 0x7A, 0x16, 0xCA][..],
        &[0xC5, 0x7B, 0x12, 0x48, 0x11][..],
        &[0xC4, 0x41, 0x7F, 0x12, 0xCA][..],
        &[0x62, 0xA1, 0x7E, 0xCB, 0x12, 0xCA][..],
        &[0x62, 0xE1, 0x7E, 0x4B, 0x16, 0x48, 0x7F][..],
        &[0x62, 0xA1, 0xFF, 0x4B, 0x12, 0xCA][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VShuffle { src2: None, .. }))
        );
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }
    let scalar = lift_single(&[0xC5, 0x7B, 0x12, 0x48, 0x11]).unwrap();
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    let full = lift_single(&[0x62, 0xE1, 0x7E, 0x4B, 0x16, 0x48, 0x7F]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            addr: Address::BaseOffset { offset: 8128, .. },
            ..
        }
    )));
    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0x12, 0xCA][..],
        &[0xF3, 0x66, 0x0F, 0x12, 0xCA][..],
        &[0xC4, 0x41, 0x76, 0x12, 0xCA][..],
        &[0x62, 0xA1, 0xFE, 0x4B, 0x12, 0xCA][..],
        &[0x62, 0xA1, 0x7E, 0x5B, 0x12, 0xCA][..],
        &[0x62, 0xA1, 0x7E, 0x80, 0x12, 0xCA][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid duplicate move accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_legacy_vex_evex_non_temporal_vector_stores() {
    let mmx = lift_single(&[0x0F, 0xE7, 0x08]).unwrap();
    assert!(matches!(
        mmx.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VStore {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                    ..
                },
                x86_hint: Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            }
        ]
    ));
    assert!(
        !mmx.ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    for (bytes, width, alignment) in [
        (&[0x0F, 0x2B, 0x08][..], VecWidth::V128, 16),
        (
            &[0x66, 0x44, 0x0F, 0x2B, 0x48, 0x10][..],
            VecWidth::V128,
            16,
        ),
        (
            &[0x66, 0x44, 0x0F, 0xE7, 0x50, 0x20][..],
            VecWidth::V128,
            16,
        ),
        (&[0xC5, 0xFC, 0x2B, 0x50, 0x20][..], VecWidth::V256, 32),
        (
            &[0x62, 0xE1, 0xFD, 0x08, 0x2B, 0x48, 0x01][..],
            VecWidth::V128,
            16,
        ),
        (
            &[0x62, 0xE1, 0x7D, 0x28, 0xE7, 0x50, 0x01][..],
            VecWidth::V256,
            32,
        ),
        (
            &[0x62, 0xE1, 0x7C, 0x48, 0x2B, 0x58, 0x01][..],
            VecWidth::V512,
            64,
        ),
        (
            &[0x62, 0xE1, 0xFD, 0x48, 0x2B, 0x60, 0x02][..],
            VecWidth::V512,
            64,
        ),
        (
            &[0x62, 0xE1, 0x7D, 0x48, 0xE7, 0x68, 0x03][..],
            VecWidth::V512,
            64,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86CheckAlignment { alignment: actual, .. } if actual == alignment
        )));
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VStore { width: actual, .. } if actual == width
        )));
    }
    let high = lift_single(&[0x62, 0xE1, 0x7D, 0x48, 0xE7, 0x68, 0x03]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VStore {
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(21))),
            addr: Address::BaseOffset { offset: 192, .. },
            ..
        }
    )));
    for bytes in [
        &[0x0F, 0x2B, 0xC1][..],
        &[0x0F, 0xE7, 0xC1][..],
        &[0x66, 0x0F, 0xE7, 0xC1][..],
        &[0xC5, 0xEC, 0x2B, 0x08][..],
        &[0x62, 0xF1, 0x7C, 0x49, 0x2B, 0x08][..],
        &[0x62, 0xF1, 0xFC, 0x48, 0x2B, 0x08][..],
        &[0x62, 0xF1, 0x7C, 0x58, 0x2B, 0x08][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid MOVNT vector store accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_legacy_vex_evex_half_vector_move_family() {
    for bytes in [
        &[0x0F, 0x12, 0x08][..],
        &[0x0F, 0x13, 0x50, 0x08][..],
        &[0x0F, 0x16, 0x18][..],
        &[0x0F, 0x17, 0x60, 0x08][..],
        &[0x0F, 0x12, 0xEE][..],
        &[0x41, 0x0F, 0x16, 0xF8][..],
        &[0x66, 0x44, 0x0F, 0x12, 0x08][..],
        &[0x66, 0x44, 0x0F, 0x17, 0x50, 0x08][..],
        &[0xC5, 0xE8, 0x12, 0x08][..],
        &[0xC5, 0xD8, 0x16, 0x18][..],
        &[0xC5, 0xD0, 0x12, 0xE6][..],
        &[0xC4, 0xC1, 0x38, 0x16, 0xF9][..],
        &[0xC5, 0x79, 0x13, 0x50, 0x08][..],
        &[0xC5, 0x19, 0x16, 0x18][..],
        &[0x62, 0xA1, 0x6C, 0x00, 0x12, 0xCB][..],
        &[0x62, 0xA1, 0x54, 0x00, 0x16, 0xE6][..],
        &[0x62, 0xE1, 0xBD, 0x00, 0x12, 0x78, 0x08][..],
        &[0x62, 0x61, 0xFD, 0x08, 0x17, 0x48, 0x08][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    }

    let movhl = lift_single(&[0x62, 0xA1, 0x6C, 0x00, 0x12, 0xCB]).unwrap();
    assert!(movhl.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
            lane: 1,
            ..
        }
    )));
    assert!(movhl.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            width: VecWidth::V128,
        }
    )));
    let compressed = lift_single(&[0x62, 0xE1, 0xBD, 0x00, 0x12, 0x78, 0x08]).unwrap();
    assert!(compressed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 64, .. },
            width: MemWidth::B8,
            ..
        }
    )));

    for bytes in [
        &[0x66, 0x0F, 0x12, 0xC1][..],
        &[0x0F, 0x13, 0xC1][..],
        &[0xC5, 0xEC, 0x12, 0x08][..],
        &[0xC5, 0xE8, 0x13, 0x08][..],
        &[0x62, 0xF1, 0x7C, 0x01, 0x12, 0x08][..],
        &[0x62, 0xF1, 0x7D, 0x00, 0x12, 0x08][..],
        &[0x62, 0xF1, 0xFC, 0x00, 0x12, 0xC1][..],
        &[0x62, 0xF1, 0x7C, 0x10, 0x12, 0xC1][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid half-vector move accepted: {bytes:02X?}"
        );
    }
}
