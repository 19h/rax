//! Independent native APC results bind these Win32 adapter regressions.
use super::*;

#[test]
fn native_single_wait_preserves_prior_ntdll_ordinals_and_trap_rvas_all_abis() {
    use crate::user::windows::dll::{self, BuiltinDll};
    use crate::user::windows::hle::{Export, Item};
    use crate::user::windows::loader::builtin;

    let current = dll::find("ntdll.dll").unwrap();
    // Reconstruct the preceding DLL by removing only this group's addition.
    // Keep the other source entries and architecture filters in their original
    // order. These bounded static test fixtures live until the test process exits.
    let prior: Vec<Export> = current
        .exports
        .iter()
        .flat_map(|table| table.iter())
        .filter(|export| export.name != "NtWaitForSingleObject")
        .map(|export| {
            let copy = match &export.item {
                Item::Func(api) => Export::func(api.name, api.conv, api.args, api.imp),
                Item::Data(size) => Export::data(export.name, *size),
                Item::Forward(target) => Export::forward(export.name, target),
            };
            copy.only(export.archs)
        })
        .collect();
    let prior: &'static [Export] = Box::leak(prior.into_boxed_slice());
    let tables: &'static [&'static [Export]] = Box::leak(vec![prior].into_boxed_slice());
    let previous = BuiltinDll {
        name: current.name,
        display: current.display,
        subsystem: current.subsystem,
        exports: tables,
    };
    for arch in WinArch::ALL {
        let before = builtin::build(&previous, arch, 0, &[]);
        let after = builtin::build(current, arch, 0, &[]);
        assert_eq!(after.ordinal_base, before.ordinal_base);
        assert_eq!(after.symbols.len(), before.symbols.len() + 1);
        assert_eq!(
            &after.symbols[..before.symbols.len()],
            before.symbols.as_slice(),
            "{arch}: existing ordinals or trap RVAs changed"
        );
        assert_eq!(after.symbols.last().unwrap().0, "NtWaitForSingleObject");
    }
}

fn status(c: &mut Ctx, result: Flow, alertable: bool) -> u64 {
    match result {
        Flow::Ret(Value::Int(value)) | Flow::Yield(Value::Int(value)) => {
            assert!(
                !alertable,
                "queued APC must precede a ready object or zero timeout"
            );
            value
        }
        Flow::Block { wait, then } => {
            assert!(
                alertable,
                "high BOOL bytes must not enable alertable waiting"
            );
            let result = sync::poll(c.p, c.t.tid, &wait, Instant::now(), true).unwrap();
            assert_eq!(result, Some(sync::WAIT_IO_COMPLETION));
            match then(c, sync::WAIT_IO_COMPLETION).unwrap() {
                Flow::Ret(Value::Int(value)) => value,
                _ => panic!("alertable API did not return its APC status"),
            }
        }
        _ => panic!("unexpected wait flow"),
    }
}

#[test]
fn native_single_wait_win32_single_apc_priority_and_alert_byte_all_abis() {
    for arch in WinArch::ALL {
        with_context(arch, |c, _| {
            for alert in [0u32, 1, 2, 255, 256, 257, u32::MAX] {
                for signal in [0, 1] {
                    let handle = integer(c, "CreateEventW", &[0, 0, signal, 0]);
                    assert_ne!(handle, 0);
                    c.t.apcs.push_back((0x50000, 0x1111));
                    let flow =
                        invoke(c, "WaitForSingleObjectEx", &[handle, 0, u64::from(alert)]).unwrap();
                    let alertable = alert as u8 != 0;
                    let actual = status(c, flow, alertable);
                    assert_eq!(
                        actual,
                        if alertable {
                            0xC0
                        } else if signal != 0 {
                            0
                        } else {
                            0x102
                        }
                    );
                    assert!(
                        matches!(c.p.objects.get(handle), Some(Object::Event { signaled, .. }) if *signaled == if alertable { signal as i32 } else { 0 })
                    );
                    c.t.apcs.clear();
                    c.p.objects.close(handle).unwrap();
                }
            }
        });
    }
}

#[test]
fn native_single_wait_win32_multiple_apc_preserves_complete_signal_set_all_abis() {
    for arch in WinArch::ALL {
        with_context(arch, |c, data| {
            for mask in 0u32..4 {
                for all in [0u32, 1, 256] {
                    for alert in [0u32, 1, 2, 255, 256, 257, u32::MAX] {
                        let a = integer(c, "CreateEventW", &[0, 0, u64::from(mask & 1 != 0), 0]);
                        let b = integer(c, "CreateEventW", &[0, 0, u64::from(mask & 2 != 0), 0]);
                        let width = c.psize();
                        c.mem().wptr(data, width, a).unwrap();
                        c.mem().wptr(data + width, width, b).unwrap();
                        c.t.apcs.push_back((0x50000, 0x1111));
                        let flow = invoke(
                            c,
                            "WaitForMultipleObjectsEx",
                            &[2, data, u64::from(all), 0, u64::from(alert)],
                        )
                        .unwrap();
                        let alertable = alert as u8 != 0;
                        let actual = status(c, flow, alertable);
                        let ready = if all != 0 { mask == 3 } else { mask != 0 };
                        let expected = if alertable {
                            0xC0
                        } else if !ready {
                            0x102
                        } else if all != 0 || mask & 1 != 0 {
                            0
                        } else {
                            1
                        };
                        assert_eq!(actual, expected);
                        let remaining = if alertable || !ready {
                            mask
                        } else if all != 0 {
                            0
                        } else {
                            mask & !(if mask & 1 != 0 { 1 } else { 2 })
                        };
                        for (handle, bit) in [(a, 1), (b, 2)] {
                            assert!(
                                matches!(c.p.objects.get(handle), Some(Object::Event { signaled, .. }) if *signaled == i32::from(remaining & bit != 0))
                            );
                            c.p.objects.close(handle).unwrap();
                        }
                        c.t.apcs.clear();
                    }
                }
            }
        });
    }
}

#[test]
fn native_single_wait_win32_sleep_ex_retains_measured_alert_byte_all_abis() {
    for arch in WinArch::ALL {
        with_context(arch, |c, _| {
            for alert in [0u32, 1, 2, 255, 256, 257, u32::MAX] {
                c.t.apcs.push_back((0x50000, 0x1111));
                let flow = invoke(c, "SleepEx", &[0, u64::from(alert)]).unwrap();
                let alertable = alert as u8 != 0;
                assert_eq!(status(c, flow, alertable), if alertable { 0xC0 } else { 0 });
                c.t.apcs.clear();
            }
        });
    }
}
