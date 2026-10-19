//! Exact native status, output bytes and guard-state replay; no host NT calls.
use super::event_tests::{attrs, handle, setup, status};
use super::*;
use crate::user::windows::object_namespace::reparse_tests;

type Profile = (
    WinArch,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);
const PROFILES: [Profile; 4] = [
    (
        WinArch::Arm64,
        "arm64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/create-arm64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/open-arm64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/release-arm64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/query-arm64.tsv"
        ),
    ),
    (
        WinArch::X64,
        "x64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/create-x64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/open-x64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/release-x64.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/query-x64.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/create-x86.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/open-x86.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/release-x86.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/query-x86.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86-laa",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/create-x86-laa.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/open-x86-laa.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/release-x86-laa.tsv"
        ),
        include_str!(
            "../../../../../../../docs/specifications/windows/native-semaphore/replay/query-x86-laa.tsv"
        ),
    ),
];
const PAIRS: [(i32, i32); 8] = [
    (0, 1),
    (1, 2),
    (-1, 2),
    (0, 0),
    (3, 2),
    (i32::MIN, i32::MAX),
    (0, i32::MAX),
    (i32::MAX, i32::MAX),
];

fn number(value: &str) -> u32 {
    value.parse().unwrap()
}
fn hex(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn allocation(p: &mut Proc) -> u64 {
    p.vm.allocate(
        None,
        2 * PAGE_SIZE,
        mem::RESERVE | mem::COMMIT,
        prot::READWRITE,
    )
    .unwrap()
    .0
}
fn reset(p: &mut Proc, base: u64) {
    p.vm.protect(base, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
    p.space
        .wr(base, &vec![0xA5; 2 * PAGE_SIZE as usize])
        .unwrap();
}
fn pointer(p: &Proc, base: u64, mode: u32) -> u64 {
    match mode {
        0 | 1 => base + 32 + u64::from(mode),
        2..=5 => base + PAGE_SIZE,
        6 => 0,
        7 => 1,
        8 => p.arch.ptr(u64::MAX - 1),
        9..=12 => base + PAGE_SIZE - 4,
        13..=15 => base + PAGE_SIZE - 1,
        _ => panic!("unknown pointer mode {mode}"),
    }
}
fn protect(p: &mut Proc, base: u64, mode: u32) {
    let protection = match mode {
        2 | 10 | 13 => prot::READONLY,
        3 | 11 | 14 => prot::NOACCESS,
        4 | 12 | 15 => prot::READWRITE | prot::GUARD,
        5 => prot::READONLY | prot::GUARD,
        _ => prot::READWRITE,
    };
    p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, protection)
        .unwrap();
}
fn prepare(p: &mut Proc, base: u64, mode: u32) -> u64 {
    reset(p, base);
    let address = pointer(p, base, mode);
    protect(p, base, mode);
    address
}
fn samples(p: &mut Proc, base: u64, normalized_handle: Option<(u32, usize)>) -> String {
    let protection = p.vm.query(base + PAGE_SIZE).unwrap().protect;
    p.vm.protect(base, PAGE_SIZE * 2, prot::READWRITE).unwrap();
    let mut bytes = Vec::with_capacity(96);
    for at in [base + 32, base + PAGE_SIZE - 32, base + PAGE_SIZE] {
        bytes.extend(p.space.bytes(at, 32).unwrap());
    }
    if let Some((mode, width)) = normalized_handle {
        let offset = match mode {
            0 => 0,
            1 => 1,
            2..=5 => 64,
            9..=12 => 60,
            13..=15 => 63,
            _ => panic!("successful invalid pointer mode"),
        };
        bytes[offset..offset + width].copy_from_slice(&[8, 7, 6, 5, 4, 3, 2, 1][..width]);
    }
    let mut text = format!("{protection:08X}:");
    for byte in bytes {
        text.push_str(&format!("{byte:02X}"));
    }
    text
}
fn attributes(p: &mut Proc, base: u64, ac: u32, descriptor: u64, opening: bool) -> u64 {
    let mode = match ac {
        4 => 1,
        5 => 7,
        6 => 2,
        7 => 3,
        8 => 4,
        _ => 0,
    };
    reset(p, base);
    let at = pointer(p, base, mode);
    if ac != 5 {
        let length = if ac == 2 {
            0
        } else {
            (if p.arch.is64() { 48 } else { 24 }) - u32::from(ac == 3)
        };
        attrs(
            p,
            at,
            length,
            u32::from(ac == 9),
            if ac == 10 { 0x1234_5678 } else { 0 },
        );
        let name = if ac == 11 {
            1
        } else if opening && ac != 1 {
            descriptor
        } else {
            0
        };
        p.space
            .wptr(at + 2 * p.arch.ptr_size(), p.arch.ptr_size(), name)
            .unwrap();
    }
    protect(p, base, mode);
    if ac == 0 { 0 } else { at }
}

#[test]
fn native_semaphore_create_replays_6144_fault_count_and_publication_cases() {
    for (arch, profile, rows, _, _, _) in PROFILES {
        let (mut process, mut t, _) = setup(arch);
        let p = process.state_mut();
        let output = allocation(p);
        let oa = allocation(p);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 7);
            let attr = attributes(p, oa, number(row[0]), 0, false);
            let out = prepare(p, output, number(row[2]));
            let (initial, maximum) = PAIRS[number(row[1]) as usize];
            let code = status(
                p,
                &mut t,
                "NtCreateSemaphore",
                &[
                    out,
                    0x1F0003,
                    attr,
                    initial as u32 as u64,
                    maximum as u32 as u64,
                ],
            );
            assert_eq!(code, hex(row[3]), "{profile} {count} {row:?}");
            if code == 0 {
                let h = handle(p, out);
                assert_eq!(p.objects.access(h), Some(0x1F0003));
                assert!(
                    matches!(p.objects.get(h), Some(Object::Semaphore { count, max }) if *count == initial && *max == maximum)
                );
                p.objects.close(h).unwrap();
            }
            assert_eq!(
                samples(
                    p,
                    output,
                    (code == 0).then_some((number(row[2]), arch.ptr_size() as usize))
                ),
                row[4],
                "{profile} {count} output"
            );
            assert_eq!(
                p.vm.query(oa + PAGE_SIZE).unwrap().protect,
                hex(row[5]),
                "{profile} {count} attribute guard"
            );
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            count += 1;
        }
        assert_eq!(count, 1536);
    }
}

#[test]
fn native_semaphore_open_replays_768_attribute_and_copy_cases() {
    for (arch, profile, _, rows, _, _) in PROFILES {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        p.object_namespace = reparse_tests::sample(0);
        let output = allocation(p);
        let oa = allocation(p);
        let baseline = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 7);
            let name: Vec<_> = format!("\\BaseNamedObjects\\AssistSemaphoreMissing-{}", row[2])
                .encode_utf16()
                .collect();
            p.space.wr(scratch, &[0; 16]).unwrap();
            p.space.w16(scratch, (name.len() * 2) as u16).unwrap();
            p.space.w16(scratch + 2, (name.len() * 2) as u16).unwrap();
            p.space
                .wptr(scratch + arch.ptr_size(), arch.ptr_size(), scratch + 128)
                .unwrap();
            p.space.put_wstr(scratch + 128, &name).unwrap();
            let attr = attributes(p, oa, number(row[0]), scratch, true);
            let out = prepare(p, output, number(row[1]));
            let code = status(p, &mut t, "NtOpenSemaphore", &[out, 0x1F0003, attr]);
            assert_eq!(code, hex(row[3]), "{profile} {count} {row:?}");
            assert_eq!(samples(p, output, None), row[4], "{profile} {count} output");
            assert_eq!(
                p.vm.query(oa + PAGE_SIZE).unwrap().protect,
                hex(row[5]),
                "{profile} {count} attribute guard"
            );
            assert_eq!(p.objects.handle_count(), baseline);
            count += 1;
        }
        assert_eq!(count, 192);
    }
}

fn handles(p: &mut Proc) -> [u64; 10] {
    let master = p.objects.insert(Object::Semaphore { count: 1, max: 2 }) as u64;
    let id = p.objects.id(master).unwrap();
    let event = p.objects.insert(Object::Event {
        manual: false,
        signaled: 0,
    }) as u64;
    [
        master,
        0,
        0x1234_5678,
        p.arch.ptr(u64::MAX),
        p.arch.ptr(u64::MAX - 1),
        event,
        p.objects.open_access(id, false, 0).unwrap() as u64,
        p.objects.open_access(id, false, 1).unwrap() as u64,
        p.objects.open_access(id, false, 2).unwrap() as u64,
        p.objects.open_access(id, false, 0x100000).unwrap() as u64,
    ]
}

#[test]
fn native_semaphore_release_replays_4480_fault_access_and_count_cases() {
    for (arch, profile, _, _, rows, _) in PROFILES {
        let (mut process, mut t, _) = setup(arch);
        let p = process.state_mut();
        let output = allocation(p);
        let hs = handles(p);
        let baseline = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 6);
            let id = p.objects.id(hs[0]).unwrap();
            let Some(Object::Semaphore { count: value, .. }) = p.objects.obj_mut(id) else {
                panic!("master semaphore lost");
            };
            *value = 1;
            let out = prepare(p, output, number(row[2]));
            let delta: i32 = row[1].parse().unwrap();
            let code = status(
                p,
                &mut t,
                "NtReleaseSemaphore",
                &[hs[number(row[0]) as usize], delta as u32 as u64, out],
            );
            assert_eq!(code, hex(row[3]), "{profile} {count} {row:?}");
            assert!(
                matches!(p.objects.get(hs[0]), Some(Object::Semaphore { count, max: 2 }) if *count == row[4].parse::<i32>().unwrap())
            );
            assert_eq!(samples(p, output, None), row[5], "{profile} {count} output");
            assert_eq!(p.objects.handle_count(), baseline);
            count += 1;
        }
        assert_eq!(count, 1120);
    }
}

#[test]
fn native_semaphore_query_replays_27916_probe_priority_alias_and_copy_cases() {
    for (arch, profile, _, _, _, rows) in PROFILES {
        let (mut process, mut t, _) = setup(arch);
        let p = process.state_mut();
        let output = allocation(p);
        let returned = allocation(p);
        let hs = handles(p);
        let baseline = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 8);
            let out = prepare(p, output, number(row[3]));
            let ret = if let Some(offset) = row[4].strip_prefix('a') {
                out + u64::from(number(offset))
            } else {
                prepare(p, returned, number(row[4]))
            };
            let code = status(
                p,
                &mut t,
                "NtQuerySemaphore",
                &[
                    hs[number(row[0]) as usize],
                    number(row[1]).into(),
                    out,
                    number(row[2]).into(),
                    ret,
                ],
            );
            assert_eq!(code, hex(row[5]), "{profile} {count} {row:?}");
            assert_eq!(samples(p, output, None), row[6], "{profile} {count} output");
            if row[7] != "-" {
                assert_eq!(
                    samples(p, returned, None),
                    row[7],
                    "{profile} {count} return length"
                );
            }
            assert_eq!(p.objects.handle_count(), baseline);
            assert!(matches!(
                p.objects.get(hs[0]),
                Some(Object::Semaphore { count: 1, max: 2 })
            ));
            count += 1;
        }
        assert_eq!(count, 6979);
    }
}
