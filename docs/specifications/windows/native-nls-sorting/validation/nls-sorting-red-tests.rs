use super::*;

#[test]
fn installed_nls_sorting_registry_snapshot_matches_independent_native_presence_and_views() {
    let registry = snapshot().unwrap();
    let path = "\\Registry\\Machine\\System\\CurrentControlSet\\Control\\Nls\\Sorting\\Versions";
    let guest: Vec<_> = path.encode_utf16().collect();
    let native: Vec<_> = path
        .strip_prefix("\\Registry\\Machine\\")
        .unwrap()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    for view in [0, 0x100, 0x200] {
        let mut handle = null_mut();
        // SAFETY: fixed terminated HKLM NLS Sorting Versions path, query/enumerate only,
        // initialized exclusive HKEY output; no guest names or host mutation.
        let status = unsafe {
            RegOpenKeyExW(
                (-2_147_483_646isize) as Hkey,
                native.as_ptr(),
                0,
                9 | view,
                &mut handle,
            )
        };
        if status == 2 {
            assert!(matches!(
                registry.lookup(&guest),
                super::super::Lookup::Missing
            ));
        } else {
            assert_eq!(status, 0);
            let host = OwnedKey(handle);
            assert!(matches!(
                registry.lookup(&guest),
                super::super::Lookup::Present(_)
            ));
            super::tests::check_ifeo(&registry, &host, &guest, 0);
        }
    }
}
