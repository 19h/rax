//! Complete 3DNow! suffix, addressing, and invalid-encoding coverage.

use super::*;
use crate::smir::lift::x86_64::*;

const ATOMIC_SUFFIXES: &[(u8, X86ThreeDNowKind)] = &[
    (0x0C, X86ThreeDNowKind::Pi2Fw),
    (0x0D, X86ThreeDNowKind::Pi2Fd),
    (0x1C, X86ThreeDNowKind::Pf2Iw),
    (0x1D, X86ThreeDNowKind::Pf2Id),
    (0x8A, X86ThreeDNowKind::PfNAcc),
    (0x8E, X86ThreeDNowKind::PfPNAcc),
    (0x90, X86ThreeDNowKind::PfCmpGe),
    (0x94, X86ThreeDNowKind::PfMin),
    (0x96, X86ThreeDNowKind::PfRcp),
    (0x97, X86ThreeDNowKind::PfRsqrt),
    (0x9A, X86ThreeDNowKind::PfSub),
    (0x9E, X86ThreeDNowKind::PfAdd),
    (0xA0, X86ThreeDNowKind::PfCmpGt),
    (0xA4, X86ThreeDNowKind::PfMax),
    (0xA6, X86ThreeDNowKind::PfRcpIt1),
    (0xA7, X86ThreeDNowKind::PfRsqIt1),
    (0xAA, X86ThreeDNowKind::PfSubR),
    (0xAE, X86ThreeDNowKind::PfAcc),
    (0xB0, X86ThreeDNowKind::PfCmpEq),
    (0xB4, X86ThreeDNowKind::PfMul),
    (0xB6, X86ThreeDNowKind::PfRcpIt2),
    (0xB7, X86ThreeDNowKind::PmulHrw),
];

const GENERIC_SUFFIXES: [u8; 2] = [0xBB, 0xBF];

#[test]
fn lift_3dnow_all_defined_suffixes_and_registers() {
    for &(suffix, expected) in ATOMIC_SUFFIXES {
        let result = lift_single(&[0x0F, 0x0F, 0xC1, suffix]).unwrap();
        assert_eq!(result.bytes_consumed, 4, "suffix {suffix:02X}");
        assert!(matches!(
            result.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86ThreeDNow {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        kind,
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                    ..
                }
            ] if *kind == expected
        ));
    }

    let avg = lift_single(&[0x0F, 0x0F, 0xC1, 0xBF]).unwrap();
    assert_eq!(avg.bytes_consumed, 4);
    assert!(matches!(
        avg.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    elem: VecElementType::I8,
                    lanes: 8,
                    op: VLaneOp::AvgRnd,
                    signed: false,
                    set_ovf: false,
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
                ..
            }
        ]
    ));

    let swap = lift_single(&[0x0F, 0x0F, 0xD1, 0xBB]).unwrap();
    assert_eq!(swap.bytes_consumed, 4);
    assert!(matches!(
        swap.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86PackedShuffleImm {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(2))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                    elem: VecElementType::I32,
                    imm: 1,
                    high_words: None,
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
                ..
            }
        ]
    ));

    let rex_ignored = lift_single(&[0x4F, 0x0F, 0x0F, 0xFF, 0xBF]).unwrap();
    assert_eq!(rex_ignored.bytes_consumed, 5);
    assert!(matches!(
        rex_ignored.ops.first().map(|op| &op.kind),
        Some(OpKind::VLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(7))),
            ..
        })
    ));
}

#[test]
fn lift_3dnow_memory_prefixes_and_fault_order() {
    let memory = lift_single(&[0x67, 0xF3, 0x0F, 0x0F, 0x54, 0x4B, 0x20, 0xBF]).unwrap();
    assert_eq!(memory.bytes_consumed, 8);
    let load = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                }
            )
        })
        .unwrap();
    let average = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLane {
                    op: VLaneOp::AvgRnd,
                    ..
                }
            )
        })
        .unwrap();
    let enter = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                }
            )
        })
        .unwrap();
    assert!(load < average && average < enter);

    for bytes in [
        &[0x66, 0x0F, 0x0F, 0xC1, 0x9E][..],
        &[0xF2, 0x0F, 0x0F, 0xC1, 0x9E][..],
        &[0xF3, 0x0F, 0x0F, 0xC1, 0x9E][..],
        &[0x67, 0x0F, 0x0F, 0xC1, 0x9E][..],
        &[0x48, 0x0F, 0x0F, 0xC1, 0x9E][..],
        &[0x64, 0x0F, 0x0F, 0xC1, 0x9E][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.first().map(|op| &op.kind),
            Some(OpKind::X86ThreeDNow {
                kind: X86ThreeDNowKind::PfAdd,
                ..
            })
        ));
    }

    assert!(matches!(
        lift_single(&[0x0F, 0x0F, 0xC1]),
        Err(LiftError::Incomplete { .. })
    ));
    for bytes in [
        &[0xF0, 0x0F, 0x0F, 0xC1, 0xBF][..],
        &[0xD5, 0x80, 0x0F, 0xC1, 0xBF][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    let reserved_escape = lift_single(&[0xD5, 0x00, 0x0F, 0x0F, 0xC1, 0xBF])
        .expect("REX2 followed by 0F is an explicit #UD");
    assert_invalid_opcode_trap(&reserved_escape, 3);
}

#[test]
fn lift_3dnow_every_unassigned_suffix_is_a_complete_invalid_opcode_trap() {
    // AMD APM volume 3 revision 3.37 section A.1.2 defines 24 suffixes and
    // leaves the other 232 values implementation-specific, including #UD.
    // RAX's configured direct profile rejects 0F 0F, so strict SMIR must expose
    // the same deterministic terminal result instead of an Unsupported barrier.
    for suffix in 0..=u8::MAX {
        if ATOMIC_SUFFIXES
            .iter()
            .any(|(defined, _)| *defined == suffix)
            || GENERIC_SUFFIXES.contains(&suffix)
        {
            continue;
        }

        let bytes = [0x0F, 0x0F, 0xC1, suffix];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("reserved suffix {suffix:02X}: {error:?}"));
        assert_invalid_opcode_trap(&result, bytes.len());
    }

    for bytes in [
        &[0x0F, 0x0F, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12, 0x00][..],
        &[0x67, 0xF3, 0x0F, 0x0F, 0x54, 0x4B, 0x20, 0x00][..],
    ] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("reserved memory suffix {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}
