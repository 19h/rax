//! Independent 264-case matrices for all four native execution views.
use super::*;
use crate::user::windows::fs::{FileIdentity, FileLifetime};
use crate::user::windows::objects::FileObj;
use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn disk() -> (Object, PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rax-volume-device-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(b"volume-query").unwrap();
    file.seek(SeekFrom::Start(7)).unwrap();
    let identity = FileIdentity::of_open(&path, Some(&file), &file.metadata().unwrap()).unwrap();
    (
        Object::File(FileObj {
            host: Some(file),
            host_path: path.clone(),
            path: "C:\\fixture.bin".into(),
            access: 0x12019f,
            share: 7,
            lifetime: Arc::new(FileLifetime::new(identity)),
            null: false,
            append: false,
            delete_on_close: false,
            directory: false,
            overlapped: false,
        }),
        path,
    )
}

fn hex(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn value(raw: &str) -> u32 {
    u32::from_str_radix(raw, 16).unwrap()
}

#[test]
fn native_volume_device_query_replays_all_1056_pointer_guard_alias_and_kind_observations() {
    let controls: [(WinArch, &str); 4] = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../../docs/specifications/windows/native-volume-query/validation/native-volume-query-v4-four-views-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../../docs/specifications/windows/native-volume-query/validation/native-volume-query-v4-four-views-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../../docs/specifications/windows/native-volume-query/validation/native-volume-query-v4-four-views-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../../docs/specifications/windows/native-volume-query/validation/native-volume-query-v4-four-views-x86-laa.log"
            ),
        ),
    ];
    let mut failures = Vec::new();
    let mut total = 0;
    for (profile_index, (arch, text)) in controls.into_iter().enumerate() {
        let mut count = 0;
        for line in text.lines().filter(|s| s.starts_with("role=")) {
            let fields: BTreeMap<_, _> = line
                .split_whitespace()
                .map(|s| s.split_once('=').unwrap())
                .collect();
            let (mut process, mut t, base) = setup(arch);
            let p = process.state_mut();
            let ib = base;
            let ob = base + 3 * PAGE_SIZE;
            let mut path = None;
            let handle = if fields["role"] == "invalid" {
                0xDEAD
            } else {
                let object = match fields["role"] {
                    "conin" => Object::Console(StdStream::In),
                    "nul" => Object::Null,
                    "pipe-read" => Object::Pipe {
                        pipe: 1,
                        write: false,
                    },
                    "event" => Object::Event {
                        manual: false,
                        signaled: 0,
                    },
                    "disk" => {
                        let (object, name) = disk();
                        path = Some(name);
                        object
                    }
                    role => panic!("unrecognized native role {role}"),
                };
                let id = p.objects.create(object);
                u64::from(p.objects.open_access(id, false, 0).unwrap())
            };
            let (mut io, mut out) = (ib + PAGE_SIZE + 256, ob + PAGE_SIZE + 256);
            let guarded = prot::READWRITE | prot::GUARD;
            match fields["case"] {
                "valid" => {}
                "io-null" => io = 0,
                "out-null" => out = 0,
                "both-null" => {
                    io = 0;
                    out = 0;
                }
                "io-bad" => io = 0xDEAD0000,
                "out-bad" => out = 0xDEAD0000,
                "io-readonly" => {
                    p.vm.protect(ib + PAGE_SIZE, PAGE_SIZE, prot::READONLY)
                        .unwrap();
                }
                "out-readonly" => {
                    p.vm.protect(ob + PAGE_SIZE, PAGE_SIZE, prot::READONLY)
                        .unwrap();
                }
                "io-guard" => {
                    p.vm.protect(ib + PAGE_SIZE, PAGE_SIZE, guarded).unwrap();
                }
                "out-guard" => {
                    p.vm.protect(ob + PAGE_SIZE, PAGE_SIZE, guarded).unwrap();
                }
                "io-noaccess" => {
                    p.vm.protect(ib + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                "out-noaccess" => {
                    p.vm.protect(ob + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                "io-unaligned" => io += 1,
                "out-unaligned" => out += 1,
                "same" => out = io,
                "out-io-plus4" => out = io + 4,
                "io-out-plus4" => io = out + 4,
                "io-cross-noaccess" => {
                    io = ib + 2 * PAGE_SIZE - 8;
                    p.vm.protect(ib + 2 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                "out-cross-noaccess" => {
                    out = ob + 2 * PAGE_SIZE - 4;
                    p.vm.protect(ob + 2 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                "both-guard" => {
                    p.vm.protect(ib + PAGE_SIZE, PAGE_SIZE, guarded).unwrap();
                    p.vm.protect(ob + PAGE_SIZE, PAGE_SIZE, guarded).unwrap();
                }
                "io-readonly-guard" => {
                    p.vm.protect(ib + PAGE_SIZE, PAGE_SIZE, prot::READONLY | prot::GUARD)
                        .unwrap();
                }
                "out-readonly-guard" => {
                    p.vm.protect(ob + PAGE_SIZE, PAGE_SIZE, prot::READONLY | prot::GUARD)
                        .unwrap();
                }
                case => panic!("unrecognized native case {case}"),
            }
            let before = p.objects.iter().map(|(id, _)| id).collect::<Vec<_>>();
            let status = invoke(
                p,
                &mut t,
                &[handle, io, out, fields["len"].parse().unwrap(), 4],
            );
            let protections = [
                ("io-protect", ib + PAGE_SIZE),
                ("out-protect", ob + PAGE_SIZE),
                ("io-tail", ib + 2 * PAGE_SIZE),
                ("out-tail", ob + 2 * PAGE_SIZE),
            ];
            let mut ok = status == value(fields["status"]) && fields["returned"] == "1";
            for (name, address) in protections {
                ok &= p.vm.query(address).unwrap().protect == value(fields[name]);
            }
            p.vm.protect(base, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            for (name, address, size) in [
                ("io", ib + PAGE_SIZE + 240, 96),
                ("out", ob + PAGE_SIZE + 240, 96),
                ("io-edge", ib + 2 * PAGE_SIZE - 32, 64),
                ("out-edge", ob + 2 * PAGE_SIZE - 32, 64),
            ] {
                ok &= p.space.bytes(address, size).unwrap() == hex(fields[name]);
            }
            ok &= p.objects.iter().map(|(id, _)| id).collect::<Vec<_>>() == before;
            if let Some(Object::File(file)) = p.objects.get_mut(handle) {
                ok &= file.host.as_mut().unwrap().stream_position().unwrap() == 7;
            }
            if !ok {
                failures.push(format!(
                    "profile{profile_index}/{arch}/{}/len{} expected{} got{status:08X} role{}",
                    fields["case"], fields["len"], fields["status"], fields["role"]
                ));
            }
            if handle != 0xDEAD {
                drop(p.objects.close(handle).unwrap());
            }
            if let Some(path) = path {
                std::fs::remove_file(path).unwrap();
            }
            count += 1;
            total += 1;
        }
        assert_eq!(count, 264);
    }
    assert_eq!(total, 1056);
    assert!(
        failures.is_empty(),
        "{} mismatches: {:?}",
        failures.len(),
        &failures[..failures.len().min(12)]
    );
}
