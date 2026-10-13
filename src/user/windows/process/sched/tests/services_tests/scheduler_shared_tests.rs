//! Captured native scheduler-data constructor, ABI, publication and lifetime.
use super::*;
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;
use std::collections::{BTreeSet, HashMap};

#[cfg(windows)]
#[path = "scheduler_shared_leaf_tests.rs"]
mod installed;

const CAPTURES: [(WinArch, &str); 4] = [
    (
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-shared-data/native-scheduler-shared-create-arm64.log"
        ),
    ),
    (
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-shared-data/native-scheduler-shared-create-x64.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-shared-data/native-scheduler-shared-create-x86.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-scheduler-shared-data/native-scheduler-shared-create-x86-laa.log"
        ),
    ),
];

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect()
}

fn number(f: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(f[key], radix).unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn setup(arch: WinArch) -> (super::super::super::super::WindowsProcess, Thread, u64) {
    let (mut process, t) = fixture(arch, "NtSetInformationProcess", 0x1c);
    let p = process.state_mut();
    let base =
        p.vm.allocate(
            None,
            PAGE_SIZE * 2,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    p.space
        .wr(base, &vec![0xa5; PAGE_SIZE as usize * 2])
        .unwrap();
    (process, t, base)
}

fn arguments(p: &mut Proc, t: &mut Thread, args: &[u64]) {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            p.space.w32(sp, 0x1234_0004).unwrap();
            p.space.w32(sp + 4, 0x1234_0000).unwrap();
            for (index, &value) in args.iter().enumerate() {
                p.space
                    .w32(sp + 8 + index as u64 * 4, value as u32)
                    .unwrap();
            }
            t.cpu.set_gpr(0, 0x1c);
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, 0x1c);
            t.cpu.set_gpr(1, 0x1234_0004);
            for (index, &value) in args.iter().enumerate() {
                t.cpu.set_gpr([10, 2, 8, 9][index], value);
            }
        }
        WinArch::Arm64 => {
            for (index, &value) in args.iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
}

fn dispatch(p: &mut Proc, t: &mut Thread) -> Outcome {
    if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, 0x1c))
    }
}

fn create(p: &mut Proc, t: &mut Thread, process: u64, out: u64, length: u64) -> u32 {
    arguments(p, t, &[process, 112, out, length]);
    assert_eq!(dispatch(p, t), Outcome::Continue);
    assert!(t.frames.is_empty());
    t.cpu.gpr(0) as u32
}

#[test]
fn native_scheduler_shared_birth_alias_close_and_process_drain_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        let count = p.objects.iter().count();
        let handles = p.objects.handle_count();
        assert_eq!(
            create(p, &mut t, arch.ptr(u64::MAX), base + 132, arch.ptr_size()),
            STATUS_SUCCESS
        );
        let h = p.space.ptr(base + 132, arch.ptr_size()).unwrap();
        let id = p.objects.id(h).unwrap();
        assert_eq!(p.objects.get(h).unwrap().type_name(), "SchedulerSharedData");
        assert_eq!(p.objects.access(h), Some(1));
        assert_eq!(p.objects.flags(h), Some(0));
        let alias = p.objects.duplicate(h, true).unwrap();
        assert_eq!(p.objects.id(alias.into()), Some(id));
        assert_eq!(p.objects.access(alias.into()), Some(1));
        assert_eq!(p.objects.flags(alias.into()), Some(1));
        assert!(p.objects.close(h).unwrap().is_none());
        assert!(p.objects.get(alias.into()).is_some());
        assert_eq!(
            p.objects.close(alias.into()).unwrap().unwrap().type_name(),
            "SchedulerSharedData"
        );
        assert_eq!(p.objects.iter().count(), count);
        assert_eq!(p.objects.handle_count(), handles);
        assert_eq!(
            create(p, &mut t, arch.ptr(u64::MAX), base + 128, arch.ptr_size()),
            STATUS_SUCCESS
        );
        assert_eq!(p.space.ptr(base + 128, arch.ptr_size()).unwrap(), h);
        assert_eq!(
            p.objects
                .drain()
                .filter(|object| object.type_name() == "SchedulerSharedData")
                .count(),
            1
        );
        assert_eq!(p.objects.handle_count(), 0);
        assert_eq!(p.objects.iter().count(), 0);
    }
}

#[test]
fn native_scheduler_shared_replays_all_16016_constructor_rows_and_owned_births() {
    let mut total = 0;
    for (arch, log) in CAPTURES {
        let (mut process, mut t, base) = setup(arch);
        let sp = t.cpu.sp();
        let p = process.state_mut();
        let mut processes = vec![arch.ptr(u64::MAX), 0, arch.ptr(u64::MAX - 1), 0x1234];
        for access in [0, 0x200, 0x2000, 0x400, 0x1000, 0x20, 0x10, 0x1fffff] {
            let id = p.objects.create(Object::Process {
                pid: p.pid,
                exit_code: None,
            });
            processes.push(p.objects.open_access(id, false, access).unwrap().into());
        }
        processes.push(
            p.objects
                .insert(Object::Event {
                    manual: false,
                    signaled: 0,
                })
                .into(),
        );
        let baseline: BTreeSet<_> = p.objects.iter().map(|(id, _)| id).collect();
        let baseline_handles = p.objects.handle_count();
        let mut count = 0;
        for line in log.lines().filter(|line| line.starts_with("case=")) {
            let f = fields(line);
            assert_eq!(number(&f, "case", 10), count);
            count += 1;
            assert_eq!(number(&f, "width", 10), arch.ptr_size());
            assert_eq!(number(&f, "exception", 16), 0);
            assert_eq!(number(&f, "restored", 10), 1);
            assert_eq!(f["handles-before"], f["handles-after"]);
            p.vm.protect(base, PAGE_SIZE * 2, prot::READWRITE).unwrap();
            p.space
                .wr(base, &vec![0xa5; PAGE_SIZE as usize * 2])
                .unwrap();
            let mode = number(&f, "output", 10);
            let out = match mode {
                0 => base + 128,
                1 => 0,
                2 => 1,
                3 => base + 129,
                4 => base + 132,
                5..=8 => base + PAGE_SIZE,
                9..=11 => base + PAGE_SIZE - 4,
                12 => base + PAGE_SIZE - 8,
                13 => arch.ptr(u64::MAX) - 7,
                _ => panic!("unknown native output mode"),
            };
            if ![1, 2, 13].contains(&mode) {
                p.space
                    .w64(
                        out,
                        if number(&f, "content", 10) == 0 {
                            0
                        } else {
                            u64::MAX
                        },
                    )
                    .unwrap();
            }
            let protection = match mode {
                5 | 11 => prot::READONLY,
                6 | 9 | 12 => prot::NOACCESS,
                7 | 10 => prot::READWRITE | prot::GUARD,
                8 => prot::READONLY | prot::GUARD,
                _ => prot::READWRITE,
            };
            p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, protection)
                .unwrap();
            t.cpu.set_sp(sp);
            let status = create(
                p,
                &mut t,
                processes[number(&f, "process", 10) as usize],
                out,
                number(&f, "length", 16),
            );
            assert_eq!(status, number(&f, "status", 16) as u32, "{arch} {line}");
            let born: Vec<_> = p
                .objects
                .iter()
                .filter_map(|(id, object)| {
                    (!baseline.contains(&id)).then_some((id, object.type_name()))
                })
                .collect();
            let expected_births = number(&f, "born-count", 10) as usize;
            assert_eq!(born.len(), expected_births, "{arch} {line}");
            assert_eq!(p.objects.handle_count(), baseline_handles + expected_births);
            let new_handles: Vec<_> = (4..65536)
                .step_by(4)
                .take(if expected_births == 0 { 0 } else { 16383 })
                .filter(|&handle| {
                    p.objects
                        .id(handle)
                        .is_some_and(|id| born.iter().any(|(created, _)| *created == id))
                })
                .collect();
            assert_eq!(
                new_handles.len(),
                expected_births,
                "fixture handle range/ownership {line}"
            );
            for &handle in &new_handles {
                assert_eq!(
                    p.objects.get(handle).unwrap().type_name(),
                    "SchedulerSharedData"
                );
                assert_eq!(p.objects.access(handle), Some(1));
                assert_eq!(p.objects.flags(handle), Some(0));
            }
            let native_born: Vec<_> = f["born"]
                .split(',')
                .filter(|item| !item.is_empty())
                .map(|item| {
                    let values: Vec<_> = item.split(':').collect();
                    assert_eq!(&values[1..], ["00000000", "00000001", "1", "0"]);
                    u64::from_str_radix(values[0], 16).unwrap()
                })
                .collect();
            assert_eq!(native_born.len(), expected_births);
            let flags: Vec<_> = f["protection"]
                .split(',')
                .filter(|item| !item.is_empty())
                .map(|value| u32::from_str_radix(value, 16).unwrap())
                .collect();
            assert_eq!(flags.len(), 2);
            for (page, expected) in flags.into_iter().enumerate() {
                assert_eq!(
                    p.vm.query(base + page as u64 * PAGE_SIZE).unwrap().protect,
                    expected,
                    "{arch} {line}"
                );
            }
            p.vm.protect(base, PAGE_SIZE * 2, prot::READWRITE).unwrap();
            for (name, offset, length) in [("bytes0", 120, 32), ("bytes1", 4080, 48)] {
                let mut expected = hex(f[name]);
                assert_eq!(expected.len(), length);
                // Relate observable identities only at the successful output
                // store. The original captured bytes and hidden births remain
                // intact; no other bytes or failure outputs are masked.
                if status == STATUS_SUCCESS
                    && out >= base + offset
                    && out + arch.ptr_size() <= base + offset + length as u64
                {
                    assert_eq!(native_born.len(), 1);
                    let at = (out - base - offset) as usize;
                    let end = at + arch.ptr_size() as usize;
                    assert_eq!(
                        &expected[at..end],
                        &native_born[0].to_le_bytes()[..arch.ptr_size() as usize]
                    );
                    expected[at..end]
                        .copy_from_slice(&new_handles[0].to_le_bytes()[..arch.ptr_size() as usize]);
                }
                assert_eq!(
                    p.space.bytes(base + offset, length).unwrap(),
                    expected,
                    "{arch} {name} {line}"
                );
            }
            for handle in new_handles {
                assert!(p.objects.close(handle).unwrap().is_some());
            }
            assert_eq!(
                p.objects.iter().map(|(id, _)| id).collect::<BTreeSet<_>>(),
                baseline
            );
            assert_eq!(p.objects.handle_count(), baseline_handles);
            total += 1;
        }
        assert_eq!(count, 4004);
    }
    assert_eq!(total, 16016);
}
