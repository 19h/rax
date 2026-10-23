use super::*;
use crate::user::windows::loader::services::tests::named_table;

fn setup(
    arch: WinArch,
    name: &str,
    number: u32,
) -> (super::super::super::super::WindowsProcess, Thread) {
    let (mut process, t) = fixture(arch, "NtClose", 0x37);
    let p = process.state_mut();
    let idx = p.modules.nt_services.as_ref().unwrap().0;
    p.modules.win32u_services = Some((idx, named_table(arch, name, number)));
    (process, t)
}

fn call(p: &mut Proc, t: &mut Thread, number: u32) -> Outcome {
    if p.arch == WinArch::X86 {
        p.space.w32(t.cpu.sp(), 0x1234_000c).unwrap();
        t.cpu.set_gpr(0, u64::from(number));
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        if p.arch == WinArch::X64 {
            t.cpu.set_gpr(0, u64::from(number));
        }
        t.cpu.set_pc(0x1234_0004);
        handle_stop(p, t, stop(p.arch, number))
    }
}

#[test]
fn win32u_named_init2_fails_without_success_or_guest_state_changes_and_keeps_ntdll_live() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = setup(arch, "NtGdiInit2", 0x12fd);
        let p = process.state_mut();
        let error = t.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
        p.space.w32(error, 0x1234_5678).unwrap();
        let sp = t.cpu.sp();
        let rng = p.rng;
        let handles = p.objects.handle_count();
        assert!(
            matches!(call(p, &mut t, 0x12fd), Outcome::Fail(reason) if reason.contains("native Win32u service NtGdiInit2 (0x12fd)") && reason.contains("not implemented"))
        );
        assert_eq!(t.cpu.sp(), sp);
        assert_eq!(p.space.u32(error).unwrap(), 0x1234_5678);
        assert_eq!(p.rng, rng);
        assert_eq!(p.objects.handle_count(), handles);
        assert!(t.frames.is_empty());
        let object = p.objects.create(Object::Event {
            manual: true,
            signaled: 0,
        });
        let handle = p.objects.open(object, false);
        match arch {
            WinArch::X86 => p.space.w32(sp + 8, handle).unwrap(),
            WinArch::X64 => t.cpu.set_gpr(10, u64::from(handle)),
            WinArch::Arm64 => t.cpu.set_gpr(0, u64::from(handle)),
        }
        assert_eq!(call(p, &mut t, 0x37), Outcome::Continue);
        assert_eq!(t.cpu.gpr(0), 0);
        assert!(p.objects.get(u64::from(handle)).is_none());
    }
}

#[test]
fn win32u_namespace_cannot_borrow_ntdll_implementation_or_outlive_its_selected_module() {
    for arch in WinArch::ALL {
        for (name, text) in [
            ("NtGdiInit", "native Win32u service NtGdiInit"),
            ("NtClose", "native Win32u service NtClose"),
        ] {
            let (mut process, mut t) = setup(arch, name, 0x12fd);
            assert!(
                matches!(call(process.state_mut(), &mut t, 0x12fd), Outcome::Fail(reason) if reason.contains(text) && reason.contains("not implemented"))
            );
        }
        let (mut process, mut t) = setup(arch, "NtGdiInit2", 0x12fd);
        let p = process.state_mut();
        p.modules.win32u_services.as_mut().unwrap().0 = usize::MAX;
        assert!(
            matches!(call(p, &mut t, 0x12fd), Outcome::Fail(reason) if reason.contains("no admitted stub"))
        );
        p.modules.win32u_services.as_mut().unwrap().0 = p.modules.nt_services.as_ref().unwrap().0;
        p.modules.nt_services.as_mut().unwrap().0 = usize::MAX;
        if arch == WinArch::X86 {
            assert!(
                matches!(call(p, &mut t, 0x12fd), Outcome::Fail(reason) if reason.contains("no live selected NTDLL"))
            );
        } else {
            assert!(
                super::super::super::super::services::dispatch(p, &mut t, 0x12fd, 0x1234_0000)
                    .is_none()
            );
        }
    }
}

#[test]
fn win32u_number_collisions_fail_before_any_implementation_and_unknown_numbers_remain_explicit() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = setup(arch, "NtGdiInit2", 0x37);
        let p = process.state_mut();
        assert!(
            matches!(call(p, &mut t, 0x37), Outcome::Fail(reason) if reason.contains("conflicting selected NTDLL and Win32u identities"))
        );
        assert!(
            matches!(call(p, &mut t, 0x13fe), Outcome::Fail(reason) if reason.contains("no admitted stub"))
        );
    }
}

#[test]
fn supplied_win32u_basename_cannot_admit_a_service_table() {
    use crate::user::windows::dll::BuiltinDll;
    use crate::user::windows::hle::{ApiResult, Conv, Ctx, DataSize, Export};
    use crate::user::windows::loader::builtin::{self, BuiltinSym};
    fn unused(_: &mut Ctx) -> ApiResult {
        panic!("fixture export must never execute")
    }
    static EXPORTS: &[Export] = &[
        Export::func("NtGdiInit2", Conv::Stdcall, &[], unused),
        Export::data("Wow64Transition", DataSize::Ptrs(1)),
    ];
    static DLL: BuiltinDll = BuiltinDll {
        name: "win32u.dll",
        display: "win32u.dll",
        subsystem: 3,
        exports: &[EXPORTS],
    };
    for arch in WinArch::ALL {
        let (mut process, _) = fixture(arch, "NtClose", 0x37);
        let p = process.state_mut();
        let mut image = builtin::build(&DLL, arch, 0x2000_0000, &[]);
        let symbol = image
            .symbols
            .iter()
            .find(|(name, _)| *name == "NtGdiInit2")
            .unwrap();
        let BuiltinSym::Rva(rva) = symbol.1 else {
            panic!("service export")
        };
        let code = if arch == WinArch::Arm64 {
            [0xD402_5FA1u32.to_le_bytes(), 0xD65F_03C0u32.to_le_bytes()].concat()
        } else if arch == WinArch::X86 {
            let BuiltinSym::Rva(slot) = image
                .symbols
                .iter()
                .find(|(name, _)| *name == "Wow64Transition")
                .unwrap()
                .1
            else {
                panic!("transition export")
            };
            let thunk = rva + 64;
            image.bytes[thunk as usize..thunk as usize + 6].copy_from_slice(
                &[
                    vec![0xff, 0x25],
                    (0x2000_0000 + slot).to_le_bytes().to_vec(),
                ]
                .concat(),
            );
            [
                vec![0xb8],
                0x12fdu32.to_le_bytes().to_vec(),
                vec![0xba],
                (0x2000_0000 + thunk).to_le_bytes().to_vec(),
                vec![0xff, 0xd2, 0xc3],
            ]
            .concat()
        } else {
            [
                vec![0x4c, 0x8b, 0xd1, 0xb8],
                0x12fdu32.to_le_bytes().to_vec(),
                vec![0x0f, 0x05, 0xc3],
            ]
            .concat()
        };
        image.bytes[rva as usize..rva as usize + code.len()].copy_from_slice(&code);
        let pe = crate::user::image::pe::PeImage::parse(image.bytes.clone()).unwrap();
        let table = crate::user::windows::loader::services::ServiceTable::from_image(arch, &pe)
            .unwrap()
            .expect("spoof fixture contains a complete service and exported transition");
        assert_eq!(table.name(arch, 0x12fd), Some("NtGdiInit2"));
        std::sync::Arc::make_mut(&mut p.cfg)
            .supplied_dlls
            .insert("c:\\app\\win32u.dll".to_owned(), image.bytes.into());
        let idx = crate::user::windows::loader::load_dll(p, "C:\\app\\win32u.dll").unwrap();
        assert_eq!(p.modules.list[idx].name, "win32u.dll");
        assert!(p.modules.win32u_services.is_none());
    }
}
