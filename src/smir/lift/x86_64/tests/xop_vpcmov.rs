//! Strict-lifting contracts for AMD XOP VPCMOV.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, VReg, VecWidth, VirtualId, X86Reg};

fn x86_vec(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VPCMOV admits only XMM/YMM"),
    }))
}

fn xop(w: bool, l: bool, pp: u8, vvvv: u8, tail: &[u8]) -> Vec<u8> {
    assert!(pp < 4 && vvvv < 16);
    let mut bytes = vec![
        0x8F,
        0xE8,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        0xA2,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

#[test]
fn strict_lifter_accepts_every_w_l_and_immediate_value_with_exact_operand_roles() {
    for w in [false, true] {
        for l in [false, true] {
            let width = if l { VecWidth::V256 } else { VecWidth::V128 };
            for immediate in 0_u8..=u8::MAX {
                // dst=3, src1=2, ModR/M source=1, IS4 source=imm[7:4].
                let bytes = xop(w, l, 0, 2, &[0xD9, immediate]);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("W={w}, L={l}, imm={immediate:#04x}: {error:?}")
                });
                let rm = x86_vec(1, width);
                let selected = x86_vec(immediate >> 4, width);
                let (expected_false, expected_mask) =
                    if w { (selected, rm) } else { (rm, selected) };

                assert_eq!(result.bytes_consumed, bytes.len());
                assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
                assert!(matches!(
                    result.ops.as_slice(),
                    [
                        SmirOp {
                            kind: OpKind::X86RequireXop,
                            ..
                        },
                        SmirOp {
                            kind: OpKind::VBitSelect {
                                dst,
                                mask,
                                src_true,
                                src_false,
                                width: actual_width,
                            },
                            ..
                        }
                    ] if *dst == x86_vec(3, width)
                        && *mask == expected_mask
                        && *src_true == x86_vec(2, width)
                        && *src_false == expected_false
                        && *actual_width == width
                ));
            }
        }
    }
}

#[test]
fn strict_lifter_extends_every_register_field_and_ignores_immediate_low_nibble() {
    // ~R=0 and ~B=0 select destination YMM11 and ModR/M YMM9; vvvv selects
    // YMM10 and IS4 selects YMM15.
    for low in 0_u8..=15 {
        let bytes = [0x8F, 0x48, 0x2C, 0xA2, 0xD9, 0xF0 | low];
        let result = lift_single(&bytes).expect("high-register VPCMOV");
        assert!(matches!(
            result.ops.last().map(|op| &op.kind),
            Some(OpKind::VBitSelect {
                dst,
                mask,
                src_true,
                src_false,
                width: VecWidth::V256,
            }) if *dst == x86_vec(11, VecWidth::V256)
                && *mask == x86_vec(15, VecWidth::V256)
                && *src_true == x86_vec(10, VecWidth::V256)
                && *src_false == x86_vec(9, VecWidth::V256)
        ));
    }
}

#[test]
fn memory_forms_preserve_width_alignment_segment_and_w_selected_role() {
    for w in [false, true] {
        for l in [false, true] {
            let width = if l { VecWidth::V256 } else { VecWidth::V128 };
            // VPCMOV {X,Y}MM1,{X,Y}MM5,{[RSP+0x10],reg6},{reg6,[RSP+0x10]}.
            let bytes = xop(w, l, 0, 5, &[0x4C, 0x24, 0x10, 0x60]);
            let result = lift_single(&bytes).expect("stack-relative VPCMOV");
            assert_eq!(result.bytes_consumed, bytes.len());
            assert!(matches!(
                result.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86RequireXop,
                        ..
                    },
                    SmirOp {
                        kind: OpKind::X86CheckAlignmentAc {
                            addr: checked,
                            access_size,
                            alignment: 16,
                            stack_segment: true,
                            natural_alignment: false,
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VLoad {
                            dst: temporary @ VReg::Virtual(VirtualId(_)),
                            addr: loaded,
                            width: actual_load_width,
                        },
                        x86_hint: Some(X86OpHint::VecAlign(X86VecAlign::Aligned)),
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VBitSelect {
                            dst,
                            mask,
                            src_true,
                            src_false,
                            width: actual_width,
                        },
                        ..
                    }
                ] if checked == loaded
                    && *checked == Address::BaseOffset {
                        base: x86_gpr(4),
                        offset: 0x10,
                        disp_size: DispSize::Disp8,
                    }
                    && u32::from(*access_size) == width.bytes()
                    && *actual_load_width == width
                    && *dst == x86_vec(1, width)
                    && *src_true == x86_vec(5, width)
                    && *actual_width == width
                    && if w {
                        *mask == *temporary && *src_false == x86_vec(6, width)
                    } else {
                        *mask == x86_vec(6, width) && *src_false == *temporary
                    }
            ));
        }
    }
}

#[test]
fn rip_relative_memory_uses_pc_after_the_trailing_immediate() {
    let pc = 0x1_0000_2000_u64;
    let bytes = xop(false, true, 0, 2, &[0x0D, 0x20, 0, 0, 0, 0x40]);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(pc, &bytes, &mut ctx)
        .expect("RIP-relative VPCMOV");
    let address = result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad { addr, .. } => Some(addr),
            _ => None,
        })
        .expect("VPCMOV vector load");
    assert!(matches!(
        address,
        Address::PcRel {
            offset: 0x20,
            base: Some(base),
            disp_size: DispSize::Disp32,
        } if *base == pc + bytes.len() as u64
    ));
}

#[test]
fn reserved_prefix_fields_terminalize_before_modrm_or_memory_work() {
    let result = lift_single(&xop(false, true, 1, 2, &[0x1B, 0x40]))
        .expect("reserved VPCMOV pp must become #UD");
    assert_invalid_opcode_trap(&result, 4);
    assert!(result.ops.is_empty());

    let mut forbidden = vec![0x66];
    forbidden.extend_from_slice(&xop(false, false, 0, 2, &[0xD9, 0x40]));
    let result = lift_single(&forbidden).expect("forbidden legacy prefix must become #UD");
    assert_invalid_opcode_trap(&result, 5);
    assert!(result.ops.is_empty());
}

#[test]
fn vpcmov_reports_each_incomplete_encoding_frontier_exactly() {
    for (bytes, have, need) in [
        (&[0x8F, 0xE8, 0x68, 0xA2][..], 4, 5),
        (&[0x8F, 0xE8, 0x68, 0xA2, 0xD9][..], 5, 6),
        (&[0x8F, 0xE8, 0x68, 0xA2, 0x44][..], 5, 6),
        (&[0x8F, 0xE8, 0x68, 0xA2, 0x44, 0x24][..], 6, 7),
        (
            &[0x8F, 0xE8, 0x68, 0xA2, 0x84, 0x24, 0x01, 0x02, 0x03][..],
            9,
            10,
        ),
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::Incomplete {
                    have: actual_have,
                    need: actual_need,
                    ..
                }) if actual_have == have && actual_need == need
            ),
            "bytes={bytes:02X?}"
        );
    }
}
