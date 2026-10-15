//! Replay installed-kernel statuses and bytes; dequeue is an internal model
//! assertion here, not a claim that NtRemoveIoCompletion is implemented.
use super::*;
use std::collections::HashMap;
type Row<'a> = HashMap<&'a str, &'a str>;
fn rows<'a>(text: &'a str, prefix: &str) -> Vec<Row<'a>> {
    text.lines()
        .filter(|line| line.starts_with(prefix))
        .map(|line| {
            line.split_whitespace()
                .map(|part| part.split_once('=').unwrap())
                .collect()
        })
        .collect()
}
fn n(row: &Row, field: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[field], radix).unwrap()
}
fn observations() -> [(WinArch, &'static str, &'static str, &'static str); 4] {
    [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-probe-arm64.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-validation-arm64.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-lifetime-v2-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-probe-x64.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-validation-x64.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-lifetime-v2-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-probe-x86.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-validation-x86.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-lifetime-v2-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-probe-x86-laa.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-validation-x86-laa.log"
            ),
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-lifetime-v2-x86-laa.log"
            ),
        ),
    ]
}
fn choice(
    p: &mut Proc,
    arch: WinArch,
    role: usize,
    mode: u64,
    handles: &[u64; 3],
) -> (u64, Option<u64>) {
    let mut extra = None;
    let selected = match mode {
        0 => handles[role],
        1 => 0,
        2 => 0x3330,
        3 | 4 => {
            let h = object(p, 2, if mode == 3 { 0x1F0003 } else { 0 });
            extra = Some(h);
            h
        }
        5..=8 => {
            let grants = [0, 1, 2, 0x100000];
            let grant = grants[(mode - 5) as usize] & if role == 0 { 0x1F0001 } else { 0x1F0003 };
            let h = object(p, role, grant);
            extra = Some(h);
            h
        }
        9 => arch.ptr(u64::MAX),
        10 => arch.ptr(u64::MAX - 1),
        _ => panic!("invalid native mode"),
    };
    (selected, extra)
}
#[test]
fn native_wait_packet_associate_replays_540_validation_observations() {
    let mut replayed = 0;
    for (arch, _, text, _) in observations() {
        let records = rows(text, "validation=");
        assert_eq!(records.len(), 135);
        for row in records {
            assert_eq!(n(&row, "width", 10), arch.ptr_size());
            assert_eq!(n(&row, "exception", 16), 0);
            assert_eq!(row["handles-before"], row["handles-after"]);
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let out = output(p);
            p.space
                .wr(out, &vec![0xA5; (2 * PAGE_SIZE) as usize])
                .unwrap();
            let mut handles = [
                object(p, 0, 0x1F0001),
                object(p, 1, 0x1F0003),
                object(p, 2, 0x1F0003),
            ];
            let original = handles;
            let role = n(&row, "role", 10) as usize;
            let (h, extra) = choice(p, arch, role, n(&row, "mode", 10), &handles);
            handles[role] = h;
            let pointer = n(&row, "pointer", 10);
            let selected = match pointer {
                0 => out,
                1 => 0,
                2 => 1,
                3 => {
                    p.vm.protect(out, PAGE_SIZE, prot::READONLY).unwrap();
                    out
                }
                4 => {
                    p.vm.protect(out, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                    out
                }
                5 => out + 1,
                6 => {
                    p.vm.protect(out + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                    out + PAGE_SIZE - 1
                }
                _ => panic!("native pointer mode"),
            };
            assert_eq!(
                associate(p, &mut t, handles[0], handles[1], handles[2], selected),
                n(&row, "status", 16) as u32,
                "{arch}/{row:?}"
            );
            assert_eq!(
                p.vm.query(out).unwrap().protect,
                n(&row, "protection", 16) as u32
            );
            p.vm.protect(out, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
            let actual = p.space.bytes(out, PAGE_SIZE as usize).unwrap();
            for (field, index) in [
                ("first", 0),
                ("unaligned", 1),
                ("last", PAGE_SIZE as usize - 1),
            ] {
                assert_eq!(
                    u64::from(actual[index]),
                    n(&row, field, 16),
                    "{arch}/{row:?}/{field}"
                );
            }
            assert_eq!(
                cancel(p, &mut t, original[0], 1),
                n(&row, "cancel", 16) as u32
            );
            if let Some(h) = extra {
                if role == 0 {
                    assert_eq!(cancel(p, &mut t, h, 1), n(&row, "cancel-extra", 16) as u32);
                }
                p.objects.close(h).unwrap();
            }
            for h in original {
                p.objects.close(h).unwrap();
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 540);
}
#[test]
fn native_wait_packet_associate_replays_60_transitions_and_220_cancel_observations() {
    let mut transitions = 0;
    let mut cancellations = 0;
    for (arch, text, _, lifetime) in observations() {
        let records = rows(text, "case=");
        assert_eq!(records.len(), 15);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let out = output(p);
            p.space.wr(out, &[0xA5; 16]).unwrap();
            let w = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let kind = n(&row, "kind", 10);
            let target = p.objects.insert(if kind == 2 {
                Object::IrTimer {
                    attributes: 8,
                    timer_id: None,
                    signaled: false,
                }
            } else {
                Object::Event {
                    manual: kind == 0,
                    signaled: n(&row, "signaled", 10) as i32,
                }
            }) as u64;
            assert_eq!(
                associate(p, &mut t, w, port, target, out),
                n(&row, "status", 16) as u32
            );
            let expected: Vec<u8> = (0..row["already"].len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&row["already"][i..i + 2], 16).unwrap())
                .collect();
            assert_eq!(p.space.bytes(out, 16).unwrap(), expected);
            let mode = row["cancel-mode"].parse::<i32>().unwrap();
            if mode >= 0 {
                assert_eq!(
                    cancel(p, &mut t, w, mode as u64),
                    n(&row, "cancel", 16) as u32
                );
            }
            if kind != 2 {
                assert_eq!(
                    status(p, &mut t, "NtSetEvent", &[target, 0]),
                    n(&row, "set", 16) as u32
                );
            }
            pump(p);
            let port_id = p.objects.id(port).unwrap();
            let got = p.objects.dequeue_completion(port_id);
            if n(&row, "removed", 16) == 0 {
                let got = got.unwrap();
                assert_eq!(got.key, n(&row, "key", 16));
                assert_eq!(got.apc, n(&row, "apc", 16));
                assert_eq!(u64::from(got.status), n(&row, "io-status", 16));
                assert_eq!(got.information, n(&row, "information", 16));
            } else {
                assert_eq!(n(&row, "removed", 16), 0x102);
                assert!(got.is_none());
            }
            assert_eq!(cancel(p, &mut t, w, 1), n(&row, "cancel-again", 16) as u32);
            let wid = p.objects.id(w).unwrap();
            assert_eq!(
                sync::try_objects(p, t.tid, &[wid], false).unwrap(),
                Some(n(&row, "final-wait", 16))
            );
            for h in [target, port, w] {
                p.objects.close(h).unwrap();
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            transitions += 1;
        }
        let records = rows(lifetime, "cancel-mode=");
        assert_eq!(records.len(), 55);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtCancelWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let w = object(p, 0, 0x1F0001);
            let (h, extra) = choice(p, arch, 0, n(&row, "cancel-mode", 10), &[w, 0, 0]);
            assert_eq!(
                cancel(p, &mut t, h, n(&row, "flag", 10)),
                n(&row, "status", 16) as u32
            );
            if let Some(h) = extra {
                p.objects.close(h).unwrap();
            }
            p.objects.close(w).unwrap();
            cancellations += 1;
        }
    }
    assert_eq!((transitions, cancellations), (60, 220));
}
#[test]
fn native_wait_packet_associate_replays_52_native_target_observations() {
    let mut replayed = 0;
    for (arch, _, _, text) in observations() {
        let records = rows(text, "target-kind=");
        assert_eq!(records.len(), 13);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let out = output(p);
            p.space.wr(out, &[0xA5; 16]).unwrap();
            let w = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let kind = n(&row, "target-kind", 10);
            let (target, real) = match kind {
                0 | 1 => (
                    p.objects.insert(Object::Event {
                        manual: kind == 0,
                        signaled: 1,
                    }) as u64,
                    true,
                ),
                2 => (
                    p.objects.insert(Object::Timer {
                        manual: false,
                        signaled: false,
                        due: None,
                        period_ms: 0,
                    }) as u64,
                    true,
                ),
                3 => (
                    p.objects.insert(Object::IrTimer {
                        attributes: 8,
                        timer_id: None,
                        signaled: false,
                    }) as u64,
                    true,
                ),
                4 => (
                    p.objects.insert(Object::Mutex {
                        owner: None,
                        count: 0,
                        abandoned: false,
                    }) as u64,
                    true,
                ),
                5 | 6 => (
                    p.objects.insert(Object::Semaphore {
                        count: if kind == 5 { 1 } else { 0 },
                        max: 2,
                    }) as u64,
                    true,
                ),
                7 => (arch.ptr(u64::MAX - 1), false),
                8 => (arch.ptr(u64::MAX), false),
                9 => (object(p, 0, 0x1F0001), true),
                10 => (object(p, 1, 0x1F0003), true),
                11 => (p.objects.insert(Object::Null) as u64, true),
                12 => {
                    let id = p.objects.create(Object::Mapping {
                        file: None,
                        size: PAGE_SIZE,
                        protect: prot::READWRITE,
                        attributes: 0x0800_0000,
                        shared: None,
                    });
                    (
                        p.objects.open_access(id, false, 0xF000F).unwrap() as u64,
                        true,
                    )
                }
                _ => panic!("native target kind"),
            };
            assert_eq!(
                associate(p, &mut t, w, port, target, out),
                n(&row, "status", 16) as u32,
                "{arch}/{row:?}"
            );
            assert_eq!(
                p.space.bytes(out, 1).unwrap(),
                vec![n(&row, "already", 16) as u8]
            );
            assert_eq!(cancel(p, &mut t, w, 1), n(&row, "cancel", 16) as u32);
            if real {
                p.objects.close(target).unwrap();
            }
            p.objects.close(port).unwrap();
            p.objects.close(w).unwrap();
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 52);
}
#[test]
fn native_wait_packet_associate_replays_80_busy_validation_order_observations() {
    let captures = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-order-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-order-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-order-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-order-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, text) in captures {
        let records = rows(text, "order=");
        assert_eq!(records.len(), 20);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let out = output(p);
            let original = [
                object(p, 0, 0x1F0001),
                object(p, 1, 0x1F0003),
                object(p, 2, 0x1F0003),
            ];
            let mut handles = original;
            let mut extra = None;
            if n(&row, "busy", 10) != 0 {
                assert_eq!(
                    associate(p, &mut t, handles[0], handles[1], handles[2], out),
                    0
                );
            }
            match n(&row, "order", 10) {
                0 => {}
                1 => handles = [0, 0, 0],
                2 => {
                    let h = object(p, 2, 0);
                    handles = [h, 0, 0];
                    extra = Some(h)
                }
                3 => {
                    let h = object(p, 0, 0);
                    handles = [h, 0, 0];
                    extra = Some(h)
                }
                4 => {
                    handles[1] = 0;
                    handles[2] = 0
                }
                5 => {
                    let h = object(p, 2, 0);
                    handles[1] = h;
                    handles[2] = 0;
                    extra = Some(h)
                }
                6 => {
                    let h = object(p, 1, 0);
                    handles[1] = h;
                    handles[2] = 0;
                    extra = Some(h)
                }
                7 => handles[2] = 0,
                8 => {
                    let h = object(p, 2, 0);
                    handles[2] = h;
                    extra = Some(h)
                }
                9 => {
                    let h = p.objects.insert(Object::Mutex {
                        owner: None,
                        count: 0,
                        abandoned: false,
                    }) as u64;
                    handles[2] = h;
                    extra = Some(h)
                }
                _ => panic!("native order"),
            };
            p.space.wr(out, &[0xA5; 16]).unwrap();
            assert_eq!(
                associate(p, &mut t, handles[0], handles[1], handles[2], out),
                n(&row, "status", 16) as u32
            );
            assert_eq!(
                p.space.bytes(out, 1).unwrap(),
                vec![n(&row, "already", 16) as u8]
            );
            cancel(p, &mut t, original[0], 1);
            if let Some(h) = extra {
                p.objects.close(h).unwrap();
            }
            for h in original {
                p.objects.close(h).unwrap();
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 80);
}
#[test]
fn native_wait_packet_associate_replays_44_queued_boolean_and_dependency_close_boundaries() {
    let captures = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-boundaries-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-boundaries-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-boundaries-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wait-packet-associate/native-wait-packet-association-boundaries-x86-laa.log"
            ),
        ),
    ];
    let mut replayed = 0;
    for (arch, text) in captures {
        let records = rows(text, "queued-flag=");
        assert_eq!(records.len(), 7);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let w = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let target = p.objects.insert(Object::Event {
                manual: true,
                signaled: 1,
            }) as u64;
            assert_eq!(
                associate(p, &mut t, w, port, target, 0),
                n(&row, "associate", 16) as u32
            );
            assert_eq!(
                cancel(p, &mut t, w, n(&row, "queued-flag", 10)),
                n(&row, "cancel", 16) as u32
            );
            let id = p.objects.id(port).unwrap();
            assert_eq!(
                p.objects.dequeue_completion(id).is_some(),
                n(&row, "removed", 16) == 0
            );
            assert_eq!(cancel(p, &mut t, w, 1), n(&row, "cancel-again", 16) as u32);
            for h in [target, port, w] {
                p.objects.close(h).unwrap();
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
        let records = rows(text, "dependency-close=");
        assert_eq!(records.len(), 4);
        for row in records {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let w = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let target = p.objects.insert(Object::Event {
                manual: true,
                signaled: n(&row, "initial", 10) as i32,
            }) as u64;
            assert_eq!(
                associate(p, &mut t, w, port, target, 0),
                n(&row, "associate", 16) as u32
            );
            let port_close = n(&row, "dependency-close", 10) == 0;
            p.objects
                .close(if port_close { port } else { target })
                .unwrap();
            if port_close {
                assert_eq!(
                    status(p, &mut t, "NtSetEvent", &[target, 0]),
                    n(&row, "signal", 16) as u32
                );
            }
            assert_eq!(
                cancel(p, &mut t, w, 0),
                n(&row, "cancel-false", 16) as u32,
                "{arch}/{row:?}"
            );
            assert_eq!(cancel(p, &mut t, w, 1), n(&row, "cancel-true", 16) as u32);
            p.objects
                .close(if port_close { target } else { port })
                .unwrap();
            p.objects.close(w).unwrap();
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            replayed += 1;
        }
    }
    assert_eq!(replayed, 44);
}
