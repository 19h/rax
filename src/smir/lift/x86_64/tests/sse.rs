//! tests::sse tests

use super::*;
use crate::smir::lift::x86_64::*;

    #[test]
    fn lift_maskmovdqu_covers_registers_implicit_addresses_and_reserved_forms() {
        for (bytes, data, mask) in [
            (
                &[0x66, 0x45, 0x0F, 0xF7, 0xC1][..],
                X86Reg::Xmm(8),
                X86Reg::Xmm(9),
            ),
            (
                &[0xC4, 0x41, 0x79, 0xF7, 0xC1][..],
                X86Reg::Xmm(8),
                X86Reg::Xmm(9),
            ),
            (
                &[0xC4, 0xE1, 0xF9, 0xF7, 0xC1][..],
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
            ),
            (
                &[0x66, 0x0F, 0xF7, 0xC0][..],
                X86Reg::Xmm(0),
                X86Reg::Xmm(0),
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
                        OpKind::PredStore {
                            width: MemWidth::B1,
                            ..
                        }
                    ))
                    .count(),
                16,
            );
            for lane in 0..16u8 {
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(actual)),
                        lane: actual_lane,
                        elem: VecElementType::I8,
                        ..
                    } if actual == data && actual_lane == lane
                )));
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(actual)),
                        lane: actual_lane,
                        elem: VecElementType::I8,
                        ..
                    } if actual == mask && actual_lane == lane
                )));
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::PredStore {
                        addr: Address::BaseOffset {
                            base: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                            offset,
                            ..
                        },
                        ..
                    } if offset == i64::from(lane)
                )));
            }
            assert!(result.ops.iter().all(|op| {
                op.kind.flags_written().is_empty()
                    && !op.kind.dests().iter().any(|dst| {
                        matches!(
                            dst,
                            VReg::Arch(ArchReg::X86(
                                X86Reg::Xmm(_) | X86Reg::Ymm(_) | X86Reg::Zmm(_)
                            ))
                        )
                    })
            }));
        }

        let addr32 = lift_single(&[0x67, 0xC4, 0x41, 0x79, 0xF7, 0xC1]).unwrap();
        assert_eq!(addr32.bytes_consumed, 6);
        let truncated = addr32
            .ops
            .iter()
            .find_map(|op| match op.kind {
                OpKind::And {
                    dst,
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                    src2: SrcOperand::Imm(0xFFFF_FFFF),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } => Some(dst),
                _ => None,
            })
            .expect("67h must zero-extend EDI");
        assert!(addr32.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredStore {
                addr: Address::BaseOffset {
                    base,
                    offset: 15,
                    ..
                },
                ..
            } if base == truncated
        )));

        let fs = lift_single(&[0x64, 0xC5, 0xF9, 0xF7, 0xC1]).unwrap();
        assert!(fs.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredStore {
                addr: Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                    base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
                    disp: 15,
                    ..
                },
                ..
            }
        )));

        for bytes in [
            &[0x66, 0x0F, 0xF7, 0x01][..],             // ModRM.mod must be 3
            &[0xF3, 0x66, 0x0F, 0xF7, 0xC1][..],       // no REP legacy form
            &[0xF0, 0x66, 0x0F, 0xF7, 0xC1][..],       // LOCK is invalid
            &[0xC5, 0xFD, 0xF7, 0xC1][..],             // VEX.L=1
            &[0xC5, 0xE9, 0xF7, 0xC1][..],             // VEX.vvvv is reserved
            &[0xC5, 0xF8, 0xF7, 0xC1][..],             // mandatory 66 absent
            &[0xC5, 0xF9, 0xF7, 0x01][..],             // ModRM.mod must be 3
            &[0x40, 0xC5, 0xF9, 0xF7, 0xC1][..],       // REX before VEX
            &[0x66, 0xC5, 0xF9, 0xF7, 0xC1][..],       // encoded SIMD prefix before VEX
            &[0x62, 0xF1, 0x7D, 0x08, 0xF7, 0xC1][..], // no EVEX form
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid MASKMOVDQU accepted: {bytes:02X?}",
            );
        }
        let mmx = lift_single(&[0x0F, 0xF7, 0xC1]).unwrap();
        assert_eq!(mmx.bytes_consumed, 3);
        assert_eq!(
            mmx.ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredStore {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            8
        );

        for lane in 0..8u8 {
            assert!(mmx.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    lane: actual,
                    elem: VecElementType::I8,
                    ..
                } if actual == lane
            )));
            assert!(mmx.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    lane: actual,
                    elem: VecElementType::I8,
                    ..
                } if actual == lane
            )));
            assert!(mmx.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::PredStore {
                    addr: Address::BaseOffset {
                        base: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                        offset,
                        ..
                    },
                    ..
                } if offset == i64::from(lane)
            )));
        }
        assert!(matches!(
            mmx.ops.last(),
            Some(SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            })
        ));
    }
    #[test]
    fn lift_aesni_vaes_covers_rounds_unary_keygen_widths_memory_and_invalids() {
        for (bytes, expected_op, width, src1, src2, arch_dst, imm) in [
            (
                &[0x66, 0x0F, 0x38, 0xDC, 0xC1][..],
                X86AesOp::Enc,
                VecWidth::V128,
                X86Reg::Xmm(0),
                Some(X86Reg::Xmm(1)),
                X86Reg::Xmm(0),
                0u8,
            ),
            (
                &[0x66, 0x0F, 0x38, 0xDB, 0xD3][..],
                X86AesOp::InvMixColumns,
                VecWidth::V128,
                X86Reg::Xmm(3),
                None,
                X86Reg::Xmm(2),
                0,
            ),
            (
                &[0x66, 0x45, 0x0F, 0x38, 0xDE, 0xC1][..],
                X86AesOp::Dec,
                VecWidth::V128,
                X86Reg::Xmm(8),
                Some(X86Reg::Xmm(9)),
                X86Reg::Xmm(8),
                0,
            ),
            (
                &[0x66, 0x0F, 0x3A, 0xDF, 0xE5, 0x1B][..],
                X86AesOp::KeygenAssist,
                VecWidth::V128,
                X86Reg::Xmm(5),
                None,
                X86Reg::Xmm(4),
                0x1B,
            ),
            (
                &[0xC4, 0xE2, 0x69, 0xDC, 0xD9][..],
                X86AesOp::Enc,
                VecWidth::V128,
                X86Reg::Xmm(2),
                Some(X86Reg::Xmm(1)),
                X86Reg::Xmm(3),
                0,
            ),
            (
                &[0xC4, 0xE2, 0x55, 0xDF, 0xF4][..],
                X86AesOp::DecLast,
                VecWidth::V256,
                X86Reg::Ymm(5),
                Some(X86Reg::Ymm(4)),
                X86Reg::Ymm(6),
                0,
            ),
            (
                &[0xC4, 0x42, 0x79, 0xDB, 0xC8][..],
                X86AesOp::InvMixColumns,
                VecWidth::V128,
                X86Reg::Xmm(8),
                None,
                X86Reg::Xmm(9),
                0,
            ),
            (
                &[0xC4, 0x43, 0x79, 0xDF, 0xDA, 0x5A][..],
                X86AesOp::KeygenAssist,
                VecWidth::V128,
                X86Reg::Xmm(10),
                None,
                X86Reg::Xmm(11),
                0x5A,
            ),
            (
                &[0x62, 0xA2, 0x6D, 0x40, 0xDC, 0xD9][..],
                X86AesOp::Enc,
                VecWidth::V512,
                X86Reg::Zmm(18),
                Some(X86Reg::Zmm(17)),
                X86Reg::Zmm(19),
                0,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86Aes {
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: actual_src2,
                    width: actual_width,
                    op: actual_op,
                    imm: actual_imm,
                    ..
                } if actual_src1 == src1
                    && actual_src2 == src2.map(|reg| VReg::Arch(ArchReg::X86(reg)))
                    && actual_width == width
                    && actual_op == expected_op
                    && actual_imm == imm
            )));
            assert!(result.ops.iter().any(|op| {
                op.kind
                    .dests()
                    .contains(&VReg::Arch(ArchReg::X86(arch_dst)))
            }));
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0xDE, 0x00]).unwrap();
        assert!(
            legacy_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        let vex_mem = lift_single(&[0xC4, 0xE2, 0x69, 0xDD, 0x18]).unwrap();
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        let evex_mem = lift_single(&[0x62, 0xE2, 0x5D, 0x20, 0xDE, 0x68, 0x02]).unwrap();
        assert!(evex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 64, .. },
                width: VecWidth::V256,
                ..
            }
        )));

        for bytes in [
            &[0x0F, 0x38, 0xDC, 0xC1][..],                   // mandatory 66 absent
            &[0xF0, 0x66, 0x0F, 0x38, 0xDC, 0xC1][..],       // LOCK
            &[0xC4, 0xE2, 0x7D, 0xDB, 0xC1][..],             // VAESIMC VEX.L=1
            &[0xC4, 0xE2, 0x71, 0xDB, 0xC1][..],             // VAESIMC reserved vvvv
            &[0xC4, 0xE3, 0x7D, 0xDF, 0xC1, 0x01][..],       // keygen VEX.L=1
            &[0xC4, 0xE3, 0x71, 0xDF, 0xC1, 0x01][..],       // keygen reserved vvvv
            &[0x62, 0xF2, 0x7D, 0x08, 0xDB, 0xC1][..],       // no EVEX VAESIMC
            &[0x62, 0xF3, 0x7D, 0x08, 0xDF, 0xC1, 0x01][..], // no EVEX keygen
            &[0x62, 0xF2, 0x75, 0x09, 0xDC, 0xC1][..],       // EVEX masking unsupported
            &[0x62, 0xF2, 0x75, 0x18, 0xDC, 0xC1][..],       // EVEX.b
            &[0x62, 0xF2, 0x75, 0x68, 0xDC, 0xC1][..],       // EVEX.L'L=3
            &[0xC4, 0xE2, 0x68, 0xDC, 0xC1][..],             // mandatory 66 absent
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid AES encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn legacy_packed_sse_results_are_bounded_to_xmm_lanes() {
        for (name, bytes, result_kind) in [
            ("ADDPS", &[0x0F, 0x58, 0xC1][..], "add"),
            ("ANDPS", &[0x0F, 0x54, 0xC1][..], "and"),
            ("PADDD", &[0x66, 0x0F, 0xFE, 0xC1][..], "add"),
            ("PMULLD", &[0x66, 0x0F, 0x38, 0x40, 0xC1][..], "mul"),
        ] {
            let result = lift_single(bytes).unwrap();
            match result_kind {
                "add" => assert!(result.ops.iter().any(|op| matches!(
                    op,
                    SmirOp {
                        kind: OpKind::VAdd {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                ))),
                "and" => assert!(result.ops.iter().any(|op| matches!(
                    op,
                    SmirOp {
                        kind: OpKind::VAnd {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                ))),
                "mul" => assert!(result.ops.iter().any(|op| matches!(
                    op,
                    SmirOp {
                        kind: OpKind::VMul {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                ))),
                _ => unreachable!(),
            }
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VInsertLane {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            ..
                        }
                    ))
                    .count(),
                0,
                "{name}: canonical legacy operation must remain directly lowerable"
            );
        }

        let register_move = lift_single(&[0x0F, 0x10, 0xC1]).unwrap();
        assert!(matches!(
            register_move.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    width: VecWidth::V128,
                },
                x86_hint: Some(X86OpHint::SseMov { .. }),
                ..
            }]
        ));

        let memory = lift_single(&[0x0F, 0x10, 0x00]).unwrap();
        assert!(memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: VecWidth::V128,
                ..
            }
        )));
    }
    #[test]
    fn lift_vcvtps2ph_reversed_operands_destinations_rounding_and_reserved_fields() {
        for (bytes, dst, src, lanes, dst_width, mask, zeroing, round, suppress) in [
            (
                &[0xC4, 0xE3, 0x79, 0x1D, 0xD1, 0x00][..],
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
                4,
                VecWidth::V64,
                None,
                false,
                FpRoundMode::RoundNearest,
                false,
            ),
            (
                &[0xC4, 0xE3, 0x7D, 0x1D, 0xD1, 0x05][..],
                X86Reg::Xmm(1),
                X86Reg::Ymm(2),
                8,
                VecWidth::V128,
                None,
                false,
                FpRoundMode::Dynamic,
                false,
            ),
            (
                &[0x62, 0xA3, 0x7D, 0xCB, 0x1D, 0xD1, 0x02][..],
                X86Reg::Ymm(17),
                X86Reg::Zmm(18),
                16,
                VecWidth::V256,
                Some(X86Reg::K(3)),
                true,
                FpRoundMode::RoundUp,
                false,
            ),
            (
                &[0x62, 0xF3, 0x7D, 0x99, 0x1D, 0xD1, 0x03][..],
                X86Reg::Ymm(1),
                X86Reg::Zmm(2),
                16,
                VecWidth::V256,
                Some(X86Reg::K(1)),
                true,
                FpRoundMode::RoundTowardZero,
                true,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::X86PackedFpConvert {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    mask: actual_mask,
                    from: VecElementType::F32,
                    to: VecElementType::F16,
                    lanes: actual_lanes,
                    dst_width: actual_width,
                    mask_zeroing: actual_zeroing,
                    zero_upper: true,
                    round: actual_round,
                    suppress_exceptions: actual_suppress,
                    report_fp16_denormal: false,
                } if actual_dst == dst && actual_src == src
                    && actual_mask == mask.map(|reg| VReg::Arch(ArchReg::X86(reg)))
                    && actual_lanes == lanes && actual_width == dst_width
                    && actual_zeroing == zeroing && actual_round == round
                    && actual_suppress == suppress
            ));
        }

        for (bytes, lanes, expected_offset, round) in [
            (
                &[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x50, 0x02, 0x00][..],
                4,
                16,
                FpRoundMode::RoundNearest,
            ),
            (
                &[0x62, 0xF3, 0x7D, 0x29, 0x1D, 0x50, 0x02, 0x01][..],
                8,
                32,
                FpRoundMode::RoundDown,
            ),
            (
                &[0x62, 0xF3, 0x7D, 0x49, 0x1D, 0x50, 0x02, 0x06][..],
                16,
                64,
                FpRoundMode::Dynamic,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::X86PackedFpConvertStore {
                    addr: Address::BaseOffset {
                        base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        offset,
                        disp_size: DispSize::Disp8,
                    },
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)
                        | X86Reg::Ymm(2)
                        | X86Reg::Zmm(2))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    lanes: actual_lanes,
                    round: actual_round,
                } if offset == expected_offset && actual_lanes == lanes && actual_round == round
            ));
        }

        for bytes in [
            &[0x67, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0x00][..],
            &[0x64, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0x00][..],
            &[0x67, 0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x00][..],
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::X86PackedFpConvertStore { .. }
            ));
        }

        for bytes in [
            &[0x62, 0xF3, 0x7D, 0x88, 0x1D, 0xD1, 0x00][..], // {z} with k0
            &[0x62, 0xF3, 0x7D, 0x89, 0x1D, 0x10, 0x00][..], // zeroing store
            &[0x62, 0xF3, 0x7D, 0x19, 0x1D, 0x10, 0x00][..], // EVEX.b memory
            &[0x62, 0xF3, 0x7D, 0x69, 0x1D, 0xD1, 0x00][..], // reserved L'L=3
            &[0x62, 0xF3, 0x75, 0x09, 0x1D, 0xD1, 0x00][..], // reserved vvvv
            &[0xC4, 0xE3, 0xF9, 0x1D, 0xD1, 0x00][..],       // VEX.W=1
            &[0xC4, 0xE3, 0x71, 0x1D, 0xD1, 0x00][..],       // reserved VEX.vvvv
            &[0x66, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0x00][..], // separate SIMD prefix
            &[0xF0, 0xC4, 0xE3, 0x79, 0x1D, 0x10, 0x00][..], // LOCK
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
        assert!(matches!(
            lift_single(&[0xC4, 0xE3, 0x79, 0x1D, 0xD1]),
            Err(LiftError::Incomplete { .. })
        ));
    }
    #[test]
    fn lift_pabs_family_covers_widths_masks_broadcasts_and_reserved_fields() {
        for (opcode, elem) in [
            (0x1C, VecElementType::I8),
            (0x1D, VecElementType::I16),
            (0x1E, VecElementType::I32),
        ] {
            let mmx = lift_single(&[0x0F, 0x38, opcode, 0xC1]).unwrap();
            assert!(matches!(
                mmx.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::VUnary {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            elem: actual,
                            lanes,
                            op: VecUnaryOp::Abs,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
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
                ] if *actual == elem
                    && u32::from(*lanes) == VecWidth::V64.lanes(elem)
                    && *actual_opcode == opcode
            ));

            let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
            assert!(legacy.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VUnary {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: actual,
                    op: VecUnaryOp::Abs,
                    ..
                } if actual == elem
            )));
            for (p2, width, dst, src) in [
                (0x79, VecWidth::V128, X86Reg::Xmm(0), X86Reg::Xmm(2)),
                (0x7D, VecWidth::V256, X86Reg::Ymm(0), X86Reg::Ymm(2)),
            ] {
                let vex = lift_single(&[0xC4, 0xE2, p2, opcode, 0xC2]).unwrap();
                assert!(matches!(
                    vex.ops.last().unwrap().kind,
                    OpKind::VUnary {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        src: VReg::Arch(ArchReg::X86(actual_src)),
                        elem: actual_elem,
                        lanes: actual_lanes,
                        op: VecUnaryOp::Abs,
                        ..
                    } if actual_dst == dst
                        && actual_src == src
                        && actual_elem == elem
                        && u32::from(actual_lanes) == width.lanes(elem)
                ));
            }
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x1D, 0x00]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let mmx_mem = lift_single(&[0x0F, 0x38, 0x1D, 0x40, 0x01]).unwrap();
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
            OpKind::VUnary {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                elem: VecElementType::I16,
                lanes: 4,
                op: VecUnaryOp::Abs,
                ..
            }
        )));
        assert!(
            !mmx_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        for (opcode, w, elem, lanes) in [
            (0x1C, 0x7D, VecElementType::I8, 64),
            (0x1D, 0x7D, VecElementType::I16, 32),
            (0x1E, 0x7D, VecElementType::I32, 16),
            (0x1F, 0xFD, VecElementType::I64, 8),
        ] {
            let evex = lift_single(&[0x62, 0xF2, w, 0x49, opcode, 0xC2]).unwrap();
            assert!(evex.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VUnary {
                    elem: actual,
                    lanes: actual_lanes,
                    op: VecUnaryOp::Abs,
                    ..
                } if actual == elem && actual_lanes == lanes
            )));
        }

        let high_d = lift_single(&[0x62, 0xA2, 0x7D, 0x48, 0x1E, 0xC2]).unwrap();
        assert!(high_d.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                elem: VecElementType::I32,
                ..
            }
        )));
        assert!(!high_d.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let masked_words = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0x1D, 0x00]).unwrap();
        assert_eq!(
            masked_words
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

        let broadcast_d = lift_single(&[0x62, 0xF2, 0x7D, 0xD9, 0x1E, 0x40, 0x01]).unwrap();
        assert!(broadcast_d.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    offset: 4,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: MemWidth::B4,
                ..
            }
        )));
        assert_eq!(
            broadcast_d
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            1
        );
        let broadcast_q = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0x1F, 0x40, 0x01]).unwrap();
        assert!(broadcast_q.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset { offset: 8, .. },
                width: MemWidth::B8,
                ..
            }
        )));

        // Byte/word W is ignored in EVEX; dword/qword W is fixed.
        assert!(lift_single(&[0x62, 0xF2, 0xFD, 0x48, 0x1C, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF2, 0xFD, 0x48, 0x1D, 0xC2]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x1C][..], // missing ModR/M
            &[0xF0, 0x66, 0x0F, 0x38, 0x1D, 0xC1][..],
            &[0xC4, 0xE2, 0x71, 0x1C, 0xC2][..], // VEX.vvvv not reserved value
            &[0xC4, 0xE2, 0x79, 0x1F, 0xC2][..], // no VEX PABSQ
            &[0x62, 0xF2, 0x7D, 0xC8, 0x1C, 0xC2][..], // {z} with k0
            &[0x62, 0xF2, 0x7D, 0x58, 0x1C, 0xC2][..], // byte broadcast
            &[0x62, 0xF2, 0x7D, 0x58, 0x1E, 0xC2][..], // register broadcast
            &[0x62, 0xF2, 0xFD, 0x48, 0x1E, 0xC2][..], // VPABSD W=1
            &[0x62, 0xF2, 0x7D, 0x48, 0x1F, 0xC2][..], // VPABSQ W=0
            &[0x62, 0xF2, 0x75, 0x48, 0x1E, 0xC2][..], // EVEX.vvvv != 1111b
            &[0x62, 0xF2, 0x7D, 0x40, 0x1E, 0xC2][..], // EVEX.V' reserved
            &[0x62, 0xF2, 0x7D, 0x68, 0x1E, 0xC2][..], // EVEX.L'L=3
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PABS encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_palignr_covers_forms_grouping_masks_memory_and_invalids() {
        let mmx = lift_single(&[0x0F, 0x3A, 0x0F, 0xC1, 0x05]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86PackedAlignRight {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        high: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        low: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                        amount: 5,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x0F,
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

        let legacy = lift_single(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, 0x05]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))),
                elem: VecElementType::I8,
                lanes: 16,
                ..
            }
        )));
        assert_eq!(
            legacy
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I8,
                        ..
                    }
                ))
                .count(),
            16
        );

        for (bytes, width, lanes, dst, high, low) in [
            (
                &[0xC4, 0xE3, 0x71, 0x0F, 0xC2, 0x05][..],
                VecWidth::V128,
                16,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                &[0xC4, 0xE3, 0x75, 0x0F, 0xC2, 0x11][..],
                VecWidth::V256,
                32,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(matches!(
                result.ops.last().unwrap().kind,
                OpKind::VShuffle {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src1: VReg::Arch(ArchReg::X86(actual_low)),
                    src2: Some(VReg::Arch(ArchReg::X86(actual_high))),
                    lanes: actual_lanes,
                    ..
                } if actual_dst == dst
                    && actual_low == low
                    && actual_high == high
                    && actual_lanes == lanes
            ));
            assert_eq!(width.lanes(VecElementType::I8) as u8, lanes);
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x3A, 0x0F, 0x00, 0x01]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let vex_mem = lift_single(&[0xC4, 0xE3, 0x75, 0x0F, 0x00, 0x01]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        let high = lift_single(&[0x62, 0xA3, 0x75, 0x40, 0x0F, 0xC2, 0x1F]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Zmm(17)))),
                lanes: 64,
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let masked_mem = lift_single(&[0x62, 0xF3, 0x75, 0xC9, 0x0F, 0x40, 0x01, 0x01]).unwrap();
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
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            60
        );
        let masked_high_only = lift_single(&[0x62, 0xF3, 0x75, 0x49, 0x0F, 0x00, 0x10]).unwrap();
        assert!(
            !masked_high_only
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        assert!(lift_single(&[0xC4, 0xE3, 0xF5, 0x0F, 0xC2, 0x05]).is_ok());
        assert!(lift_single(&[0x62, 0xF3, 0xF5, 0x48, 0x0F, 0xC2, 0x05]).is_ok());
        let mmx = lift_single(&[0x0F, 0x3A, 0x0F, 0xC1, 0x05]).unwrap();
        assert!(mmx.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedAlignRight {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                high: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                low: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                width: VecWidth::V64,
                amount: 5,
            }
        )));
        assert!(matches!(
            mmx.ops.last(),
            Some(SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            })
        ));
        let mmx_memory = lift_single(&[0x0F, 0x3A, 0x0F, 0x40, 0x01, 0x05]).unwrap();
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
            &[0xF0, 0x66, 0x0F, 0x3A, 0x0F, 0xC1, 0x05][..],
            &[0xC4, 0xE3, 0x70, 0x0F, 0xC2, 0x05][..],
            &[0xC4, 0xE3, 0x71, 0x0F, 0xC2][..],
            &[0x62, 0xF3, 0x75, 0xC8, 0x0F, 0xC2, 0x05][..],
            &[0x62, 0xF3, 0x75, 0x58, 0x0F, 0xC2, 0x05][..],
            &[0x62, 0xF3, 0x75, 0x68, 0x0F, 0xC2, 0x05][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PALIGNR encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_packed_extend_families_cover_shapes_tuples_masks_and_invalids() {
        let shapes = [
            (0x20, VecElementType::I8, VecElementType::I16, true),
            (0x21, VecElementType::I8, VecElementType::I32, true),
            (0x22, VecElementType::I8, VecElementType::I64, true),
            (0x23, VecElementType::I16, VecElementType::I32, true),
            (0x24, VecElementType::I16, VecElementType::I64, true),
            (0x25, VecElementType::I32, VecElementType::I64, true),
            (0x30, VecElementType::I8, VecElementType::I16, false),
            (0x31, VecElementType::I8, VecElementType::I32, false),
            (0x32, VecElementType::I8, VecElementType::I64, false),
            (0x33, VecElementType::I16, VecElementType::I32, false),
            (0x34, VecElementType::I16, VecElementType::I64, false),
            (0x35, VecElementType::I32, VecElementType::I64, false),
        ];
        for (opcode, src_elem, dst_elem, signed) in shapes {
            let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
            let lanes128 = VecWidth::V128.lanes(dst_elem) as usize;
            assert_eq!(
                legacy
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VExtractLane {
                            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                            elem: actual,
                            sign,
                            ..
                        } if actual == src_elem
                            && sign == if signed { SignExtend::Sign } else { SignExtend::Zero }
                    ))
                    .count(),
                lanes128
            );
            assert!(legacy.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: actual,
                    ..
                } if actual == dst_elem
            )));

            for (p2, width, dst) in [
                (0x79, VecWidth::V128, X86Reg::Xmm(0)),
                (0x7D, VecWidth::V256, X86Reg::Ymm(0)),
            ] {
                let vex = lift_single(&[0xC4, 0xE2, p2, opcode, 0xC2]).unwrap();
                let lanes = width.lanes(dst_elem) as usize;
                assert_eq!(
                    vex.ops
                        .iter()
                        .filter(|op| matches!(
                            op.kind,
                            OpKind::VInsertLane {
                                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                                elem: actual_elem,
                                ..
                            } if actual_dst == dst && actual_elem == dst_elem
                        ))
                        .count(),
                    lanes
                );
            }

            let evex = lift_single(&[0x62, 0xF2, 0x7D, 0x49, opcode, 0xC2]).unwrap();
            assert!(evex.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: actual,
                    sign,
                    ..
                } if actual == src_elem
                    && sign == if signed { SignExtend::Sign } else { SignExtend::Zero }
            )));
        }

        let high = lift_single(&[0x62, 0xA2, 0x7D, 0x48, 0x20, 0xC2]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Ymm(18))),
                elem: VecElementType::I8,
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let bw_mem = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0x20, 0x40, 0x01]).unwrap();
        assert!(bw_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 32,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert_eq!(
            bw_mem
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
            32
        );

        let bq_mem = lift_single(&[0x62, 0xF2, 0x7D, 0xC9, 0x32, 0x40, 0x01]).unwrap();
        assert!(bq_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 8, .. },
                ..
            }
        )));
        assert_eq!(
            bq_mem
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
            8
        );

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x22, 0x00]).unwrap();
        assert_eq!(
            legacy_mem
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Load {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(
            !legacy_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // W is ignored except for the fixed-W0 dword-to-qword EVEX forms.
        assert!(lift_single(&[0x62, 0xF2, 0xFD, 0x48, 0x20, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF2, 0xFD, 0x48, 0x34, 0xC2]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x20, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x30, 0xC1][..],
            &[0xC4, 0xE2, 0x71, 0x20, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0x48, 0x20, 0xC2][..],
            &[0x62, 0xF2, 0x7D, 0x40, 0x20, 0xC2][..],
            &[0x62, 0xF2, 0xFD, 0x48, 0x25, 0xC2][..],
            &[0x62, 0xF2, 0xFD, 0x48, 0x35, 0xC2][..],
            &[0x62, 0xF2, 0x7D, 0x58, 0x20, 0xC2][..],
            &[0x62, 0xF2, 0x7D, 0xC8, 0x20, 0xC2][..],
            &[0x62, 0xF2, 0x7D, 0x68, 0x20, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid packed-extend encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_packed_minmax_covers_signedness_widths_masks_broadcasts_and_invalids() {
        let shapes = [
            (0x38, VecElementType::I8, VLaneOp::Min, true),
            (0x39, VecElementType::I32, VLaneOp::Min, true),
            (0x3A, VecElementType::I16, VLaneOp::Min, false),
            (0x3B, VecElementType::I32, VLaneOp::Min, false),
            (0x3C, VecElementType::I8, VLaneOp::Max, true),
            (0x3D, VecElementType::I32, VLaneOp::Max, true),
            (0x3E, VecElementType::I16, VLaneOp::Max, false),
            (0x3F, VecElementType::I32, VLaneOp::Max, false),
        ];
        for (opcode, elem, lane_op, signed) in shapes {
            let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
            assert_eq!(legacy.ops.len(), 1);
            assert!(matches!(
                (&legacy.ops[0].kind, legacy.ops[0].x86_hint),
                (
                    OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: actual_elem,
                        lanes,
                        op: actual_op,
                        signed: actual_signed,
                        set_ovf: false,
                    },
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: actual_opcode,
                    })
                ) if *actual_elem == elem
                    && *lanes == VecWidth::V128.lanes(elem) as u8
                    && *actual_op == lane_op
                    && *actual_signed == signed
                    && actual_opcode == opcode
            ));

            for (p2, width, dst, src1, src2) in [
                (
                    0x71,
                    VecWidth::V128,
                    X86Reg::Xmm(0),
                    X86Reg::Xmm(1),
                    X86Reg::Xmm(2),
                ),
                (
                    0x75,
                    VecWidth::V256,
                    X86Reg::Ymm(0),
                    X86Reg::Ymm(1),
                    X86Reg::Ymm(2),
                ),
            ] {
                let vex = lift_single(&[0xC4, 0xE2, p2, opcode, 0xC2]).unwrap();
                assert_eq!(vex.ops.len(), 1);
                assert!(matches!(
                    (&vex.ops[0].kind, vex.ops[0].x86_hint),
                    (
                        OpKind::VLane {
                            dst: VReg::Arch(ArchReg::X86(actual_dst)),
                            src1: VReg::Arch(ArchReg::X86(actual_src1)),
                            src2: VReg::Arch(ArchReg::X86(actual_src2)),
                            elem: actual_elem,
                            lanes,
                            op: actual_op,
                            signed: actual_signed,
                            set_ovf: false,
                        },
                        Some(X86OpHint::VexOp {
                            map: X86VecMap::Map0F38,
                            pp: X86SsePrefix::OpSize,
                            opcode: actual_opcode,
                            width: actual_width,
                            ..
                        })
                    ) if *actual_dst == dst
                        && *actual_src1 == src1
                        && *actual_src2 == src2
                        && *actual_elem == elem
                        && *lanes == width.lanes(elem) as u8
                        && *actual_op == lane_op
                        && *actual_signed == signed
                        && actual_opcode == opcode
                        && actual_width == width
                ));
            }
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x38, 0x00]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let vex_mem = lift_single(&[0xC4, 0xE2, 0x75, 0x3A, 0x00]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // EVEX.W selects dword/qword for odd opcodes and preserves all three
        // high-register extension paths.
        let high = lift_single(&[0x62, 0xA2, 0xF5, 0x40, 0x39, 0xC2]).unwrap();
        assert_eq!(high.ops.len(), 1);
        assert!(matches!(
            (&high.ops[0].kind, high.ops[0].x86_hint),
            (
                OpKind::VLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    elem: VecElementType::I64,
                    lanes: 8,
                    op: VLaneOp::Min,
                    signed: true,
                    set_ovf: false,
                },
                Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x39,
                    width: VecWidth::V512,
                    w: true,
                })
            )
        ));

        let qword_broadcast = lift_single(&[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x40, 0x01]).unwrap();
        assert!(qword_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 8,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
        assert_eq!(
            qword_broadcast
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

        let dword_broadcast = lift_single(&[0x62, 0xF2, 0x75, 0xD9, 0x39, 0x40, 0x01]).unwrap();
        assert!(dword_broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 4, .. },
                ..
            }
        )));
        assert_eq!(
            dword_broadcast
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

        let word_full = lift_single(&[0x62, 0xF2, 0x75, 0x49, 0x3E, 0x40, 0x01]).unwrap();
        assert!(word_full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            }
        )));
        assert_eq!(
            word_full
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

        // W is ignored for byte/word forms.
        assert!(lift_single(&[0x62, 0xF2, 0xF5, 0x48, 0x38, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF2, 0xF5, 0x48, 0x3E, 0xC2]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x38, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x39, 0xC1][..],
            &[0xC4, 0xE2, 0x74, 0x38, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0xC8, 0x38, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0x58, 0x38, 0x00][..],
            &[0x62, 0xF2, 0x75, 0x58, 0x3A, 0x00][..],
            &[0x62, 0xF2, 0x75, 0x58, 0x39, 0xC2][..],
            &[0x62, 0xF2, 0x75, 0x68, 0x39, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid packed min/max encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_ptest_vptest_covers_reductions_flags_alignment_and_invalids() {
        let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x17, 0xC1]).unwrap();
        assert_eq!(
            legacy
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        elem: VecElementType::I64,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert_eq!(
            legacy
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: VecElementType::I64,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(
            legacy
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::AndNot { .. }))
        );
        assert!(
            legacy
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
        );
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::And {
                src2: SrcOperand::Imm(mask),
                ..
            } if mask == !0x8D5
        )));
        assert!(matches!(
            legacy.ops.last().unwrap().kind,
            OpKind::WriteFlags { .. }
        ));
        assert!(!legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(_)),
                ..
            } | OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(_)),
                ..
            }
        )));

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x17, 0x00]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        let flag_write = legacy_mem
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
            .unwrap();
        assert!(alignment < load && load < flag_write);

        for (bytes, width, first, second, chunks) in [
            (
                &[0xC4, 0x62, 0x79, 0x17, 0xC1][..],
                VecWidth::V128,
                X86Reg::Xmm(8),
                X86Reg::Xmm(1),
                2usize,
            ),
            (
                &[0xC4, 0x42, 0x7D, 0x17, 0xD1][..],
                VecWidth::V256,
                X86Reg::Ymm(10),
                X86Reg::Ymm(9),
                4usize,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VExtractLane {
                            vec: VReg::Arch(ArchReg::X86(actual)),
                            ..
                        } if actual == first
                    ))
                    .count(),
                chunks
            );
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VExtractLane {
                            vec: VReg::Arch(ArchReg::X86(actual)),
                            ..
                        } if actual == second
                    ))
                    .count(),
                chunks
            );
            assert_eq!(width.lanes(VecElementType::I64) as usize, chunks);
        }

        let vex_mem = lift_single(&[0xC4, 0xE2, 0x7D, 0x17, 0x40, 0x20]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        assert!(matches!(
            vex_mem.ops.last().unwrap().kind,
            OpKind::WriteFlags { .. }
        ));

        // VEX.W is ignored.
        assert!(lift_single(&[0xC4, 0xE2, 0xF9, 0x17, 0xC1]).is_ok());
        assert!(lift_single(&[0xC4, 0xE2, 0xFD, 0x17, 0xC1]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x17, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x17, 0xC1][..],
            &[0xF3, 0x66, 0x0F, 0x38, 0x17, 0xC1][..],
            &[0xC4, 0xE2, 0x71, 0x17, 0xC1][..],
            &[0xC4, 0xE2, 0x78, 0x17, 0xC1][..],
            &[0x62, 0xF2, 0x7D, 0x08, 0x17, 0xC1][..],
            &[0xC4, 0xE2, 0x79, 0x17][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PTEST encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_avx_permute_family_covers_domains_widths_masks_memory_and_invalids() {
        for (bytes, elem, width) in [
            (
                &[0xC4, 0xE2, 0x69, 0x0C, 0xCB][..],
                VecElementType::F32,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0xE2, 0x55, 0x0D, 0xE6][..],
                VecElementType::F64,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0xE2, 0x6D, 0x36, 0xCB][..],
                VecElementType::I32,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0xE2, 0x55, 0x16, 0xE6][..],
                VecElementType::F32,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0xE3, 0xFD, 0x00, 0xCA, 0x1B][..],
                VecElementType::I64,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0xE3, 0xFD, 0x01, 0xDC, 0xE4][..],
                VecElementType::F64,
                VecWidth::V256,
            ),
            (
                &[0x62, 0xA2, 0x6D, 0x82, 0x8D, 0xCB][..],
                VecElementType::I8,
                VecWidth::V128,
            ),
            (
                &[0x62, 0xA2, 0xD5, 0x43, 0x8D, 0xE6][..],
                VecElementType::I16,
                VecWidth::V512,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(
                lifted.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VPermute {
                        elem: actual_elem,
                        width: actual_width,
                        src2: None,
                        ..
                    } | OpKind::X86PermuteBytesWords {
                        elem: actual_elem,
                        width: actual_width,
                        table2: None,
                        ..
                    } if actual_elem == elem && actual_width == width
                )),
                "missing permutation for {bytes:02X?}"
            );
        }

        let direct_byte = lift_single(&[0x62, 0xA2, 0x6D, 0x82, 0x8D, 0xCB]).unwrap();
        assert_eq!(direct_byte.ops.len(), 1);
        assert!(matches!(
            direct_byte.ops[0].kind,
            OpKind::X86PermuteBytesWords {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                table1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                table2: None,
                indices: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                zeroing: true,
                ..
            }
        ));

        let high = lift_single(&[0x62, 0x82, 0xC5, 0x46, 0x36, 0xF0]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VPermute {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(24))),
                indices: VReg::Arch(ArchReg::X86(X86Reg::Zmm(23))),
                elem: VecElementType::I64,
                width: VecWidth::V512,
                ..
            }
        )));

        let masked_memory = lift_single(&[0x62, 0xE2, 0x55, 0xC5, 0x36, 0x20]).unwrap();
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
            16
        );
        assert!(
            !masked_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. }))
        );

        let broadcast_controls = lift_single(&[0x62, 0xE2, 0x6D, 0xD3, 0x0C, 0x08]).unwrap();
        assert_eq!(
            broadcast_controls
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
            &[0xC4, 0xE2, 0x69, 0x36, 0xCB][..],
            &[0xC4, 0xE2, 0xE9, 0x0C, 0xCB][..],
            &[0xC4, 0xE3, 0x75, 0x04, 0xC2, 0x1B][..],
            &[0x62, 0xE2, 0x55, 0x05, 0x36, 0x20][..],
            &[0x62, 0xA2, 0x6D, 0xD3, 0x0C, 0xCB][..],
            &[0x62, 0xE2, 0x6D, 0x93, 0x8D, 0x08][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid permutation accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_packed_fp32_fp64_integer_conversions_cover_all_families_and_encodings() {
        for (bytes, int_to_fp, int_elem, fp_elem, signed, truncate, lanes) in [
            (
                &[0x62, 0xF1, 0x7C, 0xCA, 0x5B, 0xCB][..],
                true,
                VecElementType::I32,
                VecElementType::F32,
                true,
                false,
                16,
            ),
            (
                &[0x62, 0xA1, 0xFC, 0xBB, 0x5B, 0xCA][..],
                true,
                VecElementType::I64,
                VecElementType::F32,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7F, 0x4A, 0x7A, 0xCB][..],
                true,
                VecElementType::I32,
                VecElementType::F32,
                false,
                false,
                16,
            ),
            (
                &[0x62, 0xF1, 0xFF, 0x4A, 0x7A, 0xCB][..],
                true,
                VecElementType::I64,
                VecElementType::F32,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7E, 0x48, 0xE6, 0xCB][..],
                true,
                VecElementType::I32,
                VecElementType::F64,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFE, 0x48, 0xE6, 0xCB][..],
                true,
                VecElementType::I64,
                VecElementType::F64,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7E, 0x4A, 0x7A, 0xCB][..],
                true,
                VecElementType::I32,
                VecElementType::F64,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFE, 0x4A, 0x7A, 0xCB][..],
                true,
                VecElementType::I64,
                VecElementType::F64,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0xDA, 0x5B, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F32,
                true,
                false,
                16,
            ),
            (
                &[0x62, 0xF1, 0x7E, 0x1A, 0x5B, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F32,
                true,
                true,
                16,
            ),
            (
                &[0x62, 0xF1, 0xFF, 0xBA, 0xE6, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F64,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x1A, 0xE6, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F64,
                true,
                true,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4A, 0x7B, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F32,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4A, 0x7A, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F32,
                true,
                true,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4A, 0x7B, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F64,
                true,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4A, 0x7A, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F64,
                true,
                true,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7C, 0x4A, 0x79, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F32,
                false,
                false,
                16,
            ),
            (
                &[0x62, 0xF1, 0x7C, 0x4A, 0x78, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F32,
                false,
                true,
                16,
            ),
            (
                &[0x62, 0xF1, 0xFC, 0x4A, 0x79, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F64,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFC, 0x4A, 0x78, 0xCB][..],
                false,
                VecElementType::I32,
                VecElementType::F64,
                false,
                true,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4A, 0x79, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F32,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0x7D, 0x4A, 0x78, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F32,
                false,
                true,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4A, 0x79, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F64,
                false,
                false,
                8,
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x4A, 0x78, 0xCB][..],
                false,
                VecElementType::I64,
                VecElementType::F64,
                false,
                true,
                8,
            ),
        ] {
            let lifted =
                lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            assert_eq!(lifted.bytes_consumed, bytes.len());
            let kind = &lifted.ops.last().unwrap().kind;
            if int_to_fp {
                assert!(
                    matches!(
                        kind,
                        OpKind::X86PackedIntToFp {
                            int_elem: actual_int,
                            fp_elem: actual_fp,
                            signed: actual_signed,
                            lanes: actual_lanes,
                            ..
                        } if *actual_int == int_elem
                            && *actual_fp == fp_elem
                            && *actual_signed == signed
                            && *actual_lanes == lanes
                    ),
                    "{bytes:02X?}: {kind:?}"
                );
            } else {
                assert!(
                    matches!(
                        kind,
                        OpKind::X86PackedFpToInt {
                            int_elem: actual_int,
                            fp_elem: actual_fp,
                            signed: actual_signed,
                            truncate: actual_truncate,
                            lanes: actual_lanes,
                            ..
                        } if *actual_int == int_elem
                            && *actual_fp == fp_elem
                            && *actual_signed == signed
                            && *actual_truncate == truncate
                            && *actual_lanes == lanes
                    ),
                    "{bytes:02X?}: {kind:?}"
                );
            }
        }

        for (bytes, int_to_fp) in [
            (&[0x0F, 0x5B, 0xCA][..], true),
            (&[0x66, 0x0F, 0x5B, 0xCA][..], false),
            (&[0xF3, 0x0F, 0x5B, 0xCA][..], false),
            (&[0xF3, 0x0F, 0xE6, 0xCA][..], true),
            (&[0xF2, 0x0F, 0xE6, 0xCA][..], false),
            (&[0x66, 0x0F, 0xE6, 0xCA][..], false),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.last().unwrap().x86_hint.is_none());
            assert!(if int_to_fp {
                matches!(
                    lifted.ops.last().unwrap().kind,
                    OpKind::X86PackedIntToFp {
                        zero_upper: false,
                        ..
                    }
                )
            } else {
                matches!(
                    lifted.ops.last().unwrap().kind,
                    OpKind::X86PackedFpToInt {
                        zero_upper: false,
                        ..
                    }
                )
            });
        }
        for bytes in [
            &[0xC5, 0xFC, 0x5B, 0xCA][..],
            &[0xC5, 0xFD, 0x5B, 0xCA][..],
            &[0xC5, 0xFE, 0x5B, 0xCA][..],
            &[0xC5, 0xFE, 0xE6, 0xCA][..],
            &[0xC5, 0xFF, 0xE6, 0xCA][..],
            &[0xC5, 0xFD, 0xE6, 0xCA][..],
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(matches!(
                lifted.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp { .. })
            ));
        }

        let broadcast = lift_single(&[0x62, 0xF1, 0x7F, 0xDA, 0x7A, 0x08]).unwrap();
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
        assert!(matches!(
            broadcast.ops.last().unwrap().kind,
            OpKind::X86PackedIntToFp {
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F32,
                signed: false,
                lanes: 16,
                src_width: VecWidth::V512,
                dst_width: VecWidth::V512,
                mask_zeroing: true,
                ..
            }
        ));

        for invalid in [
            &[0x62, 0xF1, 0x6C, 0x48, 0x5B, 0xCB][..], // reserved vvvv
            &[0x62, 0xF1, 0x7C, 0xC8, 0x5B, 0xCB][..], // {z} with k0
            &[0x62, 0xF1, 0x7C, 0x68, 0x5B, 0xCB][..], // L'L=3 without ER
            &[0x62, 0xF1, 0x7E, 0x18, 0xE6, 0xCB][..], // exact DQ-to-PD has no ER
            &[0x62, 0xF1, 0xFD, 0x48, 0x5B, 0xCB][..], // invalid W for PS-to-DQ
            &[0xC4, 0xE1, 0x7D, 0x7B, 0xCB][..],       // EVEX-only family under VEX
        ] {
            assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
        }
    }
    #[test]
    fn lift_packed_variable_shifts_cover_encodings_elements_masks_memory_and_invalids() {
        for (bytes, elem, shift) in [
            (
                &[0x62, 0xF2, 0xED, 0x08, 0x10, 0xCB][..],
                VecElementType::I16,
                ShiftOp::Lsr,
            ),
            (
                &[0x62, 0xA2, 0xD5, 0xA3, 0x11, 0xE6][..],
                VecElementType::I16,
                ShiftOp::Asr,
            ),
            (
                &[0x62, 0xA2, 0xED, 0x47, 0x12, 0xCB][..],
                VecElementType::I16,
                ShiftOp::Lsl,
            ),
            (
                &[0xC4, 0xE2, 0x69, 0x45, 0xCB][..],
                VecElementType::I32,
                ShiftOp::Lsr,
            ),
            (
                &[0xC4, 0xE2, 0xED, 0x45, 0xCB][..],
                VecElementType::I64,
                ShiftOp::Lsr,
            ),
            (
                &[0x62, 0xE2, 0x6D, 0x57, 0x46, 0x48, 0x7F][..],
                VecElementType::I32,
                ShiftOp::Asr,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x48, 0x46, 0xCB][..],
                VecElementType::I64,
                ShiftOp::Asr,
            ),
            (
                &[0xC4, 0xE2, 0x6D, 0x47, 0xCB][..],
                VecElementType::I32,
                ShiftOp::Lsl,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x48, 0x47, 0xCB][..],
                VecElementType::I64,
                ShiftOp::Lsl,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86PackedShiftVariable { elem: actual_elem, shift: actual_shift, .. }
                    if actual_elem == elem && actual_shift == shift
            )));
        }
        let memory = lift_single(&[0x62, 0xE2, 0x6D, 0x57, 0x46, 0x48, 0x7F]).unwrap();
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
            &[0xC4, 0xE2, 0xED, 0x10, 0xCB][..],       // word form EVEX-only
            &[0x62, 0xF2, 0x6D, 0x08, 0x10, 0xCB][..], // word form W=1
            &[0x62, 0xF2, 0xED, 0x18, 0x10, 0xCB][..], // word broadcast reserved
            &[0x62, 0xF2, 0xED, 0x88, 0x10, 0xCB][..], // z with k0
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_variable_blends_cover_implicit_explicit_masks_is4_aliases_and_invalids() {
        for (opcode, elem) in [
            (0x10, VecElementType::I8),
            (0x14, VecElementType::I32),
            (0x15, VecElementType::I64),
        ] {
            let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xD1]).unwrap();
            assert!(legacy.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VCmp {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: actual,
                    cond: VecCmpCond::Lt,
                    ..
                } if actual == elem
            )));
            assert!(legacy.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBitSelect {
                    src_true: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src_false: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    width: VecWidth::V128,
                    ..
                }
            )));
            assert!(legacy.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    elem: actual,
                    ..
                } if actual == elem
            )));
            assert!(
                legacy
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x10, 0x10]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        for (bytes, elem, width, dst, src1, src2, mask) in [
            (
                &[0xC4, 0xE3, 0x61, 0x4C, 0xCA, 0x40][..],
                VecElementType::I8,
                VecWidth::V128,
                X86Reg::Xmm(1),
                X86Reg::Xmm(3),
                X86Reg::Xmm(2),
                X86Reg::Xmm(4),
            ),
            (
                &[0xC4, 0x43, 0x0D, 0x4C, 0xE5, 0xF0][..],
                VecElementType::I8,
                VecWidth::V256,
                X86Reg::Ymm(12),
                X86Reg::Ymm(14),
                X86Reg::Ymm(13),
                X86Reg::Ymm(15),
            ),
            (
                &[0xC4, 0x43, 0x31, 0x4B, 0xDA, 0x80][..],
                VecElementType::I64,
                VecWidth::V128,
                X86Reg::Xmm(11),
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
                X86Reg::Xmm(8),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VCmp {
                    src1: VReg::Arch(ArchReg::X86(actual_mask)),
                    elem: actual_elem,
                    cond: VecCmpCond::Lt,
                    ..
                } if actual_mask == mask && actual_elem == elem
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBitSelect {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src_true: VReg::Arch(ArchReg::X86(actual_src2)),
                    src_false: VReg::Arch(ArchReg::X86(actual_src1)),
                    width: actual_width,
                    ..
                } if actual_dst == dst
                    && actual_src1 == src1
                    && actual_src2 == src2
                    && actual_width == width
            )));
        }

        let memory = lift_single(&[0xC4, 0xE3, 0x65, 0x4A, 0x50, 0x20, 0x40]).unwrap();
        assert!(memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        assert!(memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(4))),
                elem: VecElementType::I32,
                ..
            }
        )));

        // imm8[3:0] is ignored; both encodings name mask register 4.
        let low_nibble = lift_single(&[0xC4, 0xE3, 0x61, 0x4C, 0xCA, 0x4F]).unwrap();
        assert!(low_nibble.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(4))),
                ..
            }
        )));

        for bytes in [
            &[0x0F, 0x38, 0x10, 0xD1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x14, 0xD1][..],
            &[0xF3, 0x66, 0x0F, 0x38, 0x15, 0xD1][..],
            &[0xC4, 0xE3, 0xE1, 0x4C, 0xCA, 0x40][..],
            &[0xC4, 0xE3, 0x60, 0x4A, 0xCA, 0x40][..],
            &[0xC4, 0xE3, 0x61, 0x4C, 0xCA][..],
            &[0x62, 0xF3, 0x65, 0x08, 0x4A, 0xCA, 0x40][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid variable-blend encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmuldq_covers_even_lanes_signed_products_masks_and_broadcasts() {
        for (bytes, products) in [
            (&[0x66, 0x0F, 0x38, 0x28, 0xC1][..], 2usize),
            (&[0xC4, 0xE2, 0x75, 0x28, 0xC2][..], 4usize),
            (&[0x62, 0xA2, 0xF5, 0x40, 0x28, 0xC2][..], 8usize),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::MulS {
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                            ..
                        }
                    ))
                    .count(),
                products
            );
            assert!(
                result
                    .ops
                    .iter()
                    .filter_map(|op| match op.kind {
                        OpKind::VExtractLane {
                            lane,
                            elem: VecElementType::I32,
                            sign: SignExtend::Sign,
                            ..
                        } => Some(lane),
                        _ => None,
                    })
                    .all(|lane| lane & 1 == 0)
            );
        }
        let high = lift_single(&[0x62, 0xA2, 0xF5, 0x40, 0x28, 0xC2]).unwrap();
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                ..
            }
        )));
        assert!(high.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                ..
            }
        )));

        let broadcast = lift_single(&[0x62, 0xF2, 0xF5, 0x59, 0x28, 0x40, 0x01]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset { offset: 8, .. },
                ..
            }
        )));
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

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x28, 0x00]).unwrap();
        assert!(
            legacy_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        assert!(lift_single(&[0xC4, 0xE2, 0xF5, 0x28, 0xC2]).is_ok());
        for bytes in [
            &[0x0F, 0x38, 0x28, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x28, 0xC1][..],
            &[0x62, 0xF2, 0x75, 0x48, 0x28, 0xC2][..],
            &[0x62, 0xF2, 0xF5, 0x58, 0x28, 0xC2][..],
            &[0x62, 0xF2, 0xF5, 0xC8, 0x28, 0xC2][..],
            &[0x62, 0xF2, 0xF5, 0x68, 0x28, 0xC2][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_pmuludq_covers_unsigned_products_widths_masks_and_broadcasts() {
        for (bytes, products) in [
            (&[0x66, 0x0F, 0xF4, 0xD1][..], 2usize),
            (&[0xC5, 0xF1, 0xF4, 0xC2][..], 2usize),
            (&[0xC4, 0x41, 0x35, 0xF4, 0xC2][..], 4usize),
            (&[0x62, 0xA1, 0xF5, 0x40, 0xF4, 0xC2][..], 8usize),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::MulU {
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                            ..
                        }
                    ))
                    .count(),
                products,
            );
            assert!(
                result
                    .ops
                    .iter()
                    .filter_map(|op| match op.kind {
                        OpKind::VExtractLane {
                            lane,
                            elem: VecElementType::I32,
                            sign: SignExtend::Zero,
                            ..
                        } => Some(lane),
                        _ => None,
                    })
                    .all(|lane| lane & 1 == 0)
            );
        }

        let broadcast = lift_single(&[0x62, 0xF1, 0xF5, 0x59, 0xF4, 0x40, 0x01]).unwrap();
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
            8,
        );
        let legacy_memory = lift_single(&[0x66, 0x0F, 0xF4, 0x00]).unwrap();
        assert!(
            legacy_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        let mmx = lift_single(&[0x0F, 0xF4, 0xC1]).unwrap();
        assert_eq!(mmx.bytes_consumed, 3);
        assert_eq!(
            mmx.ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::MulU {
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(mmx.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                width: VecWidth::V64,
                ..
            }
        )));
        assert!(matches!(
            mmx.ops.last(),
            Some(SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            })
        ));
        let mmx_memory = lift_single(&[0x0F, 0xF4, 0x40, 0x01]).unwrap();
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
            &[0xF3, 0x66, 0x0F, 0xF4, 0xC1][..],
            &[0xC5, 0xF0, 0xF4, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x40, 0xF4, 0xC2][..],
            &[0x62, 0xA1, 0xF5, 0x50, 0xF4, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PMULUDQ accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmulld_pmulq_covers_widths_high_regs_masks_broadcasts_and_invalids() {
        for (bytes, elem, lanes, dst, src1, src2) in [
            (
                &[0x66, 0x0F, 0x38, 0x40, 0xD1][..],
                VecElementType::I32,
                4u8,
                X86Reg::Xmm(2),
                X86Reg::Xmm(2),
                X86Reg::Xmm(1),
            ),
            (
                &[0xC4, 0x42, 0x35, 0x40, 0xC2][..],
                VecElementType::I32,
                8,
                X86Reg::Ymm(8),
                X86Reg::Ymm(9),
                X86Reg::Ymm(10),
            ),
            (
                &[0x62, 0xA2, 0x75, 0x40, 0x40, 0xC2][..],
                VecElementType::I32,
                16,
                X86Reg::Zmm(16),
                X86Reg::Zmm(17),
                X86Reg::Zmm(18),
            ),
            (
                &[0x62, 0xA2, 0xDD, 0x40, 0x40, 0xDD][..],
                VecElementType::I64,
                8,
                X86Reg::Zmm(19),
                X86Reg::Zmm(20),
                X86Reg::Zmm(21),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMul {
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    elem: actual_elem,
                    lanes: actual_lanes,
                    ..
                } if actual_src1 == src1 && actual_src2 == src2
                    && actual_elem == elem && actual_lanes == lanes
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

        for (bytes, width, elem, lanes, offset) in [
            (
                &[0x62, 0xF2, 0x75, 0xD9, 0x40, 0x40, 0x01][..],
                MemWidth::B4,
                VecElementType::I32,
                16usize,
                4i64,
            ),
            (
                &[0x62, 0xF2, 0xED, 0x5A, 0x40, 0x58, 0x01][..],
                MemWidth::B8,
                VecElementType::I64,
                8,
                8,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::PredLoad {
                            width: actual_width,
                            ..
                        } if actual_width == width
                    ))
                    .count(),
                lanes,
            );
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseOffset {
                        offset: actual,
                        ..
                    },
                    ..
                } if actual == offset
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMul {
                    elem: actual_elem,
                    ..
                } if actual_elem == elem
            )));
        }

        for bytes in [
            &[0x0F, 0x38, 0x40, 0xC1][..],
            &[0xF3, 0x66, 0x0F, 0x38, 0x40, 0xC1][..],
            &[0xC4, 0x42, 0x34, 0x40, 0xC2][..],
            &[0x62, 0xA2, 0x75, 0xC0, 0x40, 0xC2][..],
            &[0x62, 0xA2, 0x75, 0x50, 0x40, 0xC2][..],
            &[0x62, 0xA2, 0x75, 0x60, 0x40, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PMULLD/Q accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmulhw_pmulhuw_covers_signedness_widths_masks_alignment_and_invalids() {
        for (bytes, signed, lanes, dst, src1, src2, hint) in [
            (
                &[0x66, 0x0F, 0xE5, 0xD1][..],
                true,
                8u8,
                X86Reg::Xmm(2),
                X86Reg::Xmm(2),
                X86Reg::Xmm(1),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                },
            ),
            (
                &[0x66, 0x0F, 0xE4, 0xE3][..],
                false,
                8,
                X86Reg::Xmm(4),
                X86Reg::Xmm(4),
                X86Reg::Xmm(3),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                },
            ),
            (
                &[0xC4, 0x41, 0x35, 0xE5, 0xC2][..],
                true,
                16,
                X86Reg::Ymm(8),
                X86Reg::Ymm(9),
                X86Reg::Ymm(10),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            (
                &[0x62, 0xA1, 0x75, 0x40, 0xE4, 0xC2][..],
                false,
                32,
                X86Reg::Zmm(16),
                X86Reg::Zmm(17),
                X86Reg::Zmm(18),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        (&op.kind, op.x86_hint),
                        (
                            OpKind::VMulShiftSat {
                                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                                src1: VReg::Arch(ArchReg::X86(actual_src1)),
                                src2: VReg::Arch(ArchReg::X86(actual_src2)),
                                src_elem: VecElementType::I16,
                                lanes: actual_lanes,
                                signed1: actual_signed1,
                                signed2: actual_signed2,
                                shift_left: 0,
                                round: false,
                                sat_bits: 0,
                                out_shift: 16,
                            },
                            Some(actual_hint),
                        ) if *actual_dst == dst
                            && *actual_src1 == src1
                            && *actual_src2 == src2
                            && *actual_lanes == lanes
                            && *actual_signed1 == signed
                            && *actual_signed2 == signed
                            && actual_hint == hint
                    ))
                    .count(),
                1,
            );
            assert!(!result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::MulS { .. } | OpKind::MulU { .. } | OpKind::Shr { .. }
            )));
        }

        for opcode in [0xE4, 0xE5] {
            let legacy_memory = lift_single(&[0x66, 0x0F, opcode, 0x00]).unwrap();
            assert!(
                legacy_memory
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            );
        }
        let masked_memory = lift_single(&[0x62, 0xF1, 0x75, 0xC9, 0xE5, 0x40, 0x01]).unwrap();
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
            (&op.kind, op.x86_hint),
            (
                OpKind::VMulShiftSat {
                    signed1: true,
                    signed2: true,
                    round: false,
                    out_shift: 16,
                    ..
                },
                None
            )
        )));
        assert!(lift_single(&[0xC4, 0xE1, 0xF1, 0xE5, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF1, 0xF5, 0x08, 0xE4, 0xC2]).is_ok());
        for (opcode, signed) in [(0xE4, false), (0xE5, true)] {
            let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
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
                            signed1,
                            signed2,
                            shift_left: 0,
                            round: false,
                            sat_bits: 0,
                            out_shift: 16,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
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
                ] if *signed1 == signed && *signed2 == signed && *actual_opcode == opcode
            ));
            let memory = lift_single(&[0x0F, opcode, 0x40, 0x01]).unwrap();
            assert!(memory.ops.iter().any(|op| matches!(
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
                !memory
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
            );
        }
        for bytes in [
            &[0xF3, 0x66, 0x0F, 0xE4, 0xC1][..],
            &[0xC5, 0xF0, 0xE5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0xC0, 0xE4, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x50, 0xE5, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0x59, 0xE4, 0x00][..],
            &[0x62, 0xA1, 0x74, 0x40, 0xE5, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PMULH[U]W accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pavgb_pavgw_covers_rounding_widths_masks_alignment_and_invalids() {
        for (bytes, elem, lanes, dst, src1, src2, hint) in [
            (
                &[0x66, 0x0F, 0xE0, 0xD1][..],
                VecElementType::I8,
                16,
                X86Reg::Xmm(2),
                X86Reg::Xmm(2),
                X86Reg::Xmm(1),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                },
            ),
            (
                &[0x66, 0x0F, 0xE3, 0xE3][..],
                VecElementType::I16,
                8,
                X86Reg::Xmm(4),
                X86Reg::Xmm(4),
                X86Reg::Xmm(3),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                },
            ),
            (
                &[0xC4, 0x41, 0x35, 0xE0, 0xC2][..],
                VecElementType::I8,
                32,
                X86Reg::Ymm(8),
                X86Reg::Ymm(9),
                X86Reg::Ymm(10),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE0,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            (
                &[0x62, 0xA1, 0x75, 0x40, 0xE3, 0xC2][..],
                VecElementType::I16,
                32,
                X86Reg::Zmm(16),
                X86Reg::Zmm(17),
                X86Reg::Zmm(18),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE3,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        src1: VReg::Arch(ArchReg::X86(actual_src1)),
                        src2: VReg::Arch(ArchReg::X86(actual_src2)),
                        elem: actual_elem,
                        lanes: actual_lanes,
                        op: VLaneOp::AvgRnd,
                        signed: false,
                        set_ovf: false,
                    },
                    x86_hint: Some(actual_hint),
                    ..
                }] if *actual_dst == dst && *actual_src1 == src1 && *actual_src2 == src2
                    && *actual_elem == elem && usize::from(*actual_lanes) == lanes
                    && *actual_hint == hint
            ));
        }

        for opcode in [0xE0, 0xE3] {
            let legacy_memory = lift_single(&[0x66, 0x0F, opcode, 0x00]).unwrap();
            assert!(
                legacy_memory
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            );
            assert!(legacy_memory.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VLane {
                    op: VLaneOp::AvgRnd,
                    signed: false,
                    set_ovf: false,
                    ..
                }
            )));
        }
        let masked_memory = lift_single(&[0x62, 0xF1, 0x75, 0xC9, 0xE0, 0x40, 0x01]).unwrap();
        assert_eq!(
            masked_memory
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
        for (opcode, elem, lanes) in [
            (0xE0, VecElementType::I8, 8),
            (0xE3, VecElementType::I16, 4),
        ] {
            let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                mmx.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::VLane {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            elem: actual_elem,
                            lanes: actual_lanes,
                            op: VLaneOp::AvgRnd,
                            signed: false,
                            set_ovf: false,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
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
                ] if *actual_elem == elem
                    && *actual_lanes == lanes
                    && *actual_opcode == opcode
            ));
            let memory = lift_single(&[0x0F, opcode, 0x40, 0x01]).unwrap();
            assert!(memory.ops.iter().any(|op| matches!(
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
                !memory
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
            );
        }
        // VEX.W and EVEX.W are ignored for VPAVG[BW].
        assert!(lift_single(&[0xC4, 0xE1, 0xF1, 0xE0, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF1, 0xF5, 0x48, 0xE3, 0xC2]).is_ok());
        for bytes in [
            &[0xF3, 0x66, 0x0F, 0xE0, 0xC1][..],
            &[0xC5, 0xF0, 0xE3, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0xC0, 0xE0, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x50, 0xE3, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0x59, 0xE0, 0x00][..],
            &[0x62, 0xA1, 0x74, 0x40, 0xE3, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PAVG[BW] accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pmaddwd_covers_wrap_widths_masks_complete_memory_and_invalids() {
        for (bytes, dst, src1, src2, hint) in [
            (
                &[0x66, 0x0F, 0xF5, 0xD1][..],
                X86Reg::Xmm(2),
                X86Reg::Xmm(2),
                X86Reg::Xmm(1),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                },
            ),
            (
                &[0xC4, 0x41, 0x35, 0xF5, 0xC2][..],
                X86Reg::Ymm(8),
                X86Reg::Ymm(9),
                X86Reg::Ymm(10),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            (
                &[0x62, 0xA1, 0x75, 0x40, 0xF5, 0xC2][..],
                X86Reg::Zmm(16),
                X86Reg::Zmm(17),
                X86Reg::Zmm(18),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                result.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VDotProduct {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        acc: VReg::Imm(0),
                        src1: VReg::Arch(ArchReg::X86(actual_src1)),
                        src2: VReg::Arch(ArchReg::X86(actual_src2)),
                        mask: None,
                        src_elem: VecElementType::I16,
                        acc_elem: VecElementType::I32,
                        src1_unsigned: false,
                        saturate: false,
                        zeroing: false,
                        ..
                    },
                    x86_hint: Some(actual_hint),
                    ..
                }] if *actual_dst == dst && *actual_src1 == src1 && *actual_src2 == src2
                    && *actual_hint == hint
            ));
        }

        let legacy_memory = lift_single(&[0x66, 0x0F, 0xF5, 0x00]).unwrap();
        assert!(
            legacy_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );

        // E4NF: destination masking does not suppress the full memory read.
        let masked_memory = lift_single(&[0x62, 0xF1, 0x75, 0xC9, 0xF5, 0x40, 0x01]).unwrap();
        assert!(masked_memory.ops.iter().any(|op| matches!(
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
            !masked_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        // W is ignored in VEX and EVEX encodings.
        assert!(lift_single(&[0xC4, 0x41, 0xB5, 0xF5, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xA1, 0xF5, 0x40, 0xF5, 0xC2]).is_ok());
        let mmx = lift_single(&[0x0F, 0xF5, 0xC1]).unwrap();
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
                        src_elem: VecElementType::I16,
                        acc_elem: VecElementType::I32,
                        width: VecWidth::V64,
                        src1_unsigned: false,
                        saturate: false,
                        zeroing: false,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xF5,
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
        let mmx_memory = lift_single(&[0x0F, 0xF5, 0x40, 0x01]).unwrap();
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
            &[0xF3, 0x66, 0x0F, 0xF5, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0xF5, 0xC1][..],
            &[0xC5, 0xF0, 0xF5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0xC0, 0xF5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x50, 0xF5, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0x59, 0xF5, 0x00][..],
            &[0x62, 0xA1, 0x74, 0x40, 0xF5, 0xC2][..],
            &[0x62, 0xA1, 0x75, 0x60, 0xF5, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "invalid PMADDWD accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_packed_shift_count_covers_operations_widths_masks_mem128_and_invalids() {
        for (opcode, elem, shift) in [
            (0xD1, VecElementType::I16, ShiftOp::Lsr),
            (0xD2, VecElementType::I32, ShiftOp::Lsr),
            (0xD3, VecElementType::I64, ShiftOp::Lsr),
            (0xE1, VecElementType::I16, ShiftOp::Asr),
            (0xE2, VecElementType::I32, ShiftOp::Asr),
            (0xF1, VecElementType::I16, ShiftOp::Lsl),
            (0xF2, VecElementType::I32, ShiftOp::Lsl),
            (0xF3, VecElementType::I64, ShiftOp::Lsl),
        ] {
            let result = lift_single(&[0xC5, 0xF1, opcode, 0xC2]).unwrap();
            assert_eq!(result.bytes_consumed, 4);
            assert!(!result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                    ..
                }
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86PackedShift {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    width: VecWidth::V128,
                    elem: actual_elem,
                    shift: actual_shift,
                } if actual_elem == elem && actual_shift == shift
            )));
        }

        let legacy = lift_single(&[0x66, 0x0F, 0xD1, 0xD1]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShift {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                width: VecWidth::V128,
                elem: VecElementType::I16,
                shift: ShiftOp::Lsr,
                ..
            }
        )));
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                elem: VecElementType::I16,
                ..
            }
        )));

        let vex256 = lift_single(&[0xC4, 0x41, 0x35, 0xD2, 0xC2]).unwrap();
        assert!(vex256.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShift {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                width: VecWidth::V256,
                elem: VecElementType::I32,
                ..
            }
        )));
        assert!(
            !vex256
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        );

        for (p1, elem) in [(0x75, VecElementType::I32), (0xF5, VecElementType::I64)] {
            let result = lift_single(&[0x62, 0xA1, p1, 0xC1, 0xE2, 0xC2]).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86PackedShift {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    width: VecWidth::V512,
                    elem: actual_elem,
                    shift: ShiftOp::Asr,
                    ..
                } if actual_elem == elem
            )));
            assert!(!result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                    ..
                }
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                    elem: actual_elem,
                    ..
                } if actual_elem == elem
            )));
        }

        let legacy_memory = lift_single(&[0x66, 0x0F, 0xF1, 0x00]).unwrap();
        assert!(
            legacy_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        );
        assert!(legacy_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(legacy_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                lane: 0,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
                ..
            }
        )));

        let vex_memory = lift_single(&[0xC5, 0xF5, 0xD2, 0x00]).unwrap();
        assert!(vex_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(vex_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                lane: 0,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
                ..
            }
        )));
        assert!(
            !vex_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // Mem128 has a compressed-displacement scale of 16 and is E4NF: the
        // destination mask does not predicate the count load.
        let evex_memory = lift_single(&[0x62, 0xF1, 0xF5, 0x49, 0xF3, 0x40, 0x04]).unwrap();
        assert!(evex_memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(
            !evex_memory
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );

        // W is ignored by VEX and word EVEX forms.
        assert!(lift_single(&[0xC4, 0xE1, 0xF1, 0xD2, 0xC2]).is_ok());
        assert!(lift_single(&[0x62, 0xF1, 0xF5, 0x08, 0xD1, 0xC2]).is_ok());
        for (opcode, elem, shift) in [
            (0xD1, VecElementType::I16, ShiftOp::Lsr),
            (0xD2, VecElementType::I32, ShiftOp::Lsr),
            (0xD3, VecElementType::I64, ShiftOp::Lsr),
            (0xE1, VecElementType::I16, ShiftOp::Asr),
            (0xE2, VecElementType::I32, ShiftOp::Asr),
            (0xF1, VecElementType::I16, ShiftOp::Lsl),
            (0xF2, VecElementType::I32, ShiftOp::Lsl),
            (0xF3, VecElementType::I64, ShiftOp::Lsl),
        ] {
            let result = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
            assert!(result.ops.iter().any(|op| matches!(
                (&op.kind, op.x86_hint),
                (
                    OpKind::X86PackedShift {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        count: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                        elem: actual_elem,
                        shift: actual_shift,
                    },
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    })
                ) if *actual_elem == elem && *actual_shift == shift && actual_opcode == opcode
            )));
            assert!(matches!(
                result.ops.last(),
                Some(SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                })
            ));
        }

        let mmx_memory = lift_single(&[0x0F, 0xF1, 0x40, 0x01]).unwrap();
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
            &[0xF3, 0x66, 0x0F, 0xD1, 0xC1][..],
            &[0xF0, 0x66, 0x0F, 0xF3, 0xC1][..],
            &[0xC5, 0xF0, 0xD1, 0xC2][..],
            &[0xC5, 0xF1, 0xD1][..],
            &[0x62, 0xF1, 0xFD, 0x48, 0xD2, 0xC2][..],
            &[0x62, 0xF1, 0x7D, 0x48, 0xD3, 0xC2][..],
            &[0x62, 0xF1, 0xFD, 0x48, 0xF2, 0xC2][..],
            &[0x62, 0xF1, 0x7D, 0x48, 0xF3, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0xC0, 0xE1, 0xC2][..],
            &[0x62, 0xF1, 0x75, 0x58, 0xE1, 0x00][..],
            &[0x62, 0xF1, 0x75, 0x68, 0xE1, 0xC2][..],
            &[0x62, 0xF1, 0x74, 0x48, 0xE1, 0xC2][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid packed shift-by-count encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_movntdqa_covers_width_alignment_memory_only_and_invalids() {
        for (bytes, width, alignment, dst) in [
            (
                &[0x66, 0x0F, 0x38, 0x2A, 0x00][..],
                VecWidth::V128,
                16u8,
                X86Reg::Xmm(0),
            ),
            (
                &[0xC4, 0xE2, 0x7D, 0x2A, 0x00][..],
                VecWidth::V256,
                32,
                X86Reg::Ymm(0),
            ),
            (
                &[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x40, 0x01][..],
                VecWidth::V512,
                64,
                X86Reg::Zmm(16),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert!(result.ops.iter().any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: actual, .. } if actual == alignment)));
            assert!(result.ops.iter().any(
                |op| matches!(op.kind, OpKind::VLoad { width: actual, .. } if actual == width)
            ));
            assert!(result.ops.iter().any(|op| matches!(op.kind, OpKind::VMov { dst: VReg::Arch(ArchReg::X86(actual)), width: actual_width, .. } if actual == dst && actual_width == width)) || width == VecWidth::V128);
        }
        let evex = lift_single(&[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x40, 0x01]).unwrap();
        assert!(evex.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86CheckAlignment {
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            } | OpKind::VLoad {
                addr: Address::BaseOffset { offset: 64, .. },
                ..
            }
        )));
        for bytes in [
            &[0x0F, 0x38, 0x2A, 0x00][..],
            &[0x66, 0x0F, 0x38, 0x2A, 0xC0][..],
            &[0xC4, 0xE2, 0x75, 0x2A, 0x00][..],
            &[0xC4, 0xE2, 0x7D, 0x2A, 0xC0][..],
            &[0x62, 0xE2, 0xFD, 0x48, 0x2A, 0x00][..],
            &[0x62, 0xE2, 0x7D, 0x49, 0x2A, 0x00][..],
            &[0x62, 0xE2, 0x7D, 0x58, 0x2A, 0x00][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ));
        }
    }
    #[test]
    fn lift_phminposuw_covers_first_unsigned_minimum_alignment_and_invalids() {
        let legacy = lift_single(&[0x66, 0x0F, 0x38, 0x41, 0xCA]).unwrap();
        assert!(matches!(
            legacy.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Phminposuw {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x41,
                }),
                ..
            }]
        ));

        let legacy_mem = lift_single(&[0x66, 0x44, 0x0F, 0x38, 0x41, 0x48, 0x20]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);
        assert!(
            !legacy_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86Phminposuw { .. }))
        );
        assert_eq!(
            legacy_mem
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::SetCC {
                        cond: Condition::Ult,
                        ..
                    }
                ))
                .count(),
            7
        );
        assert!(legacy_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                ..
            }
        )));

        let vex_high = lift_single(&[0xC4, 0x42, 0x79, 0x41, 0xCA]).unwrap();
        assert!(matches!(
            vex_high.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Phminposuw {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                },
                x86_hint: Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x41,
                    width: VecWidth::V128,
                    w: false,
                }),
                ..
            }]
        ));

        let vex_mem = lift_single(&[0xC4, 0x62, 0x79, 0x41, 0x48, 0x20]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86Phminposuw { .. }))
        );

        // Both legacy REX.W and VEX.W are ignored.
        assert!(lift_single(&[0x66, 0x48, 0x0F, 0x38, 0x41, 0xCA]).is_ok());
        let vex_w1 = lift_single(&[0xC4, 0x42, 0xF9, 0x41, 0xCA]).unwrap();
        assert!(matches!(
            vex_w1.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Phminposuw { .. },
                x86_hint: Some(X86OpHint::VexOp { w: true, .. }),
                ..
            }]
        ));
        for bytes in [
            &[0x0F, 0x38, 0x41, 0xCA][..],
            &[0xF0, 0x66, 0x0F, 0x38, 0x41, 0xCA][..],
            &[0xF3, 0x66, 0x0F, 0x38, 0x41, 0xCA][..],
            &[0xC4, 0xE2, 0x71, 0x41, 0xCA][..],
            &[0xC4, 0xE2, 0x7D, 0x41, 0xCA][..],
            &[0x62, 0xF2, 0x7D, 0x08, 0x41, 0xCA][..],
            &[0xC4, 0xE2, 0x79, 0x41][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PHMINPOSUW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_pclmulqdq_covers_selectors_blocks_full_mem_and_invalids() {
        for (bytes, width, products, src1, src2, src1_lanes, src2_lanes, dst) in [
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x44, 0xCA, 0x11][..],
                VecWidth::V128,
                1usize,
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
                vec![1u8],
                vec![1u8],
                X86Reg::Xmm(9),
            ),
            (
                &[0xC4, 0x43, 0x25, 0x44, 0xCA, 0x01][..],
                VecWidth::V256,
                2,
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
                vec![1, 3],
                vec![0, 2],
                X86Reg::Ymm(9),
            ),
            (
                &[0x62, 0xA3, 0x6D, 0x40, 0x44, 0xC8, 0x10][..],
                VecWidth::V512,
                4,
                X86Reg::Zmm(18),
                X86Reg::Zmm(16),
                vec![0, 2, 4, 6],
                vec![1, 3, 5, 7],
                X86Reg::Zmm(17),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::ClMul {
                            elem_bits: 64,
                            lanes: 1,
                            acc: false,
                            dst_hi: Some(_),
                            ..
                        }
                    ))
                    .count(),
                products
            );
            let lanes_for = |reg| {
                result
                    .ops
                    .iter()
                    .filter_map(|op| match op.kind {
                        OpKind::VExtractLane {
                            vec: VReg::Arch(ArchReg::X86(actual)),
                            lane,
                            elem: VecElementType::I64,
                            sign: SignExtend::Zero,
                            ..
                        } if actual == reg => Some(lane),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(lanes_for(src1), src1_lanes);
            assert_eq!(lanes_for(src2), src2_lanes);
            assert!(
                result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: actual_width,
                        ..
                    } if actual == dst && actual_width == width
                )) || width == VecWidth::V128
            );
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x0F, 0x3A, 0x44, 0x48, 0x10, 0x01]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let evex_mem = lift_single(&[0x62, 0xE3, 0x6D, 0x40, 0x44, 0x48, 0x01, 0x11]).unwrap();
        assert!(evex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 64, .. },
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(
            !evex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // W is ignored, and immediate bits other than 4 and 0 are ignored.
        assert!(lift_single(&[0xC4, 0x43, 0xA5, 0x44, 0xCA, 0xEF]).is_ok());
        assert!(lift_single(&[0x62, 0xA3, 0xED, 0x40, 0x44, 0xC8, 0xEE]).is_ok());
        for bytes in [
            &[0x0F, 0x3A, 0x44, 0xCA, 0x11][..],
            &[0xF0, 0x66, 0x0F, 0x3A, 0x44, 0xCA, 0x11][..],
            &[0xF3, 0x66, 0x0F, 0x3A, 0x44, 0xCA, 0x11][..],
            &[0x66, 0x0F, 0x3A, 0x44, 0xCA][..],
            &[0xC4, 0x43, 0x24, 0x44, 0xCA, 0x11][..],
            &[0x62, 0xA3, 0x6D, 0x60, 0x44, 0xC8, 0x11][..],
            &[0x62, 0xA3, 0x6D, 0x41, 0x44, 0xC8, 0x11][..],
            &[0x62, 0xA3, 0x6D, 0xC0, 0x44, 0xC8, 0x11][..],
            &[0x62, 0xA3, 0x6D, 0x50, 0x44, 0xC8, 0x11][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PCLMULQDQ encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_immediate_blends_cover_masks_repetition_alignment_aliases_and_invalids() {
        let lanes_from = |result: &LiftResult, reg: X86Reg, elem: VecElementType| {
            result
                .ops
                .iter()
                .filter_map(|op| match op.kind {
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(actual)),
                        lane,
                        elem: actual_elem,
                        ..
                    } if actual == reg && actual_elem == elem => Some(lane),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        for (bytes, elem, src1, src2, from1, from2, dst, width) in [
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x0C, 0xCA, 0x5A][..],
                VecElementType::I32,
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
                vec![0, 2],
                vec![1, 3],
                X86Reg::Xmm(9),
                VecWidth::V128,
            ),
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x0E, 0xCA, 0xA5][..],
                VecElementType::I16,
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
                vec![1, 3, 4, 6],
                vec![0, 2, 5, 7],
                X86Reg::Xmm(9),
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x43, 0x25, 0x0C, 0xCA, 0xA5][..],
                VecElementType::I32,
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
                vec![1, 3, 4, 6],
                vec![0, 2, 5, 7],
                X86Reg::Ymm(9),
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x43, 0x25, 0x0D, 0xCA, 0x05][..],
                VecElementType::I64,
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
                vec![1, 3],
                vec![0, 2],
                X86Reg::Ymm(9),
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x43, 0x25, 0x0E, 0xCA, 0xA5][..],
                VecElementType::I16,
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
                vec![1, 3, 4, 6, 9, 11, 12, 14],
                vec![0, 2, 5, 7, 8, 10, 13, 15],
                X86Reg::Ymm(9),
                VecWidth::V256,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(lanes_from(&result, src1, elem), from1);
            assert_eq!(lanes_from(&result, src2, elem), from2);
            assert!(
                result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(actual)),
                        width: actual_width,
                        ..
                    } if actual == dst && actual_width == width
                )) || width == VecWidth::V128
            );
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x44, 0x0F, 0x3A, 0x0D, 0x48, 0x20, 0x02]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let vex_mem = lift_single(&[0xC4, 0xE3, 0x65, 0x0C, 0x48, 0x20, 0xA5]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // VEX.W is ignored for all three forms.
        assert!(lift_single(&[0xC4, 0x43, 0xA5, 0x0C, 0xCA, 0xA5]).is_ok());
        assert!(lift_single(&[0xC4, 0x43, 0xA5, 0x0D, 0xCA, 0x05]).is_ok());
        assert!(lift_single(&[0xC4, 0x43, 0xA5, 0x0E, 0xCA, 0xA5]).is_ok());
        for bytes in [
            &[0x0F, 0x3A, 0x0C, 0xCA, 0x5A][..],
            &[0xF0, 0x66, 0x0F, 0x3A, 0x0D, 0xCA, 0x02][..],
            &[0xF3, 0x66, 0x0F, 0x3A, 0x0E, 0xCA, 0xA5][..],
            &[0x66, 0x0F, 0x3A, 0x0C, 0xCA][..],
            &[0xC4, 0x43, 0x24, 0x0C, 0xCA, 0xA5][..],
            &[0x62, 0xF3, 0x65, 0x28, 0x0D, 0xCA, 0x05][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid immediate-blend encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_mpsadbw_covers_block_selectors_widths_alignment_aliases_and_invalids() {
        for (bytes, width, dst, src1, src2, imm) in [
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x42, 0xCA, 0xE7][..],
                VecWidth::V128,
                X86Reg::Xmm(9),
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
                0xE7,
            ),
            (
                &[0xC4, 0x43, 0x21, 0x42, 0xCA, 0xE7][..],
                VecWidth::V128,
                X86Reg::Xmm(9),
                X86Reg::Xmm(11),
                X86Reg::Xmm(10),
                0xE7,
            ),
            (
                &[0xC4, 0x43, 0x25, 0x42, 0xCA, 0xE7][..],
                VecWidth::V256,
                X86Reg::Ymm(9),
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
                0xE7,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            let legacy = bytes[0] == 0x66;
            let op = result
                .ops
                .iter()
                .find(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
                .unwrap();
            assert!(matches!(
                op.kind,
                OpKind::VMpsadbw {
                    dst: actual_dst,
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    mask: None,
                    width: actual_width,
                    imm: actual_imm,
                    zeroing: false,
                } if actual_dst == VReg::Arch(ArchReg::X86(dst))
                    && actual_src1 == src1
                    && actual_src2 == src2
                    && actual_width == width
                    && actual_imm == imm
            ));
            if legacy {
                assert_eq!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: 0x42,
                    })
                );
            } else {
                assert_eq!(
                    op.x86_hint,
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x42,
                        width,
                        w: bytes[2] & 0x80 != 0,
                    })
                );
            }
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x44, 0x0F, 0x3A, 0x42, 0x48, 0x10, 0x07]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);
        assert!(
            legacy_mem
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
                .all(|op| op.x86_hint.is_none())
        );

        let vex_mem = lift_single(&[0xC4, 0x63, 0x25, 0x42, 0x48, 0x11, 0x38]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
        assert!(
            vex_mem
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
                .all(|op| op.x86_hint.is_none())
        );

        // REX.W/VEX.W are ignored, as are legacy imm[7:3] and VEX.256 imm[7:6].
        assert!(lift_single(&[0x66, 0x4D, 0x0F, 0x3A, 0x42, 0xCA, 0xFF]).is_ok());
        assert!(lift_single(&[0xC4, 0x43, 0xA5, 0x42, 0xCA, 0xFF]).is_ok());

        // AVX10.2 changes the mandatory prefix to F3, fixes W=0, adds
        // VL=512 and applies a word-granular destination writemask.
        let evex = lift_single(&[0x62, 0xA3, 0x76, 0xC3, 0x42, 0xC2, 0x3F]).unwrap();
        assert_eq!(evex.bytes_consumed, 7);
        assert!(evex.ops.iter().any(|op| {
            matches!(
                op,
                SmirOp {
                    kind: OpKind::VMpsadbw {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                        width: VecWidth::V512,
                        imm: 0x3F,
                        zeroing: true,
                    },
                    x86_hint: Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::Rep,
                        opcode: 0x42,
                        width: VecWidth::V512,
                        w: false,
                    }),
                    ..
                }
            )
        }));

        // FULLMEM disp8 is scaled by the complete 64-byte VL. E4NF does not
        // suppress memory faults, so the lifter emits an ordinary full load.
        let evex_mem = lift_single(&[0x62, 0xF3, 0x66, 0x4A, 0x42, 0x48, 0x02, 0xE7]).unwrap();
        assert!(evex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 128, .. },
                width: VecWidth::V512,
                ..
            }
        )));
        assert!(evex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMpsadbw {
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                zeroing: false,
                ..
            }
        )));
        assert!(
            evex_mem
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
                .all(|op| op.x86_hint.is_none())
        );

        for bytes in [
            &[0x0F, 0x3A, 0x42, 0xCA, 0x07][..],
            &[0xF0, 0x66, 0x0F, 0x3A, 0x42, 0xCA, 0x07][..],
            &[0xF3, 0x66, 0x0F, 0x3A, 0x42, 0xCA, 0x07][..],
            &[0x66, 0x0F, 0x3A, 0x42, 0xCA][..],
            &[0xC4, 0x43, 0x20, 0x42, 0xCA, 0x07][..],
            // AVX10.2 VMPSADBW requires F3/W0, while VDBPSADBW requires
            // 66/W0. Neither form accepts NP/F2, L'L=3, EVEX.b, or zeroing
            // without a nonzero opmask.
            &[0x62, 0xF3, 0x64, 0x08, 0x42, 0xCA, 0x07][..],
            &[0x62, 0xF3, 0x67, 0x08, 0x42, 0xCA, 0x07][..],
            &[0x62, 0xF3, 0xE6, 0x08, 0x42, 0xCA, 0x07][..],
            &[0x62, 0xF3, 0x66, 0x68, 0x42, 0xCA, 0x07][..],
            &[0x62, 0xF3, 0x66, 0x58, 0x42, 0xCA, 0x07][..],
            &[0x62, 0xF3, 0x66, 0x88, 0x42, 0xCA, 0x07][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid MPSADBW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_avx_ne_convert_covers_all_forms_memory_widths_and_reserved_encodings() {
        for (bytes, width, fp16, odd, broadcast) in [
            (
                &[0xC4, 0x62, 0x7A, 0xB1, 0x48, 0x11][..],
                VecWidth::V128,
                false,
                false,
                true,
            ),
            (
                &[0xC4, 0x62, 0x7E, 0xB1, 0x48, 0x11][..],
                VecWidth::V256,
                false,
                false,
                true,
            ),
            (
                &[0xC4, 0x62, 0x79, 0xB1, 0x48, 0x11][..],
                VecWidth::V128,
                true,
                false,
                true,
            ),
            (
                &[0xC4, 0x62, 0x7D, 0xB1, 0x48, 0x11][..],
                VecWidth::V256,
                true,
                false,
                true,
            ),
            (
                &[0xC4, 0x62, 0x7A, 0xB0, 0x48, 0x11][..],
                VecWidth::V128,
                false,
                false,
                false,
            ),
            (
                &[0xC4, 0x62, 0x7E, 0xB0, 0x48, 0x11][..],
                VecWidth::V256,
                false,
                false,
                false,
            ),
            (
                &[0xC4, 0x62, 0x79, 0xB0, 0x48, 0x11][..],
                VecWidth::V128,
                true,
                false,
                false,
            ),
            (
                &[0xC4, 0x62, 0x7D, 0xB0, 0x48, 0x11][..],
                VecWidth::V256,
                true,
                false,
                false,
            ),
            (
                &[0xC4, 0x62, 0x7B, 0xB0, 0x48, 0x11][..],
                VecWidth::V128,
                false,
                true,
                false,
            ),
            (
                &[0xC4, 0x62, 0x7F, 0xB0, 0x48, 0x11][..],
                VecWidth::V256,
                false,
                true,
                false,
            ),
            (
                &[0xC4, 0x62, 0x78, 0xB0, 0x48, 0x11][..],
                VecWidth::V128,
                true,
                true,
                false,
            ),
            (
                &[0xC4, 0x62, 0x7C, 0xB0, 0x48, 0x11][..],
                VecWidth::V256,
                true,
                true,
                false,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86Convert16ToFp32 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    width: VecWidth::V128,
                    fp16: actual_fp16,
                    odd: actual_odd,
                    broadcast: actual_broadcast,
                    ..
                } if width == VecWidth::V128
                    && actual_fp16 == fp16
                    && actual_odd == odd
                    && actual_broadcast == broadcast
            ) || matches!(
                op.kind,
                OpKind::X86Convert16ToFp32 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    width: VecWidth::V256,
                    fp16: actual_fp16,
                    odd: actual_odd,
                    broadcast: actual_broadcast,
                    ..
                } if width == VecWidth::V256
                    && actual_fp16 == fp16
                    && actual_odd == odd
                    && actual_broadcast == broadcast
            )));
            if broadcast {
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::Load {
                        addr: Address::BaseOffset { offset: 17, .. },
                        width: MemWidth::B2,
                        sign: SignExtend::Zero,
                        ..
                    }
                )));
            } else {
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VLoad {
                        addr: Address::BaseOffset { offset: 17, .. },
                        width: actual_width,
                        ..
                    } if actual_width == width
                )));
            }
            assert!(
                !result
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
            );
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        for bytes in [
            &[0xC4, 0x62, 0xFA, 0xB1, 0x08][..],
            &[0xC4, 0x62, 0x72, 0xB1, 0x08][..],
            &[0xC4, 0x62, 0x78, 0xB1, 0x08][..],
            &[0xC4, 0x62, 0x7B, 0xB1, 0x08][..],
            &[0xC4, 0x62, 0x7A, 0xB1, 0xC8][..],
            &[0xC4, 0x62, 0xFA, 0xB0, 0x08][..],
            &[0xC4, 0x62, 0x72, 0xB0, 0x08][..],
            &[0xC4, 0x62, 0x7A, 0xB0, 0xC8][..],
            &[0x62, 0x62, 0x7E, 0x08, 0xB1, 0x08][..],
            &[0xC4, 0x62, 0x7A, 0xB1][..],
            &[0xC4, 0x62, 0x7A, 0xB0][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid AVX-NE-CONVERT encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_psadbw_covers_widths_sources_alignment_tuples_wig_and_invalids() {
        let mmx = lift_single(&[0x0F, 0xF6, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VSadBytes {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xF6,
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

        let mmx_mem = lift_single(&[0x0F, 0xF6, 0x40, 0x01]).unwrap();
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

        for (bytes, width, dst, src1, src2) in [
            (
                &[0x66, 0x45, 0x0F, 0xF6, 0xCA][..],
                VecWidth::V128,
                X86Reg::Xmm(9),
                X86Reg::Xmm(9),
                X86Reg::Xmm(10),
            ),
            (
                &[0xC4, 0x41, 0x21, 0xF6, 0xCA][..],
                VecWidth::V128,
                X86Reg::Xmm(9),
                X86Reg::Xmm(11),
                X86Reg::Xmm(10),
            ),
            (
                &[0xC4, 0x41, 0x25, 0xF6, 0xCA][..],
                VecWidth::V256,
                X86Reg::Ymm(9),
                X86Reg::Ymm(11),
                X86Reg::Ymm(10),
            ),
            (
                &[0x62, 0xA1, 0x65, 0x00, 0xF6, 0xCA][..],
                VecWidth::V128,
                X86Reg::Xmm(17),
                X86Reg::Xmm(19),
                X86Reg::Xmm(18),
            ),
            (
                &[0x62, 0xA1, 0x65, 0x20, 0xF6, 0xCA][..],
                VecWidth::V256,
                X86Reg::Ymm(17),
                X86Reg::Ymm(19),
                X86Reg::Ymm(18),
            ),
            (
                &[0x62, 0xA1, 0x65, 0x40, 0xF6, 0xCA][..],
                VecWidth::V512,
                X86Reg::Zmm(17),
                X86Reg::Zmm(19),
                X86Reg::Zmm(18),
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            let legacy = bytes[0] == 0x66;
            let sad = result
                .ops
                .iter()
                .find(|op| {
                    matches!(
                        op.kind,
                        OpKind::VSadBytes {
                            dst: actual_dst,
                            src1: VReg::Arch(ArchReg::X86(actual_src1)),
                            src2: VReg::Arch(ArchReg::X86(actual_src2)),
                            width: actual_width,
                        } if (legacy || actual_dst == VReg::Arch(ArchReg::X86(dst)))
                            && actual_src1 == src1
                            && actual_src2 == src2
                            && actual_width == width
                    )
                })
                .expect("direct register PSADBW must be atomic");
            let expected_hint = if legacy {
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                }
            } else if bytes[0] == 0xC4 {
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width,
                    w: false,
                }
            } else {
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                    width,
                    w: false,
                }
            };
            assert_eq!(sad.x86_hint, Some(expected_hint));
            assert_eq!(result.ops.len(), 1);
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_mem = lift_single(&[0x66, 0x44, 0x0F, 0xF6, 0x48, 0x10]).unwrap();
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
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let vex_mem = lift_single(&[0xC5, 0x25, 0xF6, 0x48, 0x11]).unwrap();
        assert!(vex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_mem
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        // EVEX Full Mem disp8=127 scales by the complete 64-byte vector.
        let evex_mem = lift_single(&[0x62, 0xE1, 0x5D, 0x40, 0xF6, 0x58, 0x7F]).unwrap();
        assert!(evex_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 8128, .. },
                width: VecWidth::V512,
                ..
            }
        )));

        // W is ignored in VEX and EVEX encodings.
        let vex_w1 = lift_single(&[0xC4, 0x41, 0xA1, 0xF6, 0xCA]).unwrap();
        assert!(matches!(
            vex_w1.ops.as_slice(),
            [SmirOp {
                x86_hint: Some(X86OpHint::VexOp { w: true, .. }),
                ..
            }]
        ));
        let evex_w1 = lift_single(&[0x62, 0xA1, 0xE5, 0x40, 0xF6, 0xCA]).unwrap();
        assert!(matches!(
            evex_w1.ops.as_slice(),
            [SmirOp {
                x86_hint: Some(X86OpHint::EvexOp { w: true, .. }),
                ..
            }]
        ));

        for bytes in [
            &[0xF0, 0x66, 0x0F, 0xF6, 0xCA][..],
            &[0xF3, 0x66, 0x0F, 0xF6, 0xCA][..],
            &[0x66, 0x0F, 0xF6][..],
            &[0xC4, 0x41, 0x20, 0xF6, 0xCA][..],
            &[0x62, 0xA1, 0x64, 0x40, 0xF6, 0xCA][..],
            &[0x62, 0xA1, 0x65, 0x60, 0xF6, 0xCA][..],
            &[0x62, 0xA1, 0x65, 0x41, 0xF6, 0xCA][..],
            &[0x62, 0xA1, 0x65, 0xC0, 0xF6, 0xCA][..],
            &[0x62, 0xA1, 0x65, 0x50, 0xF6, 0xCA][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid PSADBW encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_round_family_covers_modes_merges_widths_alignment_and_invalids() {
        for (bytes, elem, width, lanes, scalar, dst, merge, mode, suppress) in [
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x08, 0xCA, 0xD9][..],
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                X86Reg::Xmm(9),
                X86Reg::Xmm(9),
                FpRoundMode::RoundDown,
                true,
            ),
            (
                &[0x66, 0x44, 0x0F, 0x3A, 0x09, 0x48, 0x10, 0xDA][..],
                VecElementType::F64,
                VecWidth::V128,
                2,
                false,
                X86Reg::Xmm(9),
                X86Reg::Xmm(9),
                FpRoundMode::RoundUp,
                true,
            ),
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x0A, 0xCA, 0xDC][..],
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                X86Reg::Xmm(9),
                X86Reg::Xmm(9),
                FpRoundMode::Dynamic,
                true,
            ),
            (
                &[0xC4, 0x43, 0x7D, 0x08, 0xCA, 0xD9][..],
                VecElementType::F32,
                VecWidth::V256,
                8,
                false,
                X86Reg::Ymm(9),
                X86Reg::Ymm(9),
                FpRoundMode::RoundDown,
                true,
            ),
            (
                &[0xC4, 0x43, 0x21, 0x0A, 0xCA, 0xDC][..],
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                X86Reg::Xmm(9),
                X86Reg::Xmm(11),
                FpRoundMode::Dynamic,
                true,
            ),
        ] {
            let result = lift_single(bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86Round {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    merge: VReg::Arch(ArchReg::X86(actual_merge)),
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar_source,
                    mode: actual_mode,
                    suppress_precision,
                    ..
                } if actual_dst == dst
                    && actual_merge == merge
                    && actual_elem == elem
                    && actual_width == width
                    && actual_lanes == lanes
                    && scalar_source == scalar
                    && actual_mode == mode
                    && suppress_precision == suppress
            )));
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }

        let legacy_packed = lift_single(&[0x66, 0x44, 0x0F, 0x3A, 0x09, 0x48, 0x10, 0x02]).unwrap();
        let alignment = legacy_packed
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
            .unwrap();
        let load = legacy_packed
            .ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(alignment < load);

        let vex_packed = lift_single(&[0xC4, 0x63, 0x7D, 0x09, 0x48, 0x11, 0x02]).unwrap();
        assert!(vex_packed.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !vex_packed
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );

        let scalar_mem = lift_single(&[0xC4, 0x63, 0x21, 0x0B, 0x48, 0x08, 0x0F]).unwrap();
        assert!(scalar_mem.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                width: MemWidth::B8,
                ..
            }
        )));

        // Scalar VEX.LIG and all four REX.W/VEX.W fields are ignored.
        assert!(lift_single(&[0xC4, 0x43, 0x25, 0x0A, 0xCA, 0x04]).is_ok());
        assert!(lift_single(&[0x66, 0x4D, 0x0F, 0x3A, 0x08, 0xCA, 0x00]).is_ok());
        assert!(lift_single(&[0xC4, 0x43, 0xFD, 0x09, 0xCA, 0x00]).is_ok());

        for bytes in [
            &[0x0F, 0x3A, 0x08, 0xCA, 0x00][..],
            &[0xF0, 0x66, 0x0F, 0x3A, 0x09, 0xCA, 0x00][..],
            &[0xF3, 0x66, 0x0F, 0x3A, 0x0A, 0xCA, 0x00][..],
            &[0x66, 0x0F, 0x3A, 0x0B, 0xCA][..],
            &[0xC4, 0x43, 0x75, 0x08, 0xCA, 0x00][..],
            &[0xC4, 0x43, 0x7C, 0x09, 0xCA, 0x00][..],
        ] {
            assert!(
                matches!(
                    lift_single(bytes),
                    Err(LiftError::InvalidEncoding { .. }
                        | LiftError::Unsupported { .. }
                        | LiftError::Incomplete { .. })
                ),
                "invalid ROUND encoding accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_movnti_covers_sizes_addresses_high_registers_and_invalids() {
        for (bytes, src, width, offset) in [
            (&[0x0F, 0xC3, 0x08][..], X86Reg::Rcx, MemWidth::B4, 0i64),
            (
                &[0x4D, 0x0F, 0xC3, 0x48, 0x08][..],
                X86Reg::R9,
                MemWidth::B8,
                8,
            ),
            (
                &[0x4F, 0x0F, 0xC3, 0xBC, 0xAC, 0x34, 0x12, 0x00, 0x00][..],
                X86Reg::R15,
                MemWidth::B8,
                0x1234,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            let (actual_src, addr, actual_width) = lifted
                .ops
                .iter()
                .find_map(|op| match &op.kind {
                    OpKind::Store {
                        src: VReg::Arch(ArchReg::X86(actual_src)),
                        addr,
                        width,
                    } => Some((*actual_src, addr, *width)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(actual_src, src);
            assert_eq!(actual_width, width);
            assert_eq!(
                match addr {
                    Address::Direct(_) => 0,
                    Address::BaseOffset { offset, .. } => *offset,
                    Address::BaseIndexScale { disp, .. } => i64::from(*disp),
                    other => panic!("unexpected MOVNTI address: {other:?}"),
                },
                offset,
            );
        }
        for bytes in [
            &[0x0F, 0xC3, 0xC1][..],
            &[0x66, 0x0F, 0xC3, 0x08][..],
            &[0xF3, 0x0F, 0xC3, 0x08][..],
            &[0xF0, 0x0F, 0xC3, 0x08][..],
        ] {
            assert!(
                matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
                "invalid MOVNTI accepted: {bytes:02X?}",
            );
        }
    }
    #[test]
    fn lift_mmx_movq_uses_v64_registers_ignores_rex_extensions_and_orders_faults() {
        for (bytes, dst, src, opcode) in [
            (
                &[0x45, 0x0F, 0x6F, 0xC1][..],
                X86Reg::Mm(0),
                X86Reg::Mm(1),
                0x6F,
            ),
            (
                &[0x45, 0x0F, 0x7F, 0xC1][..],
                X86Reg::Mm(1),
                X86Reg::Mm(0),
                0x7F,
            ),
        ] {
            let lifted = lift_single(bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len());
            assert!(matches!(
                lifted.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            addr: None,
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VMov {
                            dst: VReg::Arch(ArchReg::X86(actual_dst)),
                            src: VReg::Arch(ArchReg::X86(actual_src)),
                            width: VecWidth::V64,
                        },
                        x86_hint: Some(X86OpHint::SseMov {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
                        }),
                        ..
                    }
                ] if *actual_dst == dst && *actual_src == src && *actual_opcode == opcode
            ));
        }

        let load = lift_single(&[0x44, 0x0F, 0x6F, 0x08]).unwrap();
        assert!(matches!(
            load.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VLoad {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                        ..
                    },
                    x86_hint: Some(X86OpHint::SseMov {
                        prefix: X86SsePrefix::None,
                        opcode: 0x6F,
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
        let store = lift_single(&[0x44, 0x0F, 0x7F, 0x08]).unwrap();
        assert!(matches!(
            store.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VStore {
                        src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                        ..
                    },
                    x86_hint: Some(X86OpHint::SseMov {
                        prefix: X86SsePrefix::None,
                        opcode: 0x7F,
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
        for bytes in [
            &[0xF0, 0x0F, 0x6F, 0xC1][..],
            &[0xF2, 0x0F, 0x6F, 0xC1][..],
            &[0xF0, 0x0F, 0x7F, 0x08][..],
        ] {
            assert!(matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. })
            ));
        }
    }
