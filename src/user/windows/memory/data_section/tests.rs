use super::*;
use crate::user::mm::SpaceConfig;
use crate::user::windows::object_namespace::data::tests::sample;

fn memory() -> VirtualMemory {
    VirtualMemory::new(
        AddressSpace::new(SpaceConfig {
            va_limit: 1 << 32,
            arena_bytes: 16 << 20,
            reserved_phys: vec![],
        })
        .unwrap(),
        65536,
        1 << 32,
    )
}

#[test]
fn selected_data_views_preserve_holes_opaque_bytes_and_owned_lifetime() {
    let mut vm = memory();
    let section = sample();
    let weak = Arc::downgrade(&section);
    for base in [0x5000_0000, 0x5100_0000] {
        vm.map_data_section(section.clone(), base, 0, section.size, prot::READONLY)
            .unwrap();
        let first = vm.query(base).unwrap();
        assert_eq!(
            (first.kind, first.state, first.protect, first.size),
            (mem::MAPPED, mem::COMMIT, prot::READONLY, 20480)
        );
        let hole = vm.query(base + 20480).unwrap();
        assert_eq!(
            (hole.kind, hole.state, hole.protect, hole.size),
            (mem::MAPPED, mem::RESERVE, 0, 1028096)
        );
        assert_eq!(vm.space().u64(base + 40).unwrap(), 0x0000_7FF5_F57C_0000);
        assert!(vm.space().u8(base + 20480).is_err());
        assert!(vm.space().w8(base, 0).is_err());
        assert_eq!(
            vm.free(base, 0, mem::RELEASE),
            Err(VmError::CannotDeleteSection)
        );
        assert_eq!(vm.decommit(base, 4096), Err(VmError::CannotDeleteSection));
    }
    drop(section);
    assert!(weak.upgrade().is_some());
    for base in [0x5000_0000, 0x5100_0000] {
        vm.release(base).unwrap();
    }
    assert!(weak.upgrade().is_none());
}

#[test]
fn selected_data_copy_on_write_is_private_and_protection_cannot_gain_shared_write() {
    let mut vm = memory();
    let section = sample();
    vm.map_data_section(
        section.clone(),
        0x5000_0000,
        0,
        section.size,
        prot::WRITECOPY,
    )
    .unwrap();
    vm.map_data_section(
        section.clone(),
        0x5100_0000,
        0,
        section.size,
        prot::READONLY,
    )
    .unwrap();
    vm.space().w8(0x5000_0000, 0x33).unwrap();
    assert_eq!(vm.space().u8(0x5100_0000).unwrap(), 0xA5);
    assert_eq!(section.regions[0].bytes.as_ref().unwrap()[0], 0xA5);
    assert_eq!(
        vm.protect(0x5100_0000, 4096, prot::WRITECOPY).unwrap(),
        prot::READONLY
    );
    assert_eq!(
        vm.protect(0x5100_0000, 4096, prot::READWRITE),
        Err(VmError::SectionProtection)
    );
    assert_eq!(
        vm.protect(0x5100_0000, 4096, prot::EXECUTE_READ),
        Err(VmError::SectionProtection)
    );
    assert_eq!(
        vm.protect(0x5100_0000, 4096, prot::READONLY | prot::NOCACHE),
        Err(VmError::InvalidParameter4)
    );
    assert_eq!(
        vm.protect(0x5100_0000, 4096, prot::READONLY | prot::GUARD)
            .unwrap(),
        prot::WRITECOPY
    );
}

#[test]
fn selected_data_partial_reserved_views_and_failed_map_are_exact() {
    let mut vm = memory();
    let section = sample();
    vm.map_data_section(section.clone(), 0x5000_0000, 65536, 4096, prot::READONLY)
        .unwrap();
    assert_eq!(vm.query(0x5000_0000).unwrap().state, mem::RESERVE);
    assert!(vm.space().u8(0x5000_0000).is_err());
    assert_eq!(
        vm.map_data_section(section.clone(), 0x5000_0000, 0, 4096, prot::READONLY),
        Err(VmError::Conflicting)
    );
    assert_eq!(
        vm.map_data_section(section.clone(), 0x5100_0001, 0, 4096, prot::READONLY),
        Err(VmError::InvalidParameter)
    );
    assert_eq!(
        vm.map_data_section(
            section.clone(),
            0x5100_0000,
            section.size,
            4096,
            prot::READONLY
        ),
        Err(VmError::InvalidParameter)
    );
    assert!(vm.is_free(0x5100_0000, section.size));
}
