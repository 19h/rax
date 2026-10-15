use super::*;

pub(crate) fn sample() -> Arc<DataSection> {
    let mut bytes = vec![0xA5; 5 * 4096];
    // Opaque server addresses are data. A different client base does not
    // authorize scanning or rewriting address-looking words.
    bytes[40..48].copy_from_slice(&0x0000_7FF5_F57C_0000u64.to_le_bytes());
    DataSection::selected(
        0x20004,
        STATUS_PRIVILEGE_NOT_HELD,
        false,
        1 << 20,
        vec![
            DataRegion {
                offset: 0,
                size: bytes.len() as u64,
                bytes: Some(bytes.into()),
            },
            DataRegion {
                offset: 5 * 4096,
                size: (1 << 20) - 5 * 4096,
                bytes: None,
            },
        ],
    )
    .unwrap()
}

#[test]
fn selected_data_section_retains_native_grants_without_requiring_query_access() {
    let section = sample();
    for (requested, expected) in [
        (0, Err(STATUS_ACCESS_DENIED)),
        (1, Err(STATUS_ACCESS_DENIED)),
        (2, Err(STATUS_ACCESS_DENIED)),
        (4, Ok(4)),
        (5, Err(STATUS_ACCESS_DENIED)),
        (0x20000, Ok(0x20000)),
        (0x02000000, Ok(0x20004)),
        (0x01000000, Err(STATUS_PRIVILEGE_NOT_HELD)),
        (0x80000000, Err(STATUS_ACCESS_DENIED)),
        (0x40000000, Err(STATUS_ACCESS_DENIED)),
        (0x20000000, Err(STATUS_ACCESS_DENIED)),
        (0x10000000, Err(STATUS_ACCESS_DENIED)),
    ] {
        assert_eq!(section.grant(requested, false), expected, "{requested:08X}");
    }
    assert_eq!(section.grant(0, true), Ok(0));
    assert_eq!(section.grant(0x20, true), Ok(0));
    assert_eq!(
        section.regions[0].bytes.as_ref().unwrap()[40..48],
        0x0000_7FF5_F57C_0000u64.to_le_bytes()
    );
}

#[test]
fn selected_data_section_rejects_gaps_overlaps_extents_and_unbounded_storage() {
    let section = sample();
    let check = |size, regions| {
        DataSection::selected(0x20004, STATUS_PRIVILEGE_NOT_HELD, false, size, regions)
    };
    for size in [0, 1, (64 << 20) + 4096, u64::MAX] {
        assert!(check(size, section.regions.clone()).is_err());
    }
    let mut invalid = Vec::new();
    let mut runs = section.regions.clone();
    runs[1].offset += 4096;
    invalid.push(runs);
    let mut runs = section.regions.clone();
    runs[1].offset -= 4096;
    invalid.push(runs);
    let mut runs = section.regions.clone();
    runs[0].size -= 4096;
    invalid.push(runs);
    let mut runs = section.regions.clone();
    runs[0].bytes = Some(vec![0; 4096].into());
    invalid.push(runs);
    let mut runs = section.regions.clone();
    runs[1].size = u64::MAX;
    invalid.push(runs);
    invalid.push(vec![]);
    for runs in invalid {
        assert!(check(section.size, runs).is_err());
    }
    assert!(
        DataSection::selected(
            0x20005,
            STATUS_PRIVILEGE_NOT_HELD,
            false,
            section.size,
            section.regions.clone()
        )
        .is_err()
    );
    assert!(
        DataSection::selected(
            4,
            STATUS_INVALID_HANDLE,
            false,
            section.size,
            section.regions.clone()
        )
        .is_err()
    );
}
