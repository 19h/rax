//! Execute the installed Win32u leaf and retain its explicit named frontier.
use super::*;
use crate::user::windows::{
    layout,
    loader::{self, SymRef},
    process::{WindowsConfig, WindowsProcess},
};

#[test]
fn win32u_installed_init2_leaf_reaches_named_unsupported_boundary_without_false_completion() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    for arch in if host == WinArch::X86 {
        vec![host]
    } else {
        vec![host, WinArch::X86]
    } {
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
            WindowsConfig::embedded("C:\\win32u-leaf.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        config.arena_bytes = 256 << 20;
        let mut process = WindowsProcess::spawn_image(config, image.to_vec()).unwrap();
        let p = process.state_mut();
        let tid = *p.threads.keys().next().unwrap();
        let mut t = p.threads.remove(&tid).unwrap();
        let module = loader::load_dll(p, "win32u.dll").unwrap();
        assert_eq!(p.modules.win32u_services.as_ref().unwrap().0, module);
        let entry = loader::lookup(p, module, &SymRef::Name(b"NtGdiInit2".to_vec(), None))
            .unwrap()
            .unwrap();
        let sp = t.cpu.sp() - 128;
        let resume = p.traps.callback_return();
        t.cpu.set_sp(sp);
        t.cpu.set_pc(entry);
        match arch {
            WinArch::X86 => p.space.w32(sp, resume as u32).unwrap(),
            WinArch::X64 => p.space.w64(sp, resume).unwrap(),
            WinArch::Arm64 => t.cpu.set_gpr(30, resume),
        }
        let error = t.teb + layout::offsets(arch).teb_last_error;
        p.space.w32(error, 0x1234_5678).unwrap();
        let rng = p.rng;
        let handles = p.objects.handle_count();
        let mut named = false;
        for _ in 0..32 {
            let boundary = t.cpu.run(64);
            assert_ne!(
                t.cpu.pc(),
                resume,
                "unsupported Win32u leaf cannot return success"
            );
            match handle_stop(p, &mut t, boundary) {
                Outcome::Continue | Outcome::Yield => (),
                Outcome::Fail(reason) => {
                    assert!(
                        reason.contains("native Win32u service NtGdiInit2")
                            && reason.contains("not implemented"),
                        "{arch}: {reason}"
                    );
                    named = true;
                    break;
                }
                other => panic!("{arch}: {other:?}"),
            }
        }
        assert!(named, "{arch}");
        assert_eq!(p.space.u32(error).unwrap(), 0x1234_5678);
        assert_eq!(p.rng, rng);
        assert_eq!(p.objects.handle_count(), handles);
        assert!(t.frames.is_empty());
    }
}
