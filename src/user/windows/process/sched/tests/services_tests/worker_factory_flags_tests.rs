//! Independent dormant WorkerFactoryFlags native captures and ABI/error checks.
use super::worker_factory_set_tests::{call, factory, maximum};
use super::*;
use crate::user::windows::nt::status::*;
use std::collections::HashMap;
#[cfg(windows)]
#[path = "worker_factory_flags_leaf_tests.rs"]
mod leaf_tests;
const SERVICE: u32 = 0x78;
fn flags_request(p: &Proc, handle: u64) -> Option<u32> {
    let Some(Object::WorkerFactory { flags_request, .. }) = p.objects.get(handle) else {
        panic!("factory missing")
    };
    *flags_request
}
fn field(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}

#[test]
fn native_worker_factory_flags_accepts_measured_unsigned_requests_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = thread.cpu.sp() - 64;
        for value in [0, 1, 2, 3, 7, 0x80000000, u32::MAX] {
            p.space.w32(input, value).unwrap();
            assert_eq!(
                call(p, &mut thread, [handle, 13, input, 4]),
                Outcome::Continue
            );
            assert_eq!(thread.cpu.gpr(0), 0);
            assert_eq!(maximum(p, handle), 512);
            assert_eq!(p.space.u32(input).unwrap(), value);
        }
    }
}

#[test]
fn native_worker_factory_flags_length_capture_type_and_right_priority_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = thread.cpu.sp() - 64;
        p.space.w32(input, 1).unwrap();
        let id = p.objects.id(handle).unwrap();
        let denied = p.objects.open_access(id, false, 0).unwrap() as u64;
        let event = p.objects.insert(Object::Event {
            manual: true,
            signaled: 0,
        }) as u64;
        for (args, status) in [
            ([handle, 13, 0, 0], STATUS_INFO_LENGTH_MISMATCH),
            ([0x13572468, 13, 0, 4], STATUS_ACCESS_VIOLATION),
            ([0x13572468, 13, input, 4], STATUS_INVALID_HANDLE),
            ([event, 13, input, 4], STATUS_OBJECT_TYPE_MISMATCH),
            ([denied, 13, input, 4], STATUS_ACCESS_DENIED),
            ([handle, 13, input, 4], 0),
        ] {
            assert_eq!(call(p, &mut thread, args), Outcome::Continue);
            assert_eq!(thread.cpu.gpr(0), u64::from(status));
            assert_eq!(maximum(p, handle), 512);
        }
    }
}

#[test]
fn native_worker_factory_flags_replays_all_2600_native_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            "arm64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-flags/native/native-worker-factory-flags-v2-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            "x64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-flags/native/native-worker-factory-flags-v2-x64.log"
            ),
        ),
        (
            WinArch::X86,
            "x86",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-flags/native/native-worker-factory-flags-v2-x86.log"
            ),
        ),
        (
            WinArch::X86,
            "x86-laa",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-flags/native/native-worker-factory-flags-v2-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, profile, text) in profiles {
        assert!(text.contains("complete cases=650"));
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
            let id = p.objects.id(handle).unwrap();
            let Some(Object::WorkerFactory {
                maximum: hard_limit,
                reserve,
                commit,
                ..
            }) = p.objects.obj_mut(id)
            else {
                panic!("fixture factory")
            };
            *hard_limit = field(&row, "maximum-before", 16) as u32;
            *reserve = 1 << 20;
            *commit = 4096;
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
                5..=10 | 13..=17 => {
                    let access = if hm == 5 {
                        0
                    } else if hm == 10 {
                        0xF00FF
                    } else if hm >= 13 {
                        [0x10, 0x20, 0x40, 0x80, 0xC0][(hm - 13) as usize]
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
                call(p, &mut t, [selected, 13, input, field(&row, "length", 16)]),
                Outcome::Continue,
                "{profile}/{row:?}"
            );
            assert_eq!(t.cpu.gpr(0), field(&row, "status", 16), "{profile}/{row:?}");
            assert_eq!(
                flags_request(p, handle),
                (field(&row, "status", 16) == 0).then_some(value)
            );
            // Class13 must preserve the independently modeled soft request.
            let Some(Object::WorkerFactory {
                soft_maximum,
                binding_count,
                ..
            }) = p.objects.get(handle)
            else {
                panic!("factory missing")
            };
            assert_eq!(*soft_maximum, None);
            assert_eq!(*binding_count, 0);
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
    assert_eq!(replayed, 2600);
}

#[test]
fn native_worker_factory_flags_alias_lifetime_and_bookkeeping_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let before = (p.objects.handle_count(), p.objects.iter().count());
        let (handle, port) = factory(p);
        assert_eq!(flags_request(p, handle), None);
        let alias = p.objects.duplicate(handle, false).unwrap() as u64;
        let input = t.cpu.sp() - 64;
        for value in [0, 1, 3, 0x80000000, u32::MAX] {
            p.space.w32(input, value).unwrap();
            assert_eq!(call(p, &mut t, [alias, 13, input, 4]), Outcome::Continue);
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(flags_request(p, handle), Some(value));
            assert_eq!(flags_request(p, alias), Some(value));
        }
        let port_id = p.objects.id(port).unwrap();
        p.objects.close(port).unwrap();
        p.objects.close(handle).unwrap();
        assert!(p.objects.obj(port_id).is_some());
        assert_eq!(flags_request(p, alias), Some(u32::MAX));
        p.objects.close(alias).unwrap();
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
    }
}
#[test]
fn native_worker_factory_flags_guard_retry_preserves_previous_request_all_abis() {
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
        p.space.w32(input, 2).unwrap();
        assert_eq!(call(p, &mut t, [handle, 13, input, 4]), Outcome::Continue);
        assert_eq!(flags_request(p, handle), Some(2));
        p.space.w32(input, 1).unwrap();
        p.vm.protect(input, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        for (selected, length, status) in [
            (denied, 3, STATUS_INFO_LENGTH_MISMATCH),
            (denied, 4, STATUS_GUARD_PAGE_VIOLATION),
            (denied, 4, STATUS_ACCESS_DENIED),
            (handle, 4, 0),
        ] {
            assert_eq!(
                call(p, &mut t, [selected, 13, input, length]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), u64::from(status));
            assert_eq!(maximum(p, handle), 512);
            assert_eq!(
                flags_request(p, handle),
                Some(if status == 0 { 1 } else { 2 })
            );
            assert_eq!(
                p.vm.query(input).unwrap().protect,
                if status == STATUS_INFO_LENGTH_MISMATCH {
                    prot::READWRITE | prot::GUARD
                } else {
                    prot::READWRITE
                }
            );
        }
    }
}
#[test]
fn native_worker_factory_flags_interleaves_requests_and_failure_atomicity_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = t.cpu.sp() - 64;
        for (class, value, hard, soft, bindings, flags) in [
            (13, 1, 512, None, 0, Some(1)),
            (14, 768, 512, Some(768), 0, Some(1)),
            (5, 1, 1, Some(768), 0, Some(1)),
            (13, u32::MAX, 1, Some(768), 0, Some(u32::MAX)),
            (3, 4, 1, Some(768), 4, Some(u32::MAX)),
            (13, 0, 1, Some(768), 4, Some(0)),
        ] {
            p.space.w32(input, value).unwrap();
            assert_eq!(
                call(p, &mut t, [handle, class, input, 4]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), 0);
            let Some(Object::WorkerFactory {
                maximum,
                soft_maximum,
                binding_count,
                flags_request: observed,
                ..
            }) = p.objects.get(handle)
            else {
                panic!("factory")
            };
            assert_eq!(
                (*maximum, *soft_maximum, *binding_count, *observed),
                (hard, soft, bindings, flags)
            );
        }
        p.space.w32(input, 7).unwrap();
        for (pointer, length, status) in [
            (0, 4, STATUS_ACCESS_VIOLATION),
            (input, 5, STATUS_INFO_LENGTH_MISMATCH),
        ] {
            assert_eq!(
                call(p, &mut t, [handle, 13, pointer, length]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), u64::from(status));
            assert_eq!(flags_request(p, handle), Some(0));
        }
    }
}
