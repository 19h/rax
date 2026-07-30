//! Strict-lifting contracts for AMD XOP packed rotate and shift instructions.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86VecAlign, X86XopPackedBitKind};
use crate::smir::ir::types::{
    ArchReg, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};

fn x86_xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn xop(map: u8, w: bool, l: bool, pp: u8, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    assert!((8..=31).contains(&map));
    assert!(pp < 4 && vvvv < 16);
    let mut bytes = vec![
        0x8F,
        0xE0 | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

fn expected_shape(opcode: u8) -> (X86XopPackedBitKind, VecElementType) {
    match opcode {
        0x90 | 0xC0 => (X86XopPackedBitKind::Rotate, VecElementType::I8),
        0x91 | 0xC1 => (X86XopPackedBitKind::Rotate, VecElementType::I16),
        0x92 | 0xC2 => (X86XopPackedBitKind::Rotate, VecElementType::I32),
        0x93 | 0xC3 => (X86XopPackedBitKind::Rotate, VecElementType::I64),
        0x94 => (X86XopPackedBitKind::LogicalShift, VecElementType::I8),
        0x95 => (X86XopPackedBitKind::LogicalShift, VecElementType::I16),
        0x96 => (X86XopPackedBitKind::LogicalShift, VecElementType::I32),
        0x97 => (X86XopPackedBitKind::LogicalShift, VecElementType::I64),
        0x98 => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I8),
        0x99 => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I16),
        0x9A => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I32),
        0x9B => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I64),
        _ => unreachable!("test enumerates assigned packed-bit cells"),
    }
}

#[test]
fn strict_lifter_accepts_all_four_immediate_rotate_cells() {
    for opcode in 0xC0..=0xC3 {
        // VPROT{B,W,D,Q} XMM3,XMM2,0xA5.
        let bytes = xop(8, false, false, 0, 0, opcode, &[0xDA, 0xA5]);
        let result =
            lift_single(&bytes).unwrap_or_else(|error| panic!("opcode={opcode:#04x}: {error:?}"));
        let (expected_kind, expected_elem) = expected_shape(opcode);
        assert_eq!(result.bytes_consumed, 6);
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            result.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86RequireXop,
                    ..
                },
                SmirOp {
                    kind: OpKind::X86XopPackedBit {
                        dst,
                        src,
                        count: SrcOperand::Imm(0xA5),
                        elem,
                        kind,
                    },
                    ..
                }
            ] if *dst == x86_xmm(3)
                && *src == x86_xmm(2)
                && *elem == expected_elem
                && *kind == expected_kind
        ));
    }
}

#[test]
fn strict_lifter_accepts_all_twelve_variable_cells_and_both_w_operand_orders() {
    for opcode in 0x90..=0x9B {
        for w in [false, true] {
            // W=0: dst=XMM2, src=XMM3, count=XMM4.
            // W=1: dst=XMM2, src=XMM4, count=XMM3.
            let bytes = xop(9, w, false, 0, 4, opcode, &[0xD3]);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("opcode={opcode:#04x}, W={w}: {error:?}"));
            let (expected_kind, expected_elem) = expected_shape(opcode);
            let expected_src = if w { x86_xmm(4) } else { x86_xmm(3) };
            let expected_count = if w { x86_xmm(3) } else { x86_xmm(4) };
            assert_eq!(result.bytes_consumed, 5);
            assert!(matches!(
                result.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86RequireXop,
                        ..
                    },
                    SmirOp {
                        kind: OpKind::X86XopPackedBit {
                            dst,
                            src,
                            count: SrcOperand::Reg(count),
                            elem,
                            kind,
                        },
                        ..
                    }
                ] if *dst == x86_xmm(2)
                    && *src == expected_src
                    && *count == expected_count
                    && *elem == expected_elem
                    && *kind == expected_kind
            ));
        }
    }
}

#[test]
fn memory_source_and_count_forms_preserve_alignment_segment_and_operand_roles() {
    for w in [false, true] {
        // VPSHLW XMM1,{[RSP+0x10],XMM5},{XMM5,[RSP+0x10]}.
        let bytes = xop(9, w, false, 0, 5, 0x95, &[0x4C, 0x24, 0x10]);
        let result = lift_single(&bytes).expect("lift stack-relative XOP memory form");
        assert_eq!(result.bytes_consumed, 7);
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
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
                        access_size: 16,
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
                        width: VecWidth::V128,
                    },
                    x86_hint: Some(X86OpHint::VecAlign(X86VecAlign::Aligned)),
                    ..
                },
                SmirOp {
                    kind: OpKind::X86XopPackedBit {
                        dst,
                        src,
                        count,
                        elem: VecElementType::I16,
                        kind: X86XopPackedBitKind::LogicalShift,
                    },
                    ..
                }
            ] if checked == loaded
                && *checked == Address::BaseOffset {
                    base: x86_gpr(4),
                    offset: 0x10,
                    disp_size: DispSize::Disp8,
                }
                && *dst == x86_xmm(1)
                && if w {
                    *src == x86_xmm(5) && *count == SrcOperand::Reg(*temporary)
                } else {
                    *src == *temporary && *count == SrcOperand::Reg(x86_xmm(5))
                }
        ));
    }
}

#[test]
fn reserved_prefix_fields_terminalize_before_modrm_or_memory_work() {
    for (name, bytes, expected_len) in [
        (
            "immediate W=1",
            xop(8, true, false, 0, 0, 0xC0, &[0xCA, 3]),
            4,
        ),
        (
            "immediate vvvv nonzero",
            xop(8, false, false, 0, 1, 0xC0, &[0xCA, 3]),
            4,
        ),
        (
            "immediate L=1",
            xop(8, false, true, 0, 0, 0xC0, &[0xCA, 3]),
            4,
        ),
        ("variable L=1", xop(9, false, true, 0, 2, 0x94, &[0xCA]), 4),
        (
            "variable pp=01",
            xop(9, false, false, 1, 2, 0x94, &[0xCA]),
            4,
        ),
    ] {
        let result =
            lift_single(&bytes).unwrap_or_else(|error| panic!("{name}: expected #UD: {error:?}"));
        assert_invalid_opcode_trap(&result, expected_len);
        assert!(
            result.ops.is_empty(),
            "{name}: no guard/address/load may commit"
        );
    }
}

#[test]
fn packed_xop_reports_each_incomplete_encoding_frontier_exactly() {
    for (bytes, have, need) in [
        (&[0x8F, 0xE8, 0x78, 0xC0][..], 4, 5),
        (&[0x8F, 0xE8, 0x78, 0xC0, 0xCA][..], 5, 6),
        (&[0x8F, 0xE9, 0x68, 0x94][..], 4, 5),
        (&[0x8F, 0xE9, 0x68, 0x94, 0x44][..], 5, 6),
        (&[0x8F, 0xE9, 0x68, 0x94, 0x44, 0x24][..], 6, 7),
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
