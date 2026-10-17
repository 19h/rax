//! Native System subscription state, access and event-queue replays.
use super::*;
use crate::user::windows::nt::status::*;
use crate::user::windows::process::{WindowsConfig, WindowsProcess};
use crate::user::windows::wnf::{Record, system_tests as native};

#[cfg(windows)]
#[path = "wnf_system_subscription_leaf_tests.rs"]
mod installed;

const SERVICE: u32 = 0x1CD;
fn number(row: &serde_json::Value, key: &str) -> u32 {
    u32::from_str_radix(row[key].as_str().unwrap(), 16).unwrap()
}
fn decimal(row: &serde_json::Value, key: &str) -> i32 {
    row[key].as_str().unwrap().parse().unwrap()
}
fn setup(arch: WinArch, laa: bool, native: bool) -> (WindowsProcess, Thread, u64) {
    let mut image = match arch {
        WinArch::Arm64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe")
                .to_vec()
        }
        WinArch::X64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
                .to_vec()
        }
        WinArch::X86 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
                .to_vec()
        }
    };
    if laa {
        let pe = u32::from_le_bytes(image[60..64].try_into().unwrap()) as usize;
        let flags = u16::from_le_bytes(image[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
        image[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
    }
    let mut config = WindowsConfig::embedded("C:\\system-wnf.exe", vec![], vec![], 4096).unwrap();
    config.arena_bytes = 64 << 20;
    config.native_libraries = native;
    let mut process = WindowsProcess::spawn_image(config, image).unwrap();
    let p = process.state_mut();
    let tid = *p.threads.keys().next().unwrap();
    let mut thread = p.threads.remove(&tid).unwrap();
    thread.attached = true;
    thread.main = false;
    thread.cpu.set_sp(thread.cpu.sp() - 0x100);
    if !native {
        let ntdll = p.modules.by_name("ntdll.dll").unwrap();
        p.modules.nt_services = Some((ntdll, table(arch, "NtSubscribeWnfStateChange", SERVICE)));
    }
    let base =
        p.vm.allocate(None, PAGE_SIZE, mem::RESERVE | mem::COMMIT, prot::READWRITE)
            .unwrap()
            .0;
    p.space.w64(base, native::NAME).unwrap();
    (process, thread, base)
}
fn invoke(p: &mut Proc, t: &mut Thread, args: [u64; 4], installed: bool) -> Result<u32, String> {
    if installed {
        #[cfg(windows)]
        {
            let module = crate::user::windows::loader::load_dll(p, "ntdll.dll").unwrap();
            return Ok(super::virtual_memory_tests::installed::invoke(
                p,
                t,
                module,
                "NtSubscribeWnfStateChange",
                &args,
            ));
        }
        #[cfg(not(windows))]
        panic!("native Windows leaf cannot be selected on this host");
    }
    match super::wnf_subscription_tests::call(p, t, args) {
        Outcome::Continue => Ok(t.cpu.gpr(0) as u32),
        Outcome::Fail(reason) => Err(reason),
        outcome => panic!("unexpected subscription outcome {outcome:?}"),
    }
}
fn event_state(p: &Proc, event: u32) -> i32 {
    match p.objects.obj(event).unwrap() {
        Object::Event { signaled, .. } => *signaled,
        _ => panic!("event ownership changed"),
    }
}
fn reset(p: &mut Proc, event: u32) {
    match p.objects.obj_mut(event).unwrap() {
        Object::Event { signaled, .. } => *signaled = 0,
        _ => panic!("event ownership changed"),
    }
}
fn fresh(
    p: &mut Proc,
    t: &mut Thread,
    b: u64,
    installed: bool,
    selected_only: bool,
) -> (usize, usize) {
    let mut verified = 0;
    let mut refused = 0;
    for profile in native::cases()["fresh"].as_array().unwrap() {
        if selected_only && profile["name"] != "system" && profile["name"] != "zero" {
            continue;
        }
        for row in profile["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["warmup"] == "0")
        {
            if selected_only && profile["name"] == "zero" && number(row, "mask") & 0x0e != 0 {
                continue;
            }
            p.objects.drain().for_each(drop);
            if profile["name"] == "zero" {
                p.objects.select_wnf_catalog(native::select(
                    6,
                    Record::Data(crate::user::windows::wnf::Data {
                        stamp: 1,
                        bytes: std::sync::Arc::from([]),
                    }),
                ));
            } else {
                p.objects.select_wnf_catalog(native::data(
                    if profile["name"] == "system" { 7 } else { 6 },
                    profile["stamp"].as_u64().unwrap() as u32,
                ));
            }
            let handle = p.objects.insert(Object::Event {
                manual: false,
                signaled: 0,
            }) as u64;
            let event = p.objects.id(handle).unwrap();
            let first = row["after"] == "1";
            if first {
                assert_eq!(p.objects.register_wnf_event(event), Ok(true));
                assert_eq!(event_state(p, event), decimal(row, "state1"), "{row}");
            }
            let mask = number(row, "mask");
            p.space.w64(b + 32, u64::MAX).unwrap();
            let result = invoke(
                p,
                t,
                [b, number(row, "stamp").into(), mask.into(), b + 32],
                installed,
            );
            if profile["name"] != "system" && mask & 0x0e != 0 {
                assert!(result.unwrap_err().contains("subscriber-level"));
                assert_eq!(p.space.u64(b + 32).unwrap(), 0);
                assert!(p.objects.wnf_subscription(native::NAME).is_none());
                refused += 1;
                continue;
            }
            assert_eq!(result.unwrap(), number(row, "status"), "{row}");
            if !first {
                assert_eq!(event_state(p, event), decimal(row, "state1"), "{row}");
            }
            let id = p.space.u64(b + 32).unwrap();
            if !first {
                assert_eq!(p.objects.register_wnf_event(event), Ok(true));
            }
            p.objects.close(handle).unwrap();
            assert_eq!(event_state(p, event), decimal(row, "state2"), "{row}");
            assert_eq!(
                invoke(p, t, [b, u32::MAX.into(), mask.into(), b + 32], installed).unwrap(),
                number(row, "repeat")
            );
            assert_eq!(p.space.u64(b + 32).unwrap(), id);
            assert_eq!(event_state(p, event), decimal(row, "state3"), "{row}");
            assert_eq!(
                p.objects.unsubscribe_wnf(native::NAME),
                number(row, "end") == 0
            );
            assert_eq!(event_state(p, event), decimal(row, "state4"), "{row}");
            assert!(!p.objects.unsubscribe_wnf(native::NAME));
            verified += 1;
        }
    }
    p.objects.drain().for_each(drop);
    (verified, refused)
}
fn transitions(p: &mut Proc, t: &mut Thread, b: u64, installed: bool, triples: bool) -> usize {
    let key = if triples { "triples" } else { "transitions" };
    let mut count = 0;
    for row in native::cases()[key]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["warmup"] == "0")
    {
        p.objects.drain().for_each(drop);
        p.objects.select_wnf_catalog(native::data(7, 1));
        let handle = p.objects.insert(Object::Event {
            manual: false,
            signaled: 0,
        }) as u64;
        let event = p.objects.id(handle).unwrap();
        let first = row["after"] == "1";
        if first {
            assert_eq!(p.objects.register_wnf_event(event), Ok(true));
        }
        assert_eq!(
            invoke(
                p,
                t,
                [
                    b,
                    number(row, "stamp").into(),
                    number(row, "mask").into(),
                    b + 32
                ],
                installed
            )
            .unwrap(),
            0
        );
        let id = p.space.u64(b + 32).unwrap();
        assert_ne!(id, 0);
        if !first {
            assert_eq!(p.objects.register_wnf_event(event), Ok(true));
        }
        p.objects.close(handle).unwrap();
        assert_eq!(event_state(p, event), decimal(row, "state2"));
        reset(p, event);
        assert_eq!(
            invoke(
                p,
                t,
                [
                    b,
                    number(row, "next-stamp").into(),
                    number(row, "next-mask").into(),
                    b + 32
                ],
                installed
            )
            .unwrap(),
            0
        );
        assert_eq!(p.space.u64(b + 32).unwrap(), id);
        assert_eq!(
            event_state(p, event),
            decimal(row, "state3"),
            "{key}: {row}"
        );
        if triples {
            reset(p, event);
            assert_eq!(
                invoke(
                    p,
                    t,
                    [b, u32::MAX.into(), number(row, "next-mask").into(), b + 32],
                    installed
                )
                .unwrap(),
                0
            );
            assert_eq!(p.space.u64(b + 32).unwrap(), id);
            assert_eq!(event_state(p, event), decimal(row, "state4"), "{row}");
        }
        assert!(p.objects.unsubscribe_wnf(native::NAME));
        assert_eq!(
            event_state(p, event),
            decimal(row, if triples { "state5" } else { "state4" })
        );
        count += 1;
    }
    p.objects.drain().for_each(drop);
    count
}
#[test]
fn native_system_wnf_fresh_access_and_queue_order_replay_all_abis() {
    let mut totals = (0, 0);
    for (arch, laa) in [
        (WinArch::Arm64, false),
        (WinArch::X64, false),
        (WinArch::X86, false),
        (WinArch::X86, true),
    ] {
        let (mut process, mut thread, b) = setup(arch, laa, false);
        let n = fresh(process.state_mut(), &mut thread, b, false, false);
        totals.0 += n.0;
        totals.1 += n.1;
    }
    assert_eq!(totals, (1056, 2016));
}
#[test]
fn native_system_wnf_read_mask_and_stamp_transitions_replay_all_abis() {
    for (arch, laa) in [
        (WinArch::Arm64, false),
        (WinArch::X64, false),
        (WinArch::X86, false),
        (WinArch::X86, true),
    ] {
        let (mut process, mut thread, b) = setup(arch, laa, false);
        assert_eq!(
            transitions(process.state_mut(), &mut thread, b, false, false),
            288
        );
        assert_eq!(
            transitions(process.state_mut(), &mut thread, b, false, true),
            288
        );
    }
}
#[test]
fn native_system_wnf_unknown_data_and_failed_requests_are_transactional() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, b) = setup(arch, false, false);
        let p = process.state_mut();
        for record in [
            Record::Unavailable("unstable"),
            Record::Status(STATUS_ACCESS_DENIED),
        ] {
            p.objects.select_wnf_catalog(native::select(7, record));
            assert!(invoke(p, &mut thread, [b, 0, 1, b + 32], false).is_err());
            assert!(p.objects.wnf_subscription(native::NAME).is_none());
        }
        p.objects.select_wnf_catalog(native::data(7, 1));
        assert_eq!(
            invoke(p, &mut thread, [b, 1, 17, b + 32], false).unwrap(),
            0
        );
        let before = p.objects.wnf_subscription(native::NAME).unwrap().clone();
        assert_eq!(
            invoke(p, &mut thread, [b, 0, 2, b + 32], false).unwrap(),
            STATUS_ACCESS_DENIED
        );
        assert_eq!(p.objects.wnf_subscription(native::NAME), Some(&before));
        assert_eq!(
            invoke(p, &mut thread, [b, 0, 32, b + 32], false).unwrap(),
            STATUS_INVALID_PARAMETER
        );
        assert_eq!(p.objects.wnf_subscription(native::NAME), Some(&before));
    }
}
