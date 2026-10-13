//! Installed service leaves preserve caller output and WoW64 RET16 cleanup.
use super::*;
use crate::user::windows::{
    loader,
    process::{WindowsConfig, WindowsProcess},
};
#[test]
fn native_thread_token_installed_profile_copyback_preserves_measured_guard_priority() {
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
        let mut cfg =
            WindowsConfig::embedded("C:\\native-thread-token.exe", vec![], vec![], 4096).unwrap();
        cfg.native_libraries = true;
        let mut process = WindowsProcess::spawn_image(cfg, image.to_vec()).unwrap();
        let p = process.state_mut();
        let tid = *p.threads.keys().next().unwrap();
        let mut t = p.threads.remove(&tid).unwrap();
        let module = loader::load_dll(p, "ntdll.dll").unwrap();
        let out =
            p.vm.allocate(None, 8192, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
        p.space.wr(out, &vec![0xa5; 8192]).unwrap();
        p.vm.protect(out + 4096, 4096, prot::READONLY | prot::GUARD)
            .unwrap();
        let handles = p.objects.handle_count();
        for dest in [out, out + 4096, 1, 0, arch.ptr(u64::MAX) - 7] {
            let expected = if arch == WinArch::X86 && dest != 0 && dest != out {
                STATUS_ACCESS_VIOLATION
            } else {
                STATUS_NO_TOKEN
            };
            assert_eq!(
                super::super::virtual_memory_tests::installed::invoke(
                    p,
                    &mut t,
                    module,
                    "NtOpenThreadToken",
                    &[arch.ptr(u64::MAX - 1), 0x2001c, 0, dest]
                ),
                expected
            );
            assert_eq!(
                p.vm.query(out + 4096).unwrap().protect,
                prot::READONLY | prot::GUARD
            );
            assert_eq!(p.objects.handle_count(), handles);
        }
        let mut expected = vec![0xa5; 64];
        if arch == WinArch::X86 {
            expected[..4].fill(0);
        }
        assert_eq!(p.space.bytes(out, 64).unwrap(), expected);
        p.vm.protect(out + 4096, 4096, prot::READWRITE | prot::GUARD)
            .unwrap();
        let status = super::super::virtual_memory_tests::installed::invoke(
            p,
            &mut t,
            module,
            "NtOpenThreadToken",
            &[arch.ptr(u64::MAX - 1), 0x2001c, 0, out + 4096],
        );
        assert_eq!(
            status,
            if arch == WinArch::X86 {
                STATUS_GUARD_PAGE_VIOLATION
            } else {
                STATUS_NO_TOKEN
            }
        );
        assert_eq!(
            p.vm.query(out + 4096).unwrap().protect,
            if arch == WinArch::X86 {
                prot::READWRITE
            } else {
                prot::READWRITE | prot::GUARD
            }
        );
    }
}
