//! Complete original producer stdout replay; only typed identities are related.
use super::*;
use std::collections::HashMap;
const CAPTURES: [(WinArch, &str); 4] = [
    (
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-thread-slot/native-scheduler-thread-slot-probe-v2-arm64.log"
        ),
    ),
    (
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-thread-slot/native-scheduler-thread-slot-probe-v2-x64.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-thread-slot/native-scheduler-thread-slot-probe-v2-x86.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-thread-slot/native-scheduler-thread-slot-probe-v2-x86-laa.log"
        ),
    ),
];
fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|x| x.split_once('='))
        .collect()
}
fn num(f: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(f[key], radix).unwrap()
}
fn bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn native_scheduler_slot_replays_all_39488_status_guard_output_and_binding_rows() {
    let mut total = 0;
    for (arch, log) in CAPTURES {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        let mut threads = vec![arch.ptr(u64::MAX - 1), 0, arch.ptr(u64::MAX), 0x1234];
        for access in [0, 0x20, 0x400, 0x40, 0x800, 0x1fffff] {
            threads.push(p.objects.open_access(t.obj, false, access).unwrap().into());
        }
        let event = p.objects.insert(Object::Event {
            manual: false,
            signaled: 0,
        });
        threads.push(event.into());
        let proc = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        threads.push(p.objects.open_access(proc, false, 0x200).unwrap().into());
        threads.push(
            p.objects
                .open_access(t.obj, false, 0x100000)
                .unwrap()
                .into(),
        );
        threads.push(threads[9] | 3);
        let owners = [owner(p, 1), 0, 0x1234, threads[10], threads[11], threads[9]];
        let handles = p.objects.handle_count();
        let mut count = 0;
        for line in log.lines().filter(|l| l.starts_with("case=")) {
            let f = fields(line);
            assert_eq!(num(&f, "case", 10), count);
            count += 1;
            assert_eq!(num(&f, "width", 10), arch.ptr_size());
            assert_eq!(num(&f, "exception", 16), 0);
            assert_eq!(num(&f, "exact-handle-set", 10), 1);
            p.objects.free_scheduler_slot(t.tid);
            p.reclaim_scheduler_views().unwrap();
            p.vm.protect(base, PAGE_SIZE * 2, prot::READWRITE).unwrap();
            let before = if num(&f, "bound", 10) == 1 {
                assert_eq!(change(p, &mut t, base, 0, owners[0]), STATUS_SUCCESS);
                p.objects.scheduler_slot(t.tid)
            } else {
                0
            };
            p.space.wr(base, &vec![0xa5; 8192]).unwrap();
            let mode = num(&f, "buffer", 10);
            let size = if arch.is64() { 24 } else { 12 };
            let out = match mode {
                0 => base + 128,
                1 => 0,
                2 => 1,
                3 => base + 129,
                4 => base + 132,
                5..=8 => base + 4096,
                9..=11 => base + 4092,
                12 => base + 4096 - size,
                13 => arch.ptr(u64::MAX) - 7,
                _ => panic!("buffer mode"),
            };
            let action = num(&f, "action", 16) as u32;
            let input_owner = owners[num(&f, "owner", 10) as usize];
            let supplied = match num(&f, "slot-mode", 10) {
                0 => 0,
                1 => 1,
                2 => arch.ptr(u64::MAX),
                3 => before,
                _ => panic!("slot mode"),
            };
            let padding = num(&f, "padding-mode", 10);
            if ![1, 2, 13].contains(&mode) {
                record(
                    p,
                    out,
                    action,
                    input_owner,
                    supplied,
                    if padding == 0 { 0 } else { u32::MAX },
                );
                if arch == WinArch::X86 && padding != 0 {
                    p.space.w32(out + 12, u32::MAX).unwrap();
                }
            }
            let protection = match mode {
                5 | 11 => prot::READONLY,
                6 | 9 => prot::NOACCESS,
                7 | 10 => prot::READWRITE | prot::GUARD,
                8 => prot::READONLY | prot::GUARD,
                _ => prot::READWRITE,
            };
            p.vm.protect(base + 4096, 4096, protection).unwrap();
            let status = call(
                p,
                &mut t,
                threads[num(&f, "thread", 10) as usize],
                out,
                num(&f, "length", 16),
            );
            assert_eq!(status, num(&f, "status", 16) as u32, "{arch} {line}");
            let after = p.objects.scheduler_slot(t.tid);
            assert_eq!(after != 0, num(&f, "after-slot", 16) != 0, "{arch} {line}");
            if before != 0 && after != 0 {
                assert_eq!(before, after);
            }
            for (page, flag) in f["protection"].trim_end_matches(',').split(',').enumerate() {
                assert_eq!(
                    p.vm.query(base + page as u64 * 4096).unwrap().protect,
                    u32::from_str_radix(flag, 16).unwrap(),
                    "{arch} {line}"
                );
            }
            p.vm.protect(base, 8192, prot::READWRITE).unwrap();
            for (name, offset) in [("bytes0", 112), ("bytes1", 4064)] {
                let mut expected = bytes(f[name]);
                assert_eq!(expected.len(), 96);
                if ![1, 2, 13].contains(&mode) {
                    let owner_at = out + if arch.is64() { 8 } else { 4 };
                    let slot_at = owner_at + arch.ptr_size();
                    for (at, native, guest) in [
                        (owner_at, num(&f, "input-owner", 16), input_owner),
                        (slot_at, num(&f, "input-slot", 16), supplied),
                    ] {
                        if at >= base + offset && at + arch.ptr_size() <= base + offset + 96 {
                            let i = (at - base - offset) as usize;
                            let n = arch.ptr_size() as usize;
                            let captured = u64::from_le_bytes({
                                let mut b = [0; 8];
                                b[..n].copy_from_slice(&expected[i..i + n]);
                                b
                            });
                            let related = if at == slot_at && captured != native {
                                assert_eq!(action, 0);
                                assert_eq!(status, STATUS_SUCCESS);
                                assert_eq!(captured, num(&f, "after-slot", 16));
                                after
                            } else {
                                assert_eq!(captured, native, "unrelated field mutation {line}");
                                guest
                            };
                            expected[i..i + n].copy_from_slice(&related.to_le_bytes()[..n]);
                        }
                    }
                }
                assert_eq!(
                    p.space.bytes(base + offset, 96).unwrap(),
                    expected,
                    "{arch} {name} {line}"
                );
            }
            assert_eq!(p.objects.handle_count(), handles);
            p.objects.free_scheduler_slot(t.tid);
            p.reclaim_scheduler_views().unwrap();
            assert_eq!(p.objects.scheduler_slot(t.tid), 0);
            total += 1;
        }
        assert_eq!(count, 9872);
        assert!(log.contains(
            "complete cases=9872 exact-handle-set=1 owned-input-handles-closed=10 final-slot=0"
        ));
    }
    assert_eq!(total, 39488);
}
