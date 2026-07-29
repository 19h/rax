//! EVEX VPUNPCK*DQ/QDQ scalar-broadcast memory lifting coverage.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[derive(Clone, Copy, Debug)]
struct InterleaveKind {
    elem: VecElementType,
    high: bool,
    opcode: u8,
}

const KINDS: [InterleaveKind; 4] = [
    InterleaveKind {
        elem: VecElementType::I32,
        high: false,
        opcode: 0x62,
    },
    InterleaveKind {
        elem: VecElementType::I64,
        high: false,
        opcode: 0x6C,
    },
    InterleaveKind {
        elem: VecElementType::I32,
        high: true,
        opcode: 0x6A,
    },
    InterleaveKind {
        elem: VecElementType::I64,
        high: true,
        opcode: 0x6D,
    },
];

fn interleave_broadcast_bytes(
    kind: InterleaveKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 7] {
    let ll = match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!(),
    };
    let qword = kind.elem == VecElementType::I64;
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x60
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x01,
        (u8::from(qword) << 7) | (((!source1) & 0x0F) << 3) | 0x05,
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | 0x10
            | (if source1 & 16 == 0 { 0x08 } else { 0 })
            | mask,
        kind.opcode,
        0x40 | ((destination & 7) << 3) | 0x03,
        0x01,
    ]
}

fn vector(index: u8, width: VecWidth) -> VReg {
    match width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(index))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(index))),
        _ => unreachable!(),
    }
}

#[test]
fn lift_evex_broadcast_integer_interleave_matches_independent_llvm_23_encodings() {
    for (bytes, kind, width, destination, source1, mask, zeroing) in [
        (
            &[0x62, 0xF1, 0x6D, 0x58, 0x62, 0x0F][..],
            KINDS[0],
            VecWidth::V512,
            1,
            2,
            0,
            false,
        ),
        (
            &[0x62, 0x51, 0xAD, 0xBB, 0x6C, 0x4B, 0x08][..],
            KINDS[1],
            VecWidth::V256,
            9,
            10,
            3,
            true,
        ),
        (
            &[0x62, 0x51, 0x05, 0x19, 0x6A, 0x7E, 0xFC][..],
            KINDS[2],
            VecWidth::V128,
            15,
            15,
            1,
            false,
        ),
        (
            &[0x62, 0x61, 0xFD, 0xD7, 0x6D, 0x3C, 0x24][..],
            KINDS[3],
            VecWidth::V512,
            31,
            16,
            7,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        let memory_width = if kind.elem == VecElementType::I32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::Load { width, .. } | OpKind::PredLoad { width, .. }
                        if width == memory_width
                ))
                .count(),
            1,
            "{bytes:02X?}: {:#?}",
            lifted.ops
        );
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem,
                lanes,
                ..
            } if elem == kind.elem && lanes == width.lanes(kind.elem) as u8
        )));
        assert!(lifted.ops.iter().any(|op| {
            matches!(
                op.kind,
                OpKind::VInterleave {
                    dst,
                    src1,
                    elem,
                    lanes,
                    block_lanes,
                    high,
                    ..
                } if src1 == vector(source1, width)
                    && elem == kind.elem
                    && lanes == width.lanes(kind.elem) as u8
                    && block_lanes == (16 / kind.elem.bytes()) as u8
                    && high == kind.high
                    && (mask != 0 || dst == vector(destination, width))
            ) && op.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: kind.opcode,
                    width,
                    w: kind.elem == VecElementType::I64,
                })
        }));
        assert_eq!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
            mask != 0,
            "{bytes:02X?}"
        );
        assert_eq!(zeroing, bytes[3] & 0x80 != 0);
    }
}

#[test]
fn lift_evex_broadcast_integer_interleave_covers_2_880_shapes() {
    let mut lifted_count = 0usize;
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in [0, 15, 16, 31] {
                for source1 in [0, 15, 16, 31] {
                    for mask in 0..=7 {
                        for zeroing in [false, true] {
                            if mask == 0 && zeroing {
                                continue;
                            }
                            let bytes = interleave_broadcast_bytes(
                                kind,
                                width,
                                destination,
                                source1,
                                mask,
                                zeroing,
                            );
                            let lifted = lift_single(&bytes)
                                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                            assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                            assert_eq!(
                                lifted
                                    .ops
                                    .iter()
                                    .filter(|op| matches!(
                                        op.kind,
                                        OpKind::Load { .. } | OpKind::PredLoad { .. }
                                    ))
                                    .count(),
                                1,
                                "{bytes:02X?}: {:#?}",
                                lifted.ops
                            );
                            assert!(lifted.ops.iter().any(|op| matches!(
                                op.kind,
                                OpKind::VInterleave {
                                    elem,
                                    high,
                                    block_lanes,
                                    ..
                                } if elem == kind.elem
                                    && high == kind.high
                                    && block_lanes == (16 / kind.elem.bytes()) as u8
                            )));
                            lifted_count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(lifted_count, KINDS.len() * 3 * 4 * 4 * 15);
}

#[test]
fn lift_evex_broadcast_integer_interleave_scales_disp8_by_scalar_tuple() {
    for kind in KINDS {
        let mut bytes = interleave_broadcast_bytes(kind, VecWidth::V512, 1, 2, 1, false);
        bytes[6] = 0xFE;
        let lifted = lift_single(&bytes).unwrap();
        let expected_offset = -2 * i64::from(kind.elem.bytes());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::PredLoad {
                addr: Address::BaseOffset {
                    base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                    offset,
                    disp_size: DispSize::Disp8,
                },
                ..
            } if offset == expected_offset
        )));
    }
}

#[test]
fn lift_evex_broadcast_integer_interleave_rejects_reserved_shapes() {
    for kind in KINDS {
        let valid = interleave_broadcast_bytes(kind, VecWidth::V128, 1, 2, 1, false);
        let mut malformed = Vec::new();

        let mut register = valid.to_vec();
        register[5] |= 0xC0;
        register.truncate(6);
        malformed.push(register);

        let mut wrong_w = valid;
        wrong_w[2] ^= 0x80;
        malformed.push(wrong_w.to_vec());

        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        malformed.push(reserved_ll.to_vec());

        let mut zero_k0 = valid;
        zero_k0[3] = (zero_k0[3] & !7) | 0x80;
        malformed.push(zero_k0.to_vec());

        for bytes in malformed {
            assert!(
                matches!(
                    lift_single(&bytes),
                    Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
                ),
                "accepted reserved encoding {bytes:02X?}"
            );
        }
    }

    for opcode in [0x60, 0x61, 0x68, 0x69] {
        let mut byte_or_word = interleave_broadcast_bytes(KINDS[0], VecWidth::V128, 1, 2, 1, false);
        byte_or_word[4] = opcode;
        assert!(
            matches!(
                lift_single(&byte_or_word),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted EVEX.b byte/word interleave {byte_or_word:02X?}"
        );
    }
}

#[test]
fn lift_evex_integer_interleave_preserves_full_vector_memory_form() {
    for kind in KINDS {
        let mut full = interleave_broadcast_bytes(kind, VecWidth::V256, 1, 2, 1, false);
        full[3] &= !0x10;
        let lifted = lift_single(&full).expect("full-vector interleave memory form");
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(
            !lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
        );
    }
}
