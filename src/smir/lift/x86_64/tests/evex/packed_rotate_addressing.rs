//! Effective-address coverage for EVEX packed rotates and immediate shifts.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

fn first_memory_address(bytes: &[u8]) -> Address {
    let lifted = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
    lifted
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::VLoad { addr, .. }
            | OpKind::Lea { addr, .. } => Some(addr.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{bytes:02X?}: no memory address in {:#?}", lifted.ops))
}

fn sib_with_disp_size(
    base: X86Reg,
    index: X86Reg,
    scale: u8,
    disp: i32,
    disp_size: DispSize,
) -> Address {
    Address::BaseIndexScale {
        base: Some(VReg::Arch(ArchReg::X86(base))),
        index: VReg::Arch(ArchReg::X86(index)),
        scale,
        disp,
        disp_size,
    }
}

fn sib8(base: X86Reg, index: X86Reg, scale: u8, disp: i32) -> Address {
    sib_with_disp_size(base, index, scale, disp, DispSize::Disp8)
}

fn sib32(base: X86Reg, index: X86Reg, scale: u8, disp: i32) -> Address {
    sib_with_disp_size(base, index, scale, disp, DispSize::Disp32)
}

#[test]
fn immediate_packed_rotate_preserves_apx_b4_x4_memory_address_bits() {
    // LLVM 23 encodings for VPRORD xmm2,[base+index*4+16],7. The EVEX
    // full-vector tuple scales the encoded disp8=1 to 16 bytes.
    for (bytes, expected) in [
        (
            &[0x62, 0xF9, 0x6D, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07][..],
            sib8(X86Reg::R16, X86Reg::Rcx, 4, 16),
        ),
        (
            &[0x62, 0xF1, 0x69, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07][..],
            sib8(X86Reg::Rax, X86Reg::R17, 4, 16),
        ),
        (
            &[0x62, 0xF9, 0x69, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07][..],
            sib8(X86Reg::R16, X86Reg::R17, 4, 16),
        ),
    ] {
        assert_eq!(first_memory_address(bytes), expected, "{bytes:02X?}");
    }
}

#[test]
fn variable_packed_rotate_preserves_apx_b4_x4_memory_address_bits() {
    // LLVM 23 encodings for VPRORVD ymm23,ymm22,[base+index*8+48].
    for (bytes, expected) in [
        (
            &[
                0x62, 0xEA, 0x4D, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
            ][..],
            sib32(X86Reg::R20, X86Reg::Rbp, 8, 48),
        ),
        (
            &[
                0x62, 0xE2, 0x49, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
            ][..],
            sib32(X86Reg::Rsp, X86Reg::R21, 8, 48),
        ),
        (
            &[
                0x62, 0xEA, 0x49, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
            ][..],
            sib32(X86Reg::R20, X86Reg::R21, 8, 48),
        ),
    ] {
        assert_eq!(first_memory_address(bytes), expected, "{bytes:02X?}");
    }
}

#[test]
fn packed_rotate_and_shift_preserve_segment_and_address_size_overrides() {
    let fs_rotate = [0x64, 0x62, 0xF1, 0x6D, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07];
    assert_eq!(
        first_memory_address(&fs_rotate),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
            scale: 4,
            disp: 16,
        }
    );

    let addr32_rotate = [0x67, 0x62, 0xF1, 0x6D, 0x08, 0x72, 0x44, 0x88, 0x01, 0x07];
    assert_eq!(
        first_memory_address(&addr32_rotate),
        Address::X86Addr32(Box::new(sib8(X86Reg::Rax, X86Reg::Rcx, 4, 16)))
    );

    // The same immediate-group lifter owns VPSRLD. Cover it explicitly so
    // fixing rotates cannot regress the other operations in the group.
    let gs_shift = [0x65, 0x62, 0xF1, 0x6D, 0x08, 0x72, 0x54, 0x88, 0x01, 0x07];
    assert_eq!(
        first_memory_address(&gs_shift),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
            scale: 4,
            disp: 16,
        }
    );

    let addr32_shift = [0x67, 0x62, 0xF1, 0x6D, 0x08, 0x72, 0x54, 0x88, 0x01, 0x07];
    assert_eq!(
        first_memory_address(&addr32_shift),
        Address::X86Addr32(Box::new(sib8(X86Reg::Rax, X86Reg::Rcx, 4, 16)))
    );
}

#[test]
fn variable_rotate_broadcast_preserves_apx_address_and_tuple_scaling() {
    // VPROLVQ zmm27{k5}{z},zmm26,[r24+r25*4+64]{1to8}. The encoded
    // disp8=8 is scaled by the 8-byte broadcast tuple.
    let bytes = [0x62, 0x0A, 0xA9, 0xD5, 0x15, 0x5C, 0x88, 0x08];
    assert_eq!(
        first_memory_address(&bytes),
        sib8(X86Reg::R24, X86Reg::R25, 4, 64)
    );
}

#[test]
fn variable_rotate_preserves_segment_and_address_size_overrides() {
    let fs_rotate = [
        0x64, 0x62, 0xEA, 0x49, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
    ];
    assert_eq!(
        first_memory_address(&fs_rotate),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::R20))),
            index: Some(VReg::Arch(ArchReg::X86(X86Reg::R21))),
            scale: 8,
            disp: 48,
        }
    );

    let addr32_rotate = [
        0x67, 0x62, 0x82, 0x4D, 0x20, 0x14, 0xBC, 0xEC, 0x30, 0x00, 0x00, 0x00,
    ];
    assert_eq!(
        first_memory_address(&addr32_rotate),
        Address::X86Addr32(Box::new(Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::R12))),
            index: VReg::Arch(ArchReg::X86(X86Reg::R13)),
            scale: 8,
            disp: 48,
            disp_size: DispSize::Disp32,
        }))
    );
}
