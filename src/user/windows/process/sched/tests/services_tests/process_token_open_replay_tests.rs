//! Replay native status, grant, caller-byte, guard and handle-birth records.
use super::*;
use std::collections::HashMap;

macro_rules! captures {
    ($profile:literal) => {
        [
            include_str!(concat!("../../../../../../../docs/specifications/windows/native-process-token-open/native/process-token-open-initial-", $profile, ".log")),
            include_str!(concat!("../../../../../../../docs/specifications/windows/native-process-token-open/native/process-token-open-boundary-v3-", $profile, ".log")),
            include_str!(concat!("../../../../../../../docs/specifications/windows/native-process-token-open/native/process-token-open-access-", $profile, ".log")),
        ]
    };
}

const CAPTURES: [(WinArch, [&str; 3]); 4] = [
    (WinArch::Arm64, captures!("arm64")),
    (WinArch::X64, captures!("x64")),
    (WinArch::X86, captures!("x86")),
    (WinArch::X86, captures!("x86-laa")),
];

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|word| word.split_once('='))
        .collect()
}

fn number(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}

fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn inputs(p: &mut Proc) -> Vec<u64> {
    let arch = p.arch;
    let mut handles = vec![arch.ptr(u64::MAX), 0, arch.ptr(u64::MAX - 1), 0x1234, 0];
    let process = p.objects.create(Object::Process {
        pid: p.pid,
        exit_code: None,
    });
    for grant in [0x10, 0x400, 0x1000, 0x1400, 0x1F_FFFF] {
        handles.push(p.objects.open_access(process, false, grant).unwrap().into());
    }
    handles.push(
        p.objects
            .insert(Object::Event {
                manual: true,
                signaled: 0,
            })
            .into(),
    );
    handles.push(0xFFFF_FFFF);
    for value in 3..=6 {
        handles.push(arch.ptr(u64::MAX - value + 1));
    }
    assert_eq!(handles.len(), 16);
    handles
}

fn destination(p: &mut Proc, base: u64, kind: usize, row: &HashMap<&str, &str>) -> u64 {
    p.vm.protect(base, 8192, prot::READWRITE).unwrap();
    p.space.wr(base, &[0xA5; 8192]).unwrap();
    if kind == 1 {
        p.vm.protect(base + 4096, 4096, number(row, "p1", 16) as u32)
            .unwrap();
        return base + number(row, "offset", 10);
    }
    let mode = number(row, "mode", 10);
    let mut output = base + 128;
    match mode {
        0 => {}
        1 => output = 0,
        2 => output = 1,
        3 => output += 1,
        4 => {
            p.vm.protect(base, 4096, prot::READONLY).unwrap();
        }
        5 => {
            p.vm.protect(base, 4096, prot::NOACCESS).unwrap();
        }
        6 => {
            p.vm.protect(base, 4096, prot::READWRITE | prot::GUARD)
                .unwrap();
        }
        7 => {
            p.vm.protect(base, 4096, prot::READONLY | prot::GUARD)
                .unwrap();
        }
        8 | 9 => {
            output = base + 4096 - p.arch.ptr_size() + u64::from(mode == 9);
            p.vm.protect(base + 4096, 4096, prot::NOACCESS).unwrap();
        }
        10 => {
            output = base + 4092;
            p.vm.protect(base + 4096, 4096, prot::READWRITE | prot::GUARD)
                .unwrap();
        }
        11 => output = base + 8192,
        _ => panic!("unknown native mode {mode}"),
    }
    output
}

fn compare_window(p: &Proc, window: u64, output: u64, expected: &str, native_handle: Option<u64>) {
    let expected = bytes(expected);
    let mut actual = p.space.bytes(window, expected.len()).unwrap();
    if let Some(handle) = native_handle {
        let width = p.arch.ptr_size();
        let encoded = handle.to_le_bytes();
        let begin = output.max(window);
        let end = (output + width).min(window + expected.len() as u64);
        for address in begin..end {
            actual[(address - window) as usize] = encoded[(address - output) as usize];
        }
    }
    assert_eq!(
        actual, expected,
        "caller bytes at {window:#x}, output={output:#x}"
    );
}

fn replay(kind: usize, total: usize) {
    let mut calls = 0;
    for (arch, logs) in CAPTURES {
        let (mut process, mut thread) = fixture(arch, "NtOpenProcessToken", SERVICE);
        let p = process.state_mut();
        captured(p);
        let handles = inputs(p);
        let base = scratch(p);
        let mut ordinal = 0;
        for line in logs[kind].lines().filter(|line| line.starts_with("case ")) {
            let row = fields(line);
            assert_eq!(number(&row, "ordinal", 10), ordinal, "{line}");
            ordinal += 1;
            let output = destination(p, base, kind, &row);
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let status = call(
                p,
                &mut thread,
                handles[number(&row, "handle", 10) as usize],
                number(&row, "desired", 16) as u32,
                output,
            );
            assert_eq!(
                u64::from(status),
                number(&row, "status", 16),
                "{arch} {line}"
            );
            assert_eq!(
                p.vm.query(base).unwrap().protect,
                number(&row, "after0", 16) as u32,
                "{line}"
            );
            assert_eq!(
                p.vm.query(base + 4096).unwrap().protect,
                number(&row, "after1", 16) as u32,
                "{line}"
            );
            assert_eq!(
                number(&row, "exception", 16),
                0,
                "native producer exception"
            );
            assert_eq!(number(&row, "lost", 10), 0, "native input handle loss");
            assert_eq!(number(&row, "last-error", 16), u64::from(LAST_ERROR));
            let success = status == STATUS_SUCCESS;
            assert_eq!(number(&row, "births", 10), u64::from(success), "{line}");
            assert_eq!(
                p.objects.handle_count(),
                before.0 + usize::from(success),
                "{line}"
            );
            let handle = if success {
                let handle = p.space.ptr(output, arch.ptr_size()).unwrap();
                assert!(matches!(p.objects.get(handle), Some(Object::PrimaryToken)));
                let grants: Vec<_> = row["grants"]
                    .split(',')
                    .filter(|part| !part.is_empty())
                    .map(|part| u32::from_str_radix(part, 16).unwrap())
                    .collect();
                assert_eq!(grants.len(), 1);
                assert_eq!(p.objects.access(handle), Some(grants[0]), "{line}");
                assert_eq!(p.objects.flags(handle), Some(0));
                assert_eq!(number(&row, "output-is-birth", 10), 1);
                Some(handle)
            } else {
                assert_eq!(
                    p.objects.iter().count(),
                    before.1,
                    "failed call created object: {line}"
                );
                None
            };
            p.vm.protect(base, 8192, prot::READWRITE).unwrap();
            let native_handle = success.then(|| number(&row, "output", 16));
            compare_window(p, base + 120, output, row["prefix"], native_handle);
            compare_window(p, base + 4080, output, row["boundary"], native_handle);
            if let Some(handle) = handle {
                p.objects.close(handle).unwrap();
            }
            assert_eq!(p.objects.handle_count(), before.0);
            calls += 1;
        }
        assert!(ordinal > 0);
    }
    assert_eq!(calls, total);
}

#[test]
fn native_process_token_replays_4096_initial_handle_access_and_output_records() {
    replay(0, 4096);
}

#[test]
fn native_process_token_replays_8640_native_output_boundary_records() {
    replay(1, 8640);
}

#[test]
fn native_process_token_replays_52860_native_access_masks_and_exact_grants() {
    replay(2, 52860);
}
