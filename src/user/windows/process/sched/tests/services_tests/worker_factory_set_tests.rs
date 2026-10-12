//! Independent installed-kernel maximum-setting captures and ownership checks.
use super::*;
use crate::user::windows::nt::status::*;
use std::collections::HashMap;

#[cfg(windows)]
#[path = "worker_factory_set_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x78;
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 4]) -> Outcome {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (i, value) in [0x1234_0004, 0x1234_0000]
                .into_iter()
                .chain(args)
                .enumerate()
            {
                p.space.w32(sp + i as u64 * 4, value as u32).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for (register, value) in [10, 2, 8, 9].into_iter().zip(args) {
                t.cpu.set_gpr(register, value);
            }
        }
        WinArch::Arm64 => {
            for (i, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(i, value);
            }
        }
    }
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    if outcome == Outcome::Continue {
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.frames.is_empty());
    }
    t.cpu.set_sp(sp);
    outcome
}
fn factory(p: &mut Proc) -> (u64, u64) {
    let port = p.objects.insert(Object::CompletionPort {
        queue: Default::default(),
        concurrency: 4,
    });
    let factory = p.objects.insert(Object::WorkerFactory {
        port: p.objects.id(port.into()).unwrap(),
        process: None,
        pid: p.pid,
        start: 0x12345678,
        parameter: 0x87654321,
        maximum: 512,
        reserve: 65536,
        commit: 4096,
    });
    (factory.into(), port.into())
}
fn maximum(p: &Proc, handle: u64) -> u32 {
    let Some(Object::WorkerFactory { maximum, .. }) = p.objects.get(handle) else {
        panic!("factory missing")
    };
    *maximum
}
fn field(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}

#[test]
fn native_worker_factory_set_maximum_dispatches_and_preserves_alias_references_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let before = (p.objects.handle_count(), p.objects.iter().count());
        let (handle, port) = factory(p);
        let alias = p.objects.duplicate(handle, false).unwrap() as u64;
        let input = t.cpu.sp() - 64;
        for value in [0, 1, 512, 768, 0x80000000, u32::MAX] {
            p.space.w32(input, value).unwrap();
            assert_eq!(call(p, &mut t, [alias, 5, input, 4]), Outcome::Continue);
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(maximum(p, handle), value);
            assert_eq!(p.space.u32(input).unwrap(), value);
        }
        let id = p.objects.id(port).unwrap();
        p.objects.close(port).unwrap();
        assert!(p.objects.obj(id).is_some());
        p.objects.close(handle).unwrap();
        assert_eq!(maximum(p, alias), u32::MAX);
        p.objects.close(alias).unwrap();
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
    }
}

#[test]
fn native_worker_factory_set_maximum_replays_all_956_native_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            "arm64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-set-maximum/native-worker-factory-set-maximum-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            "x64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-set-maximum/native-worker-factory-set-maximum-x64.log"
            ),
        ),
        (
            WinArch::X86,
            "x86",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-set-maximum/native-worker-factory-set-maximum-x86.log"
            ),
        ),
        (
            WinArch::X86,
            "x86-laa",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-set-maximum/native-worker-factory-set-maximum-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, profile, text) in profiles {
        assert!(text.contains("complete cases=239"));
        for line in text.lines().filter(|line| line.starts_with("case=")) {
            let row: HashMap<_, _> = line
                .split_whitespace()
                .map(|f| f.split_once('=').unwrap())
                .collect();
            assert_eq!(row["query-before"], "00000000");
            assert_eq!(row["query-after"], "00000000");
            assert_eq!(row["handles-before"], row["handles-after"]);
            assert_eq!(row["total-before"], "0");
            assert_eq!(row["total-after"], "0");
            assert_eq!(field(&row, "width", 10), arch.ptr_size());
            let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let (handle, port) = factory(p);
            let mode = field(&row, "mode", 10);
            let hm = field(&row, "handle-mode", 10);
            let event = if matches!(hm, 3 | 4) {
                Some(p.objects.insert(Object::Event {
                    manual: true,
                    signaled: 0,
                }) as u64)
            } else {
                None
            };
            let mut alias = None;
            let selected = match hm {
                0 => handle,
                1 => 0,
                2 => 0x13572468,
                3 => event.unwrap(),
                4 => {
                    let h = p
                        .objects
                        .open_access(p.objects.id(event.unwrap()).unwrap(), false, 0)
                        .unwrap() as u64;
                    alias = Some(h);
                    h
                }
                5..=10 => {
                    let access = if hm == 5 {
                        0
                    } else if hm == 10 {
                        0xF003F
                    } else {
                        1u32 << (hm - 6)
                    };
                    let h = p
                        .objects
                        .open_access(p.objects.id(handle).unwrap(), false, access)
                        .unwrap() as u64;
                    alias = Some(h);
                    h
                }
                11 => arch.ptr(u64::MAX),
                12 => arch.ptr(u64::MAX - 1),
                _ => panic!("unknown handle mode"),
            };
            let memory =
                p.vm.allocate(
                    None,
                    2 * PAGE_SIZE,
                    mem::RESERVE | mem::COMMIT,
                    prot::READWRITE,
                )
                .unwrap()
                .0;
            let input = match mode {
                1 => 0,
                2 => 1,
                6 => memory + 1,
                7 => memory + PAGE_SIZE - 4,
                8 => memory + PAGE_SIZE - 2,
                _ => memory,
            };
            let value = field(&row, "value", 16) as u32;
            if !matches!(mode, 1 | 2) {
                p.space.w32(input, value).unwrap();
            }
            if let Some(protection) = match mode {
                3 => Some(prot::READONLY),
                4 => Some(prot::NOACCESS),
                5 => Some(prot::READWRITE | prot::GUARD),
                _ => None,
            } {
                p.vm.protect(memory, PAGE_SIZE, protection).unwrap();
            }
            if mode == 8 {
                p.vm.protect(memory + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            assert_eq!(
                call(p, &mut t, [selected, 5, input, field(&row, "length", 16)]),
                Outcome::Continue,
                "{profile}/{row:?}"
            );
            assert_eq!(t.cpu.gpr(0), field(&row, "status", 16), "{profile}/{row:?}");
            assert_eq!(
                maximum(p, handle),
                field(&row, "maximum-after", 16) as u32,
                "{profile}/{row:?}"
            );
            assert_eq!(
                p.vm.query(memory).unwrap().protect,
                u32::try_from(field(&row, "protection", 16)).unwrap()
            );
            if let Some(alias) = alias {
                p.objects.close(alias).unwrap();
            }
            if let Some(event) = event {
                p.objects.close(event).unwrap();
            }
            p.objects.close(handle).unwrap();
            p.objects.close(port).unwrap();
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 956);
}

#[test]
fn native_worker_factory_set_maximum_refuses_unmeasured_classes_without_state_mutation() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = t.cpu.sp() - 64;
        p.space.w32(input, 768).unwrap();
        assert!(
            matches!(call(p,&mut t,[handle,4,input,4]),Outcome::Fail(message) if message.contains("class 4"))
        );
        assert_eq!(maximum(p, handle), 512);
    }
}

#[test]
fn native_worker_factory_set_maximum_consumes_one_guard_before_access_check_and_retry() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let denied = p
            .objects
            .open_access(p.objects.id(handle).unwrap(), false, 0)
            .unwrap() as u64;
        let input =
            p.vm.allocate(None, PAGE_SIZE, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
        p.space.w32(input, 768).unwrap();
        p.vm.protect(input, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        for (selected, length, expected) in [
            (denied, 3, STATUS_INFO_LENGTH_MISMATCH),
            (denied, 4, STATUS_GUARD_PAGE_VIOLATION),
            (denied, 4, STATUS_ACCESS_DENIED),
            (handle, 4, 0),
        ] {
            assert_eq!(
                call(p, &mut t, [selected, 5, input, length]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), u64::from(expected));
            assert_eq!(maximum(p, handle), if expected == 0 { 768 } else { 512 });
            assert_eq!(
                p.vm.query(input).unwrap().protect,
                if expected == STATUS_INFO_LENGTH_MISMATCH {
                    prot::READWRITE | prot::GUARD
                } else {
                    prot::READWRITE
                }
            );
        }
        assert_eq!(p.space.u32(input).unwrap(), 768);
    }
}
