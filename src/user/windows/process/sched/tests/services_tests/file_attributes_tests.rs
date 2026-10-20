//! Compiled NT file metadata queries through the native service dispatcher.

#[path = "file_attributes_tests/semantics.rs"]
mod semantics;

#[path = "file_attributes_tests/validation.rs"]
mod validation;

#[path = "file_attributes_tests/paired.rs"]
mod paired;

#[cfg(windows)]
#[path = "file_attributes_tests/installed.rs"]
mod installed;

use super::{Outcome, fixture, handle_stop, stop};
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::loader::services::tests::table;
use crate::user::windows::memory::{Mem, mem, prot};
use crate::user::windows::nt::status::{
    STATUS_ACCESS_DENIED, STATUS_GUARD_PAGE_VIOLATION, STATUS_INVALID_PARAMETER, STATUS_SUCCESS,
};
use crate::user::windows::process::{Proc, Thread, WindowsProcess};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "rax-native-file-attributes-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("private fixture directory: {error}"),
            }
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove private metadata fixtures");
    }
}

struct Harness {
    process: WindowsProcess,
    thread: Thread,
    base: u64,
    directory: Directory,
}

impl Harness {
    fn new(arch: WinArch) -> Self {
        let directory = Directory::new();
        std::fs::write(directory.0.join("fixture.bin"), b"hello").unwrap();
        let (mut process, thread) = fixture(arch, "NtQueryAttributesFile", 0x3d);
        let p = process.state_mut();
        let config = Arc::make_mut(&mut p.cfg);
        config.host_filesystem = true;
        config.drives = crate::user::windows::fs::DriveMap::empty();
        config.drives.set('C', &directory.0);
        let base =
            p.vm.allocate(
                None,
                8 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        p.space
            .wr(base, &vec![0xa5; 8 * PAGE_SIZE as usize])
            .unwrap();
        let mut h = Self {
            process,
            thread,
            base,
            directory,
        };
        h.name("\\??\\C:\\fixture.bin");
        h
    }

    fn attrs(&self) -> u64 {
        self.base + 2 * PAGE_SIZE
    }

    fn name(&mut self, name: &str) {
        let at = self.attrs();
        let p = self.process.state_mut();
        let width = p.arch.ptr_size();
        let size = if p.arch.is64() { 48 } else { 24 };
        p.space.wr(at, &vec![0; size]).unwrap();
        p.space.w32(at, size as u32).unwrap();
        p.space.wptr(at + 2 * width, width, at + 64).unwrap();
        p.space.w32(at + 3 * width, 0x840).unwrap();
        let units: Vec<u16> = name.encode_utf16().collect();
        p.space.w16(at + 64, (units.len() * 2) as u16).unwrap();
        p.space.w16(at + 66, 0).unwrap();
        p.space.wptr(at + 64 + width, width, at + 128).unwrap();
        p.space.put_wunits(at + 128, &units).unwrap();
    }

    fn query(&mut self, name: &str, attrs: u64, output: u64) -> u32 {
        let p = self.process.state_mut();
        let module = p.modules.nt_services.as_ref().unwrap().0;
        p.modules.nt_services = Some((module, table(p.arch, name, 0x3d)));
        let sp = self.thread.cpu.sp();
        arguments(p, &mut self.thread, &[attrs, output]);
        let result = if p.arch == WinArch::X86 {
            crate::user::windows::process::services::wow64(p, &mut self.thread, 0x1234_0000)
        } else {
            handle_stop(p, &mut self.thread, stop(p.arch, 0x3d))
        };
        assert_eq!(result, Outcome::Continue, "{}/{name}", p.arch);
        assert_eq!(self.thread.cpu.pc(), 0x1234_0004);
        assert_eq!(
            self.thread.cpu.sp(),
            sp + if p.arch == WinArch::X86 { 4 } else { 0 }
        );
        assert!(self.thread.frames.is_empty());
        self.thread.cpu.set_sp(sp);
        self.thread.cpu.gpr(0) as u32
    }
}

fn arguments(p: &Proc, thread: &mut Thread, args: &[u64]) {
    let sp = thread.cpu.sp();
    thread.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            p.space.w32(sp, 0x1234_0004).unwrap();
            p.space.w32(sp + 4, 0x1234_0000).unwrap();
            for (index, value) in args.iter().enumerate() {
                p.space
                    .w32(sp + 8 + index as u64 * 4, *value as u32)
                    .unwrap();
            }
            thread.cpu.set_gpr(0, 0x3d);
        }
        WinArch::X64 => {
            thread.cpu.set_gpr(0, 0x3d);
            thread.cpu.set_gpr(1, 0x1234_0004);
            for (index, value) in args.iter().enumerate() {
                thread.cpu.set_gpr([10, 2][index], *value);
            }
        }
        WinArch::Arm64 => {
            for (index, value) in args.iter().enumerate() {
                thread.cpu.set_gpr(index, *value);
            }
        }
    }
}

#[test]
fn native_file_attributes_returns_mapped_metadata_and_preserves_padding() {
    for arch in WinArch::ALL {
        for name in ["NtQueryAttributesFile", "NtQueryFullAttributesFile"] {
            let mut h = Harness::new(arch);
            assert_eq!(h.query(name, h.attrs(), h.base), STATUS_SUCCESS);
            let p = h.process.state();
            assert_ne!(p.space.u64(h.base + 16).unwrap(), 0);
            let (attribute, copied) = if name == "NtQueryFullAttributesFile" {
                assert_eq!(p.space.u64(h.base + 40).unwrap(), 5);
                (48, 56)
            } else {
                (32, if arch == WinArch::X86 { 36 } else { 40 })
            };
            assert_ne!(p.space.u32(h.base + attribute).unwrap(), 0);
            if copied > attribute + 4 {
                assert_eq!(p.space.u32(h.base + attribute + 4).unwrap(), 0);
            }
            assert_eq!(p.space.bytes(h.base + copied, 8).unwrap(), [0xa5; 8]);
            assert!(h.directory.0.join("fixture.bin").is_file());
        }
    }
}

#[test]
fn native_file_attributes_output_validation_precedes_native_input() {
    for arch in WinArch::ALL {
        for name in ["NtQueryAttributesFile", "NtQueryFullAttributesFile"] {
            let mut h = Harness::new(arch);
            let attrs = h.attrs();
            let output = h.base + PAGE_SIZE;
            let p = h.process.state_mut();
            p.space.w32(attrs, 0).unwrap();
            p.vm.protect(output, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
            assert_eq!(
                h.query(name, attrs, output),
                if arch == WinArch::X86 {
                    STATUS_INVALID_PARAMETER
                } else {
                    STATUS_GUARD_PAGE_VIOLATION
                }
            );
            let guard = h.process.state().vm.query(output).unwrap().protect & prot::GUARD;
            assert_eq!(guard != 0, arch == WinArch::X86);
        }
    }
}

#[test]
fn native_file_attributes_rejects_host_access_when_disabled() {
    for arch in WinArch::ALL {
        let mut h = Harness::new(arch);
        Arc::make_mut(&mut h.process.state_mut().cfg).host_filesystem = false;
        assert_eq!(
            h.query("NtQueryAttributesFile", h.attrs(), h.base),
            STATUS_ACCESS_DENIED
        );
        assert_eq!(
            h.process.state().space.bytes(h.base, 64).unwrap(),
            [0xa5; 64]
        );
    }
}
