//! Partial images preserve full image metadata and view-owned file lifetime.
use super::*;

#[test]
fn native_section_file_partial_maximum_maps_rounded_extent_and_pins_file_after_close() {
    for arch in WinArch::ALL {
        for requested in [0, 1, 4096] {
            let directory = Directory::new();
            let path = directory.0.join("image.dll");
            std::fs::write(&path, image(arch)).unwrap();
            let full = u64::from(PeImage::parse(image(arch)).unwrap().headers().size_of_image);
            let (mut process, mut thread) = fixture(arch, "NtCreateSection", SERVICE);
            let p = process.state_mut();
            let baseline = p.objects.handle_count();
            let file = insert_file(p, &path, 0x100021);
            let b = scratch(p);
            p.space.w64(b + 64, requested).unwrap();
            assert_eq!(
                call(p, &mut thread, &[b, 0xD, 0, b + 64, 0x10, 0x01000000, file]),
                0
            );
            let h = get(p, b);
            let Object::ImageSection(section) = p.objects.get(h).unwrap() else {
                panic!("image section")
            };
            assert_eq!(section.size, if requested == 0 { full } else { requested });
            assert_eq!(section.image_size, full);
            let weak = Arc::downgrade(section);
            p.objects.close(file).unwrap();
            let module = p.modules.nt_services.as_ref().unwrap().0;
            p.modules.nt_services = Some((module, table(arch, "NtMapViewOfSection", 0x28)));
            ptr(p, b, 0x50000000);
            ptr(p, b + 16, 0);
            assert_eq!(
                super::super::super::section_map_tests::call(
                    p,
                    &mut thread,
                    &[h, arch.ptr(u64::MAX), b, 0, 0, 0, b + 16, 1, 0, 2]
                ),
                STATUS_IMAGE_NOT_AT_BASE
            );
            let extent = if requested == 0 {
                full
            } else {
                (requested + 4095) & !4095
            };
            assert_eq!((get(p, b), get(p, b + 16)), (0x50000000, extent));
            assert_eq!(p.vm.allocation(0x50000000).unwrap().size, extent);
            assert_eq!(
                p.vm.section_image_information(0x50000000),
                Some((0x50000000, full, u32::from(extent != full)))
            );
            p.objects.close(h).unwrap();
            assert_eq!(p.objects.handle_count(), baseline);
            assert!(weak.upgrade().is_some(), "mapped view pins the Section");
            assert_eq!(p.space.bytes(0x50000000, 2).unwrap(), b"MZ");
            p.modules.nt_services = Some((module, table(arch, "NtUnmapViewOfSection", 0x28)));
            assert_eq!(
                super::super::super::section_map_tests::call(
                    p,
                    &mut thread,
                    &[arch.ptr(u64::MAX), 0x50000000]
                ),
                0
            );
            assert!(weak.upgrade().is_none(), "final view releases the Section");
        }
    }
}

#[test]
fn native_section_file_reduced_handle_grants_control_construction_before_backing() {
    for arch in WinArch::ALL {
        let directory = Directory::new();
        let path = directory.0.join("image.dll");
        std::fs::write(&path, image(arch)).unwrap();
        let (mut process, mut thread) = fixture(arch, "NtCreateSection", SERVICE);
        let p = process.state_mut();
        let b = scratch(p);
        for access in [0, 1, 0x20, 0x21, 0x100021, 0x80000000, 0x20000000] {
            let file = insert_file(p, &path, access);
            for (page, required) in [
                (2, 1),
                (4, 3),
                (8, 1),
                (0x10, 0x20),
                (0x20, 0x21),
                (0x40, 0x23),
                (0x80, 0x21),
            ] {
                let effective = match access {
                    0x80000000 => 0x120089,
                    0x20000000 => 0x1200A0,
                    _ => access,
                };
                let expected = if effective & required == required {
                    0
                } else {
                    STATUS_ACCESS_DENIED
                };
                assert_eq!(
                    call(p, &mut thread, &[b, 0xD, 0, 0, page, 0x01000000, file]),
                    expected,
                    "{arch} handle={access:X} PAGE={page:X}"
                );
                if expected == 0 {
                    p.objects.close(get(p, b)).unwrap();
                }
            }
            p.objects.close(file).unwrap();
        }
    }
}

#[test]
fn native_section_file_shared_writable_image_is_an_explicit_boundary_without_publication() {
    for arch in WinArch::ALL {
        let directory = Directory::new();
        let path = directory.0.join("shared.dll");
        let mut bytes = image(arch).to_vec();
        let pe = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
        let optional = usize::from(u16::from_le_bytes(
            bytes[pe + 20..pe + 22].try_into().unwrap(),
        ));
        let flags = pe + 24 + optional + 36;
        let original = u32::from_le_bytes(bytes[flags..flags + 4].try_into().unwrap());
        bytes[flags..flags + 4]
            .copy_from_slice(&((original & !0x20000000) | 0xD0000000).to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let (mut process, mut thread) = fixture(arch, "NtCreateSection", SERVICE);
        let p = process.state_mut();
        let file = insert_file(p, &path, 0x100021);
        let b = scratch(p);
        ptr(p, b, 0xA5A5A5A5);
        let before = (p.objects.handle_count(), p.objects.iter().count());
        let outcome = invoke(p, &mut thread, &[b, 0xD, 0, 0, 0x10, 0x1000000, file]);
        assert!(
            matches!(outcome,Outcome::Fail(ref reason) if reason.contains("shared writable section coherence")),
            "{arch} {outcome:?}"
        );
        assert_eq!(get(p, b), 0xA5A5A5A5);
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
        p.objects.close(file).unwrap();
    }
}

#[test]
fn native_section_file_creation_preserves_retained_file_cursor() {
    use std::io::{Seek, SeekFrom};
    for arch in WinArch::ALL {
        let directory = Directory::new();
        let path = directory.0.join("image.dll");
        std::fs::write(&path, image(arch)).unwrap();
        let (mut process, mut t) = fixture(arch, "NtCreateSection", SERVICE);
        let p = process.state_mut();
        let file = insert_file(p, &path, 0x100021);
        let b = scratch(p);
        let Object::File(open) = p.objects.get(file).unwrap() else {
            panic!("file")
        };
        let mut host = open.host.as_ref().unwrap();
        assert_eq!(host.seek(SeekFrom::Start(7)).unwrap(), 7);
        assert_eq!(call(p, &mut t, &[b, 0xD, 0, 0, 0x10, 0x1000000, file]), 0);
        let section = get(p, b);
        let Object::File(open) = p.objects.get(file).unwrap() else {
            panic!("file")
        };
        assert_eq!(
            open.host.as_ref().unwrap().stream_position().unwrap(),
            7,
            "{arch}"
        );
        p.objects.close(section).unwrap();
        p.objects.close(file).unwrap();
    }
}
