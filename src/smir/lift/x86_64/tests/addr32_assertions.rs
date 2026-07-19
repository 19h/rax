//! Shared structural assertions for explicit x86 addr32 memory operands.

use crate::smir::ir::types::{Address, ArchReg, VReg, X86Reg};

pub(super) fn inner(addr: &Address) -> &Address {
    let Address::X86Addr32(inner) = addr else {
        panic!("expected explicit x86 addr32 address, got {addr:?}")
    };
    inner
}

pub(super) fn base_offset(addr: &Address, base: X86Reg, offset: i64) {
    assert!(matches!(
        inner(addr),
        Address::BaseOffset {
            base: VReg::Arch(ArchReg::X86(actual)),
            offset: actual_offset,
            ..
        } if *actual == base && *actual_offset == offset
    ));
}

pub(super) fn sib(addr: &Address, base: Option<X86Reg>, index: X86Reg, scale: u8, disp: i32) {
    let Address::BaseIndexScale {
        base: actual_base,
        index: VReg::Arch(ArchReg::X86(actual_index)),
        scale: actual_scale,
        disp: actual_disp,
        ..
    } = inner(addr)
    else {
        panic!("expected addr32 SIB address, got {addr:?}")
    };
    let actual_base = actual_base.map(|reg| match reg {
        VReg::Arch(ArchReg::X86(reg)) => reg,
        other => panic!("expected x86 SIB base, got {other:?}"),
    });
    assert_eq!(actual_base, base);
    assert_eq!(*actual_index, index);
    assert_eq!(*actual_scale, scale);
    assert_eq!(*actual_disp, disp);
}

pub(super) fn direct(addr: &Address, reg: X86Reg) {
    assert!(matches!(
        inner(addr),
        Address::Direct(VReg::Arch(ArchReg::X86(actual))) if *actual == reg
    ));
}
