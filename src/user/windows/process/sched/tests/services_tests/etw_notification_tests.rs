//! Independent installed-kernel ETW registration rows and local ownership.
use super::*;
use crate::user::windows::nt::status::*;
use std::collections::HashMap;

#[path = "etw_notification_boundary_tests.rs"]
mod boundary_tests;
#[cfg(windows)]
#[path = "etw_notification_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x1D6;
const POISON: u32 = 0xA5A5A5A5;
macro_rules! corpus {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-etw-notification/",
            $file
        ))
    };
}
fn rows<'a>(text: &'a str, prefix: &str) -> Vec<HashMap<&'a str, &'a str>> {
    text.lines()
        .filter(|l| l.starts_with(prefix))
        .map(|line| {
            line.split_whitespace()
                .filter_map(|f| f.split_once('='))
                .collect()
        })
        .collect()
}
fn field(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 6]) -> Outcome {
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
            for (i, value) in args.into_iter().enumerate() {
                if i < 4 {
                    t.cpu.set_gpr([10, 2, 8, 9][i], value);
                } else {
                    p.space.w64(sp + 40 + (i - 4) as u64 * 8, value).unwrap();
                }
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
fn check(p: &mut Proc, t: &mut Thread, args: [u64; 6], expected: u64) {
    assert_eq!(call(p, t, args), Outcome::Continue);
    assert_eq!(t.cpu.gpr(0), expected, "arch={} args={args:X?}", p.arch);
}
fn event(p: &mut Proc, manual: bool, signaled: i32) -> u64 {
    p.objects.insert(Object::Event { manual, signaled }).into()
}
fn selected(p: &mut Proc, h: u64, mode: usize) -> (u64, Option<u64>) {
    match mode {
        0 => (h, None),
        1 => (0, None),
        2 => (0x13572468, None),
        3 => (u32::MAX.into(), None),
        4 => ((u32::MAX - 1).into(), None),
        5..=10 => {
            let grant = [0, 1, 2, 3, 0x100000, 0x1F0003][mode - 5];
            let alias = p
                .objects
                .open_access(p.objects.id(h).unwrap(), false, grant)
                .unwrap() as u64;
            (alias, Some(alias))
        }
        11 => {
            let alias = p.objects.insert(Object::Semaphore { count: 0, max: 1 }) as u64;
            (alias, Some(alias))
        }
        _ => panic!("unknown mode"),
    }
}
fn submit(p: &mut Proc, t: &mut Thread, h: u64, expected: u64) {
    let input = t.cpu.sp() - 128;
    p.space.w32(input, h as u32).unwrap();
    p.space.w32(input + 16, POISON).unwrap();
    check(p, t, [27, input, 4, 0, 0, input + 16], expected);
    assert_eq!(
        p.space.u32(input + 16).unwrap(),
        if expected == 0 { 0 } else { POISON }
    );
}

#[test]
fn native_etw_notification_replays_all_1920_mature_handle_and_length_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            corpus!("native-etw-notification-event-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-etw-notification-event-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-notification-event-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-notification-event-x86-laa.log"),
        ),
    ];
    let mut count = 0;
    for (arch, text) in profiles {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 480);
        assert!(text.contains("complete cases=480"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let registered = event(p, false, 0);
        submit(p, &mut t, registered, 0);
        let input = t.cpu.sp() - 128;
        let returned = input + 32;
        for row in observed {
            let manual = row["manual"] == "1";
            let initial = field(&row, "initial", 10) as i32;
            let h = event(p, manual, initial);
            let (selected, alias) = selected(p, h, field(&row, "mode", 10) as usize);
            p.space.w64(input, selected).unwrap();
            p.space.w32(returned, POISON).unwrap();
            p.space.w32(returned + 4, 0xB6B6B6B6).unwrap();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            for key in ["status", "repeat"] {
                check(
                    p,
                    &mut t,
                    [27, input, field(&row, "length", 10), 0, 0, returned],
                    field(&row, key, 16),
                );
            }
            assert_eq!(p.space.u32(returned).unwrap(), POISON);
            assert_eq!(p.space.u32(returned + 4).unwrap(), 0xB6B6B6B6);
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            assert!(
                matches!(p.objects.get(h), Some(Object::Event { manual: m, signaled: s }) if (*m,*s)==(manual,initial))
            );
            if let Some(alias) = alias {
                p.objects.close(alias).unwrap();
            }
            p.objects.close(h).unwrap();
            count += 1;
        }
    }
    assert_eq!(count, 1920);
}

#[test]
fn native_etw_notification_replays_all_48_fresh_lifetime_sequences() {
    let text = corpus!("native-etw-notification-event-fresh-arm64.log");
    assert!(text.contains("complete cases=48 warmup=1"));
    let observed = rows(text, "case=");
    assert_eq!(observed.len(), 49);
    for row in observed.into_iter().filter(|r| r["warmup"] == "0") {
        let (mut process, mut t) = fixture(WinArch::Arm64, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let h = event(p, row["manual"] == "1", field(&row, "initial", 10) as i32);
        let id = p.objects.id(h).unwrap();
        let (selected, alias) = selected(p, h, field(&row, "mode", 10) as usize);
        submit(p, &mut t, selected, field(&row, "status", 16));
        submit(p, &mut t, selected, field(&row, "repeat", 16));
        if let Some(alias) = alias {
            p.objects.close(alias).unwrap();
        }
        p.objects.close(h).unwrap();
        assert_eq!(p.objects.obj(id).is_some(), row["status"] == "00000000");
        let replacement = event(p, true, 0);
        submit(p, &mut t, replacement, field(&row, "replacement", 16));
        submit(
            p,
            &mut t,
            replacement,
            field(&row, "replacement-repeat", 16),
        );
    }
}

#[test]
fn native_etw_notification_close_reuse_drain_and_distinct_wnf_owner_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let h = event(p, true, 1);
        let id = p.objects.id(h).unwrap();
        let alias = p.objects.duplicate(h, false).unwrap() as u64;
        submit(p, &mut t, alias, 0);
        assert_eq!(p.objects.register_wnf_event(id), Ok(true));
        submit(p, &mut t, h, STATUS_ALREADY_REGISTERED.into());
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
        assert_eq!(replacement, h);
        let other = p.objects.id(replacement).unwrap();
        submit(p, &mut t, replacement, STATUS_ALREADY_REGISTERED.into());
        p.objects.close(replacement).unwrap();
        assert!(p.objects.obj(other).is_none());
        p.objects.drain().for_each(drop);
        assert_eq!(p.objects.iter().count(), 0);
        let next = event(p, false, 0);
        submit(p, &mut t, next, 0);
    }
}

#[test]
fn native_etw_notification_unknown_functions_preserve_explicit_frontier_and_memory() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        for function in [0, 1, 16, 18, 26, 28, 46, 47, u32::MAX] {
            let output = t.cpu.sp() - 128;
            p.space.w32(output, POISON).unwrap();
            assert!(
                matches!(call(p, &mut t, [function.into(), 1, 4, 1, 4, output]), Outcome::Fail(message) if message.contains("NtTraceControl function"))
            );
            assert_eq!(p.space.u32(output).unwrap(), POISON);
        }
    }
}
