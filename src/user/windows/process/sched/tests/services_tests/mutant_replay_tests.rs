//! Replays native status, padding, alias and guard observations without host calls.
use super::event_tests::{attrs, handle, setup, status};
use super::*;

const QUERY: [(WinArch, &str, &str); 4] = [
    (
        WinArch::Arm64,
        "arm64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/query-arm64.tsv"
        ),
    ),
    (
        WinArch::X64,
        "x64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/query-x64.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/query-x86.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86-laa",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/query-x86-laa.tsv"
        ),
    ),
];
const CREATE: [(WinArch, &str, &str); 4] = [
    (
        WinArch::Arm64,
        "arm64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/create-arm64.tsv"
        ),
    ),
    (
        WinArch::X64,
        "x64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/create-x64.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/create-x86.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86-laa",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/create-x86-laa.tsv"
        ),
    ),
];

fn hex(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn decimal(value: &str) -> u32 {
    value.parse().unwrap()
}

const RELEASE: [(WinArch, &str, &str); 4] = [
    (
        WinArch::Arm64,
        "arm64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/release-arm64.tsv"
        ),
    ),
    (
        WinArch::X64,
        "x64",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/release-x64.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/release-x86.tsv"
        ),
    ),
    (
        WinArch::X86,
        "x86-laa",
        include_str!(
            "../../../../../../../docs/specifications/windows/native-mutant/replay/release-x86-laa.tsv"
        ),
    ),
];

fn state(p: &mut Proc, t: &mut Thread, h: u64, output: u64) -> String {
    p.space.wr(output, &[0xA5; 36]).unwrap();
    let code = status(p, t, "NtQueryMutant", &[h, 0, output, 8, output + 32]);
    let mut text = format!("{code:08X}/{:08X}/", p.space.u32(output + 32).unwrap());
    for byte in p.space.bytes(output, 8).unwrap() {
        text.push_str(&format!("{byte:02X}"));
    }
    text
}

#[test]
fn native_mutant_release_replays_all_200_native_fault_and_state_cases() {
    for (arch, profile, rows) in RELEASE {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let output =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let event = p.objects.insert(Object::Event {
            manual: false,
            signaled: 0,
        }) as u64;
        let initial = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 7);
            assert_eq!(
                status(p, &mut t, "NtCreateMutant", &[scratch, 0x1F0001, 0, 1]),
                0
            );
            let owner = handle(p, scratch);
            let id = p.objects.id(owner).unwrap();
            let zero = p.objects.open_access(id, false, 0).unwrap() as u64;
            let handles = [owner, zero, event, 0x1234, 0];
            let previous = prepare(p, output, decimal(row[2]));
            assert_eq!(state(p, &mut t, owner, scratch + 64), row[5]);
            assert_eq!(
                status(
                    p,
                    &mut t,
                    "NtReleaseMutant",
                    &[handles[decimal(row[1]) as usize], previous]
                ),
                hex(row[3]),
                "{profile} native release case{}",
                row[0]
            );
            assert_eq!(
                samples(p, output),
                row[4],
                "{profile} native release case{} output",
                row[0]
            );
            assert_eq!(state(p, &mut t, owner, scratch + 64), row[6]);
            p.objects.close(zero).unwrap();
            p.objects.close(owner).unwrap();
            assert_eq!(p.objects.handle_count(), initial);
            count += 1;
        }
        assert_eq!(count, 50);
        p.objects.close(event).unwrap();
    }
}

fn prepare(p: &mut Proc, base: u64, role: u32) -> u64 {
    p.vm.protect(base, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
    p.space
        .wr(base, &vec![0xA5; 2 * PAGE_SIZE as usize])
        .unwrap();
    let protection_role = match role {
        10 => 4,
        11 | 13 => 5,
        12 | 14 => 6,
        _ => role,
    };
    if (4..=7).contains(&protection_role) {
        p.vm.protect(
            base + PAGE_SIZE,
            PAGE_SIZE,
            [
                prot::READONLY,
                prot::NOACCESS,
                prot::READWRITE | prot::GUARD,
                prot::READONLY | prot::GUARD,
            ][(protection_role - 4) as usize],
        )
        .unwrap();
    }
    match role {
        0 => base + 32,
        1 => base + 33,
        2 | 10..=12 => base + PAGE_SIZE - 4,
        3 | 13 | 14 => base + PAGE_SIZE - 1,
        4..=7 => base + PAGE_SIZE,
        8 => 0,
        9 => 1,
        15 => p.arch.ptr(u64::MAX - 1),
        _ => panic!("unexpected role"),
    }
}
fn samples(p: &mut Proc, base: u64) -> String {
    let protection = p.vm.query(base + PAGE_SIZE).unwrap().protect;
    p.vm.protect(base, PAGE_SIZE * 2, prot::READWRITE).unwrap();
    let mut result = format!("{protection:08X}:");
    for at in [base + 32, base + PAGE_SIZE - 16, base + PAGE_SIZE] {
        for byte in p.space.bytes(at, 16).unwrap() {
            result.push_str(&format!("{byte:02X}"));
        }
    }
    result
}

#[test]
fn native_mutant_query_replays_all_25200_native_status_and_copy_cases() {
    for (arch, profile, rows) in QUERY {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let output =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let returned =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        assert_eq!(
            status(p, &mut t, "NtCreateMutant", &[scratch, 0x1F0001, 0, 0]),
            0
        );
        let h = handle(p, scratch);
        let id = p.objects.id(h).unwrap();
        let zero = p.objects.open_access(id, false, 0).unwrap() as u64;
        let event = p.objects.insert(Object::Event {
            manual: false,
            signaled: 0,
        }) as u64;
        let handles = [h, zero, event, 0x1234, 0];
        let before = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 9);
            let out = prepare(p, output, decimal(row[4]));
            let ret = if decimal(row[5]) == 10 {
                prepare(p, returned, 0);
                out
            } else {
                prepare(p, returned, decimal(row[5]))
            };
            let actual = status(
                p,
                &mut t,
                "NtQueryMutant",
                &[
                    handles[decimal(row[1]) as usize],
                    hex(row[2]).into(),
                    out,
                    hex(row[3]).into(),
                    ret,
                ],
            );
            assert_eq!(actual, hex(row[6]), "{profile} native query case{}", row[0]);
            assert_eq!(
                samples(p, output),
                row[7],
                "{profile} case{} output",
                row[0]
            );
            assert_eq!(
                samples(p, returned),
                row[8],
                "{profile} case{} returned",
                row[0]
            );
            assert_eq!(p.objects.handle_count(), before);
            count += 1;
        }
        assert_eq!(count, 6300);
        for h in [event, zero, h] {
            p.objects.close(h).unwrap();
        }
    }
}

#[test]
fn native_mutant_create_replays_all_12800_native_fault_status_cases() {
    for (arch, profile, rows) in CREATE {
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let output =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let area =
            p.vm.allocate(
                None,
                PAGE_SIZE * 2,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let initial = p.objects.handle_count();
        let mut count = 0;
        for line in rows.lines() {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 6);
            let role = decimal(row[1]);
            let out_role = decimal(row[2]);
            let mut out = prepare(p, output, if out_role >= 16 { 0 } else { out_role });
            let storage = prepare(p, area, 0);
            let at = match role {
                5 => storage + 1,
                7..=9 => area + PAGE_SIZE,
                _ => storage,
            };
            let width = arch.ptr_size();
            let size = if arch.is64() { 48 } else { 24 };
            attrs(
                p,
                at,
                match role {
                    2 => 0,
                    3 => size - 1,
                    4 => size + 1,
                    _ => size,
                },
                match role {
                    15 => 1,
                    16 => 0x2000,
                    17 => 0x80,
                    18 => 0x40,
                    19 => 2,
                    _ => 0,
                },
                if role == 13 { 0x1234 } else { 0 },
            );
            if role == 10 || role == 11 {
                p.space
                    .wptr(
                        at + if arch.is64() {
                            if role == 10 { 32 } else { 40 }
                        } else if role == 10 {
                            16
                        } else {
                            20
                        },
                        width,
                        1,
                    )
                    .unwrap();
            }
            if role == 12 || role == 14 {
                let descriptor = scratch + 256;
                p.space.wr(descriptor, &[0; 16]).unwrap();
                p.space.wptr(at + 2 * width, width, descriptor).unwrap();
                p.space
                    .wptr(descriptor + width, width, scratch + 512)
                    .unwrap();
                let name: Vec<u16> = if role == 12 {
                    vec![]
                } else {
                    "AssistMutantFaultUnrooted".encode_utf16().collect()
                };
                p.space.w16(descriptor, (name.len() * 2) as u16).unwrap();
                p.space.put_wstr(scratch + 512, &name).unwrap();
            }
            if (7..=9).contains(&role) {
                p.vm.protect(
                    area + PAGE_SIZE,
                    PAGE_SIZE,
                    [
                        prot::NOACCESS,
                        prot::READWRITE | prot::GUARD,
                        prot::READONLY | prot::GUARD,
                    ][(role - 7) as usize],
                )
                .unwrap();
            }
            let address = match role {
                0 => 0,
                6 => 1,
                _ => at,
            };
            if out_role >= 16 {
                out = storage + u64::from(out_role - 16) * width;
            }
            let actual = status(
                p,
                &mut t,
                "NtCreateMutant",
                &[out, hex(row[4]).into(), address, hex(row[3]).into()],
            );
            assert_eq!(
                actual,
                hex(row[5]),
                "{profile} native create case{} attrs{role} output{out_role}",
                row[0]
            );
            if (actual as i32) >= 0 {
                let h = handle(p, out);
                assert!(matches!(p.objects.get(h), Some(Object::Mutex { .. })));
                p.objects.close(h).unwrap();
            }
            assert_eq!(
                p.objects.handle_count(),
                initial,
                "{profile} native create case{} leaked",
                row[0]
            );
            count += 1;
        }
        assert_eq!(count, 3200);
    }
}
