//! Installed original service leaves and WoW64 RET16, without startup patches.
use super::*;
use crate::user::windows::{
    loader,
    process::{WindowsConfig, WindowsProcess},
};
#[test]
fn native_scheduler_slot_installed_leaf_maps_retains_releases_and_returns_all_selected_abis() {
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
            WindowsConfig::embedded("C:\\native-scheduler-slot.exe", vec![], vec![], 4096).unwrap();
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
        let invoke = super::super::virtual_memory_tests::installed::invoke;
        assert_eq!(
            invoke(
                p,
                &mut t,
                module,
                "NtSetInformationProcess",
                &[arch.ptr(u64::MAX), 112, out, arch.ptr_size()]
            ),
            STATUS_SUCCESS
        );
        let h = p.space.ptr(out, arch.ptr_size()).unwrap();
        record(p, out, 0, h, arch.ptr(u64::MAX), u32::MAX);
        assert_eq!(
            invoke(
                p,
                &mut t,
                module,
                "NtSetInformationThread",
                &[
                    arch.ptr(u64::MAX - 1),
                    57,
                    out,
                    if arch.is64() { 24 } else { 12 }
                ]
            ),
            STATUS_SUCCESS
        );
        let slot = p.objects.scheduler_slot(t.tid);
        assert_ne!(slot, 0);
        assert_eq!(
            p.space
                .ptr(out + if arch.is64() { 16 } else { 8 }, arch.ptr_size())
                .unwrap(),
            slot
        );
        assert_eq!(p.vm.query(slot).unwrap().kind, mem::MAPPED);
        assert_eq!(invoke(p, &mut t, module, "NtClose", &[h]), STATUS_SUCCESS);
        assert_eq!(p.vm.query(slot).unwrap().state, mem::COMMIT);
        record(p, out, 1, 0, 1, 0);
        assert_eq!(
            invoke(
                p,
                &mut t,
                module,
                "NtSetInformationThread",
                &[
                    arch.ptr(u64::MAX - 1),
                    57,
                    out,
                    if arch.is64() { 24 } else { 12 }
                ]
            ),
            STATUS_SUCCESS
        );
        assert_eq!(p.vm.query(slot).unwrap().state, mem::FREE);
    }
}
