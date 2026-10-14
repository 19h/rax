use super::*;
use crate::user::mm::SpaceConfig;
use crate::user::windows::object_namespace::{ImageSection, image_tests};

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
fn selected_image_views_own_private_bytes_and_keep_section_after_handle_close() {
    let mut vm = memory();
    let image = image_tests::template(0x7000_0000);
    let section = ImageSection::with_test_image(image.clone());
    let weak = Arc::downgrade(&section);
    for base in [0x5000_0000, 0x5100_0000] {
        vm.map_section_image(
            section.clone(),
            &image,
            base,
            base,
            8192,
            0,
            8192,
            false,
            false,
        )
        .unwrap();
        let header = vm.query(base).unwrap();
        assert_eq!(
            (
                header.kind,
                header.state,
                header.protect,
                header.allocation_protect
            ),
            (
                mem::IMAGE,
                mem::COMMIT,
                prot::READONLY,
                prot::EXECUTE_WRITECOPY
            )
        );
        assert_eq!(vm.query(base + 4096).unwrap().protect, prot::WRITECOPY);
        assert_eq!(vm.space().u64(base + 4096).unwrap(), 0x7000_1234);
    }
    drop(section);
    assert!(weak.upgrade().is_some());
    vm.space().w64(0x5000_1000, 0xABCD).unwrap();
    assert_eq!(vm.space().u64(0x5100_1000).unwrap(), 0x7000_1234);
    assert_eq!(&image.bytes[4096..4104], &0x7000_1234u64.to_le_bytes());
    for base in [0x5000_0000, 0x5100_0000] {
        assert_eq!(
            vm.free(base, 0, mem::RELEASE),
            Err(VmError::CannotDeleteSection)
        );
        assert_eq!(vm.decommit(base, 4096), Err(VmError::CannotDeleteSection));
        assert_eq!(
            vm.commit(base, 4096, prot::READWRITE),
            Err(VmError::AlreadyCommitted)
        );
        vm.release(base).unwrap();
        assert!(vm.section_view(base).is_none());
        assert_eq!(vm.query(base).unwrap().state, mem::FREE);
    }
    assert!(weak.upgrade().is_none());
}
#[test]
fn partial_and_large_section_views_report_logical_geometry_and_rebase_before_slice() {
    let mut vm = memory();
    let image = image_tests::template_sized(0x7060_0000, 0x30000);
    let section = ImageSection::with_test_image(image.clone());
    vm.map_section_image(
        section.clone(),
        &image,
        0x5000_0000,
        0x5000_0000,
        0x2000,
        0,
        0x2000,
        false,
        true,
    )
    .unwrap();
    assert_eq!(vm.space().u64(0x5000_1000).unwrap(), 0x5000_1234);
    assert_eq!(
        vm.section_image_information(0x5000_1000),
        Some((0x5000_0000, 0x30000, 1))
    );
    vm.map_section_image(
        section.clone(),
        &image,
        0x5100_0000,
        0x5100_0000,
        0x20000,
        0x10000,
        0x20000,
        false,
        false,
    )
    .unwrap();
    assert_eq!(vm.query(0x5100_0000).unwrap().protect, prot::WRITECOPY);
    assert_eq!(
        vm.section_image_information(0x5100_0000),
        Some((0x5100_0000, 0x30000, 1))
    );
    vm.map_section_image(
        section,
        &image,
        0x5200_0000,
        0x5206_0000,
        0x200000,
        0,
        0x30000,
        true,
        false,
    )
    .unwrap();
    assert_eq!(
        vm.section_image_information(0x5200_0000),
        Some((0x5206_0000, 0x30000, 0))
    );
    assert_eq!(
        vm.query(0x5200_0000).unwrap().protect,
        prot::EXECUTE_WRITECOPY
    );
    assert_eq!(vm.space().u64(0x5206_1000).unwrap(), 0x7060_1234);
    assert_eq!(vm.space().u64(0x5200_0000).unwrap(), 0);
}
#[test]
fn selected_mapping_rejects_invalid_geometry_before_reservation() {
    let mut vm = memory();
    let image = image_tests::template(0x7000_0000);
    let section = ImageSection::with_test_image(image.clone());
    for (base, address, allocation, offset, size) in [
        (0x5000_0001, 0x5000_0001, 8192, 0, 8192),
        (0x5000_0000, 0x4FFF_0000, 8192, 0, 8192),
        (0x5000_0000, 0x5000_0000, 4096, 0, 8192),
        (0x5000_0000, 0x5000_0000, 8192, u64::MAX, 8192),
        (0x5000_0000, 0x5000_0000, 8192, 0, 0),
    ] {
        assert_eq!(
            vm.map_section_image(
                section.clone(),
                &image,
                base,
                address,
                allocation,
                offset,
                size,
                false,
                false
            ),
            Err(VmError::InvalidParameter)
        );
        assert!(vm.allocation(0x5000_0000).is_none());
        assert!(vm.section_view(0x5000_0000).is_none());
    }
    assert_eq!(vm.committed_bytes(), 0);
}
