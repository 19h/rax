//! Read-memory calls executed through the selected installed NTDLL instruction leaf.
use super::*;
use crate::user::windows::{
    loader,
    process::{WindowsConfig, WindowsProcess},
};

#[test]
fn installed_ntdll_read_virtual_memory_leaf_copies_guest_image_bytes_and_count() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let arches = if host == WinArch::X86 {
        vec![host]
    } else {
        vec![host, WinArch::X86]
    };
    for arch in arches {
        let image: &[u8] = match arch {
            WinArch::X86 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            }
            WinArch::X64 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
            }
            WinArch::Arm64 => include_bytes!(
                "../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe"
            ),
        };
        let mut config =
            WindowsConfig::embedded("C:\\native-read-memory.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        let mut process = WindowsProcess::spawn_image(config, image.to_vec()).unwrap();
        let p = process.state_mut();
        let tid = *p.threads.keys().next().unwrap();
        let mut thread = p.threads.remove(&tid).unwrap();
        let module = loader::load_dll(p, "ntdll.dll").unwrap();
        let source = p.modules.list[module].base;
        let expected = p.space.bytes(source, 64).unwrap();
        let base =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        p.space.wr(base, &[0xA5; 80]).unwrap();
        p.space.wr(base + 128, &[0xD3; 16]).unwrap();
        assert_eq!(
            super::super::installed::invoke(
                p,
                &mut thread,
                module,
                "NtReadVirtualMemory",
                &[arch.ptr(u64::MAX), source, base, 64, base + 128]
            ),
            STATUS_SUCCESS
        );
        assert_eq!(p.space.bytes(base, 64).unwrap(), expected);
        assert_eq!(p.space.bytes(base + 64, 16).unwrap(), vec![0xA5; 16]);
        assert_eq!(p.space.ptr(base + 128, arch.ptr_size()).unwrap(), 64);
        assert_eq!(p.space.u8(base + 128 + arch.ptr_size()).unwrap(), 0xD3);
        p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        p.space.wptr(base + 128, arch.ptr_size(), u64::MAX).unwrap();
        assert_eq!(
            super::super::installed::invoke(
                p,
                &mut thread,
                module,
                "NtReadVirtualMemory",
                &[arch.ptr(u64::MAX), base + PAGE_SIZE, base, 8, base + 128]
            ),
            0x8000_000D
        );
        assert_eq!(
            p.vm.page_state(base + PAGE_SIZE).unwrap().protect,
            prot::READWRITE | prot::GUARD
        );
        assert_eq!(p.space.ptr(base + 128, arch.ptr_size()).unwrap(), 0);
        assert_eq!(p.space.bytes(base, 64).unwrap(), expected);
    }
}
