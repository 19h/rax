//! AVX-512-FP16 packed arithmetic lifting coverage.

use super::*;
use crate::smir::ir::types::{BlockId, FunctionId};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;
use crate::smir::optimize::{OptLevel, optimize_function};

fn optimized_lift(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let lifted =
        lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    optimize_function(&mut function, level);
    function
}

#[test]
fn masked_packed_fp16_broadcast_uses_one_aggregate_gated_scalar_read() {
    for (opcode, expected_op) in [
        (0x58, Avx10FP16Op::Add),
        (0x59, Avx10FP16Op::Mul),
        (0x5C, Avx10FP16Op::Sub),
        (0x5D, Avx10FP16Op::Min),
        (0x5E, Avx10FP16Op::Div),
        (0x5F, Avx10FP16Op::Max),
    ] {
        for (ll, width) in [
            (0u8, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            let lanes = width.lanes(VecElementType::F16) as u8;
            let applicable_mask = (1u64 << lanes) - 1;
            for mask_index in 1..=7u8 {
                for zeroing in [false, true] {
                    // V{ADD,MUL,SUB,MIN,DIV,MAX}PH v0{k}{z},v1,[rbx+2]{1toN}.
                    let p2 = (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | mask_index;
                    let bytes = [0x62, 0xF5, 0x74, p2, opcode, 0x43, 0x01];
                    let lifted = lift_single(&bytes)
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
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
                                    offset: 2,
                                    disp_size: DispSize::Disp8,
                                },
                            width: MemWidth::B2,
                            signed: SignExtend::Zero,
                        } => (dst, cond),
                        ref other => panic!("{bytes:02X?}: unexpected scalar read {other:?}"),
                    };

                    let active_mask = lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::And {
                                dst,
                                src1,
                                src2: SrcOperand::Imm(bits),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            } if src1 == mask && bits == applicable_mask as i64 => Some(dst),
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
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing nonzero-mask NEG"));
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
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing nonzero-mask OR"));
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

                    let broadcast = lifted
                        .ops
                        .iter()
                        .find_map(|op| match op.kind {
                            OpKind::VBroadcast {
                                dst,
                                scalar,
                                elem: VecElementType::F16,
                                lanes: actual_lanes,
                            } if scalar == loaded_scalar && actual_lanes == lanes => Some(dst),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{bytes:02X?}: missing scalar broadcast"));
                    assert!(matches!(
                        lifted.ops.last().map(|op| &op.kind),
                        Some(OpKind::VFP16Arith {
                            dst,
                            src1,
                            src2,
                            mask: Some(actual_mask),
                            op,
                            round: FpRoundMode::Dynamic,
                            width: actual_width,
                            lanes: actual_lanes,
                            zeroing: actual_zeroing,
                        }) if *dst == match width {
                            VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                            VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                            _ => unreachable!(),
                        }
                            && *src1 == match width {
                                VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                                VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                                VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                                _ => unreachable!(),
                            }
                            && *src2 == broadcast
                            && *actual_mask == mask
                            && *op == expected_op
                            && *actual_width == width
                            && *actual_lanes == lanes
                            && *actual_zeroing == zeroing
                    ));
                }
            }
        }
    }
}

#[test]
fn masked_packed_fp16_broadcast_remains_single_access_at_every_optimizer_level() {
    for opcode in [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F] {
        for ll in 0..=2u8 {
            for mask_index in 1..=7u8 {
                for zeroing in [false, true] {
                    let p2 = (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | mask_index;
                    let bytes = [0x62, 0xF5, 0x74, p2, opcode, 0x43, 0x01];
                    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                        let function = optimized_lift(&bytes, level);
                        let ops = &function.blocks[0].ops;
                        assert_eq!(
                            ops.iter()
                                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                                .count(),
                            1,
                            "{bytes:02X?} {level:?}: {ops:#?}"
                        );
                        assert!(matches!(
                            ops.last().map(|op| &op.kind),
                            Some(OpKind::VFP16Arith {
                                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(actual_mask)))),
                                ..
                            }) if *actual_mask == mask_index
                        ));
                    }
                }
            }
        }
    }
}
