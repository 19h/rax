//! Replay all native rows with guest-owned pointers, handles and memory.
use super::*;
use std::collections::{HashMap, HashSet};

const CAPTURES: [(WinArch, &str); 4] = [
    (
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-console-owner/native-console-owner-arm64.log"
        ),
    ),
    (
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-console-owner/native-console-owner-x64.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-console-owner/native-console-owner-x86.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-console-owner/native-console-owner-x86-laa.log"
        ),
    ),
];

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect()
}

fn number(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}

fn line<'a>(block: &'a str, prefix: &str) -> HashMap<&'a str, &'a str> {
    fields(block.lines().find(|line| line.starts_with(prefix)).unwrap())
}

fn compare_cells(p: &Proc, at: u64, cells: &str, context: &str) {
    assert_eq!(cells.len() % 2, 0);
    for offset in (0..cells.len()).step_by(2) {
        let cell = &cells[offset..offset + 2];
        if cell != "??" {
            let expected = u8::from_str_radix(cell, 16).unwrap();
            let address = at.checked_add(offset as u64 / 2).unwrap();
            assert_eq!(
                p.space.u8(address).unwrap(),
                expected,
                "{context} byte={offset}"
            );
        }
    }
}

/// Relocates only this independent producer's logical fixture geometry. Status,
/// bytes, guard transitions and observable owner values come from raw NT output.
fn prepare(
    p: &mut Proc,
    base: u64,
    ordinal: u32,
    row: &HashMap<&str, &str>,
) -> (bool, u64, u64, u64, Option<u64>) {
    p.vm.protect(base, PAGE_SIZE * 6, prot::READWRITE).unwrap();
    p.space
        .wr(base, &vec![0xA5; PAGE_SIZE as usize * 6])
        .unwrap();
    p.space
        .w32(base + PAGE_SIZE * 4 + 128, 0xB6B6_B6B6)
        .unwrap();
    p.console_host_process = 1; // These children have completed detached native startup.
    let setting = number(row, "setting", 10) != 0;
    let item = if ordinal >= 120 {
        ordinal - 120 + 48
    } else if setting {
        ordinal
    } else {
        ordinal - 48
    };
    let mut buffer = base + 128;
    let mut returned = base + PAGE_SIZE * 4 + 128;
    if item == 7 {
        buffer += 1;
    }
    if item == 8 {
        buffer += 4;
    }
    if item == 12 || (setting && item == 13) || (!setting && item == 43) {
        buffer = base + PAGE_SIZE * 2 - 2;
    }
    if setting {
        p.space
            .wptr(buffer, p.arch.ptr_size(), number(row, "value", 16))
            .unwrap();
    }
    if item == 5 {
        buffer = 0;
    }
    if item == 6 {
        buffer = 1;
    }
    if (setting && item == 14) || (!setting && matches!(item, 13 | 44)) {
        buffer = p.arch.ptr(u64::MAX - 1);
    }
    let h = if setting { item } else { item + 1 };
    let mut owned = None;
    let mut process = if h == 18 {
        let id = p.objects.create(Object::Event {
            manual: false,
            signaled: 0,
        });
        let handle = u64::from(p.objects.open(id, false));
        owned = Some(handle);
        handle
    } else if (19..=25).contains(&h) {
        let grants = [0, 0x200, 0x2000, 0x400, 0x1000, 0x20, 0x1F_FFFF];
        let id = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        let handle = u64::from(
            p.objects
                .open_access(id, false, grants[(h - 19) as usize])
                .unwrap(),
        );
        owned = Some(handle);
        handle
    } else {
        number(row, "process", 16)
    };
    if setting {
        match item {
            34 | 37 | 45 => buffer = 0,
            35 | 36 | 38 => buffer = base + 129,
            _ => {}
        }
        if item == 37 {
            process = 0;
        }
        if (38..=42).contains(&item) {
            process = 0x1357_2468;
        }
    } else {
        match item {
            25 => returned = 0,
            26 => returned += 1,
            30 | 42 => returned = base + PAGE_SIZE * 5 - 2,
            31 => returned = buffer,
            32 => returned = buffer + 4,
            41 => returned = 1,
            _ => {}
        }
        if item == 35 {
            buffer = base + 129;
        }
        if item == 39 {
            buffer = 0;
        }
        if matches!(item, 33 | 37 | 38) {
            process = 0x1357_2468;
        }
    }
    if matches!(ordinal, 115 | 116 | 124) {
        process = 0;
    }
    if matches!(ordinal, 117 | 127 | 128 | 129 | 130) {
        buffer = p.arch.ptr(u64::MAX - 7);
    }
    if matches!(ordinal, 119 | 131 | 134) {
        buffer = 1;
    }
    if matches!(ordinal, 125 | 126) {
        buffer = base + PAGE_SIZE * 2 - 4;
    }
    if ordinal == 135 {
        buffer = base + PAGE_SIZE * 2 - p.arch.ptr_size();
        p.space
            .wptr(buffer, p.arch.ptr_size(), number(row, "value", 16))
            .unwrap();
    }
    p.vm.protect(base, PAGE_SIZE, number(row, "input-protect", 16) as u32)
        .unwrap();
    p.vm.protect(
        base + PAGE_SIZE * 2,
        PAGE_SIZE,
        if (setting && item == 13) || (!setting && item == 43) {
            prot::READONLY
        } else {
            prot::NOACCESS
        },
    )
    .unwrap();
    p.vm.protect(
        base + PAGE_SIZE * 4,
        PAGE_SIZE,
        number(row, "return-protect", 16) as u32,
    )
    .unwrap();
    p.vm.protect(base + PAGE_SIZE * 5, PAGE_SIZE, prot::NOACCESS)
        .unwrap();
    (setting, process, buffer, returned, owned)
}

#[test]
fn native_console_owner_replays_all_544_rows_and_136_repeat_sets_from_four_native_profiles() {
    let mut total = 0;
    let mut repeat_sets = 0;
    for (arch, text) in CAPTURES {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        let baseline_handles = p.objects.handle_count();
        let baseline_objects = p.objects.iter().count();
        let baseline_charge = p.vm.committed_bytes();
        let mut seen = HashSet::new();
        for block in text.split("child-exit case=") {
            let Some(raw) = block.lines().find(|line| line.starts_with("case=")) else {
                continue;
            };
            let row = fields(raw);
            let ordinal = number(&row, "case", 10) as u32;
            assert!(seen.insert(ordinal));
            assert_eq!(number(&row, "width", 10), arch.ptr_size());
            let result = line(block, "result case=");
            assert_eq!(number(&result, "escaped", 16), 0);
            let (setting, handle, buffer, returned, owned) = prepare(p, base, ordinal, &row);
            let context = format!("{arch}/case{ordinal}");
            let args = [handle, 49, buffer, number(&row, "length", 10), returned];
            assert_eq!(
                invoke(p, &mut t, setting, &args[..if setting { 4 } else { 5 }]),
                number(&result, "status", 16) as u32,
                "{context}"
            );
            assert_eq!(
                p.vm.query(base).unwrap().protect,
                number(&result, "input-protect", 16) as u32,
                "{context}"
            );
            assert_eq!(
                p.vm.query(base + PAGE_SIZE * 4).unwrap().protect,
                number(&result, "return-protect", 16) as u32,
                "{context}"
            );
            let observed = line(block, "state phase=after ");
            if arch != WinArch::X86 {
                assert_eq!(number(&observed, "status", 16), 0);
                assert_eq!(
                    p.console_host_process,
                    number(&observed, "value", 16),
                    "{context}"
                );
            } else {
                assert_eq!(
                    number(&observed, "status", 16),
                    u64::from(STATUS_INVALID_INFO_CLASS)
                );
            }
            // Remove protections only after checking the observed guard state.
            // Compare every individually readable native byte, including the
            // prefix of a page-crossing failed copy; inaccessible cells are ?? .
            p.vm.protect(base, PAGE_SIZE * 6, prot::READWRITE).unwrap();
            compare_cells(
                p,
                buffer,
                line(block, "bytes name=buffer-after ")["cells"],
                &context,
            );
            compare_cells(
                p,
                returned,
                line(block, "bytes name=returned-after ")["cells"],
                &context,
            );
            if let Some(second) = block.lines().find(|line| line.starts_with("second value=")) {
                let second = fields(second);
                let value = number(&second, "value", 16);
                p.space
                    .wptr(base + PAGE_SIZE * 3, arch.ptr_size(), value)
                    .unwrap();
                assert_eq!(
                    invoke(
                        p,
                        &mut t,
                        true,
                        &[
                            arch.ptr(u64::MAX),
                            49,
                            base + PAGE_SIZE * 3,
                            arch.ptr_size()
                        ]
                    ),
                    number(&second, "status", 16) as u32,
                    "{context}/repeat"
                );
                if arch != WinArch::X86 {
                    assert_eq!(
                        p.console_host_process,
                        number(&line(block, "state phase=second "), "value", 16)
                    );
                }
                repeat_sets += 1;
            }
            if let Some(handle) = owned {
                assert!(p.objects.close(handle).unwrap().is_some());
            }
            assert_eq!(p.objects.handle_count(), baseline_handles, "{context}");
            assert_eq!(p.objects.iter().count(), baseline_objects, "{context}");
            assert_eq!(p.vm.committed_bytes(), baseline_charge, "{context}");
            total += 1;
        }
        assert_eq!(seen, (0..136).collect());
    }
    assert_eq!(total, 544);
    assert_eq!(repeat_sets, 136);
}
