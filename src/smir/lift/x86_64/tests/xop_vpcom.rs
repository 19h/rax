//! Strict-lifting contracts for AMD XOP VPCOM.

use super::*;
use crate::smir::ir::ops::{X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, VReg, VecCmpCond, VecElementType, VirtualId, X86Reg};

const OPCODES: &[(u8, VecElementType, bool)] = &[
    (0xCC, VecElementType::I8, true),
    (0xCD, VecElementType::I16, true),
    (0xCE, VecElementType::I32, true),
    (0xCF, VecElementType::I64, true),
    (0xEC, VecElementType::I8, false),
    (0xED, VecElementType::I16, false),
    (0xEE, VecElementType::I32, false),
    (0xEF, VecElementType::I64, false),
];

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn encoding(p0: u8, w: bool, l: bool, pp: u8, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![
        0x8F,
        p0,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

fn expected_condition(immediate: u8, signed: bool) -> VecCmpCond {
    match (immediate & 7, signed) {
        (0, true) => VecCmpCond::Lt,
        (1, true) => VecCmpCond::Le,
        (2, true) => VecCmpCond::Gt,
        (3, true) => VecCmpCond::Ge,
        (0, false) => VecCmpCond::Ltu,
        (1, false) => VecCmpCond::Leu,
        (2, false) => VecCmpCond::Gtu,
        (3, false) => VecCmpCond::Geu,
        (4, _) => VecCmpCond::Eq,
        (5, _) => VecCmpCond::Ne,
        (6, _) => VecCmpCond::False,
        (7, _) => VecCmpCond::True,
        _ => unreachable!(),
    }
}

#[test]
fn strict_lifter_accepts_all_vpcom_opcodes_and_immediate_images() {
    for &(opcode, elem, signed) in OPCODES {
        for immediate in 0..=u8::MAX {
            let bytes = encoding(0xE8, false, false, 0, 2, opcode, &[0xD9, immediate]);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("opcode={opcode:#04x}, imm={immediate:#04x}: {error:?}")
            });
            let expected_cond = expected_condition(immediate, signed);
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
                        kind: OpKind::VCmp {
                            dst,
                            src1,
                            src2,
                            cond,
                            elem: actual_elem,
                            lanes,
                        },
                        x86_hint: Some(X86OpHint::XopVpcom),
                        ..
                    }
                ] if *dst == xmm(3)
                    && *src1 == xmm(2)
                    && *src2 == xmm(1)
                    && *cond == expected_cond
                    && *actual_elem == elem
                    && u32::from(*lanes) == VecWidth::V128.lanes(elem)
            ));
        }
    }
}

#[test]
fn strict_lifter_extends_all_register_fields_without_alias_loss() {
    // ~R=0 and ~B=0 select XMM11 and XMM9; decoded vvvv selects XMM10.
    let bytes = encoding(0x48, false, false, 0, 10, 0xEF, &[0xD9, 0x82]);
    let result = lift_single(&bytes).expect("high-register VPCOMUQ");
    assert!(matches!(
        result.ops.last().map(|op| &op.kind),
        Some(OpKind::VCmp {
            dst,
            src1,
            src2,
            cond: VecCmpCond::Gtu,
            elem: VecElementType::I64,
            lanes: 2,
        }) if *dst == xmm(11) && *src1 == xmm(10) && *src2 == xmm(9)
    ));

    for register in 0..16 {
        let p0 = 0xE8 ^ if register >= 8 { 0xA0 } else { 0 };
        let modrm = 0xC0 | ((register & 7) << 3) | (register & 7);
        let bytes = encoding(p0, false, false, 0, register, 0xCC, &[modrm, 7]);
        let result = lift_single(&bytes).expect("fully aliased VPCOM");
        assert!(matches!(
            result.ops.last().map(|op| &op.kind),
            Some(OpKind::VCmp { dst, src1, src2, .. })
                if *dst == xmm(register) && *src1 == xmm(register) && *src2 == xmm(register)
        ));
    }
}

#[test]
fn memory_form_preserves_alignment_stack_address_and_full_width_load() {
    // VPCOMUD XMM1,XMM5,[RSP+0x10],0xA1.
    let bytes = encoding(0xE8, false, false, 0, 5, 0xEE, &[0x4C, 0x24, 0x10, 0xA1]);
    let result = lift_single(&bytes).expect("stack-relative VPCOMUD");
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
                    access_size: 16,
                    alignment: 16,
                    stack_segment: true,
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
                kind: OpKind::VCmp {
                    dst,
                    src1,
                    src2,
                    cond: VecCmpCond::Leu,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                x86_hint: Some(X86OpHint::XopVpcom),
                ..
            }
        ] if checked == loaded
            && *checked == Address::BaseOffset {
                base: x86_gpr(4),
                offset: 0x10,
                disp_size: DispSize::Disp8,
            }
            && *dst == xmm(1)
            && *src1 == xmm(5)
            && *src2 == *temporary
    ));
}

#[test]
fn rip_relative_memory_uses_pc_after_the_trailing_immediate() {
    let pc = 0x1_0000_2000_u64;
    let bytes = encoding(0xE8, false, false, 0, 2, 0xCC, &[0x0D, 0x20, 0, 0, 0, 0xA5]);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(pc, &bytes, &mut ctx)
        .expect("RIP-relative VPCOM");
    assert!(result.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::PcRel {
                offset: 0x20,
                base: Some(base),
                disp_size: DispSize::Disp32,
            },
            width: VecWidth::V128,
            ..
        } if *base == pc + bytes.len() as u64
    )));
}

#[test]
fn reserved_vpcom_fields_terminalize_before_modrm_or_memory_work() {
    for (name, bytes, consumed) in [
        (
            "W=1",
            encoding(0xE8, true, false, 0, 2, 0xCC, &[0x0B, 0xA5]),
            4,
        ),
        (
            "L=1",
            encoding(0xE8, false, true, 0, 2, 0xCC, &[0x0B, 0xA5]),
            4,
        ),
        (
            "pp=01",
            encoding(0xE8, false, false, 1, 2, 0xCC, &[0x0B, 0xA5]),
            4,
        ),
    ] {
        let result = lift_single(&bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, consumed);
        assert!(result.ops.is_empty(), "{name}");
    }

    let mut forbidden = vec![0xF2];
    forbidden.extend_from_slice(&encoding(0xE8, false, false, 0, 2, 0xCC, &[0xD9, 0xA5]));
    let result = lift_single(&forbidden).expect("legacy prefix must become #UD");
    assert_invalid_opcode_trap(&result, 5);
}

#[test]
fn vpcom_reports_every_incomplete_frontier_exactly() {
    for (bytes, have, need) in [
        (&[0x8F, 0xE8, 0x68, 0xCC][..], 4, 5),
        (&[0x8F, 0xE8, 0x68, 0xCC, 0xD9][..], 5, 6),
        (&[0x8F, 0xE8, 0x68, 0xCC, 0x44][..], 5, 6),
        (&[0x8F, 0xE8, 0x68, 0xCC, 0x44, 0x24][..], 6, 7),
        (
            &[0x8F, 0xE8, 0x68, 0xCC, 0x84, 0x24, 0x01, 0x02, 0x03][..],
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
