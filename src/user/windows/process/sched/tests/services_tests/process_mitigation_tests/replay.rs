use super::*;

const NATIVE: [&str; 8] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v2-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v2-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v2-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v2-four-views-x86-laa.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v4-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v4-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v4-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-process-mitigation/native/native-process-mitigation-v4-four-views-x86-laa.log"
    )),
];
fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split(' ').find_map(|s| s.strip_prefix(name)).unwrap()
}
fn number(line: &str, name: &str, radix: u32) -> u64 {
    u64::from_str_radix(field(line, name), radix).unwrap()
}
fn bytes(line: &str, name: &str) -> Vec<u8> {
    let raw = field(line, name);
    (0..raw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn native_process_mitigation_filter_replays_complete_native_fault_handle_and_alias_matrices() {
    for arch in WinArch::ALL {
        let (mut process, mut t, _) = setup(arch);
        let p = process.state_mut();
        let sp = t.cpu.sp();
        let o =
            p.vm.allocate(
                None,
                2 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let r =
            p.vm.allocate(
                None,
                2 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let mut handles = [
            arch.ptr(u64::MAX),
            0,
            0,
            0,
            0,
            arch.ptr(u64::MAX - 1),
            0xBAD,
            0,
        ];
        for (index, access) in [(1, 0x400), (2, 0x1000), (3, 0), (4, 0x20)] {
            let object = p.objects.create(Object::Process {
                pid: p.pid,
                exit_code: None,
            });
            handles[index] = u64::from(p.objects.open_access(object, false, access).unwrap());
        }
        let error = t.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
        p.space.w32(error, 0x1234_5678).unwrap();
        let before = (
            p.rng,
            p.objects.handle_count(),
            p.objects.iter().count(),
            p.vm.committed_bytes(),
        );
        let mut calls = 0;
        for native in NATIVE {
            let split_flags_guard = native.contains("name=out-flags-readonly-guard ");
            let rows = native
                .lines()
                .filter(|l| l.starts_with("QUERY "))
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 384);
            for line in rows {
                let role = number(line, "role=", 10) as usize;
                let case = number(line, "case=", 10) as usize;
                let length = number(line, "length=", 10) as u32;
                assert_eq!(number(line, "exception=", 16), 0);
                p.vm.protect(o, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
                p.vm.protect(r, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
                p.space
                    .wr(o, &vec![0xA5; (2 * PAGE_SIZE) as usize])
                    .unwrap();
                p.space
                    .wr(r, &vec![0xBB; (2 * PAGE_SIZE) as usize])
                    .unwrap();
                let output = match case {
                    11 | 20 => o + 1,
                    15 | 23 => o + PAGE_SIZE - 4,
                    16 => o + PAGE_SIZE - if split_flags_guard { 4 } else { 8 },
                    _ => o,
                };
                p.space.w32(output, 11).unwrap();
                let returned = match case {
                    12 => r + 1,
                    13 => output,
                    14 => output + 4,
                    _ => r,
                };
                let op = match case {
                    2 | 21 => prot::NOACCESS,
                    3 | 10 | 18 => prot::READWRITE | prot::GUARD,
                    4 | 19 => prot::READONLY,
                    5 | 22 => prot::READONLY | prot::GUARD,
                    _ => prot::READWRITE,
                };
                let rp = match case {
                    6 | 20 => prot::NOACCESS,
                    7 | 10 | 19 | 21 | 23 => prot::READWRITE | prot::GUARD,
                    8 | 18 => prot::READONLY,
                    9 | 22 => prot::READONLY | prot::GUARD,
                    _ => prot::READWRITE,
                };
                p.vm.protect(o, PAGE_SIZE, op).unwrap();
                p.vm.protect(r, PAGE_SIZE, rp).unwrap();
                let next = if split_flags_guard && matches!(case, 15 | 16 | 23) {
                    if case == 16 {
                        prot::READWRITE | prot::GUARD
                    } else {
                        prot::READONLY | prot::GUARD
                    }
                } else if matches!(case, 15 | 23) {
                    prot::NOACCESS
                } else {
                    prot::READWRITE
                };
                p.vm.protect(o + PAGE_SIZE, PAGE_SIZE, next).unwrap();
                t.cpu.set_sp(sp);
                arguments(
                    p,
                    &mut t,
                    handles[role],
                    52,
                    if case == 1 { 0 } else { output },
                    length,
                    if case == 17 { 0 } else { returned },
                );
                assert_eq!(dispatch(p, &mut t), Outcome::Continue, "{arch}/{line}");
                assert_eq!(t.cpu.gpr(0), number(line, "status=", 16), "{arch}/{line}");
                for (address, name) in [
                    (o, "out-protection="),
                    (o + PAGE_SIZE, "next-protection="),
                    (r, "ret-protection="),
                ] {
                    assert_eq!(
                        u64::from(p.vm.query(address).unwrap().protect),
                        number(line, name, 16),
                        "{arch}/{line}/{name}"
                    );
                }
                p.vm.protect(o, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
                p.vm.protect(r, 2 * PAGE_SIZE, prot::READWRITE).unwrap();
                assert_eq!(
                    p.space.bytes(output, 32).unwrap(),
                    bytes(line, "out="),
                    "{arch}/{line}"
                );
                assert_eq!(
                    p.space.bytes(r, 16).unwrap(),
                    bytes(line, "ret="),
                    "{arch}/{line}"
                );
                assert_eq!(p.space.u32(error).unwrap(), 0x1234_5678);
                assert_eq!(
                    (
                        p.rng,
                        p.objects.handle_count(),
                        p.objects.iter().count(),
                        p.vm.committed_bytes()
                    ),
                    before
                );
                assert!(t.frames.is_empty());
                calls += 1;
            }
        }
        assert_eq!(calls, 3072);
    }
}
