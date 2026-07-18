//! evex::mask tests

use super::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::x86_64::tests::*;

    #[test]
    fn lift_evex_packed_logic_models_masks_broadcast_high_regs_and_invalid_forms() {
        let high = lift_single(&[0x62, 0xA1, 0x7C, 0x40, 0x55, 0xD1]).unwrap();
        assert!(matches!(
            high.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VAndNot {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    width: VecWidth::V512,
                },
                ..
            }]
        ));

        for (opcode, expected) in [(0x54, "and"), (0x55, "andn"), (0x56, "or"), (0x57, "xor")] {
            let result = lift_single(&[0x62, 0xF1, 0x7C, 0xC9, opcode, 0xD1]).unwrap();
            assert!(result.ops.iter().any(|op| match (&op.kind, expected) {
                (OpKind::VAnd { width, .. }, "and")
                | (OpKind::VAndNot { width, .. }, "andn")
                | (OpKind::VOr { width, .. }, "or")
                | (OpKind::VXor { width, .. }, "xor") => *width == VecWidth::V512,
                _ => false,
            }));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::Select {
                            width: OpWidth::W32,
                            ..
                        }
                    ))
                    .count(),
                16,
                "opcode {opcode:02X}: one mask select per dword lane",
            );
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    elem: VecElementType::F32,
                    lane: 15,
                    ..
                }
            )));
        }
        let pd = lift_single(&[0x62, 0xF1, 0xFD, 0x49, 0x55, 0xD1]).unwrap();
        assert_eq!(
            pd.ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Select {
                        width: OpWidth::W64,
                        ..
                    }
                ))
                .count(),
            8,
        );
        assert!(pd.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                elem: VecElementType::F64,
                lane: 7,
                ..
            }
        )));

        let masked_memory = lift_single(&[0x62, 0xF1, 0x7C, 0x49, 0x55, 0x50, 0x02]).unwrap();
        assert_eq!(
            masked_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        assert!(masked_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 128,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));

        let broadcast = lift_single(&[0x62, 0xF1, 0x7C, 0x59, 0x55, 0x50, 0x08]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            1,
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::F32,
                lanes: 16,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF1, 0xFC, 0x48, 0x55, 0xC1][..], // VANDNPS requires W0
            &[0x62, 0xF1, 0x7D, 0x48, 0x55, 0xC1][..], // VANDNPD requires W1
            &[0x62, 0xF1, 0x7C, 0xC8, 0x55, 0xC1][..], // {z} requires a mask
            &[0x62, 0xF1, 0x7C, 0x58, 0x55, 0xC1][..], // EVEX.b reserved for reg
            &[0x62, 0xF1, 0x7C, 0x58, 0x57, 0x00][..], // VXORPS has no broadcast
            &[0x62, 0xF1, 0x7C, 0x68, 0x55, 0xC1][..], // LL=3 is reserved
            &[0x62, 0xF1, 0x7E, 0x48, 0x55, 0xC1][..], // F3 form is undefined
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid EVEX packed logic accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_vex_evex_gather_covers_vsib_shapes_masks_extensions_and_invalids() {
        for (bytes, elem, lanes, dst, index, pred_width) in [
            (
                &[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x90][..],
                VecElementType::I32,
                8usize,
                X86Reg::Ymm(3),
                X86Reg::Ymm(2),
                MemWidth::B4,
            ),
            (
                &[0xC4, 0xE2, 0x75, 0x91, 0x5C, 0x90, 0x04][..],
                VecElementType::I32,
                4,
                X86Reg::Xmm(3),
                X86Reg::Ymm(2),
                MemWidth::B4,
            ),
            (
                &[0xC4, 0xE2, 0xDD, 0x90, 0x74, 0x68, 0x08][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(6),
                X86Reg::Xmm(5),
                MemWidth::B8,
            ),
            (
                &[0xC4, 0x02, 0xAD, 0x91, 0x6C, 0x23, 0x20][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(13),
                X86Reg::Ymm(12),
                MemWidth::B8,
            ),
            (
                &[0xC4, 0xC2, 0x5D, 0x93, 0x74, 0x68, 0x08][..],
                VecElementType::I32,
                4,
                X86Reg::Xmm(6),
                X86Reg::Ymm(5),
                MemWidth::B4,
            ),
            (
                &[0xC4, 0x02, 0xC5, 0x92, 0x4C, 0xC1, 0x10][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(9),
                X86Reg::Xmm(8),
                MemWidth::B8,
            ),
            (
                &[0xC4, 0xE2, 0x75, 0x92, 0x1C, 0x90][..],
                VecElementType::I32,
                8,
                X86Reg::Ymm(3),
                X86Reg::Ymm(2),
                MemWidth::B4,
            ),
            (
                &[0xC4, 0xC2, 0xDD, 0x93, 0x74, 0x68, 0x08][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(6),
                X86Reg::Ymm(5),
                MemWidth::B8,
            ),
            (
                &[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x88][..],
                VecElementType::I32,
                16,
                X86Reg::Zmm(18),
                X86Reg::Zmm(17),
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x29, 0x90, 0x1C, 0x90][..],
                VecElementType::I32,
                8,
                X86Reg::Ymm(3),
                X86Reg::Ymm(2),
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x2A, 0x91, 0x5C, 0x90, 0x01][..],
                VecElementType::I32,
                4,
                X86Reg::Xmm(3),
                X86Reg::Ymm(2),
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x2B, 0x91, 0x5C, 0x50, 0x01][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(3),
                X86Reg::Ymm(2),
                MemWidth::B8,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x2C, 0x92, 0x5C, 0xD0, 0x02][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(3),
                X86Reg::Xmm(2),
                MemWidth::B8,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x2D, 0x93, 0x5C, 0x90, 0x04][..],
                VecElementType::I64,
                4,
                X86Reg::Ymm(3),
                X86Reg::Ymm(2),
                MemWidth::B8,
            ),
            (
                &[0x62, 0xC2, 0xFD, 0x45, 0x90, 0x64, 0xD8, 0x02][..],
                VecElementType::I64,
                8,
                X86Reg::Zmm(20),
                X86Reg::Ymm(19),
                MemWidth::B8,
            ),
            (
                &[0x62, 0xC2, 0x7D, 0x46, 0x93, 0x74, 0x69, 0x08][..],
                VecElementType::I32,
                8,
                X86Reg::Ymm(22),
                X86Reg::Zmm(21),
                MemWidth::B4,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad { width, .. } if width == pred_width
                    ))
                    .count(),
                lanes,
                "{bytes:02X?}",
            );
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(actual)),
                    sign: SignExtend::Sign,
                    ..
                } if actual == index
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    elem: actual_elem,
                    ..
                } if actual == dst && actual_elem == elem
            )));
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let addr32 = lift_single(&[0x67, 0xC4, 0xE2, 0x75, 0x90, 0x5C, 0x90, 0x04]).unwrap();
        assert_eq!(addr32.bytes_consumed, 8);
        assert!(addr32.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Add {
                width: OpWidth::W32,
                flags: FlagUpdate::None,
                ..
            }
        )));
        let segmented = lift_single(&[0x64, 0xC4, 0xE2, 0x75, 0x90, 0x5C, 0x90, 0x04]).unwrap();
        assert!(segmented.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                    ..
                },
                ..
            }
        )));

        for bytes in [
            &[0xC4, 0xE2, 0x75, 0x90, 0xD8][..],       // memory/VSIB required
            &[0xC4, 0xE2, 0x75, 0x90, 0x18][..],       // SIB required
            &[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x98][..], // destination aliases index
            &[0xC4, 0xE2, 0x65, 0x90, 0x1C, 0x90][..], // destination aliases mask
            &[0xC4, 0xE2, 0x6D, 0x90, 0x1C, 0x90][..], // index aliases mask
            &[0xC4, 0xE2, 0x74, 0x90, 0x1C, 0x90][..], // mandatory 66 absent
            &[0x62, 0xE2, 0x7D, 0x40, 0x92, 0x14, 0x88][..], // EVEX k0
            &[0x62, 0xE2, 0x7D, 0xC3, 0x92, 0x14, 0x88][..], // EVEX zeroing
            &[0x62, 0xE2, 0x7D, 0x53, 0x92, 0x14, 0x88][..], // EVEX.b
            &[0x62, 0xE2, 0x75, 0x43, 0x92, 0x14, 0x88][..], // EVEX.vvvv reserved
            &[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x90][..], // EVEX dest/index alias
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid gather accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_packed_integer_logic_models_masks_broadcast_and_high_registers() {
        for (opcode, expected) in [(0xDB, "and"), (0xDF, "andn"), (0xEB, "or"), (0xEF, "xor")] {
            let result = lift_single(&[0x62, 0xF1, 0x7D, 0xC9, opcode, 0xD1]).unwrap();
            assert!(result.ops.iter().any(|op| match (&op.kind, expected) {
                (OpKind::VAnd { width, .. }, "and")
                | (OpKind::VAndNot { width, .. }, "andn")
                | (OpKind::VOr { width, .. }, "or")
                | (OpKind::VXor { width, .. }, "xor") => *width == VecWidth::V512,
                _ => false,
            }));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::Select {
                            width: OpWidth::W32,
                            ..
                        }
                    ))
                    .count(),
                16,
                "opcode {opcode:02X}: dword mask lane count",
            );
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    elem: VecElementType::I32,
                    lane: 15,
                    ..
                }
            )));
        }

        let qword = lift_single(&[0x62, 0xF1, 0xFD, 0x49, 0xDB, 0xD1]).unwrap();
        assert_eq!(
            qword
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Select {
                        width: OpWidth::W64,
                        ..
                    }
                ))
                .count(),
            8,
        );
        assert!(qword.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                elem: VecElementType::I64,
                lane: 7,
                ..
            }
        )));

        let high = lift_single(&[0x62, 0xA1, 0x7D, 0x40, 0xDB, 0xD1]).unwrap();
        assert!(matches!(
            high.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VAnd {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    width: VecWidth::V512,
                },
                ..
            }]
        ));

        let masked_memory = lift_single(&[0x62, 0xF1, 0x7D, 0x49, 0xDB, 0x50, 0x02]).unwrap();
        assert_eq!(
            masked_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16,
        );
        assert!(masked_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 128,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));

        let broadcast = lift_single(&[0x62, 0xF1, 0xFD, 0x59, 0xEF, 0x50, 0x08]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B8,
                        ..
                    }
                ))
                .count(),
            1,
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I64,
                lanes: 8,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF1, 0x7C, 0x48, 0xDB, 0xC1][..], // 66 is mandatory
            &[0x62, 0xF1, 0x7D, 0xC8, 0xDB, 0xC1][..], // {z} requires k1-k7
            &[0x62, 0xF1, 0x7D, 0x58, 0xDB, 0xC1][..], // EVEX.b reserved for reg
            &[0x62, 0xF1, 0x7D, 0x68, 0xDB, 0xC1][..], // LL=3 is reserved
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid EVEX integer logic accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_legacy_vex_evex_packed_add_models_element_width_masks_and_broadcast() {
        for (opcode, elem, lanes) in [
            (0xFC, VecElementType::I8, 16u8),
            (0xFD, VecElementType::I16, 8),
            (0xFE, VecElementType::I32, 4),
            (0xD4, VecElementType::I64, 2),
        ] {
            let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                mmx.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            addr: None,
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VAdd {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            elem: actual_elem,
                            lanes: actual_lanes,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
                        }),
                        ..
                    }
                ] if *actual_elem == elem && *actual_lanes == lanes / 2
                    && *actual_opcode == opcode
            ));

            let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                legacy.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VAdd {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: actual_elem,
                        lanes: actual_lanes,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: actual_opcode,
                    }),
                    ..
                }] if *actual_elem == elem && *actual_lanes == lanes && *actual_opcode == opcode
            ));

            let vex = lift_single(&[0xC5, 0xF5, opcode, 0xC2]).unwrap();
            assert!(matches!(
                vex.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VAdd {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                        elem: actual_elem,
                        lanes: actual_lanes,
                    },
                    ..
                }] if *actual_elem == elem && *actual_lanes == lanes * 2
            ));
        }

        let mmx_memory = lift_single(&[0x0F, 0xFC, 0x00]).unwrap();
        assert!(matches!(
            mmx_memory.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VLoad {
                        width: VecWidth::V64,
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
                    kind: OpKind::VAdd {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        elem: VecElementType::I8,
                        lanes: 8,
                        ..
                    },
                    ..
                }
            ]
        ));

        for (bytes, elem, lanes, select_width) in [
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xFC, 0xD1][..],
                VecElementType::I8,
                64usize,
                OpWidth::W8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xFD, 0xD1][..],
                VecElementType::I16,
                32,
                OpWidth::W16,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xFE, 0xD1][..],
                VecElementType::I32,
                16,
                OpWidth::W32,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0xC9, 0xD4, 0xD1][..],
                VecElementType::I64,
                8,
                OpWidth::W64,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VAdd {
                    elem: actual_elem,
                    lanes: actual_lanes,
                    ..
                } if actual_elem == elem && usize::from(actual_lanes) == lanes
            )));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::Select { width, .. } if width == select_width
                    ))
                    .count(),
                lanes,
            );
        }

        // W is ignored for byte/word forms.
        let byte_w1 = lift_single(&[0x62, 0xF1, 0xFD, 0x48, 0xFC, 0xC1]).unwrap();
        assert!(matches!(
            byte_w1.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VAdd {
                    elem: VecElementType::I8,
                    lanes: 64,
                    ..
                },
                ..
            }]
        ));

        let masked_bytes = lift_single(&[0x62, 0xF1, 0x7D, 0x49, 0xFC, 0x50, 0x02]).unwrap();
        assert_eq!(
            masked_bytes
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            64,
        );
        assert!(masked_bytes.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 128,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));

        let dword_broadcast = lift_single(&[0x62, 0xF1, 0x7D, 0x59, 0xFE, 0x50, 0x08]).unwrap();
        assert!(dword_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(dword_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I32,
                lanes: 16,
                ..
            }
        )));

        for bytes in [
            &[0xF3, 0x0F, 0xFC, 0xC1][..],             // invalid legacy prefix
            &[0xF0, 0x66, 0x0F, 0xFC, 0xC1][..],       // LOCK is undefined
            &[0xC5, 0xF4, 0xFC, 0xC1][..],             // VEX requires 66
            &[0x62, 0xF1, 0xFD, 0x48, 0xFE, 0xC1][..], // VPADDD requires W0
            &[0x62, 0xF1, 0x7D, 0x48, 0xD4, 0xC1][..], // VPADDQ requires W1
            &[0x62, 0xF1, 0x7D, 0x58, 0xFC, 0x00][..], // VPADDB has no broadcast
            &[0x62, 0xF1, 0x7D, 0x58, 0xFD, 0x00][..], // VPADDW has no broadcast
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid packed add accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_legacy_vex_evex_packed_subtract_models_width_masks_and_broadcast() {
        for (opcode, elem, legacy_lanes) in [
            (0xF8, VecElementType::I8, 16u8),
            (0xF9, VecElementType::I16, 8),
            (0xFA, VecElementType::I32, 4),
            (0xFB, VecElementType::I64, 2),
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
                        kind: OpKind::VSub {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            elem: actual_elem,
                            lanes,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
                        }),
                        ..
                    }
                ] if *actual_elem == elem && *lanes == legacy_lanes / 2
                    && *actual_opcode == opcode
            ));

            let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                legacy.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VSub {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: actual_elem,
                        lanes,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: actual_opcode,
                    }),
                    ..
                }] if *actual_elem == elem && *lanes == legacy_lanes && *actual_opcode == opcode
            ));

            let vex = lift_single(&[0xC5, 0xF5, opcode, 0xC2]).unwrap();
            assert!(matches!(
                vex.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VSub {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                        elem: actual_elem,
                        lanes,
                    },
                    ..
                }] if *actual_elem == elem && *lanes == legacy_lanes * 2
            ));
        }

        for (bytes, elem, lanes, width) in [
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xF8, 0xD1][..],
                VecElementType::I8,
                64usize,
                OpWidth::W8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xF9, 0xD1][..],
                VecElementType::I16,
                32,
                OpWidth::W16,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0xC9, 0xFA, 0xD1][..],
                VecElementType::I32,
                16,
                OpWidth::W32,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0xC9, 0xFB, 0xD1][..],
                VecElementType::I64,
                8,
                OpWidth::W64,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VSub {
                    elem: actual_elem,
                    lanes: actual_lanes,
                    ..
                } if actual_elem == elem && usize::from(actual_lanes) == lanes
            )));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::Select { width: actual, .. } if actual == width))
                    .count(),
                lanes,
            );
        }

        let broadcast = lift_single(&[0x62, 0xF1, 0xFD, 0x59, 0xFB, 0x50, 0x08]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 64, .. },
                width: MemWidth::B8,
                ..
            }
        )));
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I64,
                lanes: 8,
                ..
            }
        )));

        for bytes in [
            &[0xC5, 0xF4, 0xF8, 0xC1][..],             // VEX requires 66
            &[0x62, 0xF1, 0xFD, 0x48, 0xFA, 0xC1][..], // VPSUBD requires W0
            &[0x62, 0xF1, 0x7D, 0x48, 0xFB, 0xC1][..], // VPSUBQ requires W1
            &[0x62, 0xF1, 0x7D, 0x58, 0xF8, 0x00][..], // VPSUBB has no broadcast
            &[0x62, 0xF1, 0x7D, 0x58, 0xF9, 0x00][..], // VPSUBW has no broadcast
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid packed subtract accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_scalar_masking_fault_suppression_and_high_registers() {
        let merge = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0xD1]).unwrap();
        assert!(merge.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::And {
                src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                src2: SrcOperand::Imm(1),
                flags: FlagUpdate::None,
                ..
            }
        )));
        assert!(merge.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                lane: 0,
                ..
            }
        )));
        assert!(
            merge
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Select { .. }))
        );
        assert!(merge.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAdd {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                elem: VecElementType::F32,
                lanes: 1,
                ..
            }
        )));

        for (bytes, expected_elem, expected) in [
            (
                &[0x62, 0xF1, 0x7E, 0x0A, 0x59, 0xE1][..],
                VecElementType::F32,
                "mul",
            ),
            (
                &[0x62, 0xF1, 0x7E, 0x8A, 0x5E, 0xE9][..],
                VecElementType::F32,
                "div",
            ),
            (
                &[0x62, 0xF1, 0xFF, 0x09, 0x58, 0xD1][..],
                VecElementType::F64,
                "add",
            ),
            (
                &[0x62, 0xF1, 0xFF, 0x89, 0x5C, 0xD9][..],
                VecElementType::F64,
                "sub",
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(
                result.ops.iter().any(|op| match (&op.kind, expected) {
                    (OpKind::VMul { elem, lanes: 1, .. }, "mul")
                    | (OpKind::VDiv { elem, lanes: 1, .. }, "div")
                    | (OpKind::VAdd { elem, lanes: 1, .. }, "add")
                    | (OpKind::VSub { elem, lanes: 1, .. }, "sub") => {
                        *elem == expected_elem
                    }
                    _ => false,
                }),
                "{expected}: {:?}",
                result.ops,
            );
        }

        let zero = lift_single(&[0x62, 0xF1, 0x7E, 0x89, 0x5C, 0xD1]).unwrap();
        assert!(zero.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov {
                src: SrcOperand::Imm(0),
                width: OpWidth::W32,
                ..
            }
        )));
        assert!(!zero.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                lane: 0,
                ..
            }
        )));

        let masked_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10]).unwrap();
        assert!(masked_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(
            !masked_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. }))
        );

        let compressed = lift_single(&[0x62, 0xF1, 0x7E, 0x08, 0x58, 0x57, 0x10]).unwrap();
        assert!(compressed.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: MemWidth::B4,
                ..
            }
        )));
        let compressed_sd = lift_single(&[0x62, 0xF1, 0xFF, 0x08, 0x58, 0x5F, 0x08]).unwrap();
        assert!(compressed_sd.ops.iter().any(|op| matches!(
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

        let high = lift_single(&[0x62, 0xA1, 0x7E, 0x00, 0x58, 0xD1]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAdd {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                ..
            }
        )));

        let high_move = lift_single(&[0x62, 0xA1, 0x7E, 0x00, 0x10, 0xD1]).unwrap();
        assert!(high_move.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                lane: 0,
                ..
            }
        )));
        assert!(high_move.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF1, 0x7E, 0x88, 0x58, 0xD1][..], // {z} with k0
            &[0x62, 0xF1, 0x7E, 0x18, 0x58, 0xD1][..], // embedded rounding/SAE
            &[0x62, 0xF1, 0xFE, 0x08, 0x58, 0xD1][..], // VADDSS with W=1
            &[0x62, 0xF1, 0x7F, 0x08, 0x58, 0xD1][..], // VADDSD with W=0
            &[0x62, 0xF1, 0xFE, 0x08, 0x10, 0xD1][..], // VMOVSS with W=1
            &[0x62, 0xF1, 0x7F, 0x08, 0x10, 0xD1][..], // VMOVSD with W=0
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_scalar_precision_conversion_family_masks_er_sae_map6_and_memory() {
        for (bytes, from, to) in [
            (
                &[0x62, 0xF1, 0xFF, 0x09, 0x5A, 0xCA][..],
                VecElementType::F64,
                VecElementType::F32,
            ),
            (
                &[0x62, 0xF1, 0x7E, 0x09, 0x5A, 0xCA][..],
                VecElementType::F32,
                VecElementType::F64,
            ),
            (
                &[0x62, 0xF5, 0xFF, 0x09, 0x5A, 0xCA][..],
                VecElementType::F64,
                VecElementType::F16,
            ),
            (
                &[0x62, 0xF5, 0x7E, 0x09, 0x5A, 0xCA][..],
                VecElementType::F16,
                VecElementType::F64,
            ),
            (
                &[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0xCA][..],
                VecElementType::F32,
                VecElementType::F16,
            ),
            (
                &[0x62, 0xF6, 0x7C, 0x09, 0x13, 0xCA][..],
                VecElementType::F16,
                VecElementType::F32,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::X86FpConvert {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    from: actual_from,
                    to: actual_to,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: true,
                } if actual_from == from && actual_to == to
            ));
        }

        let er = lift_single(&[0x62, 0xF5, 0xFF, 0x59, 0x5A, 0xCA]).unwrap();
        assert!(matches!(
            er.ops.last().unwrap().kind,
            OpKind::X86FpConvert {
                from: VecElementType::F64,
                to: VecElementType::F16,
                round: FpRoundMode::RoundUp,
                suppress_exceptions: true,
                ..
            }
        ));
        let sae = lift_single(&[0x62, 0xF6, 0x7C, 0x19, 0x13, 0xCA]).unwrap();
        assert!(matches!(
            sae.ops.last().unwrap().kind,
            OpKind::X86FpConvert {
                from: VecElementType::F16,
                to: VecElementType::F32,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: true,
                ..
            }
        ));

        let high = lift_single(&[0x62, 0xA6, 0x7C, 0x00, 0x13, 0xD1]).unwrap();
        assert!(matches!(
            high.ops.last().unwrap().kind,
            OpKind::X86FpConvert {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                from: VecElementType::F16,
                to: VecElementType::F32,
                ..
            }
        ));

        for (bytes, offset, width) in [
            (
                &[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0x48, 0x08][..],
                32i64,
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF6, 0x7C, 0x09, 0x13, 0x48, 0x08][..],
                16,
                MemWidth::B2,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset {
                        offset: actual_offset,
                        disp_size: DispSize::Disp8,
                        ..
                    },
                    width: actual_width,
                    signed: SignExtend::Zero,
                    ..
                } if actual_offset == offset && actual_width == width
            )));
        }

        for bytes in [
            &[0x62, 0xF6, 0x7C, 0x88, 0x13, 0xC2][..], // {z} without a mask
            &[0x62, 0xF5, 0xFF, 0x19, 0x5A, 0x00][..], // EVEX.b memory
            &[0x62, 0xF5, 0x7F, 0x08, 0x5A, 0xC2][..], // VCVTSD2SH W=0
            &[0x62, 0xF6, 0xFC, 0x08, 0x13, 0xC2][..], // VCVTSH2SS W=1
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_mask_blends_cover_types_sources_masks_e4_broadcast_and_invalids() {
        for (bytes, elem, lanes) in [
            (
                &[0x62, 0xA2, 0x7D, 0xC3, 0x65, 0xCA][..],
                VecElementType::I32,
                16u8,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x2A, 0x64, 0xCB][..],
                VecElementType::I64,
                4,
            ),
            (
                &[0x62, 0xA2, 0x7D, 0xC3, 0x66, 0xCA][..],
                VecElementType::I8,
                64,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x2A, 0x66, 0xCB][..],
                VecElementType::I16,
                16,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::Select { .. }))
                    .count(),
                usize::from(lanes)
            );
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: actual_elem,
                    lane,
                    ..
                } if actual_elem == elem && lane == lanes - 1
            )));
        }
        let high = lift_single(&[0x62, 0xA2, 0x7D, 0xC3, 0x65, 0xCA]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                elem: VecElementType::I32,
                ..
            }
        )));
        assert!(matches!(
            high.ops.last().unwrap().kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                ..
            }
        ));

        let memory = lift_single(&[0x62, 0xF2, 0x6D, 0xC9, 0x64, 0x48, 0x02]).unwrap();
        assert_eq!(
            memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        let broadcast = lift_single(&[0x62, 0xF2, 0xED, 0x59, 0x65, 0x48, 0x08]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B8,
                        ..
                    }
                ))
                .count(),
            8
        );

        for bytes in [
            &[0xC4, 0xE2, 0x6D, 0x64, 0xCB][..],       // EVEX-only
            &[0x62, 0xF2, 0x6C, 0x49, 0x64, 0xCB][..], // mandatory 66 absent
            &[0x62, 0xF2, 0x6D, 0x88, 0x64, 0xCB][..], // {z} requires mask
            &[0x62, 0xF2, 0x6D, 0x68, 0x64, 0xCB][..], // L'L=3
            &[0x62, 0xF2, 0x6D, 0x58, 0x64, 0xCB][..], // broadcast requires memory
            &[0x62, 0xF2, 0x6D, 0x59, 0x66, 0x08][..], // byte/word has no broadcast
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_immediate_integer_compares_cover_predicates_elements_masks_and_faults() {
        let families = [
            (0x3F, false, VecElementType::I8, true),
            (0x3E, false, VecElementType::I8, false),
            (0x3F, true, VecElementType::I16, true),
            (0x3E, true, VecElementType::I16, false),
            (0x1F, false, VecElementType::I32, true),
            (0x1E, false, VecElementType::I32, false),
            (0x1F, true, VecElementType::I64, true),
            (0x1E, true, VecElementType::I64, false),
        ];
        for (opcode, w, elem, signed) in families {
            for predicate in 0u8..8 {
                let p1 = if w { 0xF5 } else { 0x75 };
                let bytes = [0x62, 0xF3, p1, 0x08, opcode, 0xDA, predicate];
                let result = lift_single(&bytes).unwrap();
                assert_eq!(result.bytes_consumed, bytes.len());
                let expected = match predicate {
                    0 => Some(VecCmpCond::Eq),
                    1 => Some(if signed {
                        VecCmpCond::Lt
                    } else {
                        VecCmpCond::Ltu
                    }),
                    2 => Some(if signed {
                        VecCmpCond::Le
                    } else {
                        VecCmpCond::Leu
                    }),
                    3 => None,
                    4 => Some(VecCmpCond::Ne),
                    5 => Some(if signed {
                        VecCmpCond::Ge
                    } else {
                        VecCmpCond::Geu
                    }),
                    6 => Some(if signed {
                        VecCmpCond::Gt
                    } else {
                        VecCmpCond::Gtu
                    }),
                    7 => None,
                    _ => unreachable!(),
                };
                if let Some(expected) = expected {
                    assert!(result.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::VCmp {
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                            cond,
                            elem: actual_elem,
                            lanes,
                            ..
                        } if cond == expected
                            && actual_elem == elem
                            && u32::from(lanes) == VecWidth::V128.lanes(elem)
                    )));
                } else {
                    let lanes = VecWidth::V128.lanes(elem);
                    let expected = if predicate == 3 {
                        0
                    } else {
                        ((1u64 << lanes) - 1) as i64
                    };
                    assert!(
                        !result
                            .ops
                            .iter()
                            .any(|op| matches!(op.kind, OpKind::VCmp { .. }))
                    );
                    assert!(result.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::Mov {
                            src: SrcOperand::Imm(actual),
                            width: OpWidth::W64,
                            ..
                        } if actual == expected
                    )));
                }
            }
        }

        // Reserved immediate high bits do not select the predicate; Intel's
        // operation consumes imm8[2:0]. 0xFE therefore remains unsigned GT.
        let high_imm = lift_single(&[0x62, 0xF3, 0x75, 0x08, 0x1E, 0xDA, 0xFE]).unwrap();
        assert!(high_imm.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                cond: VecCmpCond::Gtu,
                elem: VecElementType::I32,
                ..
            }
        )));

        // EVEX extension bits expose ZMM17/ZMM18, while aaa is a source
        // writemask and the low ModR/M.reg bits select destination K3.
        let high = lift_single(&[0x62, 0xB3, 0x75, 0x44, 0x1E, 0xDA, 0x06]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                cond: VecCmpCond::Gtu,
                ..
            }
        )));
        assert!(matches!(
            high.ops.last().map(|op| &op.kind),
            Some(OpKind::And {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                flags: FlagUpdate::None,
                ..
            })
        ));

        // Full-vector and broadcast disp8 tuples use N=64 and N=4,
        // respectively. Constant predicates retain every active predicated
        // memory access so TRUE/FALSE cannot suppress architectural faults.
        for (bytes, offset, predicate) in [
            (&[0x62, 0xF3, 0x75, 0x4C, 0x1F, 0x58, 0x01, 0x07][..], 64, 7),
            (&[0x62, 0xF3, 0x75, 0x5C, 0x1F, 0x58, 0x01, 0x03][..], 4, 3),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseOffset {
                        offset: actual,
                        disp_size: DispSize::Disp8,
                        ..
                    },
                    ..
                } if actual == offset
            )));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: MemWidth::B4,
                            ..
                        }
                    ))
                    .count(),
                16,
                "predicate {predicate} must retain active memory accesses",
            );
        }

        // RIP-relative addressing uses the PC after the immediate byte.
        let rip = lift_single(&[0x62, 0xF3, 0x75, 0x08, 0x1F, 0x1D, 0, 0, 0, 0, 0x01]).unwrap();
        assert_eq!(rip.bytes_consumed, 11);
        assert!(rip.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::PcRel {
                    offset: 0,
                    base: Some(0x100B),
                    ..
                },
                width: VecWidth::V128,
                ..
            }
        )));

        // A 512-bit byte TRUE comparison produces all 64 architectural mask
        // bits, without a shift-by-64 overflow in the lifter.
        let all_bytes = lift_single(&[0x62, 0xF3, 0x75, 0x48, 0x3F, 0xDA, 0x07]).unwrap();
        assert!(all_bytes.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov {
                src: SrcOperand::Imm(-1),
                width: OpWidth::W64,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF3, 0x75, 0x88, 0x1F, 0xDA, 0][..], // EVEX.z reserved
            &[0x62, 0xF3, 0x75, 0x68, 0x1F, 0xDA, 0][..], // EVEX.L'L=3
            &[0x62, 0xF3, 0x74, 0x08, 0x1F, 0xDA, 0][..], // pp != 66
            &[0x62, 0xF3, 0x75, 0x18, 0x1F, 0xDA, 0][..], // broadcast register
            &[0x62, 0xF3, 0x75, 0x18, 0x3F, 0x00, 0][..], // byte broadcast
            &[0x62, 0x73, 0x75, 0x08, 0x1F, 0xDA, 0][..], // extended k destination
            &[0x62, 0xE3, 0x75, 0x08, 0x1F, 0xDA, 0][..], // EVEX.R' on k destination
            &[0x62, 0xF3, 0x75, 0x08, 0x1F][..],          // missing ModR/M
            &[0x62, 0xF3, 0x75, 0x08, 0x1F, 0xDA][..],    // missing imm8
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid immediate packed compare accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_chunk_extract_insert_covers_families_masks_e6nf_and_invalids() {
        for opcode in [0x18u8, 0x19, 0x38, 0x39] {
            for (w, elem) in [
                (
                    false,
                    if opcode < 0x30 {
                        VecElementType::F32
                    } else {
                        VecElementType::I32
                    },
                ),
                (
                    true,
                    if opcode < 0x30 {
                        VecElementType::F64
                    } else {
                        VecElementType::I64
                    },
                ),
            ] {
                for (p2, source_width) in [(0x28, VecWidth::V256), (0x48, VecWidth::V512)] {
                    let p1 = if w { 0xFD } else { 0x7D };
                    let bytes = [0x62, 0xF3, p1, p2, opcode, 0xD1, 0xFF];
                    let result = lift_single(&bytes).unwrap();
                    assert_eq!(result.bytes_consumed, bytes.len());
                    let chunk_lanes = VecWidth::V128.lanes(elem) as usize;
                    assert_eq!(
                        result
                            .ops
                            .iter()
                            .filter(|op| matches!(
                                op.kind,
                                OpKind::VExtractLane { elem: actual, .. } if actual == elem
                            ))
                            .count(),
                        chunk_lanes,
                        "opcode {opcode:02X}, W={w}, width={source_width:?}",
                    );
                    assert!(result.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::VMov { width, .. }
                            if width == if opcode & 1 == 0 { source_width } else { VecWidth::V128 }
                    )));
                }
            }
        }

        for opcode in [0x1Au8, 0x1B, 0x3A, 0x3B] {
            for (w, elem) in [
                (
                    false,
                    if opcode < 0x30 {
                        VecElementType::F32
                    } else {
                        VecElementType::I32
                    },
                ),
                (
                    true,
                    if opcode < 0x30 {
                        VecElementType::F64
                    } else {
                        VecElementType::I64
                    },
                ),
            ] {
                let p1 = if w { 0xFD } else { 0x7D };
                let bytes = [0x62, 0xF3, p1, 0x48, opcode, 0xD1, 0xFF];
                let result = lift_single(&bytes).unwrap();
                assert_eq!(result.bytes_consumed, bytes.len());
                assert_eq!(
                    result
                        .ops
                        .iter()
                        .filter(|op| matches!(
                            op.kind,
                            OpKind::VExtractLane { elem: actual, .. } if actual == elem
                        ))
                        .count(),
                    VecWidth::V256.lanes(elem) as usize,
                    "opcode {opcode:02X}, W={w}",
                );
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov { width, .. }
                        if width == if opcode & 1 == 0 { VecWidth::V512 } else { VecWidth::V256 }
                )));
            }
        }

        // High register extension, zeroing masking, and immediate chunk
        // selection are independent for extract and insert forms.
        let extract = lift_single(&[0x62, 0xA3, 0x7D, 0xCB, 0x19, 0xD1, 0x03]).unwrap();
        assert!(extract.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                lane: 12,
                elem: VecElementType::F32,
                ..
            }
        )));
        assert!(extract.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                elem: VecElementType::F32,
                ..
            }
        )));

        let insert = lift_single(&[0x62, 0xA3, 0x6D, 0xC3, 0x18, 0xCB, 0x02]).unwrap();
        assert!(insert.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(insert.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                elem: VecElementType::F32,
                ..
            }
        )));
        assert!(insert.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                elem: VecElementType::F32,
                ..
            }
        )));

        // E6NF masked extract memory performs an unconditional full tuple
        // read/merge/write. Insert memory performs an unconditional full tuple
        // read before masking. Both use Tuple4's 16-byte disp8 scale here.
        let extract_memory =
            lift_single(&[0x62, 0xF3, 0x7D, 0x2A, 0x39, 0x58, 0x02, 0x01]).unwrap();
        assert!(extract_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(extract_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VStore {
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(
            !extract_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredStore { .. }))
        );

        let insert_memory = lift_single(&[0x62, 0xF3, 0xDD, 0x2A, 0x18, 0x58, 0x02, 0x01]).unwrap();
        assert!(insert_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(
            !insert_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        // RIP-relative memory uses the PC after imm8, not after ModR/M.
        let rip = lift_single(&[0x62, 0xF3, 0x7D, 0x48, 0x18, 0x1D, 0, 0, 0, 0, 0x01]).unwrap();
        assert_eq!(rip.bytes_consumed, 11);
        assert!(rip.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::PcRel {
                    base: Some(0x100B),
                    ..
                },
                width: VecWidth::V128,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF3, 0x7D, 0x08, 0x18, 0xD1, 0][..], // VL=128
            &[0x62, 0xF3, 0x7D, 0x28, 0x1A, 0xD1, 0][..], // x8 requires VL=512
            &[0x62, 0xF3, 0x7D, 0x68, 0x18, 0xD1, 0][..], // L'L=3
            &[0x62, 0xF3, 0x7C, 0x48, 0x18, 0xD1, 0][..], // pp != 66
            &[0x62, 0xF3, 0x7D, 0x58, 0x18, 0xD1, 0][..], // EVEX.b reserved
            &[0x62, 0xF3, 0x7D, 0xC8, 0x18, 0xD1, 0][..], // {z} with k0
            &[0x62, 0xF3, 0x7D, 0xAA, 0x19, 0x10, 0][..], // {z} memory extract
            &[0x62, 0xF3, 0x75, 0x48, 0x19, 0xD1, 0][..], // extract vvvv reserved
            &[0x62, 0xF3, 0x7D, 0x41, 0x19, 0xD1, 0][..], // extract V' reserved
            &[0x62, 0xF3, 0x7D, 0x48, 0x18][..],          // missing ModR/M
            &[0x62, 0xF3, 0x7D, 0x48, 0x18, 0xD1][..],    // missing imm8
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid EVEX chunk extract/insert accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_shuffle_128_chunks_covers_selectors_masks_e4nf_and_invalids() {
        for (opcode, w, elem) in [
            (0x23u8, false, VecElementType::F32),
            (0x23, true, VecElementType::F64),
            (0x43, false, VecElementType::I32),
            (0x43, true, VecElementType::I64),
        ] {
            for (p2, width) in [(0x28, VecWidth::V256), (0x48, VecWidth::V512)] {
                let p1 = if w { 0xFD } else { 0x7D };
                let bytes = [0x62, 0xF3, p1, p2, opcode, 0xD1, 0x4E];
                let result = lift_single(&bytes).unwrap();
                assert_eq!(result.bytes_consumed, bytes.len());
                assert_eq!(
                    result
                        .ops
                        .iter()
                        .filter(|op| matches!(
                            op.kind,
                            OpKind::VExtractLane { elem: actual, .. } if actual == elem
                        ))
                        .count(),
                    width.lanes(elem) as usize,
                    "opcode {opcode:02X}, W={w}, width={width:?}",
                );
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2) | X86Reg::Zmm(2))),
                        width: actual,
                        ..
                    } if actual == width
                )));
            }
        }

        let high = lift_single(&[0x62, 0xA3, 0x6D, 0xC3, 0x23, 0xCB, 0x4E]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                lane: 8,
                elem: VecElementType::F32,
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                lane: 0,
                elem: VecElementType::F32,
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                elem: VecElementType::F32,
                ..
            }
        )));

        // Full and broadcast tuples are accessed unconditionally under E4NF.
        // Their compressed disp8 scales are 32 and 4 bytes, respectively.
        let full = lift_single(&[0x62, 0xF3, 0xED, 0xAA, 0x43, 0x48, 0x01, 0x03]).unwrap();
        assert!(full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !full
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        let broadcast = lift_single(&[0x62, 0xF3, 0x6D, 0x5A, 0x23, 0x48, 0x01, 0x1B]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset: 4,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(
            !broadcast
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        let rip = lift_single(&[0x62, 0xF3, 0x7D, 0x48, 0x23, 0x1D, 0, 0, 0, 0, 0x4E]).unwrap();
        assert_eq!(rip.bytes_consumed, 11);
        assert!(rip.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::PcRel {
                    base: Some(0x100B),
                    ..
                },
                width: VecWidth::V512,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF3, 0x7D, 0x08, 0x23, 0xD1, 0][..], // VL=128
            &[0x62, 0xF3, 0x7D, 0x68, 0x23, 0xD1, 0][..], // L'L=3
            &[0x62, 0xF3, 0x7C, 0x48, 0x23, 0xD1, 0][..], // pp != 66
            &[0x62, 0xF3, 0x7D, 0x58, 0x23, 0xD1, 0][..], // broadcast register
            &[0x62, 0xF3, 0x7D, 0xC8, 0x23, 0xD1, 0][..], // {z} with k0
            &[0x62, 0xF3, 0x7D, 0x48, 0x23][..],          // missing ModR/M
            &[0x62, 0xF3, 0x7D, 0x48, 0x23, 0xD1][..],    // missing imm8
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid EVEX 128-bit chunk shuffle accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_gfni_covers_field_ops_masks_memory_classes_and_invalids() {
        for (p3, width) in [
            (0x08, VecWidth::V128),
            (0x28, VecWidth::V256),
            (0x48, VecWidth::V512),
        ] {
            let multiply = lift_single(&[0x62, 0xF2, 0x7D, p3, 0xCF, 0xC8]).unwrap();
            assert_eq!(multiply.bytes_consumed, 6);
            assert!(multiply.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VShift {
                    shift: ShiftOp::Lsl,
                    elem: VecElementType::I8,
                    lanes,
                    ..
                } if u32::from(lanes) == width.lanes(VecElementType::I8)
            )));

            for opcode in [0xCE, 0xCF] {
                let affine = lift_single(&[0x62, 0xF3, 0xFD, p3, opcode, 0xC8, 0x63]).unwrap();
                assert_eq!(affine.bytes_consumed, 7);
                assert!(affine.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VByteShuffle {
                        lanes,
                        block_lanes: 8,
                        ..
                    } if u32::from(lanes) == width.lanes(VecElementType::I8)
                )));
            }
        }

        // Every EVEX vector extension bit remains live through the expanded
        // field arithmetic, and k3 applies byte-granular zeroing to ZMM17.
        let high_mul = lift_single(&[0x62, 0xA2, 0x6D, 0xC3, 0xCF, 0xCB]).unwrap();
        assert!(high_mul.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                ..
            }
        )));
        assert!(high_mul.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                ..
            }
        )));
        assert!(high_mul.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                elem: VecElementType::I8,
                ..
            }
        )));
        assert!(high_mul.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Shr {
                src: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                ..
            }
        )));

        let high_affine = lift_single(&[0x62, 0xA3, 0xED, 0xC3, 0xCE, 0xCB, 0x63]).unwrap();
        assert!(high_affine.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VByteShuffle {
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                block_lanes: 8,
                ..
            }
        )));
        assert!(high_affine.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                ..
            }
        )));

        // VGF2P8MULB is Type E4: each inactive byte suppresses its own
        // memory access, and the full-memory disp8 tuple scales by 64 bytes.
        let multiply_memory = lift_single(&[0x62, 0xF2, 0x4D, 0x4D, 0xCF, 0x60, 0x01]).unwrap();
        assert!(multiply_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert_eq!(
            multiply_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            64
        );

        // Affine forms are Type E4NF: the 64-bit broadcast is loaded
        // unconditionally even under k5, then replicated across eight qwords.
        let affine_broadcast =
            lift_single(&[0x62, 0xF3, 0xCD, 0x5D, 0xCF, 0x60, 0x01, 0x63]).unwrap();
        assert!(affine_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset: 8,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: MemWidth::B8,
                ..
            }
        )));
        assert!(affine_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I64,
                lanes: 8,
                ..
            }
        )));
        assert!(
            !affine_broadcast
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        let affine_full = lift_single(&[0x62, 0xF3, 0xCD, 0x4D, 0xCE, 0x60, 0x01, 0x63]).unwrap();
        assert!(affine_full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(
            !affine_full
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        for bytes in [
            &[0x62, 0xF2, 0x7C, 0x08, 0xCF, 0xC8][..], // MULB pp != 66
            &[0x62, 0xF2, 0xFD, 0x08, 0xCF, 0xC8][..], // MULB W=1
            &[0x62, 0xF2, 0x7D, 0x18, 0xCF, 0xC8][..], // MULB EVEX.b
            &[0x62, 0xF2, 0x7D, 0x68, 0xCF, 0xC8][..], // L'L=3
            &[0x62, 0xF2, 0x7D, 0x88, 0xCF, 0xC8][..], // z without a mask
            &[0x62, 0xF3, 0x7D, 0x08, 0xCE, 0xC8, 0][..], // affine W=0
            &[0x62, 0xF3, 0xFD, 0x18, 0xCE, 0xC8, 0][..], // broadcast register
            &[0x62, 0xF3, 0xFD, 0x08, 0xCE][..],       // missing ModR/M
            &[0x62, 0xF3, 0xFD, 0x08, 0xCE, 0xC8][..], // missing imm8
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid EVEX GFNI form accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_legacy_vex_evex_saturating_packs_cover_widths_masks_tuples_and_invalids() {
        for (opcode, src_elem, to_unsigned) in [
            (0x63, VecElementType::I16, false),
            (0x67, VecElementType::I16, true),
            (0x6B, VecElementType::I32, false),
        ] {
            let lifted = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                lifted.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            ..
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VPackSat {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src_elem: actual,
                            to_unsigned: actual_unsigned,
                            src_lanes,
                            block_lanes,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
                        }),
                        ..
                    }
                ] if *actual == src_elem
                    && *actual_unsigned == to_unsigned
                    && u32::from(*src_lanes) == VecWidth::V64.lanes(src_elem)
                    && *src_lanes == *block_lanes
                    && *actual_opcode == opcode
            ));
        }

        let mmx_memory = lift_single(&[0x0F, 0x63, 0x00]).unwrap();
        assert!(matches!(
            mmx_memory.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VLoad {
                        width: VecWidth::V64,
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
                    kind: OpKind::VPackSat {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Virtual(_),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src_elem: VecElementType::I16,
                        src_lanes: 4,
                        block_lanes: 4,
                        ..
                    },
                    ..
                }
            ]
        ));

        let legacy_cases = [
            (&[0x66, 0x0F, 0x63, 0xC1][..], VecElementType::I16, false),
            (&[0x66, 0x0F, 0x67, 0xC1][..], VecElementType::I16, true),
            (&[0x66, 0x0F, 0x6B, 0xC1][..], VecElementType::I32, false),
            (
                &[0x66, 0x0F, 0x38, 0x2B, 0xC1][..],
                VecElementType::I32,
                true,
            ),
        ];
        for (bytes, src_elem, to_unsigned) in legacy_cases {
            let lifted = lift_single(bytes).unwrap();
            let pack = lifted
                .ops
                .iter()
                .find(|op| matches!(op.kind, OpKind::VPackSat { .. }))
                .expect("legacy saturating pack");
            assert!(matches!(
                pack.kind,
                OpKind::VPackSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src_elem: actual,
                    to_unsigned: actual_unsigned,
                    src_lanes,
                    block_lanes,
                    ..
                } if actual == src_elem
                    && actual_unsigned == to_unsigned
                    && src_lanes == block_lanes
            ));
            assert!(matches!(
                pack.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                }) if opcode == bytes[bytes.len() - 2]
            ));
            assert_eq!(lifted.ops.len(), 1, "register pack must remain atomic");
        }

        for (bytes, src_elem, to_unsigned) in [
            (&[0xC5, 0xF5, 0x63, 0xC2][..], VecElementType::I16, false),
            (&[0xC5, 0xF5, 0x67, 0xC2][..], VecElementType::I16, true),
            (&[0xC5, 0xF5, 0x6B, 0xC2][..], VecElementType::I32, false),
            (
                &[0xC4, 0xE2, 0x75, 0x2B, 0xC2][..],
                VecElementType::I32,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::VPackSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src_elem: actual,
                    to_unsigned: actual_unsigned,
                    src_lanes,
                    block_lanes,
                } if actual == src_elem
                    && actual_unsigned == to_unsigned
                    && u32::from(src_lanes) == VecWidth::V256.lanes(src_elem)
                    && u32::from(block_lanes) == 16 / src_elem.bytes()
            ));
            assert!(matches!(
                lifted.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp {
                    map,
                    pp: X86SsePrefix::OpSize,
                    opcode,
                    width: VecWidth::V256,
                    w: false,
                }) if map == if src_elem == VecElementType::I32 && to_unsigned {
                    X86VecMap::Map0F38
                } else {
                    X86VecMap::Map0F
                } && opcode == bytes[bytes.len() - 2]
            ));
        }

        for (bytes, src_elem, to_unsigned) in [
            (
                &[0x62, 0xF1, 0x75, 0x49, 0x63, 0xC2][..],
                VecElementType::I16,
                false,
            ),
            (
                &[0x62, 0xF1, 0x75, 0x49, 0x67, 0xC2][..],
                VecElementType::I16,
                true,
            ),
            (
                &[0x62, 0xF1, 0x75, 0x49, 0x6B, 0xC2][..],
                VecElementType::I32,
                false,
            ),
            (
                &[0x62, 0xF2, 0x75, 0x49, 0x2B, 0xC2][..],
                VecElementType::I32,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VPackSat {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    src_elem: actual,
                    to_unsigned: actual_unsigned,
                    ..
                } if actual == src_elem && actual_unsigned == to_unsigned
            )));
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane { elem, .. }
                    if elem.bytes() * 2 == src_elem.bytes()
            )));
        }

        // LLVM 20 encodings: unmasked high-register native form,
        // high-register merge/zero fallback, Full tuple disp8*N=64 bytes,
        // and Full m32bcst tuple disp8*N=4 bytes.
        let high_unmasked = lift_single(&[0x62, 0xA1, 0x75, 0x40, 0x63, 0xC2]).unwrap();
        assert!(matches!(
            high_unmasked.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VPackSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src_elem: VecElementType::I16,
                    to_unsigned: false,
                    src_lanes: 32,
                    block_lanes: 8,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x63,
                    width: VecWidth::V512,
                    w: false,
                }),
                ..
            }]
        ));

        let high = lift_single(&[0x62, 0xA1, 0x75, 0xC3, 0x6B, 0xC2]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VPackSat {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let full = lift_single(&[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x40, 0x01]).unwrap();
        assert!(full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        let offsets = full
            .ops
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset, .. },
                    width: MemWidth::B4,
                    ..
                } => Some(offset),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(offsets, (0..16).map(|lane| lane * 4).collect::<Vec<_>>());

        let broadcast = lift_single(&[0x62, 0xF2, 0x75, 0x59, 0x2B, 0x40, 0x10]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));

        // W is ignored for word-to-byte packs, but W=1 is invalid for the
        // doubleword-to-word forms. Broadcast is memory-only and dword-only.
        assert!(lift_single(&[0x62, 0xF1, 0xF5, 0x08, 0x63, 0xC1]).is_ok());
        for bytes in [
            &[0xF3, 0x66, 0x0F, 0x63, 0xC1][..],       // conflicting prefix
            &[0xF0, 0x66, 0x0F, 0x67, 0xC1][..],       // LOCK
            &[0x0F, 0x38, 0x2B, 0xC1][..],             // PACKUSDW requires 66
            &[0xC5, 0xF0, 0x63, 0xC1][..],             // VEX.pp != 66
            &[0xC5, 0xF1, 0x63][..],                   // missing ModR/M
            &[0x62, 0xF1, 0xF5, 0x08, 0x6B, 0xC1][..], // dword pack W=1
            &[0x62, 0xF2, 0xF5, 0x08, 0x2B, 0xC1][..], // PACKUSDW W=1
            &[0x62, 0xF1, 0x75, 0x18, 0x63, 0x00][..], // word pack broadcast
            &[0x62, 0xF1, 0x75, 0x18, 0x6B, 0xC1][..], // register broadcast
            &[0x62, 0xF1, 0x75, 0x88, 0x63, 0xC1][..], // {z} with k0
            &[0x62, 0xF1, 0x75, 0x68, 0x63, 0xC1][..], // EVEX.L'L=3
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid saturating pack accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_legacy_vex_evex_pshufb_covers_lane_local_masks_and_fault_suppression() {
        let mmx = lift_single(&[0x0F, 0x38, 0x00, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VByteShuffle {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        control: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        lanes: 8,
                        block_lanes: 8,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x00,
                    }),
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

        let mmx_mem = lift_single(&[0x0F, 0x38, 0x00, 0x40, 0x01]).unwrap();
        assert!(mmx_mem.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            )
        )));
        assert!(
            !mmx_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x00, 0xC1]).unwrap();
        assert_eq!(legacy.bytes_consumed, 5);
        assert!(matches!(
            legacy.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VByteShuffle {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    control: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    lanes: 16,
                    block_lanes: 16,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x00,
                }),
                ..
            }]
        ));

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x00, 0x00]).unwrap();
        assert!(
            legacy_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        assert!(legacy_mem.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                },
                Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
            )
        )));
        assert!(legacy_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                elem: VecElementType::I8,
                ..
            }
        )));

        for (bytes, width, lanes) in [
            (&[0xC4, 0xE2, 0x71, 0x00, 0xC2][..], VecWidth::V128, 16),
            (&[0xC4, 0xE2, 0x75, 0x00, 0xC2][..], VecWidth::V256, 32),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::VByteShuffle {
                    dst,
                    src,
                    control,
                    lanes: actual_lanes,
                    block_lanes: 16,
                } if dst == X86_64Lifter::new().vec_reg(0, width)
                    && src == X86_64Lifter::new().vec_reg(1, width)
                    && control == X86_64Lifter::new().vec_reg(2, width)
                    && actual_lanes == lanes
            ));
            assert!(matches!(
                lifted.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: encoded_width,
                    w: false,
                }) if encoded_width == width
            ));
        }

        let evex = lift_single(&[0x62, 0xF2, 0x75, 0x49, 0x00, 0xC2]).unwrap();
        assert!(evex.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VByteShuffle {
                    dst: VReg::Virtual(_),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    control: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    lanes: 64,
                    block_lanes: 16,
                },
                Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V512,
                    w: false,
                })
            )
        )));
        assert!(evex.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                elem: VecElementType::I8,
                ..
            }
        )));

        // LLVM 20: EVEX.R'/V'/X select ZMM16/17/18. Unmasked register forms
        // are atomic native candidates; masked forms retain virtual raw state.
        let high_unmasked = lift_single(&[0x62, 0xA2, 0x75, 0x40, 0x00, 0xC2]).unwrap();
        assert!(matches!(
            high_unmasked.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VByteShuffle {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    control: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    lanes: 64,
                    block_lanes: 16,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: VecWidth::V512,
                    w: false,
                }),
                ..
            }]
        ));

        // A Full Mem disp8*N of 1 addresses 64 bytes for EVEX.512.
        let high = lift_single(&[0x62, 0xA2, 0x75, 0xC3, 0x00, 0xC2]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VByteShuffle {
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                control: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let masked_mem = lift_single(&[0x62, 0xF2, 0x75, 0x49, 0x00, 0x40, 0x01]).unwrap();
        assert!(masked_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        let offsets = masked_mem
            .ops
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset, .. },
                    width: MemWidth::B1,
                    ..
                } => Some(offset),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(offsets, (0..64).collect::<Vec<_>>());

        // W is ignored; EVEX.b is reserved because PSHUFB has no broadcast.
        let wig = lift_single(&[0x62, 0xF2, 0xF5, 0x08, 0x00, 0xC1]).unwrap();
        assert!(matches!(
            wig.ops.last().and_then(|op| op.x86_hint),
            Some(X86OpHint::EvexOp { w: true, .. })
        ));
        for bytes in [
            &[0x0F, 0x38, 0x00][..],                   // missing ModR/M
            &[0xF3, 0x66, 0x0F, 0x38, 0x00, 0xC1][..], // conflicting prefix
            &[0xF0, 0x66, 0x0F, 0x38, 0x00, 0xC1][..], // LOCK
            &[0xC4, 0xE2, 0x70, 0x00, 0xC1][..],       // VEX.pp != 66
            &[0xC4, 0xE2, 0x71, 0x00][..],             // missing ModR/M
            &[0x62, 0xF2, 0x75, 0x88, 0x00, 0xC1][..], // {z} with k0
            &[0x62, 0xF2, 0x75, 0x18, 0x00, 0xC1][..], // EVEX.b reserved
            &[0x62, 0xF2, 0x75, 0x68, 0x00, 0xC1][..], // EVEX.L'L=3
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PSHUFB encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmaddubsw_covers_legacy_vex_evex_masks_memory_and_invalids() {
        let mmx = lift_single(&[0x0F, 0x38, 0x04, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VDotProduct {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        acc: VReg::Imm(0),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        mask: None,
                        src_elem: VecElementType::I8,
                        acc_elem: VecElementType::I16,
                        width: VecWidth::V64,
                        src1_unsigned: true,
                        saturate: true,
                        zeroing: false,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x04,
                    }),
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

        let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x04, 0xC1]).unwrap();
        assert!(matches!(
            (
                &legacy.ops.last().unwrap().kind,
                legacy.ops.last().unwrap().x86_hint
            ),
            (
                OpKind::VDotProduct {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    acc: VReg::Imm(0),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V128,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x04,
                })
            )
        ));
        assert!(
            !legacy
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        );

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x04, 0x00]).unwrap();
        let alignment = legacy_mem
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .unwrap();
        let load = legacy_mem
            .ops
            .iter()
            .position(|op| {
                matches!(
                    (&op.kind, op.x86_hint),
                    (
                        OpKind::VLoad {
                            width: VecWidth::V128,
                            ..
                        },
                        Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                    )
                )
            })
            .unwrap();
        assert!(alignment < load);
        assert_eq!(
            legacy_mem
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                ))
                .count(),
            8,
            "legacy memory form must merge without clearing upper vector state"
        );

        let mmx_mem = lift_single(&[0x0F, 0x38, 0x04, 0x40, 0x01]).unwrap();
        assert!(mmx_mem.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            )
        )));
        assert!(mmx_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                width: VecWidth::V64,
                ..
            }
        )));
        assert!(
            !mmx_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        for (bytes, width, dst, src1, src2) in [
            (
                &[0xC4, 0xE2, 0x71, 0x04, 0xC2][..],
                VecWidth::V128,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                &[0xC4, 0xE2, 0x75, 0x04, 0xC2][..],
                VecWidth::V256,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(matches!(
                (&result.ops.last().unwrap().kind, result.ops.last().unwrap().x86_hint),
                (
                    OpKind::VDotProduct {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        acc: VReg::Imm(0),
                        src1: VReg::Arch(ArchReg::X86(actual_src1)),
                        src2: VReg::Arch(ArchReg::X86(actual_src2)),
                        src_elem: VecElementType::I8,
                        acc_elem: VecElementType::I16,
                        width: actual_width,
                        src1_unsigned: true,
                        saturate: true,
                        ..
                    },
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x04,
                        width: encoded_width,
                        ..
                    })
                ) if *actual_width == width
                    && encoded_width == width
                    && *actual_dst == dst
                    && *actual_src1 == src1
                    && *actual_src2 == src2
            ));
        }

        let high = lift_single(&[0x62, 0xA2, 0x75, 0x40, 0x04, 0xC2]).unwrap();
        assert!(matches!(
            (
                &high.ops.last().unwrap().kind,
                high.ops.last().unwrap().x86_hint
            ),
            (
                OpKind::VDotProduct {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    acc: VReg::Imm(0),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    width: VecWidth::V512,
                    ..
                },
                Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x04,
                    width: VecWidth::V512,
                    ..
                })
            )
        ));
        assert!(
            !high
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VMov { .. } | OpKind::VInsertLane { .. }))
        );

        for (p2, width, dst, src1, src2) in [
            (
                0x08,
                VecWidth::V128,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                0x28,
                VecWidth::V256,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
            (
                0x48,
                VecWidth::V512,
                X86Reg::Zmm(0),
                X86Reg::Zmm(1),
                X86Reg::Zmm(2),
            ),
        ] {
            let result = lift_single(&[0x62, 0xF2, 0x75, p2, 0x04, 0xC2]).unwrap();
            assert!(matches!(
                (&result.ops.last().unwrap().kind, result.ops.last().unwrap().x86_hint),
                (
                    OpKind::VDotProduct {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        acc: VReg::Imm(0),
                        src1: VReg::Arch(ArchReg::X86(actual_src1)),
                        src2: VReg::Arch(ArchReg::X86(actual_src2)),
                        width: actual_width,
                        ..
                    },
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x04,
                        width: encoded_width,
                        ..
                    })
                ) if *actual_dst == dst
                    && *actual_src1 == src1
                    && *actual_src2 == src2
                    && *actual_width == width
                    && encoded_width == width
            ));
        }

        for (p2, width, lanes) in [
            (0x09, VecWidth::V128, 8usize),
            (0x29, VecWidth::V256, 16),
            (0x49, VecWidth::V512, 32),
        ] {
            let result = lift_single(&[0x62, 0xF2, 0x75, p2, 0x04, 0xC2]).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VDotProduct { width: actual, .. } if actual == width
            )));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            dst: VReg::Arch(ArchReg::X86(
                                X86Reg::Zmm(0) | X86Reg::Ymm(0) | X86Reg::Xmm(0)
                            )),
                            elem: VecElementType::I16,
                            ..
                        }
                    ))
                    .count(),
                lanes
            );
        }

        let masked_mem = lift_single(&[0x62, 0xF2, 0x75, 0xC9, 0x04, 0x40, 0x01]).unwrap();
        assert!(masked_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(
            !masked_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        // W is ignored by every VEX/EVEX form, but retained in the exact
        // encoding hint so native lowering round-trips the guest bytes.
        for bytes in [
            &[0xC4, 0xE2, 0xF5, 0x04, 0xC2][..],
            &[0x62, 0xF2, 0xF5, 0x48, 0x04, 0xC2][..],
        ] {
            let wig = lift_single(bytes).unwrap();
            assert!(matches!(
                wig.ops.last().and_then(|op| op.x86_hint),
                Some(
                    X86OpHint::VexOp {
                        opcode: 0x04,
                        w: true,
                        ..
                    } | X86OpHint::EvexOp {
                        opcode: 0x04,
                        w: true,
                        ..
                    }
                )
            ));
        }
        for bytes in [
            &[0x0F, 0x38, 0x04][..],                   // missing ModR/M
            &[0xF3, 0x66, 0x0F, 0x38, 0x04, 0xC1][..], // conflicting prefix
            &[0xF0, 0x66, 0x0F, 0x38, 0x04, 0xC1][..], // LOCK
            &[0xC4, 0xE2, 0x70, 0x04, 0xC2][..],       // VEX.pp != 66
            &[0xC4, 0xE2, 0x71, 0x04][..],             // missing ModR/M
            &[0x62, 0xF2, 0x75, 0xC8, 0x04, 0xC2][..], // {z} with k0
            &[0x62, 0xF2, 0x75, 0x58, 0x04, 0xC2][..], // EVEX.b reserved
            &[0x62, 0xF2, 0x75, 0x68, 0x04, 0xC2][..], // EVEX.L'L=3
            &[0x62, 0xF2, 0x74, 0x48, 0x04, 0xC2][..], // EVEX.pp != 66
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PMADDUBSW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmulhrsw_covers_legacy_vex_evex_masks_memory_and_invalids() {
        let mmx = lift_single(&[0x0F, 0x38, 0x0B, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VMulShiftSat {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        src_elem: VecElementType::I16,
                        lanes: 4,
                        signed1: true,
                        signed2: true,
                        shift_left: 0,
                        round: true,
                        sat_bits: 0,
                        out_shift: 15,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x0B,
                    }),
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

        let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x0B, 0xC1]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMulShiftSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src_elem: VecElementType::I16,
                    lanes: 8,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                })
            )
        )));

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x0B, 0x00]).unwrap();
        let alignment = legacy_mem
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .unwrap();
        let load = legacy_mem
            .ops
            .iter()
            .position(|op| {
                matches!(
                    (&op.kind, op.x86_hint),
                    (
                        OpKind::VLoad {
                            width: VecWidth::V128,
                            ..
                        },
                        Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                    )
                )
            })
            .unwrap();
        assert!(alignment < load);

        let mmx_mem = lift_single(&[0x0F, 0x38, 0x0B, 0x40, 0x01]).unwrap();
        assert!(mmx_mem.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            )
        )));
        assert!(mmx_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMulShiftSat {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                lanes: 4,
                ..
            }
        )));
        assert!(
            !mmx_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        for (bytes, width, lanes, dst, src1, src2) in [
            (
                &[0xC4, 0xE2, 0x71, 0x0B, 0xC2][..],
                VecWidth::V128,
                8,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                &[0xC4, 0xE2, 0x75, 0x0B, 0xC2][..],
                VecWidth::V256,
                16,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::VMulShiftSat {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    lanes: actual_lanes,
                    ..
                } if actual_dst == dst
                    && actual_src1 == src1
                    && actual_src2 == src2
                    && actual_lanes == lanes
            ));
            assert!(matches!(
                result.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: encoded_width,
                    ..
                }) if encoded_width == width
            ));
            assert_eq!(width.lanes(VecElementType::I16) as u8, lanes);
        }

        let high = lift_single(&[0x62, 0xA2, 0x75, 0x40, 0x0B, 0xC2]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMulShiftSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    lanes: 32,
                    ..
                },
                Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V512,
                    ..
                })
            )
        )));

        let masked_mem = lift_single(&[0x62, 0xF2, 0x75, 0xC9, 0x0B, 0x40, 0x01]).unwrap();
        assert!(masked_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert_eq!(
            masked_mem
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
            32
        );

        assert!(lift_single(&[0xC4, 0xE2, 0xF5, 0x0B, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF2, 0xF5, 0x48, 0x0B, 0xC2]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x0B][..],
            &[0xF3, 0x66, 0x0F, 0x38, 0x0B, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x0B, 0xC1][..],
            &[0xC4, 0xE2, 0x70, 0x0B, 0xC2][..],
            &[0xC4, 0xE2, 0x71, 0x0B][..],
            &[0x62, 0xF2, 0x75, 0xC8, 0x0B, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0x58, 0x0B, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0x68, 0x0B, 0xC2][..],
            &[0x62, 0xF2, 0x74, 0x48, 0x0B, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PMULHRSW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_vpopcnt_covers_elements_masks_broadcasts_high_regs_and_invalids() {
        for (bytes, elem, width) in [
            (
                &[0x62, 0xA2, 0x7D, 0x8A, 0x54, 0xCA][..],
                VecElementType::I8,
                VecWidth::V128,
            ),
            (
                &[0x62, 0xA2, 0xFD, 0x2B, 0x54, 0xDC][..],
                VecElementType::I16,
                VecWidth::V256,
            ),
            (
                &[0x62, 0xA2, 0x7D, 0xCC, 0x55, 0xEE][..],
                VecElementType::I32,
                VecWidth::V512,
            ),
            (
                &[0x62, 0x82, 0xFD, 0x0D, 0x55, 0xF8][..],
                VecElementType::I64,
                VecWidth::V128,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(
                lifted.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VPopcnt {
                        elem: actual_elem,
                        width: actual_width,
                        ..
                    } if actual_elem == elem && actual_width == width
                )),
                "missing VPOPCNT for {bytes:02X?}"
            );
        }

        let direct_masked = lift_single(&[0x62, 0xA2, 0x7D, 0x8A, 0x54, 0xCA]).unwrap();
        assert!(direct_masked.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VPopcnt {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                zeroing: true,
            }
        )));
        assert_eq!(
            direct_masked
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VPopcnt { .. }))
                .count(),
            1
        );

        let broadcast = lift_single(&[0x62, 0xE2, 0x7D, 0xDE, 0x55, 0x08]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VPopcnt {
                elem: VecElementType::I32,
                width: VecWidth::V512,
                ..
            }
        )));
        let full_memory = lift_single(&[0x62, 0xE2, 0xFD, 0x2F, 0x55, 0x50, 0x01]).unwrap();
        assert!(full_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 32, .. },
                ..
            }
        )));

        for bytes in [
            &[0xC4, 0xE2, 0x7D, 0x54, 0xCA][..],
            &[0x62, 0xA2, 0x75, 0x8A, 0x54, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x80, 0x54, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x9A, 0x54, 0x0A][..],
            &[0x62, 0xA2, 0x7D, 0x9A, 0x55, 0xCA][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid VPOPCNT accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_vplzcnt_covers_dword_qword_masks_broadcast_and_invalids() {
        for (bytes, elem, width) in [
            (
                &[0x62, 0xA2, 0x7D, 0x8A, 0x44, 0xCA][..],
                VecElementType::I32,
                VecWidth::V128,
            ),
            (
                &[0x62, 0xA2, 0xFD, 0x2B, 0x44, 0xDC][..],
                VecElementType::I64,
                VecWidth::V256,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.ops.len(), 1);
            assert!(matches!(
                lifted.ops[0].kind,
                OpKind::VLeadingZeros {
                    elem: actual_elem,
                    width: actual_width,
                    mask: Some(_),
                    ..
                } if actual_elem == elem && actual_width == width
            ));
        }
        let broadcast = lift_single(&[0x62, 0xE2, 0x7D, 0xDC, 0x44, 0x28]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        for bytes in [
            &[0xC4, 0xE2, 0x7D, 0x44, 0xCA][..],
            &[0x62, 0xA2, 0x75, 0x8A, 0x44, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x80, 0x44, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x9A, 0x44, 0xCA][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_vpconflict_covers_shapes_prefix_memory_masks_and_invalids() {
        for (bytes, elem, width) in [
            (
                &[0x62, 0xA2, 0x7D, 0x8A, 0xC4, 0xCA][..],
                VecElementType::I32,
                VecWidth::V128,
            ),
            (
                &[0x62, 0xA2, 0xFD, 0x2B, 0xC4, 0xDC][..],
                VecElementType::I64,
                VecWidth::V256,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VConflict {
                    elem: actual_elem,
                    width: actual_width,
                    ..
                } if actual_elem == elem && actual_width == width
            )));
        }
        let direct_masked = lift_single(&[0x62, 0xF2, 0x7D, 0xCC, 0xC4, 0xCA]).unwrap();
        assert_eq!(direct_masked.ops.len(), 1);
        assert!(matches!(
            direct_masked.ops[0].kind,
            OpKind::VConflict {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                elem: VecElementType::I32,
                width: VecWidth::V512,
                zeroing: true,
            }
        ));
        let memory = lift_single(&[0x62, 0xF2, 0x7D, 0x89, 0xC4, 0x00]).unwrap();
        assert_eq!(
            memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            4
        );
        for bytes in [
            &[0xC4, 0xE2, 0x7D, 0xC4, 0xC1][..],
            &[0x62, 0xA2, 0x75, 0x8A, 0xC4, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x80, 0xC4, 0xCA][..],
            &[0x62, 0xA2, 0x7D, 0x9A, 0xC4, 0xCA][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_vpmadd52_covers_vex_evex_high_low_masks_broadcasts_and_invalids() {
        for (bytes, width, high) in [
            (&[0xC4, 0xE2, 0xE9, 0xB4, 0xCB][..], VecWidth::V128, false),
            (&[0xC4, 0xE2, 0xD5, 0xB5, 0xE6][..], VecWidth::V256, true),
            (
                &[0x62, 0xA2, 0xED, 0xC2, 0xB4, 0xCB][..],
                VecWidth::V512,
                false,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMultiplyAdd52 {
                    width: actual_width,
                    high: actual_high,
                    ..
                } if actual_width == width && actual_high == high
            )));
        }
        let direct_masked = lift_single(&[0x62, 0xF2, 0xED, 0xCC, 0xB4, 0xCB]).unwrap();
        assert_eq!(
            direct_masked.ops.len(),
            1,
            "register-only masked IFMA must not expand through virtual mask operations"
        );
        assert!(matches!(
            direct_masked.ops[0].kind,
            OpKind::VMultiplyAdd52 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                width: VecWidth::V512,
                high: false,
                zeroing: true,
            }
        ));
        let broadcast = lift_single(&[0x62, 0xE2, 0xD5, 0x33, 0xB5, 0x20]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B8,
                        ..
                    }
                ))
                .count(),
            4
        );
        for bytes in [
            &[0xC4, 0xE2, 0x69, 0xB4, 0xCB][..],
            &[0x62, 0xA2, 0xE4, 0xC2, 0xB4, 0xCB][..],
            &[0x62, 0xA2, 0xED, 0xC0, 0xB4, 0xCB][..],
            &[0x62, 0xA2, 0xED, 0xD2, 0xB4, 0xCB][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_vnni_dot_covers_vex_evex_variants_masks_broadcasts_and_invalids() {
        for (bytes, src_elem, unsigned, saturate, width) in [
            (
                &[0xC4, 0xE2, 0x69, 0x50, 0xCB][..],
                VecElementType::I8,
                true,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0xE2, 0x55, 0x51, 0xE6][..],
                VecElementType::I8,
                true,
                true,
                VecWidth::V256,
            ),
            (
                &[0x62, 0xA2, 0x6D, 0xC2, 0x52, 0xCB][..],
                VecElementType::I16,
                false,
                false,
                VecWidth::V512,
            ),
            (
                &[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20][..],
                VecElementType::I16,
                false,
                true,
                VecWidth::V256,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VDotProduct {
                    src_elem: actual_elem,
                    src1_unsigned: actual_unsigned,
                    saturate: actual_saturate,
                    width: actual_width,
                    acc_elem: VecElementType::I32,
                    ..
                } if actual_elem == src_elem
                    && actual_unsigned == unsigned
                    && actual_saturate == saturate
                    && actual_width == width
            )));
        }

        let direct_masked = lift_single(&[0x62, 0xF2, 0x6D, 0xCC, 0x50, 0xCB]).unwrap();
        assert_eq!(
            direct_masked.ops.len(),
            1,
            "register-only masked VNNI must not expand through virtual mask operations"
        );
        assert!(matches!(
            direct_masked.ops[0].kind,
            OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V512,
                src1_unsigned: true,
                saturate: false,
                zeroing: true,
            }
        ));
        let broadcast = lift_single(&[0x62, 0xE2, 0x55, 0x33, 0x53, 0x20]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            8
        );
        for bytes in [
            &[0xC4, 0xE2, 0xE9, 0x50, 0xCB][..],
            &[0x62, 0xA2, 0x6C, 0xC2, 0x52, 0xCB][..],
            &[0x62, 0xA2, 0x6D, 0xC0, 0x52, 0xCB][..],
            &[0x62, 0xA2, 0x6D, 0xD2, 0x52, 0xCB][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_vmovsh_covers_aliases_masks_load_store_fault_suppression_and_invalids() {
        let register = lift_single(&[0x62, 0xA5, 0x6E, 0x83, 0x10, 0xCB]).unwrap();
        assert_eq!(register.bytes_consumed, 6);
        assert!(register.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                lane: 0,
                elem: VecElementType::F16,
                ..
            }
        )));
        assert_eq!(
            register
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                        lane: 1..=7,
                        elem: VecElementType::F16,
                        ..
                    }
                ))
                .count(),
            7,
        );
        assert!(register.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                lane: 7,
                elem: VecElementType::F16,
                ..
            }
        )));

        let register_alias = lift_single(&[0x62, 0xF5, 0x6E, 0x0A, 0x11, 0xD9]).unwrap();
        assert!(register_alias.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                lane: 0,
                elem: VecElementType::F16,
                ..
            }
        )));
        assert!(register_alias.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                lane: 7,
                elem: VecElementType::F16,
                ..
            }
        )));

        let load = lift_single(&[0x62, 0xF5, 0x7E, 0x0A, 0x10, 0x48, 0x7F]).unwrap();
        assert!(load.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 254, .. },
                width: MemWidth::B2,
                ..
            }
        )));
        assert!(matches!(
            load.ops.last().unwrap().kind,
            OpKind::VBroadcast {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                elem: VecElementType::F16,
                lanes: 1,
                ..
            }
        ));

        let store = lift_single(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x50, 0x7F]).unwrap();
        assert!(store.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                lane: 0,
                elem: VecElementType::F16,
                ..
            }
        )));
        assert!(matches!(
            store.ops.last().unwrap().kind,
            OpKind::PredStore {
                addr: Address::BaseOffset { offset: 254, .. },
                width: MemWidth::B2,
                ..
            }
        ));

        // LLIG leaves L'L uninterpreted for every VMOVSH form.
        assert!(lift_single(&[0x62, 0xF5, 0x7E, 0x68, 0x10, 0xCB]).is_ok());

        for invalid in [
            &[0x62, 0xF5, 0xFE, 0x08, 0x10, 0xCB][..], // W=1
            &[0x62, 0xF5, 0x7E, 0x18, 0x10, 0xCB][..], // EVEX.b
            &[0x62, 0xF5, 0x7E, 0x88, 0x10, 0xCB][..], // {z} with k0
            &[0x62, 0xF5, 0x7D, 0x08, 0x10, 0xCB][..], // pp != F3
            &[0x62, 0xF5, 0x6E, 0x08, 0x10, 0x08][..], // memory load reserved vvvv
            &[0x62, 0xF5, 0x7E, 0x8A, 0x11, 0x10][..], // memory store cannot zero
            &[0x62, 0xF5, 0x6E, 0x0A, 0x11, 0x10][..], // memory store reserved vvvv
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
    }
    #[test]
    fn lift_vpshufbitqmb_covers_widths_high_sources_masks_memory_and_invalids() {
        for (bytes, width, dst, src, indices) in [
            (
                &[0x62, 0xF2, 0x6D, 0x08, 0x8F, 0xCB][..],
                VecWidth::V128,
                X86Reg::K(1),
                X86Reg::Xmm(2),
                X86Reg::Xmm(3),
            ),
            (
                &[0x62, 0xF2, 0x55, 0x2B, 0x8F, 0xE6][..],
                VecWidth::V256,
                X86Reg::K(4),
                X86Reg::Ymm(5),
                X86Reg::Ymm(6),
            ),
            (
                &[0x62, 0xB2, 0x6D, 0x42, 0x8F, 0xFB][..],
                VecWidth::V512,
                X86Reg::K(7),
                X86Reg::Zmm(18),
                X86Reg::Zmm(19),
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VShuffleBitQM {
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    indices: VReg::Arch(ArchReg::X86(actual_indices)),
                    width: actual_width,
                    ..
                } if actual_src == src && actual_indices == indices && actual_width == width
            )));
            assert!(
                lifted
                    .ops
                    .iter()
                    .any(|op| op.kind.dests().contains(&VReg::Arch(ArchReg::X86(dst))))
            );
        }

        let memory = lift_single(&[0x62, 0xF2, 0x55, 0x43, 0x8F, 0x60, 0x01]).unwrap();
        assert_eq!(
            memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            64
        );
        assert!(matches!(
            memory.ops.last().unwrap().kind,
            OpKind::VShuffleBitQM {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(4))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                width: VecWidth::V512,
                ..
            }
        ));

        for bytes in [
            &[0xC4, 0xE2, 0x6D, 0x8F, 0xCB][..],       // EVEX-only
            &[0x62, 0xF2, 0xED, 0x08, 0x8F, 0xCB][..], // W=1
            &[0x62, 0xF2, 0x6D, 0x68, 0x8F, 0xCB][..], // L'L=3
            &[0x62, 0xF2, 0x6D, 0x88, 0x8F, 0xCB][..], // EVEX.z is reserved
            &[0x62, 0xF2, 0x6D, 0x18, 0x8F, 0xCB][..], // EVEX.b is reserved
            &[0x62, 0x72, 0x6D, 0x08, 0x8F, 0xCB][..], // extended K destination
            &[0x62, 0xE2, 0x6D, 0x08, 0x8F, 0xCB][..], // EVEX.R' on K destination
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_evex_scatter_covers_integer_float_layouts_vsib_masks_and_invalids() {
        for (bytes, stores, width) in [
            (
                &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90][..],
                4,
                MemWidth::B4,
            ),
            (
                &[0x62, 0xC2, 0xFD, 0x07, 0xA0, 0x4C, 0xD5, 0x01][..],
                2,
                MemWidth::B8,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x2A, 0xA1, 0x24, 0x58][..],
                4,
                MemWidth::B4,
            ),
            (
                &[0x62, 0xC2, 0xFD, 0x43, 0xA1, 0x6C, 0xE1, 0x08][..],
                8,
                MemWidth::B8,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x29, 0xA2, 0x0C, 0x90][..],
                8,
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x29, 0xA2, 0x0C, 0xD0][..],
                4,
                MemWidth::B8,
            ),
            (
                &[0x62, 0xF2, 0x7D, 0x49, 0xA3, 0x0C, 0x90][..],
                8,
                MemWidth::B4,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x49, 0xA3, 0x0C, 0xD0][..],
                8,
                MemWidth::B8,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::PredStore { width: actual, .. } if actual == width))
                    .count(),
                stores
            );
            let last_store = lifted
                .ops
                .iter()
                .rposition(|op| matches!(op.kind, OpKind::PredStore { .. }))
                .unwrap();
            assert!(lifted.ops[last_store + 1..].iter().any(|op| matches!(
                op.kind,
                OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(_))),
                    ..
                }
            )));
        }

        for bytes in [
            &[0xC4, 0xE2, 0x7D, 0xA0, 0x0C, 0x90][..], // EVEX-only
            &[0x62, 0xF2, 0x7C, 0x09, 0xA0, 0x0C, 0x90][..], // mandatory 66 absent
            &[0x62, 0xF2, 0x7D, 0x08, 0xA0, 0x0C, 0x90][..], // k0 reserved
            &[0x62, 0xF2, 0x7D, 0x89, 0xA0, 0x0C, 0x90][..], // {z} reserved
            &[0x62, 0xF2, 0x75, 0x09, 0xA0, 0x0C, 0x90][..], // EVEX.vvvv reserved
            &[0x62, 0xF2, 0x7D, 0x19, 0xA0, 0x0C, 0x90][..], // EVEX.b reserved
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0xC1][..], // register destination
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x08][..], // non-VSIB memory
            &[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x88][..], // source aliases index
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "accepted reserved scatter encoding {bytes:02X?}"
            );
        }
    }
    #[test]
    fn lift_evex_integer_test_masks_cover_elements_polarities_e4_memory_and_invalids() {
        for bytes in [
            &[0x62, 0xF2, 0x65, 0x08, 0x26, 0xD4][..],
            &[0x62, 0xB2, 0xDD, 0x21, 0x26, 0xDD][..],
            &[0x62, 0xF2, 0x75, 0x52, 0x27, 0x60, 0x7F][..],
            &[0x62, 0x92, 0x8D, 0x40, 0x27, 0xEF][..],
            &[0x62, 0xF2, 0x66, 0x08, 0x26, 0xD4][..],
            &[0x62, 0xB2, 0xDE, 0x21, 0x26, 0xDD][..],
            &[0x62, 0xF2, 0x76, 0x52, 0x27, 0x60, 0x7F][..],
            &[0x62, 0x92, 0x8E, 0x40, 0x27, 0xEF][..],
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(_))),
                    ..
                } | OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(_))),
                    ..
                }
            ));
        }
        let memory = lift_single(&[0x62, 0xF2, 0x75, 0x52, 0x27, 0x60, 0x7F]).unwrap();
        assert_eq!(
            memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );

        for bytes in [
            &[0xC4, 0xE2, 0x65, 0x26, 0xD4][..],       // EVEX-only
            &[0x62, 0xF2, 0x64, 0x08, 0x26, 0xD4][..], // mandatory prefix absent
            &[0x62, 0xF2, 0x65, 0x68, 0x26, 0xD4][..], // L'L=3
            &[0x62, 0xF2, 0x65, 0x88, 0x26, 0xD4][..], // EVEX.z reserved
            &[0x62, 0xF2, 0x65, 0x18, 0x26, 0xD4][..], // byte broadcast reserved
            &[0x62, 0xE2, 0x65, 0x08, 0x26, 0xD4][..], // extended K destination
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "accepted reserved integer-test-mask encoding {bytes:02X?}"
            );
        }
    }
    #[test]
    fn lift_load_broadcasts_cover_scalar_tuple_gpr_masks_compressed_disp_and_invalids() {
        for (bytes, elem, lanes, destination) in [
            (
                &[0x62, 0xE2, 0x7D, 0xCB, 0x18, 0xCA][..],
                VecElementType::F32,
                16u8,
                X86Reg::Zmm(17),
            ),
            (
                &[0x62, 0xE2, 0x7D, 0xCB, 0x78, 0xCA][..],
                VecElementType::I8,
                64,
                X86Reg::Zmm(17),
            ),
            (
                &[0x62, 0xC2, 0xFD, 0xCB, 0x7C, 0xC9][..],
                VecElementType::I64,
                8,
                X86Reg::Zmm(17),
            ),
            (
                &[0xC4, 0xE2, 0x7D, 0x58, 0xC1][..],
                VecElementType::I32,
                8,
                X86Reg::Ymm(0),
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBroadcast {
                    elem: actual_elem,
                    lanes: actual_lanes,
                    ..
                } if actual_elem == elem && actual_lanes == lanes
            )));
            assert!(
                lifted.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        ..
                    } if actual_dst == destination
                )) || lifted.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        ..
                    } if actual_dst == destination
                ))
            );
        }

        let pair = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0x19, 0xCA]).unwrap();
        let extracted = pair
            .ops
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    lane,
                    elem: VecElementType::F32,
                    ..
                } => Some(lane),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(extracted, (0..16).map(|lane| lane % 2).collect::<Vec<_>>());

        let tuple = lift_single(&[0x62, 0xF2, 0x7D, 0xC9, 0x1A, 0x48, 0x08]).unwrap();
        assert_eq!(
            tuple
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            4
        );
        assert!(tuple.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 128,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));

        for (bytes, elem, source, destination, source_mask) in [
            (
                &[0x62, 0xE2, 0xFE, 0x48, 0x2A, 0xCF][..],
                VecElementType::I64,
                7,
                X86Reg::Zmm(17),
                0xFF,
            ),
            (
                &[0x62, 0xF2, 0x7E, 0x28, 0x3A, 0xD3][..],
                VecElementType::I32,
                3,
                X86Reg::Ymm(2),
                0xFFFF,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(matches!(
                lifted.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::And {
                            src1: VReg::Arch(ArchReg::X86(X86Reg::K(actual_source))),
                            src2: SrcOperand::Imm(actual_mask),
                            flags: FlagUpdate::None,
                            ..
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VBroadcast {
                            dst: VReg::Arch(ArchReg::X86(actual_destination)),
                            elem: actual_elem,
                            ..
                        },
                        ..
                    }
                ] if *actual_source == source
                    && *actual_mask == source_mask
                    && *actual_destination == destination
                    && *actual_elem == elem
            ));
        }

        for bytes in [
            &[0x62, 0xF2, 0x7D, 0x68, 0x18, 0xCA][..], // EVEX.L'L=3
            &[0x62, 0xF2, 0x7D, 0x88, 0x18, 0xCA][..], // {z} with k0
            &[0x62, 0xF2, 0x7D, 0x58, 0x18, 0xCA][..], // EVEX.b reserved
            &[0x62, 0xF2, 0x75, 0x48, 0x18, 0xCA][..], // vvvv reserved field
            &[0x62, 0xF2, 0x7D, 0x08, 0x19, 0xCA][..], // F32X2 has no VL=128
            &[0x62, 0xF2, 0x7D, 0x48, 0x1A, 0xCA][..], // F32X4 is memory-only
            &[0x62, 0xF2, 0x7D, 0x28, 0x5B, 0x08][..], // I32X8 requires VL=512
            &[0x62, 0xF2, 0x7D, 0x48, 0x7C, 0x08][..], // GPR form rejects memory
            &[0xC4, 0xE2, 0xFD, 0x58, 0xC1][..],       // VEX VPBROADCASTD requires W0
            &[0xC4, 0xE2, 0x79, 0x5A, 0x08][..],       // VEX I128 requires VL=256
            &[0x62, 0xF2, 0xFE, 0x09, 0x2A, 0xC9][..], // mask broadcast has no writemask
            &[0x62, 0xF2, 0x7E, 0x08, 0x2A, 0xC9][..], // MB2Q requires W1
            &[0x62, 0xF2, 0xFE, 0x08, 0x2A, 0x09][..], // mask source is register-only
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "accepted reserved broadcast encoding {bytes:02X?}"
            );
        }
    }
    #[test]
    fn lift_evex_mask_vector_conversions_cover_elements_widths_high_vectors_and_invalids() {
        for (bytes, elem, width, mask_to_vector, vector, mask) in [
            (
                &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1][..],
                VecElementType::I8,
                VecWidth::V128,
                true,
                X86Reg::Xmm(2),
                X86Reg::K(1),
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x28, 0x28, 0xE3][..],
                VecElementType::I16,
                VecWidth::V256,
                true,
                X86Reg::Ymm(4),
                X86Reg::K(3),
            ),
            (
                &[0x62, 0xE2, 0x7E, 0x48, 0x38, 0xD2][..],
                VecElementType::I32,
                VecWidth::V512,
                true,
                X86Reg::Zmm(18),
                X86Reg::K(2),
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x08, 0x38, 0xD9][..],
                VecElementType::I64,
                VecWidth::V128,
                true,
                X86Reg::Xmm(3),
                X86Reg::K(1),
            ),
            (
                &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xCA][..],
                VecElementType::I8,
                VecWidth::V128,
                false,
                X86Reg::Xmm(2),
                X86Reg::K(1),
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x28, 0x29, 0xDC][..],
                VecElementType::I16,
                VecWidth::V256,
                false,
                X86Reg::Ymm(4),
                X86Reg::K(3),
            ),
            (
                &[0x62, 0xB2, 0x7E, 0x48, 0x39, 0xD2][..],
                VecElementType::I32,
                VecWidth::V512,
                false,
                X86Reg::Zmm(18),
                X86Reg::K(2),
            ),
            (
                &[0x62, 0xF2, 0xFE, 0x08, 0x39, 0xCB][..],
                VecElementType::I64,
                VecWidth::V128,
                false,
                X86Reg::Xmm(3),
                X86Reg::K(1),
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            if mask_to_vector {
                assert!(matches!(
                    lifted.ops.last().unwrap().kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: actual_width,
                        ..
                    } if actual == vector && actual_width == width
                ));
                assert_eq!(
                    lifted
                        .ops
                        .iter()
                        .filter(|op| matches!(op.kind, OpKind::VInsertLane { elem: actual, .. } if actual == elem))
                        .count(),
                    width.lanes(elem) as usize
                );
                assert!(lifted.ops.iter().any(|op| {
                    op.kind
                        .source_vregs()
                        .contains(&VReg::Arch(ArchReg::X86(mask)))
                }));
            } else {
                assert!(matches!(
                    lifted.ops.last().unwrap().kind,
                    OpKind::Mov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: OpWidth::W64,
                        ..
                    } if actual == mask
                ));
                assert!(lifted.ops.iter().any(|op| {
                    op.kind
                        .source_vregs()
                        .contains(&VReg::Arch(ArchReg::X86(vector)))
                }));
            }
        }

        for bytes in [
            &[0xC4, 0xE2, 0x7E, 0x28, 0xD1][..],       // EVEX-only
            &[0x62, 0xF2, 0x7E, 0x09, 0x28, 0xD1][..], // E7NM forbids writemasks
            &[0x62, 0xF2, 0x76, 0x08, 0x28, 0xD1][..], // EVEX.vvvv reserved
            &[0x62, 0xF2, 0x7E, 0x18, 0x28, 0xD1][..], // EVEX.b reserved
            &[0x62, 0xF2, 0x7E, 0x68, 0x28, 0xD1][..], // L'L=3
            &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0x11][..], // memory operand
            &[0x62, 0xD2, 0x7E, 0x08, 0x28, 0xD1][..], // extended K source
            &[0x62, 0x72, 0x7E, 0x08, 0x29, 0xCA][..], // extended K destination
            &[0x62, 0xE2, 0x7E, 0x08, 0x29, 0xCA][..], // EVEX.R' K destination
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_pmullw_covers_legacy_vex_evex_masks_alignment_and_invalids() {
        for (bytes, lanes, dst, src1, src2) in [
            (
                &[0x66, 0x0F, 0xD5, 0xD1][..],
                8u8,
                X86Reg::Xmm(2),
                X86Reg::Xmm(2),
                X86Reg::Xmm(1),
            ),
            (
                &[0xC5, 0xF1, 0xD5, 0xC2][..],
                8,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                &[0xC4, 0x41, 0x35, 0xD5, 0xC2][..],
                16,
                X86Reg::Ymm(8),
                X86Reg::Ymm(9),
                X86Reg::Ymm(10),
            ),
            (
                &[0x62, 0xA1, 0x75, 0x01, 0xD5, 0xC2][..],
                8,
                X86Reg::Xmm(16),
                X86Reg::Xmm(17),
                X86Reg::Xmm(18),
            ),
            (
                &[0x62, 0xA1, 0x75, 0xA2, 0xD5, 0xC2][..],
                16,
                X86Reg::Ymm(16),
                X86Reg::Ymm(17),
                X86Reg::Ymm(18),
            ),
            (
                &[0x62, 0xA1, 0x75, 0x40, 0xD5, 0xC2][..],
                32,
                X86Reg::Zmm(16),
                X86Reg::Zmm(17),
                X86Reg::Zmm(18),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMul {
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    elem: VecElementType::I16,
                    lanes: actual_lanes,
                    ..
                } if actual_src1 == src1 && actual_src2 == src2 && actual_lanes == lanes
            )));
            assert!(
                result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMul {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        ..
                    } if actual_dst == dst
                )) || result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        ..
                    } if actual_dst == dst
                )) || result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        ..
                    } if actual_dst == dst
                ))
            );
        }

        let legacy_memory = lift_single(&[0x66, 0x0F, 0xD5, 0x00]).unwrap();
        assert!(
            legacy_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        let masked_memory = lift_single(&[0x62, 0xF1, 0x75, 0xC9, 0xD5, 0x40, 0x01]).unwrap();
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
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            }
        )));
        assert!(lift_single(&[0xC4, 0xE1, 0xF1, 0xD5, 0xC2]).is_ok());

        let mmx = lift_single(&[0x0F, 0xD5, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VMul {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: VecElementType::I16,
                        lanes: 4,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xD5,
                    }),
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
        let mmx_memory = lift_single(&[0x0F, 0xD5, 0x40, 0x01]).unwrap();
        assert!(mmx_memory.ops.iter().any(|op| matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            )
        )));
        assert!(
            !mmx_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        for bytes in [
            &[0xF3, 0x66, 0x0F, 0xD5, 0xC1][..],
            &[0xC5, 0xF0, 0xD5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0xC0, 0xD5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x50, 0xD5, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0x59, 0xD5, 0x00][..],
            &[0x62, 0xA1, 0x74, 0x40, 0xD5, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PMULLW accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_vdbpsadbw_decomposes_shuffle_sad_masks_and_full_tuple_memory() {
        let high = lift_single(&[0x62, 0xA3, 0x6D, 0xC3, 0x42, 0xCB, 0xE4]).unwrap();
        assert_eq!(high.bytes_consumed, 7);
        assert_eq!(
            high.ops
                .iter()
                .filter_map(|op| match op.kind {
                    OpKind::VMpsadbw {
                        dst: VReg::Virtual(_),
                        src1: VReg::Virtual(_),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                        mask: None,
                        width: VecWidth::V512,
                        imm,
                        zeroing: false,
                    } => Some(imm),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![0, 9, 54, 63],
        );
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                lane: 15,
                elem: VecElementType::I32,
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                elem: VecElementType::I16,
                ..
            }
        )));

        let ymm = lift_single(&[0x62, 0x53, 0x35, 0x28, 0x42, 0xC2, 0x63]).unwrap();
        assert_eq!(ymm.bytes_consumed, 7);
        assert_eq!(
            ymm.ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VMpsadbw {
                        width: VecWidth::V256,
                        ..
                    }
                ))
                .count(),
            4,
        );
        assert!(matches!(
            ymm.ops.last().map(|op| &op.kind),
            Some(OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
                width: VecWidth::V256,
                ..
            })
        ));

        // E4NF uses a complete, non-fault-suppressible source read. FULLMEM
        // disp8 scales by the selected vector length.
        let memory = lift_single(&[0x62, 0xF3, 0x6D, 0x4A, 0x42, 0x48, 0x02, 0xA5]).unwrap();
        let load = memory
            .ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        addr: Address::BaseOffset { offset: 128, .. },
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .unwrap();
        let first_sad = memory
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
            .unwrap();
        assert!(load < first_sad);
        assert!(
            !memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        for bytes in [
            &[0x62, 0xF3, 0xED, 0x48, 0x42, 0xC8, 0][..], // W=1
            &[0x62, 0xF3, 0x6D, 0x68, 0x42, 0xC8, 0][..], // L'L=3
            &[0x62, 0xF3, 0x6D, 0x18, 0x42, 0xC8, 0][..], // EVEX.b register
            &[0x62, 0xF3, 0x6D, 0x58, 0x42, 0x08, 0][..], // EVEX.b memory
            &[0x62, 0xF3, 0x6D, 0x88, 0x42, 0xC8, 0][..], // zeroing with k0
            &[0x62, 0xF3, 0x6C, 0x48, 0x42, 0xC8, 0][..], // pp != 66/F3
            &[0x62, 0xF3, 0x6D, 0x48, 0x42, 0xC8][..],    // missing imm8
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid VDBPSADBW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_evex_packed_immediate_shuffle_covers_masks_broadcast_and_reserved_bits() {
        for (bytes, width, elem, lanes) in [
            (
                &[0x62, 0xA1, 0x7D, 0x0B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V128,
                VecElementType::I32,
                4usize,
            ),
            (
                &[0x62, 0xA1, 0x7E, 0x0B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V128,
                VecElementType::I16,
                8,
            ),
            (
                &[0x62, 0xA1, 0x7F, 0x0B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V128,
                VecElementType::I16,
                8,
            ),
            (
                &[0x62, 0xA1, 0x7D, 0x2B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V256,
                VecElementType::I32,
                8,
            ),
            (
                &[0x62, 0xA1, 0x7E, 0xAB, 0x70, 0xCA, 0x1B][..],
                VecWidth::V256,
                VecElementType::I16,
                16,
            ),
            (
                &[0x62, 0xA1, 0x7F, 0xAB, 0x70, 0xCA, 0x1B][..],
                VecWidth::V256,
                VecElementType::I16,
                16,
            ),
            (
                &[0x62, 0xA1, 0x7D, 0x4B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V512,
                VecElementType::I32,
                16,
            ),
            (
                &[0x62, 0xA1, 0x7E, 0x4B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V512,
                VecElementType::I16,
                32,
            ),
            (
                &[0x62, 0xA1, 0x7F, 0x4B, 0x70, 0xCA, 0x1B][..],
                VecWidth::V512,
                VecElementType::I16,
                32,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(op.kind, OpKind::VShuffle {
                src1: VReg::Arch(ArchReg::X86(src)), elem: actual_elem, lanes: actual_lanes, ..
            } if src == match width { VecWidth::V128 => X86Reg::Xmm(18), VecWidth::V256 => X86Reg::Ymm(18), VecWidth::V512 => X86Reg::Zmm(18), _ => unreachable!() }
                && actual_elem == elem && usize::from(actual_lanes) == lanes)));
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::Select { .. }))
                    .count(),
                lanes
            );
        }

        let full = lift_single(&[0x62, 0xE1, 0x7E, 0x4B, 0x70, 0x48, 0x7F, 0x1B]).unwrap();
        assert!(full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 8128, .. },
                width: VecWidth::V512,
                ..
            }
        )));
        let broadcast = lift_single(&[0x62, 0xE1, 0x7D, 0x5B, 0x70, 0x48, 0x7F, 0x1B]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset { offset: 508, .. },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I32,
                lanes: 16,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xA1, 0xFD, 0x0B, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xA1, 0x75, 0x0B, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xA1, 0x7D, 0x03, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xA1, 0x7D, 0x6B, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xA1, 0x7D, 0x80, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xA1, 0x7D, 0x1B, 0x70, 0xCA, 0x1B][..],
            &[0x62, 0xE1, 0x7E, 0x5B, 0x70, 0x08, 0x1B][..],
            &[0x62, 0xA1, 0x7D, 0x0B, 0x70, 0xCA][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Incomplete { .. }
                        | LiftError::Unsupported { .. })
                ),
                "invalid EVEX packed shuffle accepted: {bytes:02X?}"
            );
        }
    }
    #[test]
    fn lift_evex_aligned_moves_masks_every_element_and_checks_alignment_first() {
        for (bytes, elem, lanes, dst) in [
            (
                &[0x62, 0xF1, 0x7C, 0x49, 0x28, 0xD1][..],
                VecElementType::F32,
                16,
                X86Reg::Zmm(2),
            ),
            (
                &[0x62, 0xF1, 0xFD, 0xCA, 0x28, 0xE3][..],
                VecElementType::F64,
                8,
                X86Reg::Zmm(4),
            ),
            (
                &[0x62, 0xA1, 0x7D, 0x49, 0x6F, 0xC8][..],
                VecElementType::I32,
                16,
                X86Reg::Zmm(17),
            ),
            (
                &[0x62, 0xF1, 0xFD, 0xCA, 0x6F, 0xE3][..],
                VecElementType::I64,
                8,
                X86Reg::Zmm(4),
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            dst: VReg::Arch(ArchReg::X86(actual_dst)),
                            elem: actual_elem,
                            ..
                        } if actual_dst == dst && actual_elem == elem
                    ))
                    .count(),
                lanes,
                "masked aligned register move: {bytes:02X?}"
            );
        }

        for (bytes, elem, mem_width, alignment, lanes) in [
            (
                &[0x62, 0xF1, 0x7C, 0x09, 0x28, 0x10][..],
                VecElementType::F32,
                MemWidth::B4,
                16,
                4,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x2A, 0x28, 0x10][..],
                VecElementType::F64,
                MemWidth::B8,
                32,
                4,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4B, 0x6F, 0x28][..],
                VecElementType::I32,
                MemWidth::B4,
                64,
                16,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4C, 0x6F, 0x28][..],
                VecElementType::I64,
                MemWidth::B8,
                64,
                8,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            let check = lifted
                .ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86CheckAlignment { alignment: actual, .. }
                            if actual == alignment
                    )
                })
                .unwrap();
            let first_load = lifted
                .ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: actual,
                            ..
                        } if actual == mem_width
                    )
                })
                .unwrap();
            assert!(check < first_load, "Type E1 ordering: {bytes:02X?}");
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: actual,
                            ..
                        } if actual == mem_width
                    ))
                    .count(),
                lanes
            );
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            elem: actual,
                            ..
                        } if actual == elem
                    ))
                    .count(),
                lanes * 2
            );
        }

        for (bytes, mem_width, alignment, lanes) in [
            (
                &[0x62, 0xF1, 0x7C, 0x09, 0x29, 0x08][..],
                MemWidth::B4,
                16,
                4,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x2A, 0x29, 0x08][..],
                MemWidth::B8,
                32,
                4,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4B, 0x7F, 0x08][..],
                MemWidth::B4,
                64,
                16,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4C, 0x7F, 0x08][..],
                MemWidth::B8,
                64,
                8,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            let check = lifted
                .ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::X86CheckAlignment { alignment: actual, .. }
                            if actual == alignment
                    )
                })
                .unwrap();
            let first_store = lifted
                .ops
                .iter()
                .position(|op| {
                    matches!(
                        op.kind,
                        OpKind::PredStore {
                            width: actual,
                            ..
                        } if actual == mem_width
                    )
                })
                .unwrap();
            assert!(check < first_store, "Type E1 ordering: {bytes:02X?}");
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredStore {
                            width: actual,
                            ..
                        } if actual == mem_width
                    ))
                    .count(),
                lanes
            );
        }

        for bytes in [
            &[0x62, 0xF1, 0x7C, 0x49, 0x28, 0x50, 0x01][..],
            &[0x62, 0xF1, 0xFD, 0x4B, 0x7F, 0x58, 0x01][..],
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseOffset {
                        offset: 64,
                        disp_size: DispSize::Disp8,
                        ..
                    },
                    ..
                }
            )));
        }

        let high_store = lift_single(&[0x62, 0xC1, 0xFD, 0x4C, 0x29, 0x29]).unwrap();
        assert!(high_store.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(21))),
                elem: VecElementType::F64,
                ..
            }
        )));

        for bytes in [
            &[0x62, 0xF1, 0x7C, 0xC8, 0x28, 0xC1][..], // {z} with k0
            &[0x62, 0xF1, 0x7C, 0xC9, 0x29, 0x08][..], // {z} memory store
            &[0x62, 0xF1, 0xFC, 0x49, 0x28, 0xC1][..], // VMOVAPS with W=1
            &[0x62, 0xF1, 0x7D, 0x49, 0x28, 0xC1][..], // VMOVAPD with W=0
            &[0x62, 0xF1, 0x7C, 0x69, 0x28, 0xC1][..], // reserved L'L=3
            &[0x62, 0xF1, 0x7C, 0x59, 0x28, 0xC1][..], // reserved EVEX.b
            &[0x62, 0xF1, 0x74, 0x49, 0x28, 0xC1][..], // reserved vvvv
            &[0x62, 0xF1, 0x7C, 0x41, 0x28, 0xC1][..], // reserved V'
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid masked aligned move accepted: {bytes:02X?}"
            );
        }
    }
    #[test]
    fn lift_evex_get_exponent_covers_all_formats_shapes_masks_sae_and_memory() {
        for (bytes, elem, width, lanes, scalar) in [
            (
                &[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB][..],
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
            ),
            (
                &[0x62, 0xF2, 0xFD, 0x28, 0x42, 0xCB][..],
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
            ),
            (
                &[0x62, 0xF6, 0x7D, 0x48, 0x42, 0xCB][..],
                VecElementType::F16,
                VecWidth::V512,
                32,
                false,
            ),
            (
                &[0x62, 0xF2, 0x6D, 0x08, 0x43, 0xCB][..],
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x68, 0x43, 0xCB][..],
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
            ),
            (
                &[0x62, 0xF6, 0x6D, 0x08, 0x43, 0xCB][..],
                VecElementType::F16,
                VecWidth::V128,
                1,
                true,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.last().map(|op| &op.kind),
                Some(OpKind::X86GetExponent {
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar: actual_scalar,
                    suppress_exceptions: false,
                    ..
                }) if *actual_elem == elem
                    && *actual_width == width
                    && *actual_lanes == lanes
                    && *actual_scalar == scalar
            ));
        }

        let packed_sae = lift_single(&[0x62, 0xA2, 0x7D, 0x9A, 0x42, 0xCB]).unwrap();
        assert!(matches!(
            packed_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86GetExponent {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    merge: None,
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: true,
                    suppress_exceptions: true,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: VecWidth::V512,
                    w: false,
                }),
                ..
            }]
        ));

        let scalar_sae = lift_single(&[0x62, 0xA6, 0x6D, 0x12, 0x43, 0xCB]).unwrap();
        assert!(matches!(
            scalar_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86GetExponent {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                    merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F16,
                    scalar: true,
                    suppress_exceptions: true,
                    ..
                },
                ..
            }]
        ));

        let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x4A, 0x42, 0x48, 0x01]).unwrap();
        assert_eq!(
            full_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        let broadcast = lift_single(&[0x62, 0xF6, 0x7D, 0x1A, 0x42, 0x48, 0x01]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 2, .. },
                width: MemWidth::B2,
                ..
            }
        )));
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            1
        );
        let scalar_memory = lift_single(&[0x62, 0xF2, 0x6D, 0x8A, 0x43, 0x48, 0x01]).unwrap();
        assert!(scalar_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 4, .. },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(matches!(
            scalar_memory.ops.last().map(|op| &op.kind),
            Some(OpKind::X86GetExponent {
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                src: VReg::Virtual(_),
                mask_zeroing: true,
                ..
            })
        ));

        for invalid in [
            &[0x62, 0xF2, 0x7C, 0x08, 0x42, 0xCB][..], // pp != 66
            &[0x62, 0xF6, 0xFD, 0x08, 0x42, 0xCB][..], // FP16 W=1
            &[0x62, 0xF2, 0x75, 0x08, 0x42, 0xCB][..], // packed reserved vvvv
            &[0x62, 0xF2, 0x7D, 0x00, 0x42, 0xCB][..], // packed reserved V'
            &[0x62, 0xF2, 0x7D, 0x68, 0x42, 0xCB][..], // packed L'L=3
            &[0x62, 0xF2, 0x6D, 0x18, 0x43, 0x08][..], // scalar EVEX.b memory
            &[0x62, 0xF2, 0x6D, 0x88, 0x43, 0xCB][..], // {z} with k0
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
    }
    #[test]
    fn lift_evex_get_mantissa_covers_all_formats_controls_masks_sae_and_memory() {
        for (bytes, elem, width, lanes, scalar, imm) in [
            (
                &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x03][..],
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                0x03,
            ),
            (
                &[0x62, 0xF3, 0xFD, 0x28, 0x26, 0xCB, 0x07][..],
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
                0x07,
            ),
            (
                &[0x62, 0xF3, 0x7C, 0x48, 0x26, 0xCB, 0x0B][..],
                VecElementType::F16,
                VecWidth::V512,
                32,
                false,
                0x0B,
            ),
            (
                &[0x62, 0xF3, 0x6D, 0x08, 0x27, 0xCB, 0x03][..],
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                0x03,
            ),
            (
                &[0x62, 0xF3, 0xED, 0x68, 0x27, 0xCB, 0x02][..],
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                0x02,
            ),
            (
                &[0x62, 0xF3, 0x6C, 0x08, 0x27, 0xCB, 0x01][..],
                VecElementType::F16,
                VecWidth::V128,
                1,
                true,
                0x01,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.last().map(|op| &op.kind),
                Some(OpKind::X86GetMantissa {
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    imm: actual_imm,
                    scalar: actual_scalar,
                    suppress_exceptions: false,
                    ..
                }) if *actual_elem == elem
                    && *actual_width == width
                    && *actual_lanes == lanes
                    && *actual_imm == imm
                    && *actual_scalar == scalar
            ));
        }

        let packed_sae = lift_single(&[0x62, 0xA3, 0x7C, 0x1A, 0x26, 0xCB, 0x0B]).unwrap();
        assert!(matches!(
            packed_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86GetMantissa {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    merge: None,
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F16,
                    width: VecWidth::V512,
                    lanes: 32,
                    imm: 0x0B,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: true,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::None,
                    opcode: 0x26,
                    width: VecWidth::V512,
                    w: false,
                }),
                ..
            }]
        ));

        let scalar_sae = lift_single(&[0x62, 0xA3, 0x6D, 0x92, 0x27, 0xCB, 0x03]).unwrap();
        assert!(matches!(
            scalar_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86GetMantissa {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                    merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    imm: 3,
                    scalar: true,
                    mask_zeroing: true,
                    suppress_exceptions: true,
                    ..
                },
                ..
            }]
        ));

        let full_memory = lift_single(&[0x62, 0xF3, 0x7D, 0x4A, 0x26, 0x48, 0x01, 0x03]).unwrap();
        assert_eq!(
            full_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        assert!(full_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            }
        )));

        let broadcast = lift_single(&[0x62, 0xF3, 0x7C, 0x5A, 0x26, 0x48, 0x01, 0x03]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            1
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 2, .. },
                width: MemWidth::B2,
                ..
            }
        )));

        let scalar_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x8A, 0x27, 0x48, 0x01, 0x03]).unwrap();
        assert!(scalar_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 4, .. },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(matches!(
            scalar_memory.ops.last().map(|op| &op.kind),
            Some(OpKind::X86GetMantissa {
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                src: VReg::Virtual(_),
                mask_zeroing: true,
                ..
            })
        ));

        let high_imm = lift_single(&[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0xF3]).unwrap();
        assert!(matches!(
            high_imm.ops.last().map(|op| &op.kind),
            Some(OpKind::X86GetMantissa { imm: 0xF3, .. })
        ));

        for invalid in [
            &[0x62, 0xF3, 0x7E, 0x08, 0x26, 0xCB, 0x03][..], // pp=F3
            &[0x62, 0xF3, 0xFC, 0x08, 0x26, 0xCB, 0x03][..], // FP16 W=1
            &[0x62, 0xF3, 0x75, 0x08, 0x26, 0xCB, 0x03][..], // packed reserved vvvv
            &[0x62, 0xF3, 0x7D, 0x00, 0x26, 0xCB, 0x03][..], // packed reserved V'
            &[0x62, 0xF3, 0x7D, 0x68, 0x26, 0xCB, 0x03][..], // packed L'L=3
            &[0x62, 0xF3, 0x6D, 0x18, 0x27, 0x08, 0x03][..], // scalar EVEX.b memory
            &[0x62, 0xF3, 0x6D, 0x88, 0x27, 0xCB, 0x03][..], // {z} with k0
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
        assert!(matches!(
            lift_single(&[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB]),
            Err(LiftError::Incomplete { .. })
        ));
    }
    #[test]
    fn lift_evex_round_scale_covers_all_formats_controls_masks_sae_and_memory() {
        for (bytes, elem, width, lanes, scalar, imm) in [
            (
                &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x53][..],
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                0x53,
            ),
            (
                &[0x62, 0xF3, 0xFD, 0x28, 0x09, 0xCB, 0xA7][..],
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
                0xA7,
            ),
            (
                &[0x62, 0xF3, 0x7C, 0x48, 0x08, 0xCB, 0xB9][..],
                VecElementType::F16,
                VecWidth::V512,
                32,
                false,
                0xB9,
            ),
            (
                &[0x62, 0xF3, 0x6D, 0x08, 0x0A, 0xCB, 0x4D][..],
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                0x4D,
            ),
            (
                &[0x62, 0xF3, 0xED, 0x08, 0x0B, 0xCB, 0x21][..],
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                0x21,
            ),
            (
                &[0x62, 0xF3, 0x6C, 0x08, 0x0A, 0xCB, 0x10][..],
                VecElementType::F16,
                VecWidth::V128,
                1,
                true,
                0x10,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.last().map(|op| &op.kind),
                Some(OpKind::X86RoundScale {
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    imm: actual_imm,
                    scalar: actual_scalar,
                    suppress_exceptions: false,
                    ..
                }) if *actual_elem == elem
                    && *actual_width == width
                    && *actual_lanes == lanes
                    && *actual_imm == imm
                    && *actual_scalar == scalar
            ));
        }

        let packed_sae = lift_single(&[0x62, 0xA3, 0x7C, 0x9A, 0x08, 0xCB, 0xB9]).unwrap();
        assert!(matches!(
            packed_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86RoundScale {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    merge: None,
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F16,
                    width: VecWidth::V512,
                    lanes: 32,
                    imm: 0xB9,
                    scalar: false,
                    mask_zeroing: true,
                    suppress_exceptions: true,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::None,
                    opcode: 0x08,
                    width: VecWidth::V512,
                    w: false,
                }),
                ..
            }]
        ));

        let scalar_sae = lift_single(&[0x62, 0xA3, 0x6D, 0x92, 0x0A, 0xCB, 0x4D]).unwrap();
        assert!(matches!(
            scalar_sae.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86RoundScale {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                    merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    imm: 0x4D,
                    scalar: true,
                    mask_zeroing: true,
                    suppress_exceptions: true,
                    ..
                },
                ..
            }]
        ));

        let full_memory = lift_single(&[0x62, 0xF3, 0x7D, 0x4A, 0x08, 0x48, 0x01, 0x53]).unwrap();
        assert_eq!(
            full_memory
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B4,
                        ..
                    }
                ))
                .count(),
            16
        );
        assert!(full_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            }
        )));

        let broadcast = lift_single(&[0x62, 0xF3, 0x7C, 0x5A, 0x08, 0x48, 0x01, 0x33]).unwrap();
        assert_eq!(
            broadcast
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            1
        );
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 2, .. },
                width: MemWidth::B2,
                ..
            }
        )));

        let scalar_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x8A, 0x0A, 0x48, 0x01, 0x4D]).unwrap();
        assert!(scalar_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 4, .. },
                width: MemWidth::B4,
                ..
            }
        )));
        assert!(matches!(
            scalar_memory.ops.last().map(|op| &op.kind),
            Some(OpKind::X86RoundScale {
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                src: VReg::Virtual(_),
                mask_zeroing: true,
                ..
            })
        ));

        for invalid in [
            &[0x62, 0xF3, 0x7E, 0x08, 0x08, 0xCB, 0x53][..], // pp=F3
            &[0x62, 0xF3, 0xFC, 0x08, 0x08, 0xCB, 0x53][..], // FP16 W=1
            &[0x62, 0xF3, 0xFD, 0x08, 0x08, 0xCB, 0x53][..], // F64 opcode 08
            &[0x62, 0xF3, 0x75, 0x08, 0x08, 0xCB, 0x53][..], // packed reserved vvvv
            &[0x62, 0xF3, 0x7D, 0x00, 0x08, 0xCB, 0x53][..], // packed reserved V'
            &[0x62, 0xF3, 0x7D, 0x68, 0x08, 0xCB, 0x53][..], // packed L'L=3
            &[0x62, 0xF3, 0x6D, 0x18, 0x0A, 0x08, 0x4D][..], // scalar EVEX.b memory
            &[0x62, 0xF3, 0x6D, 0x88, 0x0A, 0xCB, 0x4D][..], // {z} with k0
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
        assert!(matches!(
            lift_single(&[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB]),
            Err(LiftError::Incomplete { .. })
        ));
    }
    #[test]
    fn lift_avx512_four_fma_covers_groups_masks_tuple_fault_suppression_and_invalids() {
        let packed = lift_single(&[0x62, 0xC2, 0x5F, 0xC2, 0xAA, 0x48, 0x02]).unwrap();
        assert_eq!(packed.bytes_consumed, 7);
        assert_eq!(
            packed
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredVLoad {
                        width: VecWidth::V128,
                        ..
                    }
                ))
                .count(),
            1,
            "masked Tuple1_4X must perform one all-or-none 16-byte read"
        );
        assert!(matches!(
            packed.ops.last().map(|op| &op.kind),
            Some(OpKind::X86FourFma {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src0: VReg::Arch(ArchReg::X86(X86Reg::Zmm(20))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(21))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(22))),
                src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(23))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                scalar: false,
                negate_product: true,
                mask_zeroing: true,
                ..
            })
        ));

        // The encoded low two source-index bits select a member of the same
        // aligned source block. LL is ignored for the scalar family.
        for p2 in [0x08, 0x28, 0x48] {
            let scalar = lift_single(&[0x62, 0xF2, 0x57, p2, 0x9B, 0x08]).unwrap();
            assert!(matches!(
                scalar.ops.last().map(|op| &op.kind),
                Some(OpKind::X86FourFma {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src0: VReg::Arch(ArchReg::X86(X86Reg::Xmm(4))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(5))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(6))),
                    src3: VReg::Arch(ArchReg::X86(X86Reg::Xmm(7))),
                    scalar: true,
                    negate_product: false,
                    ..
                })
            ));
        }

        let unmasked = lift_single(&[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0x08]).unwrap();
        assert!(unmasked.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V128,
                ..
            }
        )));

        for invalid in [
            &[0x62, 0xF2, 0x5F, 0x48, 0x9A, 0xC8][..], // ModRM.mod=11b
            &[0x62, 0xF2, 0x5F, 0x58, 0x9A, 0x08][..], // EVEX.b
            &[0x62, 0xF2, 0xDF, 0x48, 0x9A, 0x08][..], // EVEX.W=1
            &[0x62, 0xF2, 0x5F, 0x28, 0x9A, 0x08][..], // packed VL=256
            &[0x62, 0xF2, 0x5F, 0x68, 0x9B, 0x08][..], // EVEX.L'L=3
            &[0x62, 0xF2, 0x5F, 0xC8, 0x9A, 0x08][..], // {z} with k0
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
        let different_pp = lift_single(&[0x62, 0xF2, 0x5D, 0x48, 0x9A, 0x08]).unwrap();
        assert!(
            !different_pp
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86FourFma { .. }))
        );
        assert!(matches!(
            lift_single(&[0x62, 0xF2, 0x5F, 0x48, 0x9A]),
            Err(LiftError::Incomplete { .. })
        ));
    }
    #[test]
    fn lift_avx512_four_dot_product_covers_groups_masks_tuple_and_invalids() {
        let saturating = lift_single(&[0x62, 0xC2, 0x5F, 0xC2, 0x53, 0x48, 0x02]).unwrap();
        assert_eq!(saturating.bytes_consumed, 7);
        assert_eq!(
            saturating
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredVLoad {
                        width: VecWidth::V128,
                        ..
                    }
                ))
                .count(),
            1,
            "masked Tuple1_4X must perform one all-or-none 16-byte read"
        );
        assert!(saturating.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredVLoad {
                addr: Address::BaseOffset { offset: 32, .. },
                ..
            }
        )));
        assert!(matches!(
            saturating.ops.last().map(|op| &op.kind),
            Some(OpKind::X86FourDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src0: VReg::Arch(ArchReg::X86(X86Reg::Zmm(20))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(21))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(22))),
                src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(23))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                saturating: true,
                mask_zeroing: true,
                ..
            })
        ));

        let wrapping = lift_single(&[0x62, 0xF2, 0x7F, 0x48, 0x52, 0x08]).unwrap();
        assert!(wrapping.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(matches!(
            wrapping.ops.last().map(|op| &op.kind),
            Some(OpKind::X86FourDotProduct {
                saturating: false,
                mask: None,
                ..
            })
        ));

        for invalid in [
            &[0x62, 0xF2, 0x7F, 0x48, 0x52, 0xC8][..], // ModRM.mod=11b
            &[0x62, 0xF2, 0x7F, 0x58, 0x52, 0x08][..], // EVEX.b
            &[0x62, 0xF2, 0xFF, 0x48, 0x52, 0x08][..], // EVEX.W=1
            &[0x62, 0xF2, 0x7F, 0x28, 0x52, 0x08][..], // VL=256
            &[0x62, 0xF2, 0x7F, 0x68, 0x52, 0x08][..], // EVEX.L'L=3
            &[0x62, 0xF2, 0x7F, 0xC8, 0x52, 0x08][..], // {z} with k0
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
        let different_pp = lift_single(&[0x62, 0xF2, 0x7D, 0x48, 0x52, 0x08]).unwrap();
        assert!(
            !different_pp
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86FourDotProduct { .. }))
        );
        assert!(matches!(
            lift_single(&[0x62, 0xF2, 0x7F, 0x48, 0x52]),
            Err(LiftError::Incomplete { .. })
        ));
    }
