//! Lossless byte/protection/status replay of independent installed-NTDLL probes.
use super::*;
use serde_json::Value;

macro_rules! capture {
    ($version:literal, $profile:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/specifications/windows/native-read-virtual-memory/native/",
            "native-read-virtual-memory-",
            $version,
            "-four-views-",
            $profile,
            ".log"
        ))
    };
}
fn number(row: &Value, name: &str) -> u64 {
    row[name].as_u64().unwrap()
}
fn bytes(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&value[at..at + 2], 16).unwrap())
        .collect()
}
fn pointer(kind: u64, role: usize, bases: &[u64; 3], faults: &[u64; 3], arch: WinArch) -> u64 {
    match kind {
        0 => bases[role] + PAGE_SIZE + 16,
        1 => 0,
        2 => 1,
        3 => bases[role] + PAGE_SIZE + 17,
        4 => bases[role] + faults[role] * PAGE_SIZE - 4,
        5 => bases[role] + faults[role] * PAGE_SIZE,
        6 => {
            if arch == WinArch::X86 {
                0xFFFF_FFF0
            } else {
                0x7FFF_FFFF_FFF0
            }
        }
        8..=12 => bases[0] + PAGE_SIZE + [16, 17, 24, 15, 8][kind as usize - 8],
        16..=31 => bases[role] + PAGE_SIZE + kind,
        100.. => bases[role] + faults[role] * PAGE_SIZE - (kind - 100),
        _ => arch.ptr(u64::MAX),
    }
}
fn replay(captures: &[(WinArch, bool, &str)], expected_rows: usize) {
    for &(arch, laa, capture) in captures {
        let (mut process, mut thread) = if laa {
            use crate::user::windows::process::{WindowsConfig, WindowsProcess};
            let mut image = include_bytes!(
                "../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe"
            )
            .to_vec();
            let pe = u32::from_le_bytes(image[0x3C..0x40].try_into().unwrap()) as usize;
            let flags = u16::from_le_bytes(image[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
            image[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
            let mut config =
                WindowsConfig::embedded("C:\\read-memory-laa.exe", vec![], vec![], 1).unwrap();
            config.arena_bytes = 64 << 20;
            let mut process = WindowsProcess::spawn_image(config, image).unwrap();
            let p = process.state_mut();
            let module = p.modules.by_name("ntdll.dll").unwrap();
            p.modules.nt_services = Some((module, table(arch, "NtReadVirtualMemory", 0x23)));
            let tid = *p.threads.keys().next().unwrap();
            let mut thread = p.threads.remove(&tid).unwrap();
            thread.attached = true;
            thread.main = false;
            (process, thread)
        } else {
            fixture(arch, "NtReadVirtualMemory", 0x23)
        };
        let p = process.state_mut();
        if arch == WinArch::X86 {
            assert_eq!(p.vm.high(), if laa { 0xFFFF0000 } else { 0x7FFF0000 });
        }
        let process_id = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        let event = p.objects.insert(Object::Event {
            manual: false,
            signaled: 0,
        });
        let grants: Vec<_> = [0, 0x10, 0x1400, 0x1000]
            .into_iter()
            .map(|grant| p.objects.open_access(process_id, false, grant).unwrap())
            .collect();
        let handles = [
            arch.ptr(u64::MAX),
            0,
            arch.ptr(u64::MAX - 1),
            u64::from(event),
            u64::from(grants[0]),
            u64::from(grants[1]),
            u64::from(grants[2]),
            u64::from(grants[3]),
        ];
        let mut count = 0;
        let mut seeds = std::collections::BTreeMap::<(u64, u8), [Vec<u8>; 3]>::new();
        for line in capture.lines().filter(|line| line.starts_with('{')) {
            let row: Value = serde_json::from_str(line).unwrap();
            assert_eq!(number(&row, "id"), count as u64);
            assert_eq!(number(&row, "exception"), 0);
            assert_eq!(number(&row, "last-error"), 0x12345678);
            let extent = row["extent"].as_u64().unwrap_or(PAGE_SIZE * 3);
            let faults = std::array::from_fn(|role| row["fault-pages"][role].as_u64().unwrap_or(2));
            let pattern = number(&row, "pattern") as u8;
            let bases = std::array::from_fn(|_| {
                p.vm.allocate(None, extent, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                    .unwrap()
                    .0
            });
            // Cache only immutable preimages; every row owns a fresh clone.
            // Full native afterimages and all observed bytes remain checked.
            let initial = seeds.entry((extent, pattern)).or_insert_with(|| {
                std::array::from_fn(|role| {
                    (0..extent)
                        .map(|at| {
                            if pattern == 0x80 && role < 2 {
                                0
                            } else if role == 0 {
                                (at as u8)
                                    .wrapping_mul(17)
                                    .wrapping_add(5)
                                    .wrapping_add(pattern)
                            } else {
                                (if role == 1 { 0xA5 } else { 0xD3 }) ^ pattern
                            }
                        })
                        .collect()
                })
            });
            let mut expected = initial.clone();
            for role in 0..3 {
                if !(pattern == 0x80 && role < 2) {
                    p.space.wr(bases[role], &expected[role]).unwrap();
                }
                let protection = number(
                    &row,
                    ["source-protect", "output-protect", "returned-protect"][role],
                ) as u32;
                p.vm.protect(bases[role] + PAGE_SIZE, PAGE_SIZE, protection)
                    .unwrap();
                p.vm.protect(
                    bases[role] + faults[role] * PAGE_SIZE,
                    PAGE_SIZE,
                    prot::NOACCESS,
                )
                .unwrap();
            }
            for change in row["changes"].as_array().unwrap() {
                let role = number(change, "role") as usize;
                let at = number(change, "offset") as usize;
                let data = bytes(change["data"].as_str().unwrap());
                let repeat = change["repeat"].as_u64().unwrap_or(1) as usize;
                for index in 0..repeat {
                    expected[role][at + index * data.len()..at + (index + 1) * data.len()]
                        .copy_from_slice(&data);
                }
            }
            let source = pointer(number(&row, "source"), 0, &bases, &faults, arch);
            let output = pointer(number(&row, "output"), 1, &bases, &faults, arch);
            let returned = match number(&row, "returned") {
                0 => pointer(0, 2, &bases, &faults, arch),
                1 => 0,
                2 => 1,
                3 => pointer(3, 2, &bases, &faults, arch),
                4 => output,
                5 => arch.ptr(output.wrapping_add(4)),
                6 => source,
                7 => pointer(4, 2, &bases, &faults, arch),
                _ => arch.ptr(u64::MAX),
            };
            let handle_kind = number(&row, "handle") as usize;
            if handle_kind >= 4 {
                assert_eq!(
                    p.objects.access(handles[handle_kind]),
                    Some(number(&row, "grant") as u32)
                );
            }
            let sp = thread.cpu.sp();
            arguments(
                p,
                &mut thread,
                &[
                    handles[handle_kind],
                    source,
                    output,
                    number(&row, "length"),
                    returned,
                ],
            );
            assert_eq!(
                dispatch(p, &mut thread),
                Outcome::Continue,
                "{arch} laa={laa} row={count}"
            );
            assert_eq!(
                thread.cpu.gpr(0) as u32,
                number(&row, "status") as u32,
                "{arch} laa={laa} row={count}"
            );
            assert!(thread.frames.is_empty());
            thread.cpu.set_sp(sp);
            for role in 0..3 {
                for (index, page) in [0, 1, faults[role]].into_iter().enumerate() {
                    assert_eq!(
                        p.vm.query(bases[role] + page * PAGE_SIZE).unwrap().protect,
                        row["protections"][role][index].as_u64().unwrap() as u32,
                        "{arch} laa={laa} row={count} role={role} page={page}"
                    );
                }
                let mut actual = vec![0; extent as usize];
                p.space.read_raw(bases[role], &mut actual).unwrap();
                if actual != expected[role] {
                    let at = actual
                        .iter()
                        .zip(&expected[role])
                        .position(|(a, b)| a != b)
                        .expect("different afterimages");
                    panic!(
                        "{arch} laa={laa} row={count} role={role} offset={at:#x}: actual={:#x} native={:#x}",
                        actual[at], expected[role][at]
                    );
                }
                p.vm.free(bases[role], 0, mem::RELEASE).unwrap();
            }
            count += 1;
        }
        assert_eq!(count, expected_rows, "{arch} laa={laa}");
    }
}
macro_rules! replay_test {
    ($name:ident,$version:literal,$rows:literal) => {
        #[test]
        fn $name() {
            replay(
                &[
                    (WinArch::Arm64, false, capture!($version, "arm64")),
                    (WinArch::X64, false, capture!($version, "x64")),
                    (WinArch::X86, false, capture!($version, "x86")),
                    (WinArch::X86, true, capture!($version, "x86-laa")),
                ],
                $rows,
            );
        }
    };
}
replay_test!(
    native_read_virtual_memory_replays_four_view_fault_and_grant_matrix,
    "v1",
    1528
);
replay_test!(
    native_read_virtual_memory_replays_four_view_blocks_and_aliases,
    "v2",
    520
);
replay_test!(
    native_read_virtual_memory_replays_four_view_large_aliases_and_lazy_pages,
    "v3",
    99
);
replay_test!(
    native_read_virtual_memory_replays_four_view_destination_store_boundaries,
    "v4",
    609
);

replay_test!(
    native_read_virtual_memory_replays_shifted_source_boundaries,
    "v6",
    609
);
replay_test!(
    native_read_virtual_memory_replays_all_source_byte_alignments,
    "v7",
    1920
);
