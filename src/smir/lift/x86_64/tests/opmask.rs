//! Exhaustive strict-lift coverage for VEX-encoded AVX-512 opmask instructions.

use super::*;
use crate::smir::ir::ops::{
    X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource, X86OpmaskOp,
    X86OpmaskShiftKind, X86OpmaskTestKind,
};

fn k(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::K(index)))
}

#[allow(clippy::too_many_arguments)]
fn vex3(
    map: u8,
    w: bool,
    vvvv: u8,
    l: bool,
    pp: u8,
    r: bool,
    x: bool,
    b: bool,
    opcode: u8,
    modrm: u8,
    immediate: Option<u8>,
) -> Vec<u8> {
    let mut bytes = vec![
        0xC4,
        (u8::from(!r) << 7) | (u8::from(!x) << 6) | (u8::from(!b) << 5) | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
        modrm,
    ];
    bytes.extend(immediate);
    bytes
}

fn canonical_vex(
    map: u8,
    w: bool,
    vvvv: u8,
    l: bool,
    pp: u8,
    opcode: u8,
    modrm: u8,
    immediate: Option<u8>,
) -> Vec<u8> {
    vex3(
        map, w, vvvv, l, pp, false, false, false, opcode, modrm, immediate,
    )
}

fn logic_encoding(width: OpWidth) -> (u8, bool) {
    match width {
        OpWidth::W8 => (1, false),
        OpWidth::W16 => (0, false),
        OpWidth::W32 => (1, true),
        OpWidth::W64 => (0, true),
        OpWidth::W128 => unreachable!(),
    }
}

fn gpr_move_encoding(width: OpWidth) -> (u8, bool) {
    match width {
        OpWidth::W8 => (1, false),
        OpWidth::W16 => (0, false),
        OpWidth::W32 => (3, false),
        OpWidth::W64 => (3, true),
        OpWidth::W128 => unreachable!(),
    }
}

fn exact_opmask(bytes: &[u8]) -> X86OpmaskOp {
    let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_eq!(result.ops.len(), 1, "{bytes:02X?}: {:#?}", result.ops);
    let OpKind::X86Opmask(opmask) = &result.ops[0].kind else {
        panic!(
            "expected one X86Opmask op for {bytes:02X?}: {:#?}",
            result.ops
        );
    };
    assert!(result.ops[0].x86_hint.is_none());
    opmask.clone()
}

#[test]
fn strict_lifter_accepts_every_logical_arithmetic_not_and_test_width() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let (pp, w) = logic_encoding(width);
        for (opcode, kind) in [
            (0x41, X86OpmaskBinaryKind::And),
            (0x42, X86OpmaskBinaryKind::AndNot),
            (0x45, X86OpmaskBinaryKind::Or),
            (0x46, X86OpmaskBinaryKind::Xnor),
            (0x47, X86OpmaskBinaryKind::Xor),
            (0x4A, X86OpmaskBinaryKind::Add),
        ] {
            let bytes = canonical_vex(1, w, 1, true, pp, opcode, 0xDA, None);
            assert_eq!(
                exact_opmask(&bytes),
                X86OpmaskOp::Binary {
                    kind,
                    dst: k(3),
                    src1: k(1),
                    src2: k(2),
                    width,
                },
                "opcode={opcode:#x} width={width:?}"
            );
        }

        let bytes = canonical_vex(1, w, 0, false, pp, 0x44, 0xDA, None);
        assert_eq!(
            exact_opmask(&bytes),
            X86OpmaskOp::Not {
                dst: k(3),
                src: k(2),
                width,
            }
        );

        for (opcode, kind) in [
            (0x99, X86OpmaskTestKind::And),
            (0x98, X86OpmaskTestKind::Or),
        ] {
            let bytes = canonical_vex(1, w, 0, false, pp, opcode, 0xCA, None);
            assert_eq!(
                exact_opmask(&bytes),
                X86OpmaskOp::Test {
                    kind,
                    src1: k(1),
                    src2: k(2),
                    width,
                }
            );
        }
    }
}

#[test]
fn strict_lifter_accepts_every_unpack_and_immediate_shift_width() {
    for (width, pp, w) in [
        (OpWidth::W16, 1, false),
        (OpWidth::W32, 0, false),
        (OpWidth::W64, 0, true),
    ] {
        let bytes = canonical_vex(1, w, 1, true, pp, 0x4B, 0xDA, None);
        assert_eq!(
            exact_opmask(&bytes),
            X86OpmaskOp::Unpack {
                dst: k(3),
                src1: k(1),
                src2: k(2),
                width,
            }
        );
    }

    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let w = matches!(width, OpWidth::W16 | OpWidth::W64);
        let opcode_delta = u8::from(matches!(width, OpWidth::W32 | OpWidth::W64));
        for (base_opcode, kind) in [
            (0x30, X86OpmaskShiftKind::Right),
            (0x32, X86OpmaskShiftKind::Left),
        ] {
            let opcode = base_opcode + opcode_delta;
            let bytes = canonical_vex(3, w, 0, false, 1, opcode, 0xDA, Some(0xFF));
            assert_eq!(
                exact_opmask(&bytes),
                X86OpmaskOp::Shift {
                    kind,
                    dst: k(3),
                    src: k(2),
                    width,
                    count: 0xFF,
                }
            );
        }
    }
}

#[test]
fn strict_lifter_accepts_every_kmov_direction_width_and_operand_class() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));

    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let (pp, w) = logic_encoding(width);

        let to_mask = canonical_vex(1, w, 0, false, pp, 0x90, 0xDA, None);
        assert_eq!(
            exact_opmask(&to_mask),
            X86OpmaskOp::MoveToMask {
                dst: k(3),
                src: X86OpmaskMoveSource::Mask(k(2)),
                width,
            }
        );

        let load = canonical_vex(1, w, 0, false, pp, 0x90, 0x18, None);
        assert_eq!(
            exact_opmask(&load),
            X86OpmaskOp::MoveToMask {
                dst: k(3),
                src: X86OpmaskMoveSource::Memory(Address::Direct(rax)),
                width,
            }
        );

        let store = canonical_vex(1, w, 0, false, pp, 0x91, 0x10, None);
        assert_eq!(
            exact_opmask(&store),
            X86OpmaskOp::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(Address::Direct(rax)),
                src: k(2),
                width,
            }
        );

        let (gpr_pp, gpr_w) = gpr_move_encoding(width);
        let gpr_to_mask = vex3(
            1, gpr_w, 0, false, gpr_pp, false, false, true, 0x92, 0xD8, None,
        );
        assert_eq!(
            exact_opmask(&gpr_to_mask),
            X86OpmaskOp::MoveToMask {
                dst: k(3),
                src: X86OpmaskMoveSource::Gpr(r8),
                width,
            }
        );

        let mask_to_gpr = vex3(
            1, gpr_w, 0, false, gpr_pp, true, false, false, 0x93, 0xC2, None,
        );
        assert_eq!(
            exact_opmask(&mask_to_gpr),
            X86OpmaskOp::MoveFromMask {
                dst: X86OpmaskMoveDestination::Gpr(r8),
                src: k(2),
                width,
            }
        );
    }
}

#[test]
fn strict_lifter_preserves_kmov_address_size_segment_sib_and_rip_relative_metadata() {
    let fs_addr32 = lift_single(&[0x64, 0x67, 0xC4, 0x81, 0x78, 0x90, 0x5C, 0x5A, 0x08])
        .expect("FS addr32 KMOVW k3,[r10d+r11d*2+8]");
    assert_eq!(fs_addr32.bytes_consumed, 9);
    assert!(matches!(
        fs_addr32.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Opmask(X86OpmaskOp::MoveToMask {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                src: X86OpmaskMoveSource::Memory(Address::X86Addr32(addr)),
                width: OpWidth::W16,
            }),
            ..
        }] if matches!(
            addr.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R10))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::R11))),
                scale: 2,
                disp: 8,
            }
        )
    ));

    let rip =
        lift_single(&[0xC5, 0xF9, 0x91, 0x15, 0x20, 0x00, 0x00, 0x00]).expect("KMOVB [RIP+32],k2");
    assert_eq!(rip.bytes_consumed, 8);
    assert!(matches!(
        rip.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Opmask(X86OpmaskOp::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(Address::PcRel {
                    offset: 0x20,
                    base: Some(0x1008),
                    ..
                }),
                src: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
                width: OpWidth::W8,
            }),
            ..
        }]
    ));
}

#[test]
fn strict_lifter_rejects_every_reserved_opmask_encoding_axis() {
    let invalid = [
        // Binary operations require L=1, register ModR/M, and K0-K7 operands.
        canonical_vex(1, false, 1, false, 0, 0x41, 0xDA, None),
        canonical_vex(1, false, 1, true, 0, 0x41, 0x18, None),
        vex3(1, false, 1, true, 0, true, false, false, 0x41, 0xDA, None),
        vex3(1, false, 1, true, 0, false, false, true, 0x41, 0xDA, None),
        canonical_vex(1, false, 8, true, 0, 0x41, 0xDA, None),
        // Unary, move, and test forms reserve L and vvvv.
        canonical_vex(1, false, 0, true, 0, 0x44, 0xDA, None),
        canonical_vex(1, false, 1, false, 0, 0x44, 0xDA, None),
        canonical_vex(1, false, 1, false, 0, 0x90, 0xDA, None),
        // F2 belongs only to the opcode-92/93 GPR forms. It is not an alias
        // for dword/qword opcode-90/91 mask-or-memory forms.
        canonical_vex(1, false, 0, false, 3, 0x90, 0xDA, None),
        canonical_vex(1, true, 0, false, 3, 0x90, 0x18, None),
        canonical_vex(1, false, 0, false, 3, 0x91, 0x18, None),
        canonical_vex(1, true, 0, false, 3, 0x91, 0x18, None),
        // Opcode 91 is MR-only; ModR/M.mod=11 is reserved.
        canonical_vex(1, false, 0, false, 0, 0x91, 0xD3, None),
        canonical_vex(1, false, 0, true, 0, 0x99, 0xCA, None),
        canonical_vex(1, false, 0, false, 0, 0x99, 0x08, None),
        // GPR KMOV has no memory form and F2 is the only dword/qword pp.
        canonical_vex(1, false, 0, false, 3, 0x92, 0x18, None),
        canonical_vex(1, false, 0, false, 2, 0x92, 0xD8, None),
        vex3(1, false, 0, false, 0, false, false, true, 0x93, 0xCA, None),
        // KUNPCK has three exact pp/W forms.
        canonical_vex(1, true, 1, true, 1, 0x4B, 0xDA, None),
        // KSHIFT requires map 0F3A, pp=66, L=0, vvvv=0, and register source.
        canonical_vex(3, false, 0, false, 0, 0x30, 0xDA, Some(1)),
        canonical_vex(3, false, 0, true, 1, 0x30, 0xDA, Some(1)),
        canonical_vex(3, false, 1, false, 1, 0x30, 0xDA, Some(1)),
        canonical_vex(3, false, 0, false, 1, 0x30, 0x18, Some(1)),
    ];

    for bytes in invalid {
        assert!(
            matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
            "reserved opmask encoding accepted: {bytes:02X?}"
        );
    }
}

#[test]
fn strict_lifter_reports_truncated_modrm_displacement_and_shift_immediate_precisely() {
    for bytes in [
        &[0xC5, 0xF4, 0x41][..],
        &[0xC5, 0xF8, 0x90, 0x84, 0x88, 0x00, 0x00][..],
        &[0xC4, 0xE3, 0x79, 0x31, 0xDA][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated opmask encoding did not report Incomplete: {bytes:02X?}"
        );
    }
}

#[test]
fn known_llvm_encodings_lift_without_an_interpreter_frontier() {
    for bytes in [
        &[0xC5, 0xF4, 0x41, 0xDA][..],             // kandw k3,k1,k2
        &[0xC5, 0xF5, 0x42, 0xFA][..],             // kandnb k7,k1,k2
        &[0xC4, 0xE1, 0xF5, 0x45, 0xE2][..],       // kord k4,k1,k2
        &[0xC4, 0xE1, 0xF4, 0x46, 0xCA][..],       // kxnorq k1,k1,k2
        &[0xC4, 0xE3, 0xF9, 0x33, 0xE3, 0x11][..], // kshiftlq k4,k3,17
        &[0xC4, 0xE1, 0xFB, 0x92, 0xC8][..],       // kmovq k1,rax
        &[0xC4, 0x61, 0xFB, 0x93, 0xCD][..],       // kmovq r9,k5
        &[0xC5, 0xF8, 0x99, 0xCA][..],             // ktestw k1,k2
        &[0xC5, 0xF8, 0x98, 0xCA][..],             // kortestw k1,k2
    ] {
        exact_opmask(bytes);
    }

    let bytes = [
        0xC5, 0xF8, 0x92, 0xC8, // kmovw k1,eax
        0xC5, 0xF8, 0x92, 0xD3, // kmovw k2,ebx
        0xC5, 0xF4, 0x41, 0xDA, // kandw k3,k1,k2
        0xF4,
    ];
    let memory = TestMemory::new(0x1000, bytes.to_vec());
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &memory, &mut context)
        .expect("complete opmask block must strictly lift");
    assert_eq!(block.ops.len(), 3);
    assert!(
        block
            .ops
            .iter()
            .all(|op| matches!(op.kind, OpKind::X86Opmask(_)))
    );
    let hlt_frontier = context.get_or_create_block(0x100C);
    assert!(matches!(
        block.terminator,
        Terminator::Branch { target } if target == hlt_frontier
    ));
}
