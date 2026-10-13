//! Actual installed constructor/close leaves, including WoW64 RET16 cleanup.
use super::*;
use crate::user::windows::{
    loader,
    process::{WindowsConfig, WindowsProcess},
};

#[test]
fn native_scheduler_shared_installed_leaf_preserves_native_and_wow_births() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let archs = if host == WinArch::X86 {
        vec![host]
    } else {
        vec![host, WinArch::X86]
    };
    for arch in archs {
        let image: &[u8] = match arch {
            WinArch::Arm64 => include_bytes!(
                "../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe"
            ),
            WinArch::X64 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
            }
            WinArch::X86 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            }
        };
        let mut config =
            WindowsConfig::embedded("C:\\native-scheduler-data.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        let mut process = WindowsProcess::spawn_image(config, image.to_vec()).unwrap();
        let p = process.state_mut();
        let tid = *p.threads.keys().next().unwrap();
        let mut t = p.threads.remove(&tid).unwrap();
        let module = loader::load_dll(p, "ntdll.dll").unwrap();
        let base =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let invoke = super::super::virtual_memory_tests::installed::invoke;
        let before = p.objects.handle_count();
        assert_eq!(
            invoke(
                p,
                &mut t,
                module,
                "NtSetInformationProcess",
                &[arch.ptr(u64::MAX), 112, base + 132, arch.ptr_size()]
            ),
            STATUS_SUCCESS
        );
        let h = p.space.ptr(base + 132, arch.ptr_size()).unwrap();
        assert_eq!(p.objects.get(h).unwrap().type_name(), "SchedulerSharedData");
        assert_eq!(p.objects.access(h), Some(1));
        assert_eq!(p.objects.flags(h), Some(0));
        assert_eq!(invoke(p, &mut t, module, "NtClose", &[h]), STATUS_SUCCESS);
        assert_eq!(p.objects.handle_count(), before);
        p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        assert_eq!(
            invoke(
                p,
                &mut t,
                module,
                "NtSetInformationProcess",
                &[arch.ptr(u64::MAX), 112, base + PAGE_SIZE, arch.ptr_size()]
            ),
            STATUS_GUARD_PAGE_VIOLATION
        );
        assert_eq!(
            p.vm.query(base + PAGE_SIZE).unwrap().protect,
            prot::READWRITE
        );
        assert_eq!(
            p.objects.handle_count(),
            before + usize::from(arch == WinArch::X86)
        );
        // WoW64's unreturned successful birth belongs to the process table
        // and is released by its drain. Native64 creates no guarded birth.
        assert_eq!(
            p.objects
                .drain()
                .filter(|object| object.type_name() == "SchedulerSharedData")
                .count(),
            usize::from(arch == WinArch::X86)
        );
    }
}
