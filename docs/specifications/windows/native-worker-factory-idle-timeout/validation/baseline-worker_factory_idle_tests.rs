//! Native dormant class2 timeout captures, validation priority and state.
use super::worker_factory_set_tests::{call, factory, maximum};
use super::*;
use crate::user::windows::nt::status::*;
use std::collections::HashMap;
const SERVICE: u32 = 0x78;
fn field(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}
#[test]
fn native_worker_factory_idle_accepts_signed_intervals_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = thread.cpu.sp() - 64;
        for value in [
            0i64,
            1,
            -1,
            -9999999,
            -10000000,
            -10000001,
            -300000000,
            -5999999999,
            -6000000000,
            -6000000001,
            i64::MIN,
            i64::MAX,
        ] {
            p.space.w64(input, value as u64).unwrap();
            assert_eq!(
                call(p, &mut thread, [handle, 2, input, 8]),
                Outcome::Continue
            );
            assert_eq!(
                thread.cpu.gpr(0),
                if value < 0 {
                    0
                } else {
                    u64::from(STATUS_INVALID_PARAMETER)
                }
            );
            assert_eq!(maximum(p, handle), 512);
        }
    }
}
#[test]
fn native_worker_factory_idle_length_alignment_capture_type_access_value_priority_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
        let p = process.state_mut();
        let (handle, _) = factory(p);
        let input = thread.cpu.sp() - 64;
        p.space.w64(input, 0).unwrap();
        let denied = p
            .objects
            .open_access(p.objects.id(handle).unwrap(), false, 0)
            .unwrap() as u64;
        let event = p.objects.insert(Object::Event {
            manual: true,
            signaled: 0,
        }) as u64;
        for (args, status) in [
            ([handle, 2, 1, 7], STATUS_INFO_LENGTH_MISMATCH),
            ([0x13572468, 2, 1, 8], STATUS_DATATYPE_MISALIGNMENT),
            ([0x13572468, 2, 0, 8], STATUS_ACCESS_VIOLATION),
            ([0x13572468, 2, input, 8], STATUS_INVALID_HANDLE),
            ([event, 2, input, 8], STATUS_OBJECT_TYPE_MISMATCH),
            ([denied, 2, input, 8], STATUS_ACCESS_DENIED),
            ([handle, 2, input, 8], STATUS_INVALID_PARAMETER),
            (
                [handle, 2, input, 9],
                if arch == WinArch::X86 {
                    STATUS_INVALID_PARAMETER
                } else {
                    STATUS_INFO_LENGTH_MISMATCH
                },
            ),
        ] {
            assert_eq!(call(p, &mut thread, args), Outcome::Continue);
            assert_eq!(thread.cpu.gpr(0), u64::from(status));
        }
    }
}
#[test]
fn native_worker_factory_idle_replays_all_8944_native_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            "v1-arm64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v1-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            "v1-x64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v1-x64.log"
            ),
        ),
        (
            WinArch::X86,
            "v1-x86",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v1-x86.log"
            ),
        ),
        (
            WinArch::X86,
            "v1-x86-laa",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v1-x86-laa.log"
            ),
        ),
        (
            WinArch::Arm64,
            "v2-arm64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v2-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            "v2-x64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v2-x64.log"
            ),
        ),
        (
            WinArch::X86,
            "v2-x86",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v2-x86.log"
            ),
        ),
        (
            WinArch::X86,
            "v2-x86-laa",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v2-x86-laa.log"
            ),
        ),
        (
            WinArch::Arm64,
            "v3-arm64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v3-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            "v3-x64",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v3-x64.log"
            ),
        ),
        (
            WinArch::X86,
            "v3-x86",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v3-x86.log"
            ),
        ),
        (
            WinArch::X86,
            "v3-x86-laa",
            include_str!(
                "../../../../../../../docs/specifications/windows/native-worker-factory-idle-timeout/native/native-worker-factory-idle-v3-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, profile, text) in profiles {
        let rows: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("case="))
            .collect();
        assert!(text.contains(&format!("complete cases={}", rows.len())));
        assert!(!rows.is_empty());
        for line in rows {
            let row: HashMap<_, _> = line
                .split_whitespace()
                .map(|f| f.split_once('=').unwrap())
                .collect();
            let (mut process, mut t) = fixture(arch, "NtSetInformationWorkerFactory", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let (handle, port) = factory(p);
            let seed = t.cpu.sp() - 64;
            p.space.w32(seed, 1).unwrap();
            assert_eq!(call(p, &mut t, [handle, 13, seed, 4]), Outcome::Continue);
            assert_eq!(t.cpu.gpr(0), 0);
            let basic = decode_hex(row["before"]);
            let offset = if arch == WinArch::X86 { 84 } else { 96 };
            let width = arch.ptr_size() as usize;
            let sized = |at: usize| -> u64 {
                basic[at..at + width]
                    .iter()
                    .enumerate()
                    .fold(0, |v, (i, b)| v | (u64::from(*b) << (8 * i)))
            };
            let Some(Object::WorkerFactory {
                reserve, commit, ..
            }) = p.objects.obj_mut(p.objects.id(handle).unwrap())
            else {
                panic!("factory")
            };
            *reserve = sized(offset);
            *commit = sized(offset + width);
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
                _ => panic!("handle mode"),
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
                6 | 9..=11 => memory + 1,
                7 => memory + PAGE_SIZE - 8,
                8 | 12..=20 => memory + PAGE_SIZE - 4,
                21 | 22 => memory + PAGE_SIZE - 3,
                _ => memory,
            };
            let value = field(&row, "value", 16);
            if !matches!(mode, 1 | 2) {
                p.space.w64(input, value).unwrap();
            }
            let mut first = match mode {
                3 | 10 | 19 => prot::READONLY,
                4 | 11 | 16 => prot::NOACCESS,
                5 | 9 | 15 | 17 | 18 | 20 | 21 | 22 => prot::READWRITE | prot::GUARD,
                _ => prot::READWRITE,
            };
            let second = match mode {
                8 | 15 | 21 => prot::NOACCESS,
                13 | 20 => prot::READONLY,
                14 | 16 | 17 | 19 | 22 => prot::READWRITE | prot::GUARD,
                _ => prot::READWRITE,
            };
            p.vm.protect(memory, PAGE_SIZE, first).unwrap();
            p.vm.protect(memory + PAGE_SIZE, PAGE_SIZE, second).unwrap();
            assert_eq!(
                call(p, &mut t, [selected, 2, input, field(&row, "length", 16)]),
                Outcome::Continue,
                "{profile}/{row:?}"
            );
            assert_eq!(t.cpu.gpr(0), field(&row, "status", 16), "{profile}/{row:?}");
            assert_eq!(maximum(p, handle), 512);
            let Some(Object::WorkerFactory {
                soft_maximum,
                flags_request,
                binding_count,
                ..
            }) = p.objects.get(handle)
            else {
                panic!("factory")
            };
            assert_eq!(
                (*soft_maximum, *flags_request, *binding_count),
                (None, Some(1), 0)
            );
            first = field(&row, "protection", 16) as u32;
            assert_eq!(
                p.vm.query(memory).unwrap().protect,
                first,
                "{profile}/{row:?}"
            );
            if row.contains_key("protection-second") {
                assert_eq!(
                    p.vm.query(memory + PAGE_SIZE).unwrap().protect,
                    field(&row, "protection-second", 16) as u32,
                    "{profile}/{row:?}"
                );
            }
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
    assert_eq!(replayed, 8944);
}
fn decode_hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
