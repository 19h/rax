//! Relative-root identity and independent native DuplicateHandle policy.
use super::*;
fn duplicate(p: &mut Proc, t: &mut Thread, args: [u64; 7]) -> u64 {
    use crate::user::windows::hle::{Ctx, Flow, Item, Value};
    let api = crate::user::windows::dll::find("kernel32.dll")
        .unwrap()
        .exports
        .iter()
        .flat_map(|e| e.iter())
        .find_map(|e| match &e.item {
            Item::Func(api) if api.name == "DuplicateHandle" => Some(api),
            _ => None,
        })
        .unwrap();
    let sp = t.cpu.sp();
    for (i, v) in args.into_iter().enumerate() {
        match p.arch {
            WinArch::X86 => p.space.w32(sp + 4 + i as u64 * 4, v as u32).unwrap(),
            WinArch::X64 if i < 4 => t.cpu.set_gpr([1, 2, 8, 9][i], v),
            WinArch::X64 => p.space.w64(sp + 40 + (i - 4) as u64 * 8, v).unwrap(),
            WinArch::Arm64 => t.cpu.set_gpr(i, v),
        }
    }
    let mut c = Ctx {
        p,
        t,
        api,
        entry_pc: 0x1234,
        entry_sp: sp,
        ret_addr: 0,
        cursor: sp - 256,
    };
    match (api.imp)(&mut c).unwrap() {
        Flow::Ret(Value::Int(v)) => v,
        _ => panic!("integer"),
    }
}
#[test]
fn native_directory_open_replays_all_3456_relative_root_grant_and_tag_rows() {
    const PATHS: [&str; 3] = ["\\", "\\KnownDlls", "\\KnownDlls32"];
    const NAMES: [&str; 8] = [
        "",
        "KnownDlls",
        "KnownDllPath",
        "ntdll.dll",
        "\\",
        "\\KnownDlls",
        "rax-absent-20261012",
        "ntdll.dll\\x",
    ];
    let mut total = 0;
    for (arch, text) in profiles("reference") {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), 2064);
        assert!(text.contains("complete cases=2064"));
        let (mut process, mut t, b) = setup(arch, 3);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs.into_iter().filter(|r| r["family"] == "0") {
            let h = opened(
                p,
                &mut t,
                b,
                PATHS[field(&r, "parent", 10) as usize],
                field(&r, "source-access", 16) as u32,
            );
            assert_eq!(
                p.objects.access(h),
                Some(field(&r, "root-grant", 16) as u32)
            );
            let selected = h | match field(&r, "tag", 10) {
                v @ 0..=3 => v,
                4 => {
                    if arch.is64() {
                        0x1_0000_0000
                    } else {
                        0x4000_0000
                    }
                }
                _ => {
                    if arch.is64() {
                        0xFFFF_0000_0000_0000
                    } else {
                        0xFFFF_0000
                    }
                }
            };
            let name: Vec<_> = NAMES[field(&r, "name", 10) as usize]
                .encode_utf16()
                .collect();
            p.space.put_wunits(b + 128, &name).unwrap();
            descriptor(
                p,
                b + 64,
                b + 128,
                (name.len() * 2) as u16,
                (name.len() * 2 + 2) as u16,
            );
            attributes(
                p,
                b + 16,
                b + 64,
                selected,
                0x40,
                if arch.is64() { 48 } else { 24 },
            );
            p.space
                .wptr(b, arch.ptr_size(), arch.ptr(0xA5A5_A5A5_A5A5_A5A5))
                .unwrap();
            let expected = field(&r, "status", 16) as u32;
            assert_eq!(call(p, &mut t, [b, 3, b + 16]), expected, "row={r:?}");
            let out = p.space.ptr(b, arch.ptr_size()).unwrap();
            if expected == 0 {
                assert_eq!(p.objects.access(out), Some(field(&r, "grant", 16) as u32));
                if name.is_empty() {
                    assert_eq!(p.objects.id(out), p.objects.id(h));
                }
                p.objects.close(out).unwrap();
            } else {
                assert_eq!(out, field(&r, "output", 16));
            }
            p.objects.close(h).unwrap();
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 3456);
}
#[test]
fn native_directory_handles_replay_all_4800_duplicate_grants_inheritance_and_close_rows() {
    replay_duplicates("reference", 4800);
}
#[test]
fn native_directory_handles_replay_all_4800_protected_and_same_attribute_policy_rows() {
    replay_duplicates("duplicate-policy", 4800);
}
fn replay_duplicates(family: &str, expected_rows: usize) {
    const PATHS: [&str; 3] = ["\\", "\\KnownDlls", "\\KnownDlls32"];
    let mut total = 0;
    for (arch, text) in profiles(family) {
        let rs = rows(text, "case=");
        let (mut process, mut t, b) = setup(arch, 3);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs.into_iter().filter(|r| r["family"] == "1") {
            let h = opened(
                p,
                &mut t,
                b,
                PATHS[field(&r, "parent", 10) as usize],
                field(&r, "source-access", 16) as u32,
            );
            let id = p.objects.id(h).unwrap();
            let source_flags = r
                .get("source-flags")
                .map_or(0, |s| u32::from_str_radix(s, 16).unwrap());
            assert!(p.objects.set_flags(h, 3, source_flags));
            assert_eq!(
                r.get("exception")
                    .map_or(0, |s| u32::from_str_radix(s, 16).unwrap()),
                0
            );
            assert_eq!(
                p.objects.access(h),
                Some(field(&r, "source-grant", 16) as u32)
            );
            p.space
                .wptr(b, arch.ptr_size(), arch.ptr(0xA5A5_A5A5_A5A5_A5A5))
                .unwrap();
            p.space
                .w32(
                    t.teb + crate::user::windows::layout::offsets(arch).teb_last_error,
                    0xBADC0DE,
                )
                .unwrap();
            let pseudo = arch.ptr(u64::MAX);
            let options = field(&r, "options", 16);
            assert_eq!(
                duplicate(
                    p,
                    &mut t,
                    [pseudo, h, pseudo, b, field(&r, "desired", 16), 1, options]
                ),
                field(&r, "ok", 10),
                "row={r:?}"
            );
            assert_eq!(
                p.space
                    .u32(t.teb + crate::user::windows::layout::offsets(arch).teb_last_error)
                    .unwrap() as u64,
                field(&r, "error", 10),
                "row={r:?}"
            );
            let out = p.space.ptr(b, arch.ptr_size()).unwrap();
            if field(&r, "ok", 10) != 0 {
                assert_eq!(p.objects.id(out), Some(id));
                assert_eq!(p.objects.access(out), Some(field(&r, "grant", 16) as u32));
                assert_eq!(p.objects.flags(out), Some(field(&r, "flags", 10) as u32));
                assert!(p.objects.set_flags(out, 2, 0));
                p.objects.close(out).unwrap();
            } else {
                assert_eq!(out, field(&r, "output", 16));
            }
            if field(&r, "source-live", 10) != 0 {
                assert_eq!(p.objects.id(h), Some(id), "row={r:?}");
                assert!(p.objects.set_flags(h, 2, 0));
                p.objects.close(h).unwrap();
            } else {
                assert!(p.objects.id(h).is_none(), "row={r:?}");
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, expected_rows);
}
#[test]
fn native_directory_handles_replay_all_448_duplicate_pointer_guard_and_unreturned_rows() {
    replay_duplicate_boundaries("duplicate-boundaries", 112, false);
}
#[test]
fn native_directory_handles_replay_all_672_protected_duplicate_copy_fault_and_unreturned_rows() {
    replay_duplicate_boundaries("duplicate-protected-boundaries", 168, true);
}
fn replay_duplicate_boundaries(family: &str, expected_rows: usize, protected: bool) {
    let mut total = 0;
    for (arch, text) in profiles(family) {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), expected_rows);
        assert!(text.contains(&format!("complete cases={expected_rows}")));
        let (mut process, mut t, b) = setup(arch, 4);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs {
            p.vm.protect(b, 4 * PAGE_SIZE, prot::READWRITE).unwrap();
            let h = opened(p, &mut t, b + 2 * PAGE_SIZE, "\\KnownDlls", 1);
            if protected {
                assert!(p.objects.set_flags(h, 3, 3));
            }
            let before_handles = p.objects.handle_count();
            let before: std::collections::BTreeSet<_> =
                p.objects.iter().map(|(id, _)| id).collect();
            p.space.wr(b, &vec![0xA5; 2 * PAGE_SIZE as usize]).unwrap();
            let kind = field(&r, "output-kind", 10);
            let out = match kind {
                1 => 0,
                2 => 1,
                6 => b + PAGE_SIZE - arch.ptr_size() / 2,
                _ => b,
            };
            if (3..=5).contains(&kind) {
                p.vm.protect(
                    b,
                    PAGE_SIZE,
                    match kind {
                        3 => prot::READWRITE | prot::GUARD,
                        4 => prot::NOACCESS,
                        _ => prot::READONLY,
                    },
                )
                .unwrap();
            }
            if kind == 6 {
                p.vm.protect(b + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            let pseudo = arch.ptr(u64::MAX);
            p.space
                .w32(
                    t.teb + crate::user::windows::layout::offsets(arch).teb_last_error,
                    0xBADC0DE,
                )
                .unwrap();
            assert_eq!(
                duplicate(
                    p,
                    &mut t,
                    [
                        pseudo,
                        h,
                        pseudo,
                        out,
                        field(&r, "desired", 16),
                        1,
                        field(&r, "options", 16)
                    ]
                ),
                field(&r, "ok", 10),
                "row={r:?}"
            );
            assert_eq!(
                p.space
                    .u32(t.teb + crate::user::windows::layout::offsets(arch).teb_last_error)
                    .unwrap() as u64,
                field(&r, "error", 10)
            );
            assert_eq!(
                p.vm.query(b).unwrap().protect as u64,
                field(&r, "output-protect", 16)
            );
            let delta =
                field(&r, "captured-after", 10) as i64 - field(&r, "captured-before", 10) as i64;
            assert_eq!(
                p.objects.handle_count() as i64 - before_handles as i64,
                delta,
                "row={r:?}"
            );
            assert_eq!(
                p.objects.id(h).is_some() as u64,
                field(&r, "source-live", 10),
                "row={r:?}"
            );
            p.vm.protect(b, 4 * PAGE_SIZE, prot::READWRITE).unwrap();
            if field(&r, "observed-output", 10) != 0 {
                let v = p.space.ptr(out, arch.ptr_size()).unwrap();
                if field(&r, "ok", 10) == 0 {
                    assert_eq!(v, field(&r, "output-after", 16));
                } else {
                    assert_eq!(p.objects.get(v).unwrap().type_name(), "Directory");
                }
            }
            // Close only handles owned by this case, including native-compatible NULL outputs.
            let mut owned = Vec::new();
            for candidate in
                (4..=((p.objects.handle_count() + baseline.0 + 4) * 4) as u64).step_by(4)
            {
                if let Some(id) = p.objects.id(candidate) {
                    if candidate == h
                        || !before.contains(&id)
                        || matches!(p.objects.get(candidate), Some(Object::Directory(_)))
                    {
                        owned.push(candidate);
                    }
                }
            }
            for h in owned {
                assert!(p.objects.set_flags(h, 2, 0));
                p.objects.close(h).unwrap();
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
