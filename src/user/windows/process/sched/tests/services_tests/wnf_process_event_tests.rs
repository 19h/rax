//! Installed-kernel WNF event registration and reference-lifetime profile.
use super::*;
use crate::user::windows::nt::status::*;
use std::collections::HashMap;

#[cfg(windows)]
#[path = "wnf_process_event_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x1C5;
const ALREADY: u64 = 0xC0000718;
fn call(p: &mut Proc, t: &mut Thread, handle: u64) -> Outcome {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (i, value) in [0x1234_0004, 0x1234_0000, handle].into_iter().enumerate() {
                p.space.w32(sp + i as u64 * 4, value as u32).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(10, handle);
            t.cpu.set_gpr(1, 0x1234_0004);
        }
        WinArch::Arm64 => t.cpu.set_gpr(0, handle),
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
fn check(p: &mut Proc, t: &mut Thread, handle: u64, expected: u64) {
    assert_eq!(call(p, t, handle), Outcome::Continue);
    assert_eq!(t.cpu.gpr(0), expected);
}
fn event(p: &mut Proc, manual: bool, signaled: i32) -> u64 {
    p.objects.insert(Object::Event { manual, signaled }).into()
}
fn selected(p: &mut Proc, arch: WinArch, handle: u64, mode: usize) -> (u64, Option<u64>) {
    match mode {
        0 => (handle, None),
        1 => (0, None),
        2 => (0x13572468, None),
        3 => (arch.ptr(u64::MAX), None),
        4 => (arch.ptr(u64::MAX - 1), None),
        5..=10 => {
            let grant = [0, 1, 2, 3, 0x100000, 0x1F0003][mode - 5];
            let alias = p
                .objects
                .open_access(p.objects.id(handle).unwrap(), false, grant)
                .unwrap() as u64;
            (alias, Some(alias))
        }
        11 => {
            let other = p.objects.insert(Object::Semaphore { count: 0, max: 1 }) as u64;
            (other, Some(other))
        }
        _ => panic!("unknown mode"),
    }
}
fn rows(text: &str) -> Vec<HashMap<&str, &str>> {
    text.lines()
        .filter(|x| x.starts_with("case="))
        .map(|line| {
            line.split_whitespace()
                .map(|f| f.split_once('=').unwrap())
                .collect()
        })
        .collect()
}
fn field(row: &HashMap<&str, &str>, name: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[name], radix).unwrap()
}

#[test]
fn native_wnf_process_event_first_registration_aliases_and_process_drain_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetWnfProcessNotificationEvent", SERVICE);
        let p = process.state_mut();
        let h = event(p, true, 1);
        let id = p.objects.id(h).unwrap();
        let alias = p.objects.duplicate(h, false).unwrap() as u64;
        let count = p.objects.handle_count();
        check(p, &mut t, alias, 0);
        check(p, &mut t, h, ALREADY);
        assert_eq!(p.objects.handle_count(), count);
        p.objects.close(h).unwrap();
        p.objects.close(alias).unwrap();
        assert!(matches!(
            p.objects.obj(id),
            Some(Object::Event {
                manual: true,
                signaled: 1
            })
        ));
        let replacement = event(p, false, 0);
        assert_eq!(replacement, h, "handle reused");
        let replacement_id = p.objects.id(replacement).unwrap();
        check(p, &mut t, replacement, ALREADY);
        p.objects.close(replacement).unwrap();
        assert!(
            p.objects.obj(replacement_id).is_none(),
            "busy call must not pin replacement"
        );
        p.objects.drain().for_each(drop);
        assert_eq!(p.objects.iter().count(), 0);
        let next = event(p, false, 0);
        check(p, &mut t, next, 0);
    }
}

#[test]
fn native_wnf_process_event_replays_all_48_fresh_native_sequences() {
    let text = include_str!(
        "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-process-event-fresh-arm64.log"
    );
    assert!(text.contains("complete cases=48 warmup=1"));
    let observed = rows(text);
    assert_eq!(observed.len(), 49);
    let mut replayed = 0;
    for row in observed.iter().filter(|r| r["warmup"] == "0") {
        assert_eq!(row["calls"], "4");
        assert_eq!(row["exit"], "77");
        assert_eq!(row["query-before"], "00000000");
        assert_eq!(row["query-after"], "00000000");
        assert_eq!(row["handles-before"], row["handles-after"]);
        let (mut process, mut t) =
            fixture(WinArch::Arm64, "NtSetWnfProcessNotificationEvent", SERVICE);
        let p = process.state_mut();
        let manual = row["manual"] == "1";
        let initial = field(row, "initial", 10) as i32;
        let h = event(p, manual, initial);
        let id = p.objects.id(h).unwrap();
        let mode = field(row, "mode", 10) as usize;
        let (selected, alias) = selected(p, WinArch::Arm64, h, mode);
        check(p, &mut t, selected, field(row, "status", 16));
        check(p, &mut t, selected, field(row, "repeat", 16));
        if let Some(alias) = alias {
            p.objects.close(alias).unwrap();
        }
        p.objects.close(h).unwrap();
        assert_eq!(p.objects.obj(id).is_some(), row["status"] == "00000000");
        if let Some(Object::Event {
            manual: actual,
            signaled,
        }) = p.objects.obj(id)
        {
            assert_eq!((*actual, *signaled), (manual, initial));
        }
        let replacement = event(p, true, 0);
        check(p, &mut t, replacement, field(row, "replacement", 16));
        check(p, &mut t, replacement, field(row, "replacement-repeat", 16));
        p.objects.drain().for_each(drop);
        replayed += 1;
    }
    assert_eq!(replayed, 48);
}

#[test]
fn native_wnf_process_event_replays_all_192_initialized_native_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-process-event-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-process-event-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-process-event-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-process-event-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, text) in profiles {
        assert!(text.contains("complete cases=48"));
        let observed = rows(text);
        assert_eq!(observed.len(), 48);
        for row in observed {
            let (mut process, mut t) = fixture(arch, "NtSetWnfProcessNotificationEvent", SERVICE);
            let p = process.state_mut();
            let registered = event(p, false, 0);
            check(p, &mut t, registered, 0);
            check(p, &mut t, 0, field(&row, "reset", 16));
            let manual = row["manual"] == "1";
            let initial = field(&row, "initial", 10) as i32;
            let h = event(p, manual, initial);
            let (selected, _) = selected(p, arch, h, field(&row, "mode", 10) as usize);
            let before = (p.objects.handle_count(), p.objects.iter().count());
            check(p, &mut t, selected, field(&row, "status", 16));
            check(p, &mut t, selected, field(&row, "repeat", 16));
            check(p, &mut t, 0, field(&row, "clear", 16));
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            assert!(
                matches!(p.objects.get(h), Some(Object::Event { manual: m, signaled: s }) if (*m,*s)==(manual,initial))
            );
            p.objects.drain().for_each(drop);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 192);
}

#[test]
fn native_wnf_process_event_type_and_access_errors_precede_busy_without_pins() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSetWnfProcessNotificationEvent", SERVICE);
        let p = process.state_mut();
        let h = event(p, false, 0);
        for busy in [false, true] {
            if busy {
                check(p, &mut t, h, 0);
            }
            for mode in 1..12 {
                let (handle, alias) = selected(p, arch, h, mode);
                let expected = match mode {
                    1 | 2 => STATUS_INVALID_HANDLE,
                    3 | 4 | 11 => STATUS_OBJECT_TYPE_MISMATCH,
                    5 | 6 | 9 => STATUS_ACCESS_DENIED,
                    _ => {
                        if busy {
                            ALREADY as u32
                        } else {
                            0
                        }
                    }
                };
                // Valid requests are exercised by the independent matrix; keep
                // this fixture unregistered during the first error partition.
                if !matches!(mode, 7 | 8 | 10) {
                    check(p, &mut t, handle, expected.into());
                }
                if let Some(alias) = alias {
                    p.objects.close(alias).unwrap();
                }
            }
        }
    }
}

#[test]
fn native_wnf_process_event_isolated_processes_and_one_argument_never_probe_guest_memory() {
    for arch in WinArch::ALL {
        for manual in [false, true] {
            for signaled in [0, 1, 2, 255] {
                let (mut process, mut t) =
                    fixture(arch, "NtSetWnfProcessNotificationEvent", SERVICE);
                let p = process.state_mut();
                let h = event(p, manual, signaled);
                let guarded =
                    p.vm.allocate(
                        None,
                        PAGE_SIZE,
                        mem::RESERVE | mem::COMMIT,
                        prot::READWRITE | prot::GUARD,
                    )
                    .unwrap()
                    .0;
                // Extra argument registers/stack values are not formal inputs.
                match arch {
                    WinArch::Arm64 => {
                        t.cpu.set_gpr(1, guarded);
                        t.cpu.set_gpr(2, 1);
                    }
                    WinArch::X64 => {
                        t.cpu.set_gpr(2, guarded);
                        t.cpu.set_gpr(8, 1);
                    }
                    WinArch::X86 => {
                        p.space.w32(t.cpu.sp() + 12, guarded as u32).unwrap();
                    }
                }
                check(p, &mut t, h, 0);
                assert_eq!(
                    p.vm.query(guarded).unwrap().protect,
                    prot::READWRITE | prot::GUARD
                );
                assert!(
                    matches!(p.objects.get(h),Some(Object::Event {manual:m,signaled:s}) if (*m,*s)==(manual,signaled))
                );
            }
        }
    }
}

#[test]
fn native_wnf_process_event_status_translation_matches_installed_rtl() {
    let text = include_str!(
        "../../../../../../../docs/specifications/windows/native-wnf-process-event/native-wnf-status-arm64.log"
    );
    assert!(text.contains("complete cases=5"));
    let observed = rows(text);
    assert_eq!(observed.len(), 5);
    for row in observed {
        assert_eq!(
            crate::user::windows::nt::status_to_error(field(&row, "status", 16) as u32),
            field(&row, "error", 10) as u32
        );
    }
}
