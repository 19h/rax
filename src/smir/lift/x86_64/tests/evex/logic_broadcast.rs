//! EVEX VXORPS/VXORPD scalar-broadcast memory lifting coverage.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

fn xor_broadcast_bytes(
    elem: VecElementType,
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
    let f64 = elem == VecElementType::F64;
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x60
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x01,
        (u8::from(f64) << 7) | (((!source1) & 0x0F) << 3) | 0x04 | u8::from(f64),
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | 0x10
            | (if source1 & 16 == 0 { 0x08 } else { 0 })
            | mask,
        0x57,
        0x40 | ((destination & 7) << 3) | 0x03,
        0x01,
    ]
}

#[test]
fn lift_evex_broadcast_xor_matches_independent_llvm_23_encodings() {
    for (bytes, elem, width, destination, source1, mask, zeroing) in [
        (
            &[0x62, 0xF1, 0x6C, 0x58, 0x57, 0x0F][..],
            VecElementType::F32,
            VecWidth::V512,
            1,
            2,
            0,
            false,
        ),
        (
            &[0x62, 0x51, 0xAD, 0xBB, 0x57, 0x4B, 0x08][..],
            VecElementType::F64,
            VecWidth::V256,
            9,
            10,
            3,
            true,
        ),
        (
            &[0x62, 0x51, 0x04, 0x19, 0x57, 0x7E, 0xFC][..],
            VecElementType::F32,
            VecWidth::V128,
            15,
            15,
            1,
            false,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
        let memory_width = if elem == VecElementType::F32 {
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
                elem: actual_elem,
                lanes,
                ..
            } if actual_elem == elem && lanes == width.lanes(elem) as u8
        )));
        assert!(lifted.ops.iter().any(|op| {
            matches!(
                op.kind,
                OpKind::VXor {
                    src1,
                    width: actual_width,
                    ..
                } if actual_width == width
                    && match width {
                        VecWidth::V128 => src1
                            == VReg::Arch(ArchReg::X86(X86Reg::Xmm(source1))),
                        VecWidth::V256 => src1
                            == VReg::Arch(ArchReg::X86(X86Reg::Ymm(source1))),
                        VecWidth::V512 => src1
                            == VReg::Arch(ArchReg::X86(X86Reg::Zmm(source1))),
                        _ => false,
                    }
            ) && op.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: if elem == VecElementType::F32 {
                        X86SsePrefix::None
                    } else {
                        X86SsePrefix::OpSize
                    },
                    opcode: 0x57,
                    width,
                    w: elem == VecElementType::F64,
                })
        }));
        if mask == 0 {
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VXor { dst, .. }
                    if match width {
                        VecWidth::V128 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Xmm(destination))),
                        VecWidth::V256 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Ymm(destination))),
                        VecWidth::V512 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Zmm(destination))),
                        _ => false,
                    }
            )));
        } else {
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    cond: VReg::Virtual(_),
                    ..
                }
            )));
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst,
                    lane: 0,
                    elem: actual_elem,
                    ..
                } if actual_elem == elem
                    && match width {
                        VecWidth::V128 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Xmm(destination))),
                        VecWidth::V256 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Ymm(destination))),
                        VecWidth::V512 => dst
                            == VReg::Arch(ArchReg::X86(X86Reg::Zmm(destination))),
                        _ => false,
                    }
            )));
            assert_eq!(zeroing, bytes[3] & 0x80 != 0);
        }
    }
}

#[test]
fn lift_evex_broadcast_xor_covers_1_440_width_mask_alias_and_high_register_shapes() {
    let mut lifted_count = 0usize;
    for elem in [VecElementType::F32, VecElementType::F64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in [0, 15, 16, 31] {
                for source1 in [0, 15, 16, 31] {
                    for mask in 0..=7 {
                        for zeroing in [false, true] {
                            if mask == 0 && zeroing {
                                continue;
                            }
                            let bytes = xor_broadcast_bytes(
                                elem,
                                width,
                                destination,
                                source1,
                                mask,
                                zeroing,
                            );
                            let lifted = lift_single(&bytes)
                                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                            assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
                            let memory_ops = lifted
                                .ops
                                .iter()
                                .filter(|op| {
                                    matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. })
                                })
                                .count();
                            assert_eq!(memory_ops, 1, "{bytes:02X?}: {:#?}", lifted.ops);
                            assert!(lifted.ops.iter().any(|op| matches!(
                                op.kind,
                                OpKind::VBroadcast {
                                    elem: actual_elem,
                                    lanes,
                                    ..
                                } if actual_elem == elem
                                    && lanes == width.lanes(elem) as u8
                            )));
                            lifted_count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(lifted_count, 2 * 3 * 4 * 4 * 15);
}

#[test]
fn lift_evex_broadcast_xor_rejects_reserved_shapes_and_preserves_full_memory_form() {
    let valid = xor_broadcast_bytes(VecElementType::F32, VecWidth::V128, 1, 2, 1, false);
    let mut malformed = Vec::new();
    let mut register = valid.to_vec();
    register[5] |= 0xC0;
    register.truncate(6);
    malformed.push(register);
    let mut reserved_ll = valid;
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll.to_vec());
    let mut zero_k0 = valid;
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0.to_vec());
    let mut mismatched_w_pp = valid;
    mismatched_w_pp[2] ^= 0x80;
    malformed.push(mismatched_w_pp.to_vec());

    for bytes in malformed {
        assert!(
            matches!(
                lift_single(&bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved encoding {bytes:02X?}"
        );
    }

    let mut full_memory = valid;
    full_memory[3] &= !0x10;
    let lifted = lift_single(&full_memory).expect("EVEX VXORPS full-vector memory form");
    assert_eq!(
        lifted
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
        4,
        "full-vector XMM memory form must retain four lane loads"
    );
}
