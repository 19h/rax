//! x86 FMA3/FMA4 source-order and floating-point lift contracts.

use super::*;
use crate::smir::ir::ops::X86FmaOp;

fn fma4_cases() -> [(u8, VecElementType, X86FmaKind, bool); 20] {
    [
        (0x5C, VecElementType::F32, X86FmaKind::AddSub, false),
        (0x5D, VecElementType::F64, X86FmaKind::AddSub, false),
        (0x5E, VecElementType::F32, X86FmaKind::SubAdd, false),
        (0x5F, VecElementType::F64, X86FmaKind::SubAdd, false),
        (0x68, VecElementType::F32, X86FmaKind::Add, false),
        (0x69, VecElementType::F64, X86FmaKind::Add, false),
        (0x6A, VecElementType::F32, X86FmaKind::Add, true),
        (0x6B, VecElementType::F64, X86FmaKind::Add, true),
        (0x6C, VecElementType::F32, X86FmaKind::Sub, false),
        (0x6D, VecElementType::F64, X86FmaKind::Sub, false),
        (0x6E, VecElementType::F32, X86FmaKind::Sub, true),
        (0x6F, VecElementType::F64, X86FmaKind::Sub, true),
        (
            0x78,
            VecElementType::F32,
            X86FmaKind::NegativeMultiplyAdd,
            false,
        ),
        (
            0x79,
            VecElementType::F64,
            X86FmaKind::NegativeMultiplyAdd,
            false,
        ),
        (
            0x7A,
            VecElementType::F32,
            X86FmaKind::NegativeMultiplyAdd,
            true,
        ),
        (
            0x7B,
            VecElementType::F64,
            X86FmaKind::NegativeMultiplyAdd,
            true,
        ),
        (
            0x7C,
            VecElementType::F32,
            X86FmaKind::NegativeMultiplySub,
            false,
        ),
        (
            0x7D,
            VecElementType::F64,
            X86FmaKind::NegativeMultiplySub,
            false,
        ),
        (
            0x7E,
            VecElementType::F32,
            X86FmaKind::NegativeMultiplySub,
            true,
        ),
        (
            0x7F,
            VecElementType::F64,
            X86FmaKind::NegativeMultiplySub,
            true,
        ),
    ]
}

fn fma4_encoding(opcode: u8, w: bool, l: bool, modrm: u8, is4: u8, low: u8) -> [u8; 6] {
    assert!(is4 < 16 && low < 16);
    [
        0xC4,
        0xE3,
        0x69 | (u8::from(w) << 7) | (u8::from(l) << 2),
        opcode,
        modrm,
        (is4 << 4) | low,
    ]
}

fn fma4_vec(reg: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(reg),
        VecWidth::V256 => X86Reg::Ymm(reg),
        _ => unreachable!("FMA4 admits only 128-bit and 256-bit vectors"),
    }))
}

#[test]
fn vex_fma4_strictly_lifts_every_opcode_w_l_and_is4_low_nibble() {
    for (opcode, elem, kind, scalar) in fma4_cases() {
        for w in [false, true] {
            for l in [false, true] {
                for low in 0..=0x0F {
                    // dest=1, VEX.vvvv=2, /is4=3, ModR/M.r/m=4.
                    let bytes = fma4_encoding(opcode, w, l, 0xCC, 3, low);
                    let lifted = lift_single(&bytes).expect("legal FMA4 register form");
                    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                    assert!(matches!(lifted.control_flow, ControlFlow::Fallthrough));
                    assert_eq!(lifted.ops.len(), 2, "{bytes:02X?}");

                    let operation_width = if scalar || !l {
                        VecWidth::V128
                    } else {
                        VecWidth::V256
                    };
                    let lanes = if scalar {
                        1
                    } else {
                        operation_width.lanes(elem) as u8
                    };
                    let OpKind::X86Fma(fma) = lifted.ops[0].kind else {
                        panic!("expected exact FMA4 semantic op for {bytes:02X?}")
                    };
                    assert_eq!(fma.src1, fma4_vec(2, operation_width));
                    assert_eq!(fma.src2, fma4_vec(if w { 3 } else { 4 }, operation_width));
                    assert_eq!(fma.src3, fma4_vec(if w { 4 } else { 3 }, operation_width));
                    assert_eq!(fma.mask, None);
                    assert_eq!(fma.elem, elem);
                    assert_eq!(fma.kind, kind);
                    assert_eq!(fma.order, X86FmaOrder::Order123);
                    assert_eq!(fma.round, FpRoundMode::Dynamic);
                    assert_eq!(fma.lanes, lanes);
                    assert!(fma.shape_valid());
                    assert!(matches!(
                        lifted.ops[0].x86_hint,
                        Some(X86OpHint::VexOp {
                            map: X86VecMap::Map0F3A,
                            pp: X86SsePrefix::OpSize,
                            opcode: actual_opcode,
                            width,
                            w: actual_w,
                        }) if actual_opcode == opcode && width == operation_width && actual_w == w
                    ));
                    assert!(matches!(
                        lifted.ops[1],
                        SmirOp {
                            kind: OpKind::VMov { dst, src, width },
                            x86_hint: None,
                            ..
                        } if dst == fma4_vec(1, operation_width)
                            && src == fma.dst
                            && width == operation_width
                    ));
                }
            }
        }
    }
}

#[test]
fn vex_fma4_memory_source_uses_full_instruction_rip_and_w_selected_role() {
    let pc = 0x1_0000_2000u64;
    for (opcode, elem, _kind, scalar) in fma4_cases() {
        for w in [false, true] {
            for l in [false, true] {
                // dest=1, VEX.vvvv=2, r/m=[RIP+0x20], /is4=3.
                let p1 = 0x69 | (u8::from(w) << 7) | (u8::from(l) << 2);
                let bytes = [0xC4, 0xE3, p1, opcode, 0x0D, 0x20, 0x00, 0x00, 0x00, 0x3F];
                let mut lifter = X86_64Lifter::strict();
                let mut ctx = LiftContext::new(SourceArch::X86_64);
                let lifted = lifter
                    .lift_insn(pc, &bytes, &mut ctx)
                    .expect("legal FMA4 memory form");
                assert_eq!(lifted.bytes_consumed, bytes.len());

                let operation_width = if scalar || !l {
                    VecWidth::V128
                } else {
                    VecWidth::V256
                };
                let expected_addr = pc + bytes.len() as u64;
                let memory_vector = if scalar {
                    let (scalar_value, addr, width) = lifted
                        .ops
                        .iter()
                        .find_map(|op| match &op.kind {
                            OpKind::Load {
                                dst, addr, width, ..
                            } => Some((*dst, addr, *width)),
                            _ => None,
                        })
                        .expect("scalar FMA4 memory load");
                    assert_eq!(
                        width,
                        if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        }
                    );
                    assert!(matches!(
                        addr,
                        Address::PcRel {
                            offset: 0x20,
                            base: Some(base),
                            ..
                        } if *base == expected_addr
                    ));
                    lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::VBroadcast {
                                dst,
                                scalar,
                                elem: actual_elem,
                                lanes: 1,
                            } if scalar == scalar_value && actual_elem == elem => Some(dst),
                            _ => None,
                        })
                        .expect("scalar FMA4 broadcast")
                } else {
                    let (dst, addr, width) = lifted
                        .ops
                        .iter()
                        .find_map(|op| match &op.kind {
                            OpKind::VLoad { dst, addr, width } => Some((*dst, addr, *width)),
                            _ => None,
                        })
                        .expect("packed FMA4 memory load");
                    assert_eq!(width, operation_width);
                    assert!(matches!(
                        addr,
                        Address::PcRel {
                            offset: 0x20,
                            base: Some(base),
                            ..
                        } if *base == expected_addr
                    ));
                    dst
                };
                let fma = lifted
                    .ops
                    .iter()
                    .find_map(|op| match op.kind {
                        OpKind::X86Fma(fma) => Some(fma),
                        _ => None,
                    })
                    .expect("FMA4 semantic operation");
                assert_eq!(fma.src1, fma4_vec(2, operation_width));
                assert_eq!(
                    fma.src2,
                    if w {
                        fma4_vec(3, operation_width)
                    } else {
                        memory_vector
                    }
                );
                assert_eq!(
                    fma.src3,
                    if w {
                        memory_vector
                    } else {
                        fma4_vec(3, operation_width)
                    }
                );
            }
        }
    }
}

#[test]
fn vex_fma4_addr32_memory_preserves_base_index_scale_and_displacement() {
    // VFMADDPS xmm1,xmm2,[ecx+edx*4+0x20],xmm3, with VEX.W=0.
    let bytes = [0x67, 0xC4, 0xE3, 0x69, 0x68, 0x4C, 0x91, 0x20, 0x30];
    let lifted = lift_single(&bytes).expect("FMA4 addr32 memory source");
    assert_eq!(lifted.bytes_consumed, bytes.len());
    assert!(lifted.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::X86Addr32(inner),
            width: VecWidth::V128,
            ..
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x20,
                ..
            } if *base == x86_gpr(1) && *index == x86_gpr(2)
        )
    )));
}

#[test]
fn vex_fma4_extends_every_register_field_independently() {
    for (p1, expected_src2, expected_src3) in [(0x2D, 12, 11), (0xAD, 11, 12)] {
        // dest=ymm9 (VEX.R), vvvv=ymm10, r/m=ymm12 (VEX.B), /is4=ymm11.
        let bytes = [0xC4, 0x43, p1, 0x68, 0xCC, 0xB0];
        let lifted = lift_single(&bytes).expect("high-register FMA4 form");
        assert!(matches!(
            lifted.ops.as_slice(),
            [
                SmirOp {
                    kind:
                        OpKind::X86Fma(X86FmaOp {
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
                            src2,
                            src3,
                            ..
                        }),
                    ..
                },
                SmirOp {
                    kind: OpKind::VMov {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                        width: VecWidth::V256,
                        ..
                    },
                    ..
                }
            ] if *src2 == fma4_vec(expected_src2, VecWidth::V256)
                && *src3 == fma4_vec(expected_src3, VecWidth::V256)
        ));
    }
}

#[test]
fn vex_fma4_rejects_wrong_mandatory_prefix_and_reports_truncation_exactly() {
    for p1 in [0x68, 0x6A, 0x6B] {
        assert!(matches!(
            lift_single(&[0xC4, 0xE3, p1, 0x68, 0xCC, 0x30]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    assert!(matches!(
        lift_single(&[0xC4, 0xE3, 0x69, 0x68]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 4,
            need: 5,
        })
    ));
    assert!(matches!(
        lift_single(&[0xC4, 0xE3, 0x69, 0x68, 0xCC]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 5,
            need: 6,
        })
    ));
}

#[cfg(feature = "smir-jit")]
#[test]
fn vex_fma4_native_admission_requires_exact_register_provenance() {
    use crate::smir::lower::runtime::is_native_clobber_safe;

    let bytes = fma4_encoding(0x68, false, true, 0xCC, 3, 0);
    let lifted = lift_single(&bytes).expect("strict FMA4 lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), 0x1000),
        X86InstructionBytes::new(&bytes).expect("FMA4 provenance"),
    );

    assert!(is_native_clobber_safe(&function));
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(is_native_clobber_safe(&function));

    function.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&function));
}

#[test]
fn evex_fma3_embedded_rounding_accepts_every_rc_and_implies_full_width() {
    for (p2, expected_round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        for (p1, elem, lanes) in [
            (0x75, VecElementType::F32, 16),
            (0xF5, VecElementType::F64, 8),
        ] {
            let lifted = lift_single(&[0x62, 0xF2, p1, p2, 0x98, 0xC2]).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op,
                SmirOp {
                    kind: OpKind::X86Fma(X86FmaOp {
                        elem: actual_elem,
                        round,
                        lanes: actual_lanes,
                        ..
                    }),
                    x86_hint: Some(X86OpHint::EvexOp {
                        width: VecWidth::V512,
                        ..
                    }),
                    ..
                } if *actual_elem == elem && *round == expected_round && *actual_lanes == lanes
            )));
        }

        let scalar = lift_single(&[0x62, 0xF2, 0x75, p2, 0x99, 0xC2]).unwrap();
        assert!(scalar.ops.iter().any(|op| matches!(
            op,
            SmirOp {
                kind: OpKind::X86Fma(X86FmaOp {
                    elem: VecElementType::F32,
                    round,
                    lanes: 1,
                    ..
                }),
                x86_hint: Some(X86OpHint::EvexOp {
                    width: VecWidth::V128,
                    ..
                }),
                ..
            } if *round == expected_round
        )));
    }
}

#[test]
fn evex_scalar_fma3_rejects_memory_evex_b_because_broadcast_is_unsupported() {
    for (p1, opcode) in [(0x75, 0x99), (0xF5, 0x99)] {
        assert!(matches!(
            lift_single(&[0x62, 0xF2, p1, 0x18, opcode, 0x02]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn x86_fma3_ir_retains_architectural_sources_mask_kind_and_order() {
    let lifted = lift_single(&[0x62, 0xA2, 0x75, 0x43, 0xB6, 0xC2]).unwrap();
    assert!(lifted.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Fma(X86FmaOp {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            elem: VecElementType::F32,
            kind: X86FmaKind::AddSub,
            order: X86FmaOrder::Order231,
            round: FpRoundMode::Dynamic,
            lanes: 16,
            ..
        })
    )));
    assert_eq!(
        lifted
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Fma(_)))
            .count(),
        1,
        "alternating FMA must aggregate exceptions in one semantic operation"
    );
}

#[test]
fn masked_packed_evex_fma3_broadcast_uses_one_aggregate_gated_scalar_read() {
    for (p0, p1, elem, memory_width) in [
        (0xF6, 0x75, VecElementType::F16, MemWidth::B2),
        (0xF2, 0x75, VecElementType::F32, MemWidth::B4),
        (0xF2, 0xF5, VecElementType::F64, MemWidth::B8),
    ] {
        for (ll, width) in [
            (0u8, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            let lanes = width.lanes(elem) as u8;
            let applicable_lane_mask = (1u64 << lanes) - 1;
            for mask_index in 1..=7u8 {
                for zeroing in [false, true] {
                    // VFMADD132P{H,S,D} v0{k}{z},v1,[rbx+disp8]{1toN}.
                    let p2 = (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | mask_index;
                    let bytes = [0x62, p0, p1, p2, 0x98, 0x43, 0x01];
                    let lifted = lift_single(&bytes)
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));

                    let pred_loads: Vec<_> = lifted
                        .ops
                        .iter()
                        .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                        .collect();
                    assert_eq!(
                        pred_loads.len(),
                        1,
                        "{bytes:02X?}: one architectural scalar memory operand"
                    );
                    let (loaded_scalar, condition) = match pred_loads[0].kind {
                        OpKind::PredLoad {
                            dst,
                            cond,
                            addr:
                                Address::BaseOffset {
                                    base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                                    offset,
                                    disp_size: DispSize::Disp8,
                                },
                            width: actual_width,
                            signed: SignExtend::Zero,
                        } if actual_width == memory_width && offset == i64::from(elem.bytes()) => {
                            (dst, cond)
                        }
                        ref other => panic!("{bytes:02X?}: unexpected scalar read {other:?}"),
                    };
                    let active_mask = lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::And {
                                dst,
                                src1,
                                src2: SrcOperand::Imm(lane_mask),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            } if src1 == mask && lane_mask == applicable_lane_mask as i64 => {
                                Some(dst)
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing applicable-mask AND"));
                    let negated = lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::Neg {
                                dst,
                                src,
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            } if src == active_mask => Some(dst),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing predicate negation"));
                    let combined = lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::Or {
                                dst,
                                src1,
                                src2: SrcOperand::Reg(src2),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            } if src1 == active_mask && src2 == negated => Some(dst),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing predicate OR"));
                    assert!(lifted.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::Shr {
                            dst,
                            src,
                            amount: SrcOperand::Imm(63),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        } if dst == condition && src == combined
                    )));
                    assert_eq!(
                        lifted
                            .ops
                            .iter()
                            .filter(|op| matches!(
                                op.kind,
                                OpKind::And { src1, .. } if src1 == mask
                            ))
                            .count(),
                        1,
                        "{bytes:02X?}: packed lifting must not emit a dead bit-0 mask test"
                    );
                    assert!(lifted.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        } if dst == loaded_scalar
                    )));
                    let broadcast = lifted.ops.iter().find_map(|op| match op.kind {
                        OpKind::VBroadcast {
                            dst,
                            scalar,
                            elem: actual_elem,
                            lanes: actual_lanes,
                        } if scalar == loaded_scalar
                            && actual_elem == elem
                            && actual_lanes == lanes =>
                        {
                            Some(dst)
                        }
                        _ => None,
                    });
                    let broadcast =
                        broadcast.unwrap_or_else(|| panic!("{bytes:02X?}: missing broadcast"));
                    assert!(lifted.ops.iter().any(|op| match &op.kind {
                        OpKind::X86Fma(fma) => {
                            fma.src3 == broadcast && fma.mask == Some(mask)
                        }
                        OpKind::X86FP16Fma {
                            src3,
                            mask: actual_mask,
                            ..
                        } => *src3 == broadcast && *actual_mask == Some(mask),
                        _ => false,
                    }));
                    assert!(
                        !lifted.ops.iter().any(|op| {
                            matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. })
                        }),
                        "{bytes:02X?}: aggregate-gated broadcast must not issue eager reads"
                    );
                }
            }
        }
    }
}
