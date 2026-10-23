use rax::user::image::pe::exports::{ExportDirectory, ExportTarget};
use rax::user::image::pe::{PeImage, dir};
use rax::user::windows::arch::WinArch;
use rax::user::windows::loader::{self, SymRef};
use rax::user::windows::{WindowsConfig, WindowsProcess, process::RunStatus};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const UNKNOWN: u32 = 0x1FEF;

fn number(arch: WinArch, bytes: &[u8]) -> u32 {
    match arch {
        WinArch::Arm64 => (u32::from_le_bytes(bytes[..4].try_into().unwrap()) >> 5) & 0xFFFF,
        WinArch::X64 => u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        WinArch::X86 => u32::from_le_bytes(bytes[1..5].try_into().unwrap()),
    }
}

fn replacement(arch: WinArch) -> (usize, Vec<u8>) {
    match arch {
        WinArch::Arm64 => (0, (0xD400_0001 | (UNKNOWN << 5)).to_le_bytes().to_vec()),
        WinArch::X64 => (4, UNKNOWN.to_le_bytes().to_vec()),
        WinArch::X86 => (1, UNKNOWN.to_le_bytes().to_vec()),
    }
}

fn main() {
    let source_root = std::env::args().nth(1).unwrap();
    let output = std::env::args().nth(2).unwrap();
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
        let directory = if arch == WinArch::X86 && host != WinArch::X86 {
            "SysWOW64"
        } else {
            "System32"
        };
        let system = Path::new("C:/Windows").join(directory).join("win32u.dll");
        let original = std::fs::read(&system).unwrap();
        let pe = PeImage::parse(original.clone()).unwrap();
        assert_eq!(WinArch::from_machine(pe.headers().machine), Some(arch));
        let memory = pe.memory_image();
        let exports = ExportDirectory::read(&memory, pe.headers().directory(dir::EXPORT))
            .unwrap()
            .unwrap();
        let (_, ExportTarget::Rva(rva)) = exports
            .by_name(&memory, b"NtGdiInit2", None)
            .unwrap()
            .unwrap()
        else {
            panic!("native service must be an instruction export")
        };
        let observed = number(arch, &memory[rva as usize..rva as usize + 64]);
        assert_ne!(observed, UNKNOWN);
        let (at, patch) = replacement(arch);
        let file_offset = pe.rva_to_file_offset(rva).unwrap() as usize + at;
        let mut forged = original.clone();
        forged[file_offset..file_offset + 4].copy_from_slice(&patch);
        let folder = Path::new(&output).join(arch.to_string());
        std::fs::create_dir_all(&folder).unwrap();
        let fake = folder.join("win32u.dll");
        assert!(!fake.exists());
        std::fs::write(&fake, &forged).unwrap();
        let image = std::fs::read(
            Path::new(&source_root)
                .join("tests/fixtures/user/windows/bin")
                .join(arch.to_string())
                .join("smoke.exe"),
        )
        .unwrap();
        for case in ["selected", "supplied", "file"] {
            let mut config =
                WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
            config.native_libraries = true;
            config.host_filesystem = true;
            config.arena_bytes = 256 << 20;
            config.slice_insns = 64;
            let mut process = WindowsProcess::spawn_image(config, image.clone()).unwrap();
            let p = process.state_mut();
            let selected = loader::load_dll(p, "win32u.dll").unwrap();
            let entry = loader::lookup(p, selected, &SymRef::Name(b"NtGdiInit2".to_vec(), None))
                .unwrap()
                .unwrap();
            if case != "selected" {
                let path = fake.to_str().unwrap();
                if case == "supplied" {
                    Arc::make_mut(&mut p.cfg).supplied_dlls.insert(
                        path.replace('/', "\\").to_ascii_lowercase(),
                        forged.clone().into(),
                    );
                }
                let other = loader::load_dll(p, path).unwrap();
                assert_ne!(other, selected);
                assert_eq!(
                    p.modules.list[other].name.to_ascii_lowercase(),
                    "win32u.dll"
                );
                p.vm.poke(entry + at as u64, &patch).unwrap();
            }
            let resume = p.traps.callback_return();
            let thread = p.threads.values_mut().next().unwrap();
            let sp = thread.cpu.sp() - 128;
            thread.cpu.set_sp(sp);
            thread.cpu.set_pc(entry);
            match arch {
                WinArch::Arm64 => thread.cpu.set_gpr(30, resume),
                WinArch::X86 => {
                    use rax::user::windows::memory::Mem;
                    p.space.w32(sp, resume as u32).unwrap();
                }
                WinArch::X64 => {
                    use rax::user::windows::memory::Mem;
                    p.space.w64(sp, resume).unwrap();
                }
            }
            let cancel = AtomicBool::new(false);
            let mut terminal = None;
            for _ in 0..32 {
                if let RunStatus::Complete(result) = process.run_slice(1, &cancel) {
                    terminal = Some(format!("{result:?}"));
                    break;
                }
            }
            let terminal = terminal
                .expect("isolated service boundary must terminate within the instruction budget");
            println!("case={case} arch={arch} original-service={observed:#x} terminal={terminal}");
            if case == "selected" {
                assert!(
                    terminal.contains("native Win32u service NtGdiInit2")
                        && terminal.contains("not implemented")
                );
            } else {
                assert!(
                    terminal.contains("0x1fef") && terminal.contains("has no admitted stub"),
                    "forged path or guest stub bytes admitted a service: {terminal}"
                );
                assert!(!terminal.contains("native Win32u service NtGdiInit2"));
            }
            assert!(!terminal.contains("Exited(0)"));
        }
        assert_eq!(std::fs::read(&system).unwrap(), original);
        assert_eq!(std::fs::read(&fake).unwrap(), forged);
    }
}
