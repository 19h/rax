//! Exact fault priority, guard consumption and output state from the host oracle.
use super::*;
#[test]
fn native_directory_open_replays_all_20000_pointer_guard_alignment_and_crossing_rows() {
    replay("boundaries", 5000);
}
#[test]
fn native_directory_open_replays_all_1680_flag_length_and_pointer_priority_rows() {
    replay("flag-boundaries", 420);
}
fn replay(family: &str, expected_rows: usize) {
    let mut total = 0;
    for (arch, text) in profiles(family) {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), expected_rows);
        assert!(text.contains(&format!("complete cases={expected_rows}")));
        let (mut process, mut t, b) = setup(arch, 12);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs {
            p.vm.protect(b, 12 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(b, &vec![0xA5; 12 * PAGE_SIZE as usize]).unwrap();
            let ak = field(&r, "attributes", 10);
            let ok = field(&r, "output", 10);
            let nk = field(&r, "unicode", 10);
            let bk = field(&r, "buffer", 10);
            let local = b + 10 * PAGE_SIZE;
            let size = if arch.is64() { 48 } else { 24 };
            let width = arch.ptr_size();
            let aa = match ak {
                0 => local,
                1 => 0,
                2 => 1,
                6 => b + PAGE_SIZE - size / 2,
                7 => local + 1,
                8 => local + 4,
                9 => local + 8,
                _ => b,
            };
            let oa = match ok {
                0 => local + 256,
                1 => 0,
                2 => 1,
                6 => b + 3 * PAGE_SIZE - width / 2,
                7 => local + 257,
                8 => local + 258,
                9 => local + 259,
                _ => b + 2 * PAGE_SIZE,
            };
            let na = match nk {
                0 => local + 512,
                1 => 0,
                2 => 1,
                _ => b + 4 * PAGE_SIZE,
            };
            let ba = match bk {
                0 => local + 1024,
                1 => 0,
                2 => 1,
                _ => b + 6 * PAGE_SIZE,
            };
            for at in [
                local
                    + match ak {
                        7 => 1,
                        8 => 4,
                        9 => 8,
                        _ => 0,
                    },
                b,
                b + PAGE_SIZE - size / 2,
            ] {
                attributes(
                    p,
                    at,
                    na,
                    0,
                    r.get("flags")
                        .map_or(0x40, |s| u32::from_str_radix(s, 16).unwrap()),
                    field(&r, "attributes-length", 10) as u32,
                );
            }
            for at in [local + 512, b + 4 * PAGE_SIZE] {
                descriptor(
                    p,
                    at,
                    ba,
                    r.get("name-length").map_or(20, |s| s.parse().unwrap()),
                    22,
                );
            }
            for at in [local + 1024, b + 6 * PAGE_SIZE] {
                p.space
                    .put_wunits(at, &"\\KnownDlls\0".encode_utf16().collect::<Vec<_>>())
                    .unwrap();
            }
            for (kind, at) in [
                (ak, b),
                (ok, b + 2 * PAGE_SIZE),
                (nk, b + 4 * PAGE_SIZE),
                (bk, b + 6 * PAGE_SIZE),
            ] {
                if (3..=5).contains(&kind) {
                    p.vm.protect(
                        at,
                        PAGE_SIZE,
                        match kind {
                            3 => prot::READWRITE | prot::GUARD,
                            4 => prot::NOACCESS,
                            _ => prot::READONLY,
                        },
                    )
                    .unwrap();
                }
            }
            if ak == 6 {
                p.vm.protect(b + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            if ok == 6 {
                p.vm.protect(b + 3 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            let expected = field(&r, "status", 16) as u32;
            assert_eq!(call(p, &mut t, [oa, 3, aa]), expected, "row={r:?}");
            assert_eq!(field(&r, "exception", 16), 0);
            for (key, at) in [
                ("attributes-protect", b),
                ("attributes-upper-protect", b + PAGE_SIZE),
                ("output-protect", b + 2 * PAGE_SIZE),
                ("output-upper-protect", b + 3 * PAGE_SIZE),
                ("unicode-protect", b + 4 * PAGE_SIZE),
                ("buffer-protect", b + 6 * PAGE_SIZE),
            ] {
                assert_eq!(
                    p.vm.query(at).unwrap().protect as u64,
                    field(&r, key, 16),
                    "{key} row={r:?}"
                );
            }
            p.vm.protect(b, 12 * PAGE_SIZE, prot::READWRITE).unwrap();
            if field(&r, "observed-output", 10) != 0 {
                let h = p.space.ptr(oa, width).unwrap();
                if expected == 0 {
                    assert_eq!(p.objects.get(h).unwrap().type_name(), "Directory");
                    assert_eq!(p.objects.access(h), Some(3));
                    p.objects.close(h).unwrap();
                } else {
                    assert_eq!(h, field(&r, "output-after", 16), "row={r:?}");
                }
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 4 * expected_rows);
}
