//! Full native pointer/protection matrices and fresh fault/alias sequences.
use super::*;
macro_rules! corpus {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-etw-notification/",
            $file
        ))
    };
}

#[test]
fn native_etw_notification_replays_all_21000_buffer_status_and_guard_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            corpus!("native-etw-notification-boundaries-arm64.log"),
            corpus!("native-etw-notification-output-readonly-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-etw-notification-boundaries-x64.log"),
            corpus!("native-etw-notification-output-readonly-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-notification-boundaries-x86.log"),
            corpus!("native-etw-notification-output-readonly-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-notification-boundaries-x86-laa.log"),
            corpus!("native-etw-notification-output-readonly-x86-laa.log"),
        ),
    ];
    let mut count = 0;
    for (arch, text, readonly) in profiles {
        let observed = rows(text, "case=");
        let more = rows(readonly, "case=");
        assert_eq!(observed.len(), 4200);
        assert_eq!(more.len(), 1050);
        assert!(text.contains("complete cases=4200"));
        assert!(readonly.contains("complete cases=1050"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let h = event(p, false, 0);
        submit(p, &mut t, h, 0);
        let base =
            p.vm.allocate(
                None,
                6 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let poison = vec![0xA5; (6 * PAGE_SIZE) as usize];
        let before = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed.into_iter().chain(more) {
            p.vm.protect(base, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(base, &poison).unwrap();
            let ik = field(&row, "input", 10);
            let rk = field(&row, "returned", 10);
            let ok = field(&row, "output", 10);
            let local = base + 5 * PAGE_SIZE;
            let input = match ik {
                0 => local,
                1 => 0,
                2 => 1,
                6 => base + PAGE_SIZE - 2,
                7..=9 => local + ik - 6,
                _ => base,
            };
            let returned = match rk {
                0 => local + 64,
                1 => 0,
                2 => 1,
                6 => base + 3 * PAGE_SIZE - 2,
                _ => base + 2 * PAGE_SIZE,
            };
            let output = match ok {
                0 => 0,
                1 => local + 128,
                _ => base + 4 * PAGE_SIZE,
            };
            p.space
                .w32(local + if ik >= 7 { ik - 6 } else { 0 }, h as u32)
                .unwrap();
            p.space.w32(base, h as u32).unwrap();
            p.space.w32(base + PAGE_SIZE - 2, h as u32).unwrap();
            for (kind, address) in [(ik, base), (rk, base + 2 * PAGE_SIZE)] {
                if (3..=5).contains(&kind) {
                    p.vm.protect(
                        address,
                        PAGE_SIZE,
                        match kind {
                            3 => prot::READWRITE | prot::GUARD,
                            4 => prot::NOACCESS,
                            _ => prot::READONLY,
                        },
                    )
                    .unwrap();
                }
            }
            if ik == 6 {
                p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            if rk == 6 {
                p.vm.protect(base + 3 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            if ok >= 2 {
                p.vm.protect(
                    base + 4 * PAGE_SIZE,
                    PAGE_SIZE,
                    match ok {
                        2 => prot::NOACCESS,
                        3 => prot::READWRITE | prot::GUARD,
                        _ => prot::READONLY,
                    },
                )
                .unwrap();
            }
            check(
                p,
                &mut t,
                [
                    27,
                    input,
                    field(&row, "input-length", 10),
                    output,
                    field(&row, "output-length", 10),
                    returned,
                ],
                field(&row, "status", 16),
            );
            for (page, key) in [
                (0, "input-protect"),
                (2, "returned-protect"),
                (3, "returned-upper-protect"),
                (4, "output-protect"),
            ] {
                assert_eq!(
                    u64::from(p.vm.query(base + page * PAGE_SIZE).unwrap().protect),
                    field(&row, key, 16),
                    "{arch}: {row:?}"
                );
            }
            p.vm.protect(base, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            if returned > 1 {
                assert_eq!(
                    u64::from(p.space.u32(returned).unwrap()),
                    field(&row, "return-after", 16)
                );
            }
            if output != 0 {
                assert_eq!(
                    u64::from(p.space.u32(output).unwrap()),
                    field(&row, "output-after", 16)
                );
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            count += 1;
        }
    }
    assert_eq!(count, 21000);
}

#[test]
fn native_etw_notification_replays_all_120_fresh_return_fault_transactions() {
    let text = corpus!("native-etw-notification-return-fresh-arm64.log");
    let observed = rows(text, "case=");
    let outputs = rows(text, "trace-returned");
    assert_eq!(observed.len(), 121);
    assert_eq!(outputs.len(), 121);
    assert!(text.contains("complete cases=120 warmup=1"));
    for (row, output_row) in observed
        .into_iter()
        .zip(outputs)
        .filter(|(r, _)| r["warmup"] == "0")
    {
        assert_eq!(row["case"], output_row["case"]);
        let (mut process, mut t) = fixture(WinArch::Arm64, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let h = event(p, false, 0);
        let id = p.objects.id(h).unwrap();
        let (selected, alias) = selected(p, h, field(&row, "mode", 10) as usize);
        let base =
            p.vm.allocate(
                None,
                2 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        p.space
            .wr(base, &vec![0xA5; (2 * PAGE_SIZE) as usize])
            .unwrap();
        p.space.w32(base, selected as u32).unwrap();
        let kind = field(&output_row, "kind", 10);
        let returned = match kind {
            0 => base + 8,
            1 => 0,
            2 => 1,
            6 | 7 | 9 => base + PAGE_SIZE - 2,
            8 => base + 9,
            _ => base + PAGE_SIZE,
        };
        if kind >= 3 && kind != 8 {
            p.vm.protect(
                base + PAGE_SIZE,
                PAGE_SIZE,
                match kind {
                    3 | 6 => prot::NOACCESS,
                    4 | 7 => prot::READONLY,
                    _ => prot::READWRITE | prot::GUARD,
                },
            )
            .unwrap();
        }
        check(
            p,
            &mut t,
            [27, base, 4, 0, 0, returned],
            field(&row, "status", 16),
        );
        assert_eq!(
            u64::from(p.vm.query(base + PAGE_SIZE).unwrap().protect),
            field(&output_row, "protect", 16)
        );
        p.vm.protect(base, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
        if returned > 1 {
            assert_eq!(
                u64::from(p.space.u32(returned).unwrap()),
                field(&output_row, "a", 16)
            );
        }
        submit(p, &mut t, selected, field(&row, "repeat", 16));
        if let Some(alias) = alias {
            p.objects.close(alias).unwrap();
        }
        p.objects.close(h).unwrap();
        assert_eq!(
            p.objects.obj(id).is_some(),
            row["status"] == "00000000" || row["repeat"] == "00000000"
        );
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
fn native_etw_notification_replays_all_72_fresh_output_alias_sequences() {
    let text = corpus!("native-etw-notification-alias-fresh-arm64.log");
    let observed = rows(text, "case=");
    let outputs = rows(text, "trace-returned");
    assert_eq!(observed.len(), 73);
    assert_eq!(outputs.len(), 73);
    assert!(text.contains("complete cases=72 warmup=1"));
    for (row, output_row) in observed
        .into_iter()
        .zip(outputs)
        .filter(|(r, _)| r["warmup"] == "0")
    {
        let (mut process, mut t) = fixture(WinArch::Arm64, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let h = event(p, false, 0);
        let (selected, alias) = selected(p, h, field(&row, "mode", 10) as usize);
        let input = t.cpu.sp() - 128;
        p.space.wr(input, &[0xA5; 32]).unwrap();
        p.space.w32(input, selected as u32).unwrap();
        let kind = field(&output_row, "kind", 10);
        let returned = input + if kind <= 3 { kind } else { 8 };
        let output = if kind == 4 {
            input
        } else if kind == 5 {
            input + 8
        } else {
            0
        };
        let before = p.space.u32(returned).unwrap();
        check(
            p,
            &mut t,
            [
                27,
                input,
                4,
                output,
                if kind >= 4 { 4 } else { 0 },
                returned,
            ],
            field(&row, "status", 16),
        );
        assert_eq!(
            p.space.u32(returned).unwrap(),
            if row["status"] == "00000000" {
                0
            } else {
                before
            }
        );
        assert_eq!(p.space.u32(input + 16).unwrap(), POISON);
        submit(p, &mut t, selected, field(&row, "repeat", 16));
        if let Some(alias) = alias {
            p.objects.close(alias).unwrap();
        }
        p.objects.close(h).unwrap();
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
