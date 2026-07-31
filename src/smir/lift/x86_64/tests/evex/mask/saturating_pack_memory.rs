//! EVEX saturating-pack memory-source lifting tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

fn memory_address(bytes: &[u8]) -> Address {
    let lifted = lift_single(bytes).unwrap_or_else(|error| {
        panic!("failed to lift EVEX saturating pack {bytes:02X?}: {error:?}")
    });
    let addresses = lifted
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => Some(addr.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        addresses.len(),
        1,
        "expected one complete E4NF memory access for {bytes:02X?}: {:?}",
        lifted.ops
    );
    addresses[0].clone()
}

#[test]
fn lift_evex_saturating_pack_memory_uses_one_complete_e4nf_access() {
    for (ll, width) in [
        (0x00, VecWidth::V128),
        (0x20, VecWidth::V256),
        (0x40, VecWidth::V512),
    ] {
        for (map, opcode, src_elem, to_unsigned) in [
            (0xF1, 0x63, VecElementType::I16, false),
            (0xF1, 0x67, VecElementType::I16, true),
            (0xF1, 0x6B, VecElementType::I32, false),
            (0xF2, 0x2B, VecElementType::I32, true),
        ] {
            for masked in [false, true] {
                let p2 = ll | 0x08 | u8::from(masked);
                let bytes = [0x62, map, 0x75, p2, opcode, 0x00];
                let lifted = lift_single(&bytes).unwrap();
                assert_eq!(lifted.bytes_consumed, bytes.len());
                assert_eq!(
                    lifted
                        .ops
                        .iter()
                        .filter(|op| matches!(op.kind, OpKind::VLoad { width: actual, .. } if actual == width))
                        .count(),
                    1,
                    "full E4NF source must be one {width:?} load: {bytes:02X?}"
                );
                assert!(
                    !lifted
                        .ops
                        .iter()
                        .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
                    "destination writemask suppressed E4NF source: {bytes:02X?}"
                );
                assert!(lifted.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VPackSat {
                        src_elem: actual_elem,
                        to_unsigned: actual_unsigned,
                        ..
                    } if actual_elem == src_elem && actual_unsigned == to_unsigned
                )));

                if src_elem == VecElementType::I32 {
                    let broadcast_bytes = [0x62, map, 0x75, p2 | 0x10, opcode, 0x00];
                    let broadcast = lift_single(&broadcast_bytes).unwrap();
                    assert_eq!(
                        broadcast
                            .ops
                            .iter()
                            .filter(|op| matches!(
                                op.kind,
                                OpKind::Load {
                                    width: MemWidth::B4,
                                    ..
                                }
                            ))
                            .count(),
                        1,
                        "E4NF broadcast must read one 4-byte scalar: {broadcast_bytes:02X?}"
                    );
                    assert!(
                        !broadcast
                            .ops
                            .iter()
                            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                    );
                    assert!(broadcast.ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::VBroadcast {
                            elem: VecElementType::I32,
                            lanes,
                            ..
                        } if u32::from(lanes) == width.lanes(VecElementType::I32)
                    )));
                }
            }
        }
    }
}

#[test]
fn lift_evex_saturating_pack_memory_preserves_tuple_and_address_prefixes() {
    let full = lift_single(&[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x40, 0x01]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
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

    let broadcast = lift_single(&[0x62, 0xF2, 0x75, 0x59, 0x2B, 0x40, 0x10]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B4,
            ..
        }
    )));

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    assert_eq!(
        memory_address(&[0x67, 0x62, 0xF1, 0x75, 0x49, 0x63, 0x00]),
        Address::X86Addr32(Box::new(Address::Direct(rax)))
    );
    assert_eq!(
        memory_address(&[0x64, 0x62, 0xF1, 0x75, 0x49, 0x63, 0x00]),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(rax),
            index: None,
            scale: 1,
            disp: 0,
        }
    );
}

#[test]
fn lift_evex_saturating_pack_memory_preserves_apx_base_and_index_extensions() {
    for (bytes, expected_address) in [
        (
            &[0x62, 0xF9, 0x75, 0x49, 0x63, 0x00][..],
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R16))),
        ),
        (
            &[0x62, 0xF1, 0x71, 0x49, 0x63, 0x04, 0x20][..],
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R20)),
                scale: 1,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.first(),
            Some(SmirOp {
                kind: OpKind::X86RequireApx,
                ..
            })
        ));
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                .count(),
            1
        );
        assert_eq!(memory_address(bytes), expected_address);
        for (index, op) in lifted.ops.iter().enumerate() {
            assert_eq!(op.id, OpId(index as u16));
        }
    }
}
