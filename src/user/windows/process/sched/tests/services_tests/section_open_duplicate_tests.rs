//! Native same-process section aliases, grants, protection and slot reuse.
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
fn native_section_open_replays_all_71040_duplicate_grants_flags_and_slot_reuse_rows() {
    let mut total = 0;
    for (arch, text) in [
        (
            WinArch::Arm64,
            corpus!("native-section-open-duplicate-v2-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-section-open-duplicate-v2-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-section-open-duplicate-v2-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-section-open-duplicate-v2-x86-laa.log"),
        ),
    ] {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), 17760);
        assert!(text.contains("complete cases=17760"));
        let restores = rows(text, "restored ");
        assert_eq!(restores.len(), 17760);
        assert!(restores.iter().all(|r| r["exact-section-handles"] == "1"));
        let (mut process, mut t) = fixture(arch, "NtOpenSection", SERVICE);
        let p = process.state_mut();
        p.object_namespace = namespace();
        let b =
            p.vm.allocate(None, 4096, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs {
            let path = match field(&r, "parent", 10) {
                0 => "\\KnownDlls\\kernel32.dll",
                1 => "\\KnownDlls32\\kernel32.dll",
                _ => "\\KnownDllsChpe32\\kernel32.dll",
            };
            let section = p
                .object_namespace
                .lookup_section(None, &path.encode_utf16().collect::<Vec<_>>(), 0x40, false)
                .unwrap();
            let grant = section
                .grant(field(&r, "source-access", 16) as u32, false)
                .unwrap();
            assert_eq!(u64::from(grant), field(&r, "source-grant", 16));
            let h = u64::from(p.objects.open_image_section(section, false, grant).unwrap());
            let id = p.objects.id(h).unwrap();
            assert!(
                p.objects
                    .set_flags(h, 3, field(&r, "source-flags", 16) as u32)
            );
            assert_eq!(field(&r, "exception", 16), 0);
            p.space
                .wptr(b, arch.ptr_size(), arch.ptr(0xA5A5_A5A5_A5A5_A5A5))
                .unwrap();
            let error = t.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
            p.space.w32(error, 0xBADC0DE).unwrap();
            let pseudo = arch.ptr(u64::MAX);
            assert_eq!(
                duplicate(
                    p,
                    &mut t,
                    [
                        pseudo,
                        h,
                        pseudo,
                        b,
                        field(&r, "desired", 16),
                        1,
                        field(&r, "options", 16)
                    ]
                ),
                field(&r, "ok", 10),
                "arch={arch} row={r:?}"
            );
            assert_eq!(
                u64::from(p.space.u32(error).unwrap()),
                field(&r, "error", 10),
                "arch={arch} row={r:?}"
            );
            let out = p.space.ptr(b, arch.ptr_size()).unwrap();
            if field(&r, "ok", 10) != 0 {
                assert_eq!(p.objects.id(out), Some(id));
                assert_eq!(
                    u64::from(p.objects.access(out).unwrap()),
                    field(&r, "grant", 16)
                );
                assert_eq!(
                    u64::from(p.objects.flags(out).unwrap()),
                    field(&r, "flags", 10)
                );
                assert!(p.objects.set_flags(out, 2, 0));
                p.objects.close(out).unwrap();
            } else {
                assert_eq!(out, field(&r, "output", 16), "arch={arch} row={r:?}");
            }
            if field(&r, "source-live", 10) != 0 {
                assert_eq!(p.objects.id(h), Some(id), "arch={arch} row={r:?}");
                assert!(p.objects.set_flags(h, 2, 0));
                p.objects.close(h).unwrap();
            } else {
                assert!(p.objects.id(h).is_none(), "arch={arch} row={r:?}");
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline,
                "arch={arch} row={r:?}"
            );
            total += 1;
        }
    }
    assert_eq!(total, 71040);
}
