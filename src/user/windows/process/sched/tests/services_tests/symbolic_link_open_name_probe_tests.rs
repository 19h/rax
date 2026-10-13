//! Independent name read extent, prefix short-circuit, and WoW64 copy ordering.
use super::*;
#[test]
fn native_symbolic_link_open_replays_all_36608_name_extent_root_and_null_output_rows() {
    const NAMES: [&str; 4] = [
        "\\KnownDlls\\KnownDllPath",
        "\\KnownDlls32\\KnownDllPath",
        "\\Absent\\KnownDllPath",
        "KnownDllPath",
    ];
    let mut total = 0;
    for (arch, text) in profiles("name-probe") {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), 9152);
        assert!(text.contains("complete cases=9152"));
        let (mut process, mut t, b) = setup(arch, 20);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs {
            p.vm.protect(b, 20 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(b, &vec![0xA5; 20 * PAGE_SIZE as usize]).unwrap();
            let prefix = field(&r, "prefix", 10);
            let layout = field(&r, "layout", 10);
            let buffer = b + if layout == 0 { 0 } else { PAGE_SIZE - prefix };
            p.space
                .put_wunits(buffer, &vec![b'X' as u16; 32767])
                .unwrap();
            p.space
                .put_wunits(
                    buffer,
                    &NAMES[field(&r, "pattern", 10) as usize]
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            let attrs = b + 19 * PAGE_SIZE;
            let name = attrs + 64;
            let length = field(&r, "length", 10) as u16;
            descriptor(p, name, buffer, length, length);
            attributes(
                p,
                attrs,
                name,
                if field(&r, "root", 10) == 0 {
                    0
                } else {
                    arch.ptr(u64::MAX)
                },
                field(&r, "flags", 16) as u32,
                if arch.is64() { 48 } else { 24 },
            );
            let output = field(&r, "output", 10);
            let out = match output {
                0 => 0,
                1 => 1,
                _ => b + 18 * PAGE_SIZE,
            };
            let guard = b + if layout == 0 { 0 } else { PAGE_SIZE };
            p.vm.protect(guard, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
            if output == 2 {
                p.vm.protect(b + 18 * PAGE_SIZE, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            let expected = field(&r, "status", 16) as u32;
            assert_eq!(call(p, &mut t, [out, 1, attrs]), expected, "row={r:?}");
            assert_eq!(field(&r, "exception", 16), 0);
            for (key, at) in [
                ("buffer-protect", guard),
                ("output-protect", b + 18 * PAGE_SIZE),
            ] {
                assert_eq!(
                    p.vm.query(at).unwrap().protect as u64,
                    field(&r, key, 16),
                    "{key} row={r:?}"
                );
            }
            p.vm.protect(b, 20 * PAGE_SIZE, prot::READWRITE).unwrap();
            if output >= 2 {
                let h = p.space.ptr(out, arch.ptr_size()).unwrap();
                if expected == 0 {
                    assert_eq!(p.objects.access(h), Some(1));
                    p.objects.close(h).unwrap();
                } else {
                    assert_eq!(h, field(&r, "output-after", 16), "row={r:?}");
                }
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline,
                "row={r:?}"
            );
            total += 1;
        }
    }
    assert_eq!(total, 36608);
}
