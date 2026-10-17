use super::*;
use crate::user::windows::memory::{mem, prot};

pub(crate) fn template(preferred: u64) -> Arc<ImageTemplate> {
    let base = image_tests::template(preferred);
    ImageTemplate::selected_extended(
        base.bytes.to_vec(),
        base.regions.clone(),
        [
            Some(ImageExtension {
                rva: 8192,
                bytes: vec![0xA5; 4096].into(),
            }),
            Some(ImageExtension {
                rva: 12288,
                bytes: vec![0x5A; 4096].into(),
            }),
        ],
    )
    .unwrap()
}
#[test]
fn image_extension_owner_bounds_geometry_and_preserves_declared_image_size() {
    let image = template(0x7000_0000);
    assert_eq!(image.bytes.len(), 8192);
    assert_eq!(image.allocation_size(), 16384);
    assert_eq!(
        ImageTemplate::extension_region(image.extensions[0].as_ref().unwrap()),
        ImageRegion {
            offset: 8192,
            size: 4096,
            state: mem::COMMIT,
            protection: prot::EXECUTE_READ
        }
    );
    for (rva, len) in [
        (4096, 4096),
        (8193, 4096),
        (8192, 0),
        (8192, 1),
        (8192, 69632),
        (u64::MAX - 4095, 4096),
        (8192 + 131072, 4096),
    ] {
        assert!(
            ImageTemplate::selected_extended(
                image.bytes.to_vec(),
                image.regions.clone(),
                [
                    Some(ImageExtension {
                        rva,
                        bytes: vec![0; len].into()
                    }),
                    None
                ]
            )
            .is_err()
        );
    }
    let mut overlap = image.extensions.clone();
    overlap[1].as_mut().unwrap().rva = 8192;
    assert!(
        ImageTemplate::selected_extended(image.bytes.to_vec(), image.regions.clone(), overlap)
            .is_err()
    );
}
#[test]
fn image_extension_native_rebase_requires_two_equal_normalized_views() {
    let original = template(0x7000_0000);
    let first = template(0x5000_0000);
    let second = template(0x5100_0000);
    let selected = ImageTemplate::with_rebase(original, &first, &second).unwrap();
    let bytes = selected.view_bytes(0x5200_0000, true).unwrap();
    assert_eq!(&bytes[4096..4104], &0x5200_1234u64.to_le_bytes());
    assert_eq!(&selected.bytes[4096..4104], &0x7000_1234u64.to_le_bytes());
    let mut corrupted = second.bytes.to_vec();
    corrupted[1000] ^= 1;
    let corrupted = ImageTemplate::selected_extended(
        corrupted,
        second.regions.clone(),
        second.extensions.clone(),
    )
    .unwrap();
    assert!(ImageTemplate::with_rebase(template(0x7000_0000), &first, &corrupted).is_err());
    assert!(
        ImageTemplate::with_rebase(
            template(0x7000_0000),
            &first,
            &image_tests::template(0x5100_0000)
        )
        .is_err()
    );
}

#[test]
fn image_extension_rebase_rejects_duplicate_address_and_preserves_compatibility_page_policy() {
    let one = template(0x5000_0000);
    assert!(ImageTemplate::with_rebase(template(0x7000_0000), &one, &one).is_err());
    let image = ImageTemplate::without_large_pages(template(0x7000_0000)).unwrap();
    assert!(!image.large_pages);
    let first = ImageTemplate::without_large_pages(template(0x5000_0000)).unwrap();
    let second = ImageTemplate::without_large_pages(template(0x5100_0000)).unwrap();
    assert!(
        !ImageTemplate::with_rebase(image, &first, &second)
            .unwrap()
            .large_pages
    );
}
