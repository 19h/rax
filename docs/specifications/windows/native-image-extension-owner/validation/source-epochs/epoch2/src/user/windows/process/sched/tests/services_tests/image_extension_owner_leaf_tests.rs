use super::*;
use crate::user::windows::{
    loader,
    process::{WindowsConfig, WindowsProcess},
};

#[repr(C)]
#[derive(Default)]
struct NativeExtension {
    kind: u32,
    flags: u32,
    rva: usize,
    size: usize,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut std::ffi::c_void;
    fn ReadProcessMemory(
        process: *mut std::ffi::c_void,
        address: *const std::ffi::c_void,
        output: *mut std::ffi::c_void,
        size: usize,
        copied: *mut usize,
    ) -> i32;
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryVirtualMemory(
        process: *mut std::ffi::c_void,
        address: *mut std::ffi::c_void,
        class: u32,
        output: *mut std::ffi::c_void,
        size: usize,
        returned: *mut usize,
    ) -> i32;
}

#[test]
fn image_extension_owner_installed_ntdll_leaf_reports_owned_bootstrap_tail() {
    let arch = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let input: &[u8] = match arch {
        WinArch::Arm64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe")
        }
        WinArch::X64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
        }
        WinArch::X86 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
        }
    };
    let mut cfg =
        WindowsConfig::embedded("C:\\image-extension-owner.exe", vec![], vec![], 4096).unwrap();
    cfg.native_libraries = true;
    let mut process = WindowsProcess::spawn_image(cfg, input.to_vec()).unwrap();
    let p = process.state_mut();
    let tid = *p.threads.keys().next().unwrap();
    let mut t = p.threads.remove(&tid).unwrap();
    let module = loader::load_dll(p, "ntdll.dll").unwrap();
    let base = p.modules.list[module].base;
    let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: fixed terminated UTF-16 name; the loaded NTDLL has process
    // lifetime and this call only returns its already loaded module base.
    let native = unsafe { GetModuleHandleW(name.as_ptr()) };
    assert!(!native.is_null());
    if arch.is64() {
        assert_eq!(base, native as usize as u64);
    }
    let out =
        p.vm.allocate(None, 4096, mem::RESERVE | mem::COMMIT, prot::READWRITE)
            .unwrap()
            .0;
    let invoke = super::super::super::installed::invoke;
    assert_eq!(
        invoke(
            p,
            &mut t,
            module,
            "NtQueryVirtualMemory",
            &[arch.ptr(u64::MAX), base, 6, out, 24, 0]
        ),
        STATUS_SUCCESS
    );
    let present = p.vm.section_image_extension(base, 0);
    assert_eq!(
        p.space.u32(out + arch.ptr_size() * 2).unwrap() & 0x40,
        if present.is_some() { 0x40 } else { 0 }
    );
    for kind in [0, 1] {
        p.space.wr(out, &[0; 64]).unwrap();
        p.space.w32(out, kind).unwrap();
        let status = invoke(
            p,
            &mut t,
            module,
            "NtQueryVirtualMemory",
            &[arch.ptr(u64::MAX), base, 14, out, 24, out + 32],
        );
        if arch == WinArch::X86 {
            assert_eq!(status, STATUS_INVALID_INFO_CLASS);
            continue;
        }
        assert_eq!(status, STATUS_SUCCESS);
        let (rva, size) = p.vm.section_image_extension(base, kind).unwrap_or((0, 0));
        let mut expected = NativeExtension {
            kind,
            ..Default::default()
        };
        let mut returned = 0;
        // SAFETY: current process, live loaded NTDLL base, fixed class and exact
        // aligned exclusive initialized native outputs. No guest address enters.
        let native_status = unsafe {
            NtQueryVirtualMemory(
                (-1isize) as _,
                native,
                14,
                (&mut expected as *mut NativeExtension).cast(),
                std::mem::size_of::<NativeExtension>(),
                &mut returned,
            )
        };
        assert_eq!(native_status, 0);
        assert_eq!(returned, 24);
        assert_eq!((rva, size), (expected.rva as u64, expected.size as u64));
        assert_eq!(
            (
                p.space.u64(out + 8).unwrap(),
                p.space.u64(out + 16).unwrap()
            ),
            (rva, size)
        );
        assert_eq!(p.space.u64(out + 32).unwrap(), 24);
        if size != 0 {
            let region = p.vm.query(base + rva).unwrap();
            assert_eq!(
                (region.allocation_base, region.state, region.protect),
                (base, mem::COMMIT, prot::EXECUTE_READ)
            );
            let mut bytes = vec![0u8; size as usize];
            let mut copied = 0;
            // SAFETY: native class14 bounded this process-owned module tail.
            // The kernel copies exactly into the initialized exclusive buffer.
            assert_ne!(
                unsafe {
                    ReadProcessMemory(
                        (-1isize) as _,
                        (native as usize + expected.rva) as _,
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                        &mut copied,
                    )
                },
                0
            );
            assert_eq!(copied, bytes.len());
            assert_eq!(p.space.bytes(base + rva, size as usize).unwrap(), bytes);
        }
    }
}
