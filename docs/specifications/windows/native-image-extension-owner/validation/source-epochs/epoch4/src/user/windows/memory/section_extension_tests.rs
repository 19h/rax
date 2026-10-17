use super::*;
use crate::user::mm::SpaceConfig;
use crate::user::windows::object_namespace::{ImageSection, image_extension_tests};

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
fn image_extension_owner_maps_full_views_and_releases_every_tail_page() {
    let mut vm = memory();
    let image = image_extension_tests::template(0x7000_0000);
    let section = ImageSection::with_test_image(image.clone());
    for base in [0x5000_0000, 0x5100_0000] {
        vm.map_section_image(
            section.clone(),
            &image,
            base,
            base,
            16384,
            0,
            8192,
            false,
            false,
        )
        .unwrap();
        for (offset, fill) in [(8192, 0xA5), (12288, 0x5A)] {
            let q = vm.query(base + offset).unwrap();
            assert_eq!(
                (q.allocation_base, q.kind, q.state, q.protect),
                (base, mem::IMAGE, mem::COMMIT, prot::EXECUTE_READ)
            );
            assert_eq!(vm.space().u8(base + offset).unwrap(), fill);
            assert_eq!(
                vm.section_image_information(base + offset),
                Some((base, 8192, 0x40))
            );
            assert_eq!(
                vm.section_image_extension(base + offset, 0),
                Some((8192, 4096))
            );
            assert_eq!(
                vm.section_image_extension(base + offset, 1),
                Some((12288, 4096))
            );
            for protection in [
                prot::NOACCESS,
                prot::READONLY,
                prot::READWRITE,
                prot::EXECUTE_READ,
            ] {
                assert_eq!(
                    vm.protect(base + offset, 4096, protection),
                    Err(VmError::NotCommitted)
                );
                assert_eq!(vm.query(base + offset).unwrap().protect, prot::EXECUTE_READ);
            }
        }
        assert_eq!(
            vm.protect(base + 4096, 8192, prot::READONLY),
            Err(VmError::NotCommitted)
        );
        assert_eq!(vm.query(base + 4096).unwrap().protect, prot::WRITECOPY);
    }
    vm.release(0x5000_0000).unwrap();
    assert!(vm.section_image_extension(0x5000_2000, 0).is_none());
    assert_eq!(vm.query(0x5000_3000).unwrap().state, mem::FREE);
    assert_eq!(vm.query(0x5100_3000).unwrap().state, mem::COMMIT);
    vm.release(0x5100_0000).unwrap();
}
#[test]
fn image_extension_owner_omits_partial_tails_and_rejects_insufficient_reservations() {
    let mut vm = memory();
    let image = image_extension_tests::template(0x7000_0000);
    let section = ImageSection::with_test_image(image.clone());
    assert_eq!(
        vm.map_section_image(
            section.clone(),
            &image,
            0x5000_0000,
            0x5000_0000,
            8192,
            0,
            8192,
            false,
            false
        ),
        Err(VmError::InvalidParameter)
    );
    assert_eq!(vm.query(0x5000_0000).unwrap().state, mem::FREE);
    vm.map_section_image(
        section,
        &image,
        0x5000_0000,
        0x5000_0000,
        4096,
        0,
        4096,
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        vm.section_image_information(0x5000_0000),
        Some((0x5000_0000, 8192, 1))
    );
    assert!(vm.section_image_extension(0x5000_0000, 0).is_none());
    assert_eq!(vm.query(0x5000_1000).unwrap().state, mem::FREE);
}

#[test]
fn image_extension_owner_large_view_does_not_publish_an_extension() {
    let mut vm = memory();
    let image = image_extension_tests::template(0x7000_0000);
    let section = ImageSection::with_test_image(image.clone());
    vm.map_section_image(
        section,
        &image,
        0x5000_0000,
        0x5000_0000,
        0x200000,
        0,
        8192,
        true,
        false,
    )
    .unwrap();
    assert_eq!(
        vm.section_image_information(0x5000_2000),
        Some((0x5000_0000, 8192, 0))
    );
    assert!(vm.section_image_extension(0x5000_2000, 0).is_none());
    assert_eq!(vm.space().u8(0x5000_2000).unwrap(), 0);
}
